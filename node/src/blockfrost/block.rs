// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Block and mc-hash data source: stable-block selection and Cardano tip queries.

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use blockfrost::blockfrost_openapi::models::block_content::BlockContent;
use lru::LruCache;
use pallet_sidechain_rpc::SidechainRpcDataSource;
use sidechain_domain::*;
use sidechain_mc_hash::{McHashDataSource, StableBlockByHashResult};
use sp_timestamp::Timestamp;

use super::client::*;
use super::convert::*;
use super::support::*;

fn mainchain_block(b: &BlockContent) -> Result<MainchainBlock, BoxError> {
	Ok(MainchainBlock {
		number: McBlockNumber(block_height(b)?),
		hash: McBlockHash(decode_hash32(&b.hash)?),
		epoch: McEpochNumber(u32::try_from(b.epoch.ok_or("block has no epoch")?)?),
		slot: McSlotNumber(u64::try_from(b.slot.ok_or("block has no slot")?)?),
		timestamp: u64::try_from(b.time)?,
	})
}

fn block_time_ms(b: &BlockContent) -> i64 {
	i64::from(b.time) * 1000
}

// ---------------------------------------------------------------------------
// Block data source: McHashDataSource + SidechainRpcDataSource
// ---------------------------------------------------------------------------

/// Mirrors `BlockDataSourceImpl` from `partner-chains-db-sync-data-sources`.
pub struct BlockfrostBlockDataSource {
	client: Arc<BlockfrostClient>,
	security_parameter: u32,
	block_stability_margin: u32,
	/// `security_parameter / active_slots_coeff` in milliseconds (`k/f`).
	min_slot_boundary_ms: i64,
	/// `3k/f` in milliseconds.
	max_slot_boundary_ms: i64,
	max_latest_block_age_seconds: u64,
	/// Blocks classified stable by `get_stable_block_for`, for cheap re-verification.
	stable_blocks: Mutex<LruCache<McBlockHash, MainchainBlock>>,
}

/// Pure stable-block classification, mirroring `get_stable_block_by_hash_from_db`:
/// the timestamp-range check comes FIRST (final error) before the confirmation-count
/// check (transient error).
fn classify_stable(
	block: MainchainBlock,
	latest_height: u32,
	reference_timestamp_ms: i64,
	security_parameter: u32,
	min_slot_boundary_ms: i64,
	max_slot_boundary_ms: i64,
) -> StableBlockByHashResult {
	let block_time_ms = i64::try_from(block.timestamp).unwrap_or(i64::MAX) * 1000;
	let min_allowed = reference_timestamp_ms - max_slot_boundary_ms;
	let max_allowed = reference_timestamp_ms - min_slot_boundary_ms;
	if !(min_allowed <= block_time_ms && block_time_ms <= max_allowed) {
		return StableBlockByHashResult::BlockTimestampOutRange { info: block };
	}
	if block.number.0.saturating_add(security_parameter) > latest_height {
		return StableBlockByHashResult::NotEnoughConfirmations { info: block };
	}
	StableBlockByHashResult::BlockStable { info: block }
}

impl BlockfrostBlockDataSource {
	pub fn new(
		client: Arc<BlockfrostClient>,
		security_parameter: u32,
		active_slots_coeff: f64,
		block_stability_margin: u32,
		slot_duration_millis: u64,
	) -> Self {
		let k: f64 = security_parameter.into();
		let slot_ms = slot_duration_millis as f64;
		let min_slot_boundary_ms = (slot_ms * k / active_slots_coeff).round() as i64;
		let expected_block_interval_secs = ((slot_ms / 1000.0) / active_slots_coeff).round() as u64;
		Self {
			client,
			security_parameter,
			block_stability_margin,
			min_slot_boundary_ms,
			max_slot_boundary_ms: 3 * min_slot_boundary_ms,
			max_latest_block_age_seconds: u64::from(block_stability_margin.max(1))
				* expected_block_interval_secs,
			stable_blocks: Mutex::new(LruCache::new(NonZeroUsize::new(100).unwrap())),
		}
	}

