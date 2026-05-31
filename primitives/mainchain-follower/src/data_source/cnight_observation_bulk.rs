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

//! Bulk-read cNIGHT observation data source.
//!
//! At startup, run the four observation queries against db-sync across
//! `[0, current_cardano_tip]` and hold the result in memory. Bulk observation
//! queries thereafter come from the in-memory cache; a sliding-window refresh
//! extends the cache as the chain advances. Queries past the current horizon
//! delegate to a live db-backed source so the node keeps importing.
//!
//! Trade vs. an on-disk snapshot file: pay ~2 min of postgres work per node
//! start instead of carrying multi-MB binaries in the repo.

use crate::data_source::candidates_data_source::observed_async_trait;
use crate::data_source::cnight_observation::MidnightCNightObservationDataSourceImpl;
use crate::data_source::metrics::MidnightDataSourceMetrics;
use crate::db::MultiAssetCache;
use crate::{MidnightCNightObservationDataSource, ObservedUtxo};
use cardano_serialization_lib::{Address, EnterpriseAddress};
use midnight_primitives_cnight_observation::{CNightAddresses, CardanoPosition, ObservedUtxos};
use sidechain_domain::McBlockHash;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Effectively-no-limit page size for bulk pulls. The query path supports
/// `LIMIT` for paged use but the sliding window wants the whole range in
/// one shot.
const LARGE_LIMIT: usize = 5_000_000;

/// Default number of cardano blocks to keep in the sliding window when the
/// node config doesn't override it. Memory cost ≈ 5 KB × events-per-block,
/// so 100k blocks ≈ a few hundred MB on a busy chain.
pub const DEFAULT_WINDOW_SIZE: u32 = 100_000;

/// If the next-needed cardano position (`start_position`) is within this many
/// blocks of the cache's `end`, kick an async refresh that slides the window
/// forward.
const REFRESH_THRESHOLD: u32 = 10_000;

/// Pull every cnight observation event in `[start, end]` (inclusive) and
/// return them sorted ascending by `tx_position`.
///
/// Both endpoints are full `CardanoPosition`s so the per-call data source can
/// pass exact `(block_number, tx_index_in_block)` boundaries while the bulk
/// /sliding-window paths can pass whole-block ranges via `whole_block_range`.
pub async fn bulk_pull(
	pool: &PgPool,
	cfg: &CNightAddresses,
	start: &CardanoPosition,
	end: &CardanoPosition,
	// Per-query SQL row limit (over-fetch bound). The consensus inherent path
	// passes the runtime-supplied `utxo_overestimate`; the background cache
	// refresh passes `LARGE_LIMIT` to pull a whole multi-block window.
	limit: usize,
) -> Result<Vec<ObservedUtxo>, Box<dyn std::error::Error + Send + Sync>> {
	let ds = MidnightCNightObservationDataSourceImpl::new(pool.clone(), None, 0);

	let mapping_validator_address = Address::from_bech32(&cfg.mapping_validator_address)
		.map_err(|e| format!("invalid mapping validator address: {e}"))?;
	let cardano_network =
		mapping_validator_address.network_id().map_err(|e| format!("network_id: {e}"))?;
	let mapping_validator_policy_id = EnterpriseAddress::from_address(&mapping_validator_address)
		.ok_or("mapping validator address is not EnterpriseAddress")?
		.payment_cred()
		.to_scripthash()
		.ok_or("mapping validator address has no script hash")?;

	let asset_cache = MultiAssetCache::new(pool.clone());
	let auth_token_ident = asset_cache
		.resolve_ident(
			&mapping_validator_policy_id.to_bytes(),
			cfg.auth_token_asset_name.as_bytes(),
		)
		.await?;
	let cnight_ident = asset_cache
		.resolve_ident(&cfg.cnight_policy_id, cfg.cnight_asset_name.as_bytes())
		.await?;

	let (low_bounds, high_bounds) = tokio::try_join!(
		crate::db::get_low_bounds(pool, start.block_number.into()),
		crate::db::get_high_bounds(pool, end.block_number.into()),
	)?;
	let low_bounds = low_bounds
		.ok_or_else(|| format!("get_low_bounds({}) returned None", start.block_number))?;
	let high_bounds = high_bounds
		.ok_or_else(|| format!("get_high_bounds({}) returned None", end.block_number))?;

	let paged = crate::db::PagedQuery {
		start,
		end,
		limit,
		offset: 0,
		low_bound: low_bounds,
		high_bound: high_bounds,
	};

	let mut all = Vec::new();
	let mut counts = (0usize, 0usize, 0usize, 0usize);
	if let Some(ident) = auth_token_ident {
		let v = ds
			.get_registration_utxos(cardano_network, ident, &cfg.mapping_validator_address, &paged)
			.await?;
		counts.0 = v.len();
		all.extend(v);
	}
	let v = ds
		.get_deregistration_utxos(cardano_network, &cfg.mapping_validator_address, &paged)
		.await?;
	counts.1 = v.len();
	all.extend(v);
	if let Some(ident) = cnight_ident {
		let v = ds.get_asset_create_utxos(cardano_network, ident, &paged).await?;
		counts.2 = v.len();
		all.extend(v);
		let v = ds.get_asset_spend_utxos(cardano_network, ident, &paged).await?;
		counts.3 = v.len();
		all.extend(v);
	}
	all.sort();
	log::info!(
		target: "cnight::sliding-window",
		"bulk_pull [{}/{}, {}/{}] -> reg={} dereg={} create={} spend={} (auth_ident={:?} cnight_ident={:?})",
		start.block_number, start.tx_index_in_block,
		end.block_number, end.tx_index_in_block,
		counts.0, counts.1, counts.2, counts.3, auth_token_ident, cnight_ident,
	);
	Ok(all)
}

