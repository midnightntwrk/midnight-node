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

//! Federated authority observation data source: reads the council and technical
//! committee governance UTXOs.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use blockfrost::BlockCursor;
use cardano_serialization_lib::PlutusData;
use lru::LruCache;
use midnight_primitives_federated_authority_observation::{
	AuthoritiesData, FederatedAuthorityData, FederatedAuthorityObservationConfig,
};
use midnight_primitives_mainchain_follower::data_source::metrics::{
	MidnightDataSourceMetrics, start_sub_query_timer,
};
use midnight_primitives_mainchain_follower::{
	FederatedAuthorityObservationDataSource, FederatedAuthorityObservationDataSourceImpl,
};
use sidechain_domain::*;

use super::client::*;
use super::convert::*;
use super::support::*;

const FEDAUTH_CACHE_SIZE: usize = 1000;

// ---------------------------------------------------------------------------
// Federated authority observation
// ---------------------------------------------------------------------------

type FedAuthCacheKey = (McBlockHash, String, PolicyId, String, PolicyId);

/// Where the current governance answer for one body came from, so later queries can
/// reuse it after a single "anything new at this address?" range check.
#[derive(Clone)]
struct GovernanceMemo {
	/// Block height of the tx providing the winning UTXO (None = no UTXO existed).
	found_at: Option<u32>,
	/// Highest block up to which the address is known to have no newer transactions.
	checked_up_to: u32,
	result: AuthoritiesData,
}

/// Mirrors `FederatedAuthorityObservationDataSourceImpl`, including its LRU keyed by
/// Cardano block hash *and* the governance config.
pub struct BlockfrostFederatedAuthorityObservationDataSource {
	client: Arc<BlockfrostClient>,
	cache: Mutex<LruCache<FedAuthCacheKey, FederatedAuthorityData>>,
	governance_memo: Mutex<HashMap<(String, PolicyId), GovernanceMemo>>,
	metrics_opt: Option<MidnightDataSourceMetrics>,
}

impl BlockfrostFederatedAuthorityObservationDataSource {
	pub fn new(
		client: Arc<BlockfrostClient>,
		metrics_opt: Option<MidnightDataSourceMetrics>,
	) -> Self {
		let cap = NonZeroUsize::new(FEDAUTH_CACHE_SIZE).unwrap();
		Self {
			client,
			cache: Mutex::new(LruCache::new(cap)),
			governance_memo: Mutex::new(HashMap::new()),
			metrics_opt,
		}
	}

	/// The most recent (block desc, tx index desc) output at the governance body address
	/// holding any asset under `policy_id`, with a datum, created at or before `block`.
	/// Spent-ness is deliberately not checked: spending a governance UTXO is always a
	/// replacement, never a removal (same as the db-sync query).
	///
	/// Incremental caching: "latest matching output ≤ N" can only change if a new tx
	/// appears at the address, so once computed, a single range query over the
	/// yet-unchecked blocks decides whether the memoized answer still stands. This
	/// avoids re-discovering the same governance UTXO for every imported block.
	async fn governance_body_authorities(
		&self,
		address: &str,
		policy_id: &PolicyId,
		block: u32,
	) -> Result<AuthoritiesData, BoxError> {
		let memo_key = (address.to_string(), policy_id.clone());
		let prior =
			self.governance_memo.lock().ok().and_then(|memos| memos.get(&memo_key).cloned());
		if let Some(memo) = &prior {
			// Served directly when the winning tx is at or below the queried block and
			// no newer tx can exist below it either (block ≤ checked_up_to).
			if block <= memo.checked_up_to && memo.found_at.is_none_or(|found| found <= block) {
				return Ok(memo.result.clone());
			}
			if block > memo.checked_up_to {
				let new_txs = self
					.client
					.range_txs(
						TxSource::Address(address),
						Some(BlockCursor::block(u64::from(memo.checked_up_to + 1))),
						Some(BlockCursor::block(u64::from(block))),
					)
					.await?;
				if new_txs.is_empty() {
					// Only extend the entry we actually validated. A concurrent call may
					// have replaced it while `range_txs` was in flight, and stamping this
					// `checked_up_to` onto that one would bless a result never checked to
					// this height — later fast-path hits would return stale authorities.
					if let Ok(mut memos) = self.governance_memo.lock()
						&& memos.get(&memo_key).is_some_and(|current| {
							current.found_at == memo.found_at
								&& current.checked_up_to == memo.checked_up_to
						}) {
						memos.insert(
							memo_key,
							GovernanceMemo {
								found_at: memo.found_at,
								checked_up_to: block,
								result: memo.result.clone(),
							},
						);
					}
					return Ok(memo.result.clone());
				}
			}
		}

		let (result, found_at) = self.scan_governance_body(address, policy_id, block).await?;
		if let Ok(mut memos) = self.governance_memo.lock() {
			memos.insert(
				memo_key,
				GovernanceMemo { found_at, checked_up_to: block, result: result.clone() },
			);
		}
		Ok(result)
	}