	async fn latest(&self) -> Result<BlockContent, BoxError> {
		let _t = Timer::new("GET blocks/latest");
		deadline("blocks/latest", self.client.api.blocks_latest())
			.await?
			.map_err(box_err)
	}

	/// Highest block with height ≤ `max_height` and time ≤ `max_time_ms`.
	///
	/// Block timestamps are monotonically non-decreasing, so when the block at
	/// `max_height` is too new, a binary search over heights finds the boundary
	/// (≤ ~25 lookups; only happens when Cardano block production stalls).
	async fn highest_block_at_or_before_time(
		&self,
		max_height: u32,
		max_time_ms: i64,
	) -> Result<Option<BlockContent>, BoxError> {
		let Some(top) = self.client.block_by_id(&max_height.to_string()).await? else {
			return Ok(None);
		};
		if block_time_ms(&top) <= max_time_ms {
			return Ok(Some(top));
		}
		let mut lo: u32 = 0;
		let mut hi: u32 = max_height; // invariant: block at `hi` is too new
		let mut best: Option<BlockContent> = None;
		while lo < hi {
			let mid = lo + (hi - lo) / 2;
			// A height with no block is the SQL's "no row", not a failure: on a young
			// chain every block can be newer than the window, and `mid` reaches 0.
			// Treat it as too-new so the search narrows upward instead of erroring.
			let Some(block) = self.client.block_by_id(&mid.to_string()).await? else {
				hi = mid;
				continue;
			};
			if block_time_ms(&block) <= max_time_ms {
				best = Some(block);
				lo = mid + 1;
			} else {
				hi = mid;
			}
		}
		Ok(best)
	}

	/// Mirrors `BlockDataSourceImpl::get_latest_block` (the `get_highest_block` SQL):
	/// highest block ≤ `max_height` with time within
	/// `[reference - 3k/f, reference - k/f]`.
	async fn latest_block_in_window(
		&self,
		max_height: u32,
		reference_timestamp_ms: i64,
	) -> Result<Option<BlockContent>, BoxError> {
		let max_time_ms = reference_timestamp_ms - self.min_slot_boundary_ms;
		let min_time_ms = reference_timestamp_ms - self.max_slot_boundary_ms;
		let candidate = self.highest_block_at_or_before_time(max_height, max_time_ms).await?;
		Ok(candidate.filter(|b| block_time_ms(b) >= min_time_ms))
	}

	fn now_ms() -> i64 {
		SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.map(|d| d.as_millis() as i64)
			.unwrap_or_default()
	}
}

#[async_trait::async_trait]
impl McHashDataSource for BlockfrostBlockDataSource {
	async fn get_latest_stable_block_for(
		&self,
		reference_timestamp: Timestamp,
	) -> Result<Option<MainchainBlock>, BoxError> {
		let _t = Timer::new(format!("get_latest_stable_block_for[{reference_timestamp:?}]"));
		let latest = self.latest().await?;
		let offset = self.security_parameter + self.block_stability_margin;
		let stable_height = block_height(&latest)?.saturating_sub(offset);
		let reference_ms = i64::try_from(reference_timestamp.as_millis())?;
		let block = self.latest_block_in_window(stable_height, reference_ms).await?;
		block.map(|b| mainchain_block(&b)).transpose()
	}

	async fn get_stable_block_for(
		&self,
		hash: McBlockHash,
		reference_timestamp: Timestamp,
	) -> Result<StableBlockByHashResult, BoxError> {
		let _t = Timer::new(format!("get_stable_block_for[{hash}]"));
		let reference_ms = i64::try_from(reference_timestamp.as_millis())?;
		// A block classified stable once stays stable; only the timestamp window
		// depends on the reference (mirrors the db-sync stable-block cache).
		if let Ok(mut cache) = self.stable_blocks.lock()
			&& let Some(block) = cache.get(&hash)
		{
			let time_ms = i64::try_from(block.timestamp).unwrap_or(i64::MAX) * 1000;
			if reference_ms - self.max_slot_boundary_ms <= time_ms
				&& time_ms <= reference_ms - self.min_slot_boundary_ms
			{
				return Ok(StableBlockByHashResult::BlockStable { info: block.clone() });
			}
		}
		let Some(block) = self.client.block_by_id(&hex::encode(hash.0)).await? else {
			return Ok(StableBlockByHashResult::BlockNotFound);
		};
		let latest = self.latest().await?;
		let result = classify_stable(
			mainchain_block(&block)?,
			block_height(&latest)?,
			reference_ms,
			self.security_parameter,
			self.min_slot_boundary_ms,
			self.max_slot_boundary_ms,
		);
		if let StableBlockByHashResult::BlockStable { info } = &result
			&& let Ok(mut cache) = self.stable_blocks.lock()
		{
			cache.put(hash, info.clone());
		}
		Ok(result)
	}