/// Build a `[from_block, to_block]` whole-block `CardanoPosition` range,
/// suitable for `bulk_pull`. The `to` endpoint uses `i32::MAX as u32` so the
/// SQL bind path's `as i32` cast doesn't underflow to `-1`.
pub fn whole_block_range(from_block: u32, to_block: u32) -> (CardanoPosition, CardanoPosition) {
	let max_tx_index = u32::try_from(i32::MAX).expect("i32::MAX is non-negative");
	let start = CardanoPosition {
		block_hash: McBlockHash([0u8; 32]),
		block_number: from_block,
		block_timestamp: Default::default(),
		tx_index_in_block: 0,
	};
	let end = CardanoPosition {
		block_hash: McBlockHash([0u8; 32]),
		block_number: to_block,
		block_timestamp: Default::default(),
		tx_index_in_block: max_tx_index,
	};
	(start, end)
}

/// Truncate a sorted, unique-position event list to at most `tx_capacity`
/// whole transactions. Returns the truncated `ObservedUtxos` plus a flag
/// indicating whether the full input fit (`true`: all events accepted up to
/// `fallback_end`; `false`: capacity hit and `result.end` is the position
/// just past the last accepted event).
pub fn truncate_to_tx_capacity(
	events: Vec<ObservedUtxo>,
	tx_capacity: usize,
	start_position: &CardanoPosition,
	fallback_end: CardanoPosition,
) -> (ObservedUtxos, bool) {
	let mut truncated: Vec<ObservedUtxo> = Vec::with_capacity(events.len().min(tx_capacity * 64));
	let mut num_txs: usize = 0;
	let mut cur_tx: Option<CardanoPosition> = None;
	for utxo in events {
		if cur_tx.as_ref().is_none_or(|tx| tx < &utxo.header.tx_position) {
			num_txs += 1;
			cur_tx = Some(utxo.header.tx_position.clone());
		}
		if num_txs == tx_capacity {
			break;
		}
		truncated.push(utxo);
	}
	let full_window = num_txs < tx_capacity;
	let end = if full_window {
		fallback_end
	} else {
		truncated
			.last()
			.map(|u| u.header.tx_position.clone())
			.unwrap_or_else(|| start_position.clone())
			.increment()
	};
	(ObservedUtxos { start: start_position.clone(), end, utxos: truncated }, full_window)
}