	/// Full descending scan for the governance UTXO; returns the authorities and the
	/// block height of the winning tx (None when no matching UTXO exists).
	async fn scan_governance_body(
		&self,
		address: &str,
		policy_id: &PolicyId,
		block: u32,
	) -> Result<(AuthoritiesData, Option<u32>), BoxError> {
		let empty = AuthoritiesData { authorities: vec![], round: 0 };
		let policy_hex = hex::encode(policy_id.0);
		let mut found: Option<(Option<PlutusData>, u32)> = None;
		let mut reached_end = false;
		'pages: for page in 1..=MAX_PAGES {
			let batch =
				self.client.range_txs_desc_page(TxSource::Address(address), block, page).await?;
			let last_page = batch.len() < PAGE_SIZE;
			for row in &batch {
				let utxos = self.client.tx_utxos(&row.tx_hash).await?;
				let mut outputs: Vec<_> = utxos
					.outputs
					.iter()
					.filter(|o| {
						!o.collateral
							&& o.address == address
							&& (o.inline_datum.is_some() || o.data_hash.is_some())
							&& o.amount.iter().any(|a| a.unit.starts_with(&policy_hex))
					})
					.collect();
				outputs.sort_by_key(|o| o.output_index);
				for output in outputs {
					// An output whose datum can't be resolved (datum hash whose body was
					// never revealed) is skipped, not treated as an empty answer: the SQL
					// `INNER JOIN datum` excludes such rows and keeps scanning older ones.
					if let Some(datum) = self
						.client
						.datum(output.inline_datum.as_ref(), output.data_hash.as_ref())
						.await?
					{
						found = Some((Some(datum), row.block_height));
						break 'pages;
					}
					log::debug!(
						"Governance output {}#{} at {address} has an unresolvable datum; skipping",
						row.tx_hash,
						output.output_index,
					);
				}
			}
			if last_page {
				reached_end = true;
				break;
			}
		}
		// Scanning the whole history without a hit is a legitimate answer (empty list
		// below); exhausting the page cap is not, so it must not be reported as one.
		if found.is_none() && !reached_end {
			return Err(too_many_pages(&format!("address {address}/transactions")));
		}
		let Some((Some(datum), found_at)) = found else {
			log::warn!(
				"No governance body UTXO found for Cardano block {block} (address: {address}, policy_id: {policy_id}). Using empty list.",
			);
			return Ok((empty, found.map(|(_, height)| height)));
		};
		let result =
			match FederatedAuthorityObservationDataSourceImpl::decode_governance_datum(&datum) {
				Ok(datums) => AuthoritiesData::from(datums),
				Err(e) => {
					log::warn!("Failed to decode governance datum: {e}. Using empty list.");
					empty
				},
			};
		Ok((result, Some(found_at)))
	}
}

#[async_trait::async_trait]
impl FederatedAuthorityObservationDataSource for BlockfrostFederatedAuthorityObservationDataSource {
	async fn get_federated_authority_data(
		&self,
		config: &FederatedAuthorityObservationConfig,
		mc_block_hash: &McBlockHash,
	) -> Result<FederatedAuthorityData, BoxError> {
		let _t = Timer::new(format!("get_federated_authority_data[{mc_block_hash}]"));
		let key: FedAuthCacheKey = (
			mc_block_hash.clone(),
			config.council.address.clone(),
			config.council.policy_id.clone(),
			config.technical_committee.address.clone(),
			config.technical_committee.policy_id.clone(),
		);
		if let Ok(mut cache) = self.cache.lock()
			&& let Some(cached) = cache.get(&key)
		{
			return Ok(cached.clone());
		}

		let block = {
			let _sq_timer = start_sub_query_timer(&self.metrics_opt, "fedauth_get_block_by_hash");
			self.client
				.block_by_id(&hex::encode(mc_block_hash.0))
				.await?
				.ok_or_else(|| format!("Block not found for hash: {mc_block_hash:?}"))?
		};
		let block_number = block_height(&block)?;

		let (council_authorities, technical_committee_authorities) = tokio::try_join!(
			async {
				let _sq_timer =
					start_sub_query_timer(&self.metrics_opt, "fedauth_get_council_utxo");
				self.governance_body_authorities(
					&config.council.address,
					&config.council.policy_id,
					block_number,
				)
				.await
			},
			async {
				let _sq_timer = start_sub_query_timer(
					&self.metrics_opt,
					"fedauth_get_technical_committee_utxo",
				);
				self.governance_body_authorities(
					&config.technical_committee.address,
					&config.technical_committee.policy_id,
					block_number,
				)
				.await
			},
		)?;

		let result = FederatedAuthorityData {
			council_authorities,
			technical_committee_authorities,
			mc_block_hash: mc_block_hash.clone(),
		};
		if let Ok(mut cache) = self.cache.lock() {
			cache.put(key, result.clone());
		}
		Ok(result)
	}
}