	async fn get_block_by_hash(
		&self,
		hash: McBlockHash,
	) -> Result<Option<MainchainBlock>, BoxError> {
		let _t = Timer::new(format!("get_block_by_hash[{hash}]"));
		if let Ok(mut cache) = self.stable_blocks.lock()
			&& let Some(block) = cache.get(&hash)
		{
			return Ok(Some(block.clone()));
		}
		let block = self.client.block_by_id(&hex::encode(hash.0)).await?;
		block.map(|b| mainchain_block(&b)).transpose()
	}

	async fn is_cardano_tip_fresh(&self) -> Result<bool, BoxError> {
		let _t = Timer::new("is_cardano_tip_fresh");
		let latest = mainchain_block(&self.latest().await?)?;
		let now_secs = (Self::now_ms() / 1000) as u64;
		Ok(now_secs.saturating_sub(latest.timestamp) < self.max_latest_block_age_seconds)
	}

	async fn is_cardano_ok(&self) -> Result<bool, BoxError> {
		let _t = Timer::new("is_cardano_ok");
		let now_ms = Self::now_ms();
		let latest = self.latest().await?;
		// Praos chain quality rule check.
		if now_ms - self.min_slot_boundary_ms > block_time_ms(&latest) {
			return Ok(false);
		}
		let stable_height = block_height(&latest)?.saturating_sub(self.security_parameter);
		// Praos chain growth rule.
		Ok(self.latest_block_in_window(stable_height, now_ms).await?.is_some())
	}
}

#[async_trait::async_trait]
impl SidechainRpcDataSource for BlockfrostBlockDataSource {
	async fn get_latest_block_info(&self) -> Result<MainchainBlock, BoxError> {
		let _t = Timer::new("get_latest_block_info");
		mainchain_block(&self.latest().await?)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn block(number: u32, timestamp: u64) -> MainchainBlock {
		MainchainBlock {
			number: McBlockNumber(number),
			hash: McBlockHash([1; 32]),
			epoch: McEpochNumber(1),
			slot: McSlotNumber(u64::from(number)),
			timestamp,
		}
	}

	const K: u32 = 432;

	const MIN_BOUNDARY_MS: i64 = 1_000_000;

	const MAX_BOUNDARY_MS: i64 = 3_000_000;

	fn classify(block_ts_secs: u64, block_no: u32, latest: u32) -> StableBlockByHashResult {
		classify_stable(
			block(block_no, block_ts_secs),
			latest,
			10_000_000,
			K,
			MIN_BOUNDARY_MS,
			MAX_BOUNDARY_MS,
		)
	}

	#[test]
	fn classify_stable_checks_timestamp_before_confirmations() {
		// Block too new for the window AND with too few confirmations: the timestamp
		// error wins because it is final while NotEnoughConfirmations is transient.
		assert!(matches!(
			classify(9_500, 1000, 1000),
			StableBlockByHashResult::BlockTimestampOutRange { .. }
		));
		// In window but too few confirmations.
		assert!(matches!(
			classify(8_000, 1000, 1000),
			StableBlockByHashResult::NotEnoughConfirmations { .. }
		));
		// In window with enough confirmations.
		assert!(matches!(
			classify(8_000, 1000, 1000 + K),
			StableBlockByHashResult::BlockStable { .. }
		));
		// Too old for the window (below reference - 3k/f).
		assert!(matches!(
			classify(6_000, 1000, 1000 + K),
			StableBlockByHashResult::BlockTimestampOutRange { .. }
		));
	}
}