/// Cached result of the previous `get_utxos_up_to_capacity` call. During
/// initial sync many consecutive Midnight blocks share the same Cardano tip,
/// so recomputing the window each time is wasted work.
#[derive(Clone)]
struct LastObservation {
	start_position: CardanoPosition,
	current_tip: McBlockHash,
	result: ObservedUtxos,
	/// True iff the previous call returned all data up to the requested `end`
	/// (i.e. not truncated by `tx_capacity`). Only full-window results can be
	/// sliced when `start_position` advances.
	full_window: bool,
}

/// A `MidnightCNightObservationDataSource` backed by an in-memory event vector
/// built once at startup, with an async sliding-window refresh and a live
/// db-backed fallback for queries past the current horizon.
pub struct BulkCachedCNightObservationDataSource {
	/// Sorted events. `RwLock<Arc<...>>` lets the refresh task swap in an
	/// extended vec without blocking concurrent readers.
	all_events: Arc<std::sync::RwLock<Arc<Vec<ObservedUtxo>>>>,
	/// Used exclusively for `get_block_by_hash` — a single indexed lookup
	/// per call when the block is not yet in `block_position_cache`.
	pool: PgPool,
	/// Memoizes `current_tip` (cardano block hash) → `CardanoPosition`. Many
	/// consecutive midnight blocks share the same Cardano tip during sync,
	/// so without this every call would do a postgres round-trip.
	block_position_cache: Arc<Mutex<HashMap<McBlockHash, CardanoPosition>>>,
	last_observation: Arc<Mutex<Option<LastObservation>>>,
	/// Smallest cardano block number for which we have events. Anything
	/// older has been trimmed by a previous refresh.
	snapshot_start_block: Arc<std::sync::RwLock<Option<u32>>>,
	/// Largest cardano block number for which we have events. Queries whose
	/// `start_position` goes past this delegate to `db_fallback` AND trigger
	/// an async refresh.
	snapshot_end_block: Arc<std::sync::RwLock<Option<u32>>>,
	db_fallback: Arc<MidnightCNightObservationDataSourceImpl>,
	/// cNIGHT addresses cached so the sliding-window refresh can re-run the
	/// observation queries without re-reading the chainspec JSON.
	cnight_addresses: CNightAddresses,
	/// Cardano blocks to leave un-fetched past the requested target
	/// (re-org safety). Equals `cardano_security_parameter + block_stability_margin`.
	stability_margin: u32,
	/// Cardano blocks to keep in the sliding window.
	window_size: u32,
	/// Single-flight gate for sliding-window refreshes.
	refresh_in_flight: Arc<Mutex<bool>>,
	#[allow(dead_code)]
	metrics_opt: Option<MidnightDataSourceMetrics>,
}

impl BulkCachedCNightObservationDataSource {
	pub fn new(
		events: Vec<ObservedUtxo>,
		window_start_block: u32,
		window_end_block: u32,
		window_size: u32,
		pool: PgPool,
		db_fallback: Arc<MidnightCNightObservationDataSourceImpl>,
		cnight_addresses: CNightAddresses,
		stability_margin: u32,
		metrics_opt: Option<MidnightDataSourceMetrics>,
	) -> Self {
		// Initial window covers `[window_start_block, window_end_block]`.
		// Caller is responsible for bulk-pulling that range; we just record
		// the bookkeeping.
		Self {
			all_events: Arc::new(std::sync::RwLock::new(Arc::new(events))),
			pool,
			block_position_cache: Arc::new(Mutex::new(HashMap::new())),
			last_observation: Arc::new(Mutex::new(None)),
			snapshot_start_block: Arc::new(std::sync::RwLock::new(Some(window_start_block))),
			snapshot_end_block: Arc::new(std::sync::RwLock::new(Some(window_end_block))),
			db_fallback,
			cnight_addresses,
			stability_margin,
			window_size,
			refresh_in_flight: Arc::new(Mutex::new(false)),
			metrics_opt,
		}
	}

	/// Trigger an async sliding-window refresh if not already in flight.
	/// Returns immediately. Single-flight: concurrent triggers are no-ops.
	fn maybe_kick_refresh(&self, target_end: u32) {
		{
			let mut g = match self.refresh_in_flight.lock() {
				Ok(g) => g,
				Err(_) => return,
			};
			if *g {
				return;
			}
			*g = true;
		}

		let pool = self.pool.clone();
		let cfg = self.cnight_addresses.clone();
		let all_events = Arc::clone(&self.all_events);
		let last_observation = Arc::clone(&self.last_observation);
		let snapshot_start_block = Arc::clone(&self.snapshot_start_block);
		let snapshot_end_block = Arc::clone(&self.snapshot_end_block);
		let in_flight = Arc::clone(&self.refresh_in_flight);
		let window_size = self.window_size;

		tokio::spawn(async move {
			if let Err(e) = refresh_window(
				&pool,
				&cfg,
				&all_events,
				&last_observation,
				&snapshot_start_block,
				&snapshot_end_block,
				target_end,
				window_size,
			)
			.await
			{
				log::warn!(
					target: "cnight::sliding-window",
					"refresh failed (ignored, db_fallback continues to serve): {e}"
				);
			}
			if let Ok(mut g) = in_flight.lock() {
				*g = false;
			}
		});
	}
}

/// Extend the cache forward to `target_end`, pulling events in `(old_end,
/// target_end]`. Trim happens *behind the follower*, not behind the tip —
/// dropping events older than `last_observation.start_position - window_size`
/// is safe (the follower will never re-read that far back), but trimming
/// behind `target_end - window_size` would drop events the runtime still
/// needs during catchup, breaking consensus. New events sort strictly after
/// every retained event, so no global re-sort is needed.
async fn refresh_window(
	pool: &PgPool,
	cfg: &CNightAddresses,
	all_events: &Arc<std::sync::RwLock<Arc<Vec<ObservedUtxo>>>>,
	last_observation: &Arc<Mutex<Option<LastObservation>>>,
	snapshot_start_block: &Arc<std::sync::RwLock<Option<u32>>>,
	snapshot_end_block: &Arc<std::sync::RwLock<Option<u32>>>,
	target_end: u32,
	window_size: u32,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let old_end = snapshot_end_block
		.read()
		.map_err(|e| format!("snapshot_end_block read poisoned: {e}"))?
		.unwrap_or(0);
	if target_end <= old_end {
		return Ok(());
	}
	let from_block = old_end.saturating_add(1);
	// Anchor the trim point on the follower's last-seen position. During
	// catchup the follower can be hundreds of thousands of blocks behind tip
	// and still needs that history, so trimming behind `target_end - W`
	// would silently drop required events.
	//
	// On the first refresh we may not have seen a follower call yet
	// (everything has gone to db_fallback while waiting for this very
	// fetch), so fall back to the existing `snapshot_start` — never move it
	// backward, otherwise we'd lie about coverage.
	let existing_start = snapshot_start_block
		.read()
		.map_err(|e| format!("snapshot_start_block read poisoned: {e}"))?
		.unwrap_or(from_block);
	let new_window_start = match last_observation
		.lock()
		.ok()
		.and_then(|g| g.as_ref().map(|last| last.start_position.block_number))
	{
		Some(anchor) => existing_start.max(anchor.saturating_sub(window_size)),
		None => existing_start,
	};
	log::info!(
		target: "cnight::sliding-window",
		"refresh kicked off: extending to {target_end} (was end={old_end}); trim behind {new_window_start}"
	);
	let t0 = std::time::Instant::now();
	let (start, end) = whole_block_range(from_block, target_end);
	// Warming the cache means pulling a whole multi-block window, so the per-query
	// limit is the wide `LARGE_LIMIT` rather than the per-block over-fetch bound.
	let extension = bulk_pull(pool, cfg, &start, &end, LARGE_LIMIT).await?;
	{
		let mut events_guard =
			all_events.write().map_err(|e| format!("all_events write poisoned: {e}"))?;
		let new_vec = slide_events(&events_guard, extension, new_window_start);
		*events_guard = Arc::new(new_vec);
	}
	*snapshot_start_block
		.write()
		.map_err(|e| format!("snapshot_start_block write poisoned: {e}"))? = Some(new_window_start);
	*snapshot_end_block
		.write()
		.map_err(|e| format!("snapshot_end_block write poisoned: {e}"))? = Some(target_end);
	log::info!(
		target: "cnight::sliding-window",
		"refresh done: window now [{new_window_start}, {target_end}] (took {:?})",
		t0.elapsed()
	);
	Ok(())
}

/// Build a fresh sorted vec covering `[new_window_start, ...]`: take retained
/// events from `existing` (those at or after `new_window_start`), then append
/// `extension` (events strictly after the existing end). Pure helper for unit
/// testing.
fn slide_events(
	existing: &[ObservedUtxo],
	extension: Vec<ObservedUtxo>,
	new_window_start: u32,
) -> Vec<ObservedUtxo> {
	// `existing` is sorted ascending by tx_position.block_number, so a
	// partition_point gives the first retained index in O(log n).
	let trim_at =
		existing.partition_point(|u| u.header.tx_position.block_number < new_window_start);
	let mut out: Vec<ObservedUtxo> = Vec::with_capacity(existing.len() - trim_at + extension.len());
	out.extend_from_slice(&existing[trim_at..]);
	out.extend(extension);
	out
}

/// From a sorted vec, return the slice `[a..b)` covering events whose
/// `tx_position` falls in `[start, end)`.
fn slice_range<'a>(
	vec: &'a [ObservedUtxo],
	start: &CardanoPosition,
	end: &CardanoPosition,
) -> &'a [ObservedUtxo] {
	let a = vec.partition_point(|u| u.header.tx_position < *start);
	let b = vec.partition_point(|u| u.header.tx_position < *end);
	&vec[a..b]
}

observed_async_trait!(
impl MidnightCNightObservationDataSource for BulkCachedCNightObservationDataSource {
	async fn get_utxos_up_to_capacity(
		&self,
		config: &CNightAddresses,
		start_position: &CardanoPosition,
		current_tip: McBlockHash,
		tx_capacity: usize,
		utxo_overestimate: usize,
	) -> Result<ObservedUtxos, Box<dyn std::error::Error + Send + Sync>> {
		// Same-tip cache: if `current_tip` hasn't advanced, the Cardano window
		// hasn't grown. Reuse the previous result directly (exact match) or
		// filter it to `>= start_position` (advanced within a full window —
		// only safe when the prior call wasn't truncated).
		if let Ok(guard) = self.last_observation.lock()
			&& let Some(last) = guard.as_ref()
			&& last.current_tip == current_tip
		{
			if last.start_position == *start_position {
				return Ok(last.result.clone());
			} else if last.full_window
				&& *start_position >= last.start_position
				&& *start_position <= last.result.end
			{
				let filtered: Vec<_> = last
					.result
					.utxos
					.iter()
					.filter(|u| u.header.tx_position >= *start_position)
					.cloned()
					.collect();
				return Ok(ObservedUtxos {
					start: start_position.clone(),
					end: last.result.end.clone(),
					utxos: filtered,
				});
			}
		}

		// Resolve `current_tip` (cardano block hash) → CardanoPosition.
		let cached = self
			.block_position_cache
			.lock()
			.ok()
			.and_then(|g| g.get(&current_tip).cloned());
		let tip_pos: CardanoPosition = match cached {
			Some(pos) => pos,
			None => {
				let block = crate::db::get_block_by_hash(&self.pool, current_tip.clone())
					.await?
					.ok_or_else(|| format!("missing block for tip {:?}", current_tip))?;
				let pos: CardanoPosition = block.into();
				if let Ok(mut guard) = self.block_position_cache.lock() {
					guard.insert(current_tip.clone(), pos.clone());
				}
				pos
			},
		};

		// CORRECTNESS: the runtime expects every event in
		// `[start_position, tip_pos]`. The cache only covers
		// `[snapshot_start, snapshot_end]`. If either endpoint of the query
		// falls outside, we'd return a strict subset of the block author's
		// observations and `CheckInherents` would reject the block. So we
		// serve from cache only when `[start_position, tip_pos] ⊂ [snapshot_start,
		// snapshot_end]`; otherwise delegate to db_fallback (which always has
		// the complete picture).
		//
		// Note `tip_pos` is the cardano tip from the *importing block's*
		// mc-hash digest — not real-time. So during catchup it advances with
		// the midnight chain, making a sliding window viable: the cache only
		// needs to track `tip_pos`, not the live cardano tip.
		let horizon_opt = self.snapshot_end_block.read().ok().and_then(|g| *g);
		let snapshot_start_opt = self.snapshot_start_block.read().ok().and_then(|g| *g);
		if let Some(horizon) = horizon_opt {
			// Refresh proactively when tip_pos is closing on horizon.
			if tip_pos.block_number.saturating_add(REFRESH_THRESHOLD) >= horizon {
				let target_end = tip_pos
					.block_number
					.saturating_add(REFRESH_THRESHOLD)
					.saturating_add(self.stability_margin);
				self.maybe_kick_refresh(target_end);
			}
			let tip_past_horizon = tip_pos.block_number > horizon;
			let start_below_snapshot_start = snapshot_start_opt
				.is_some_and(|ss| start_position.block_number < ss);
			if tip_past_horizon || start_below_snapshot_start {
				log::debug!(
					"cNIGHT observation: query [{} .. {}] outside cache window [{:?} .. {}], delegating to DB",
					start_position.block_number, tip_pos.block_number, snapshot_start_opt, horizon,
				);
				return self
					.db_fallback
					.get_utxos_up_to_capacity(
						config,
						start_position,
						current_tip,
						tx_capacity,
						utxo_overestimate,
					)
					.await;
			}
		} else {
			// No horizon yet — cache hasn't been populated. Delegate while
			// we wait for the first refresh to complete.
			return self
				.db_fallback
				.get_utxos_up_to_capacity(
					config,
					start_position,
					current_tip,
					tx_capacity,
					utxo_overestimate,
				)
				.await;
		}

		let end = tip_pos.increment();
		// Snapshot the current Arc<Vec> so a concurrent refresh that swaps in
		// a new Arc doesn't disturb our local clone.
		let events_snapshot: Arc<Vec<ObservedUtxo>> = self
			.all_events
			.read()
			.map(|g| Arc::clone(&g))
			.unwrap_or_else(|_| Arc::new(Vec::new()));
		let window: Vec<ObservedUtxo> =
			slice_range(&events_snapshot, start_position, &end).to_vec();
		let (result, full_window) =
			truncate_to_tx_capacity(window, tx_capacity, start_position, end);

		if let Ok(mut guard) = self.last_observation.lock() {
			*guard = Some(LastObservation {
				start_position: start_position.clone(),
				current_tip: current_tip.clone(),
				result: result.clone(),
				full_window,
			});
		}

		Ok(result)
	}
}
);

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{ObservedUtxoData, ObservedUtxoHeader, RegistrationData, UtxoIndexInTx};
	use midnight_primitives_cnight_observation::CardanoRewardAddressBytes;
	use sidechain_domain::{McBlockHash, McTxHash};

	/// Minimal `ObservedUtxo` at `(block_number, tx_index_in_block)`. Just
	/// enough to drive tx_position-based comparisons.
	fn utxo(block_number: u32, tx_index: u32) -> ObservedUtxo {
		ObservedUtxo {
			header: ObservedUtxoHeader {
				tx_position: CardanoPosition {
					block_hash: McBlockHash([0u8; 32]),
					block_number,
					block_timestamp: Default::default(),
					tx_index_in_block: tx_index,
				},
				tx_hash: McTxHash([0u8; 32]),
				utxo_tx_hash: McTxHash([0u8; 32]),
				utxo_index: UtxoIndexInTx(0),
			},
			data: ObservedUtxoData::Registration(RegistrationData {
				cardano_reward_address: CardanoRewardAddressBytes([0u8; 29]),
				dust_public_key: vec![0u8; 33].try_into().unwrap(),
			}),
		}
	}

	fn pos(block_number: u32, tx_index: u32) -> CardanoPosition {
		CardanoPosition {
			block_hash: McBlockHash([0u8; 32]),
			block_number,
			block_timestamp: Default::default(),
			tx_index_in_block: tx_index,
		}
	}

	#[test]
	fn slice_range_returns_half_open_subrange() {
		let events: Vec<_> = (0..10).map(|n| utxo(n, 0)).collect();
		let got = slice_range(&events, &pos(2, 0), &pos(7, 0));
		let block_numbers: Vec<u32> =
			got.iter().map(|u| u.header.tx_position.block_number).collect();
		// Half-open: block 7 excluded.
		assert_eq!(block_numbers, vec![2, 3, 4, 5, 6]);
	}

	#[test]
	fn slice_range_empty_when_start_eq_end() {
		let events: Vec<_> = (0..10).map(|n| utxo(n, 0)).collect();
		assert!(slice_range(&events, &pos(5, 0), &pos(5, 0)).is_empty());
	}

	#[test]
	fn slice_range_empty_when_above_data() {
		let events: Vec<_> = (0..10).map(|n| utxo(n, 0)).collect();
		assert!(slice_range(&events, &pos(20, 0), &pos(30, 0)).is_empty());
	}

	#[test]
	fn slide_events_trims_front_and_appends_back() {
		// Existing window covers blocks [10..30); slide to new_start=15
		// while appending blocks [30..35).
		let existing: Vec<_> = (10..30).map(|n| utxo(n, 0)).collect();
		let extension: Vec<_> = (30..35).map(|n| utxo(n, 0)).collect();
		let result = slide_events(&existing, extension, 15);
		let block_numbers: Vec<u32> =
			result.iter().map(|u| u.header.tx_position.block_number).collect();
		assert_eq!(block_numbers, (15..35).collect::<Vec<_>>());
	}

	#[test]
	fn slide_events_no_trim_when_start_below_existing() {
		let existing: Vec<_> = (10..15).map(|n| utxo(n, 0)).collect();
		let extension: Vec<_> = (15..18).map(|n| utxo(n, 0)).collect();
		let result = slide_events(&existing, extension, 5);
		assert_eq!(result.len(), 8);
		assert_eq!(result[0].header.tx_position.block_number, 10);
	}

	#[test]
	fn slide_events_full_trim_when_start_above_existing() {
		let existing: Vec<_> = (10..15).map(|n| utxo(n, 0)).collect();
		let extension: Vec<_> = (20..25).map(|n| utxo(n, 0)).collect();
		let result = slide_events(&existing, extension, 100);
		// Everything from `existing` is dropped; only extension survives.
		let block_numbers: Vec<u32> =
			result.iter().map(|u| u.header.tx_position.block_number).collect();
		assert_eq!(block_numbers, vec![20, 21, 22, 23, 24]);
	}

	#[test]
	fn slide_events_empty_extension_just_trims() {
		let existing: Vec<_> = (10..20).map(|n| utxo(n, 0)).collect();
		let result = slide_events(&existing, vec![], 14);
		let block_numbers: Vec<u32> =
			result.iter().map(|u| u.header.tx_position.block_number).collect();
		assert_eq!(block_numbers, (14..20).collect::<Vec<_>>());
	}
}
