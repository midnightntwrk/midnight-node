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

//! Authority selection data source: ariadne parameters, candidate registrations and
//! epoch nonce, mirroring the db-sync candidates queries.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use authority_selection_inherents::{AriadneParameters, AuthoritySelectionDataSource};
use blockfrost::{
	BlockCursor, Order, Pagination,
	blockfrost_openapi::models::{block_content::BlockContent, tx_content_utxo::TxContentUtxo},
};
use cardano_serialization_lib::{Ed25519KeyHash, PlutusData};
use lru::LruCache;
use midnight_primitives_mainchain_follower::data_source::metrics::{
	MidnightDataSourceMetrics, start_sub_query_timer,
};
use partner_chains_plutus_data::{
	permissioned_candidates::PermissionedCandidateDatums,
	registered_candidates::RegisterValidatorDatum,
};
use sidechain_domain::*;

use super::client::*;
use super::convert::*;
use super::support::*;

/// Same value as `CANDIDATES_FOR_EPOCH_CACHE_SIZE` used for the db-sync candidates cache.
const AUTHORITY_CACHE_SIZE: usize = 64;

// ---------------------------------------------------------------------------
// Authority selection (candidates)
// ---------------------------------------------------------------------------

struct AuthorityCaches {
	ariadne: LruCache<(u32, PolicyId), AriadneParameters>,
	candidates: LruCache<(u32, String), Vec<CandidateRegistrations>>,
	nonce: LruCache<u32, EpochNonce>,
	highest_seen_stable_epoch: Option<u32>,
}

/// Mirrors midnight's `CandidatesDataSourceImpl` (+ its `CandidateDataSourceCached`
/// stability-gated LRU caching).
pub struct BlockfrostAuthoritySelectionDataSource {
	client: Arc<BlockfrostClient>,
	security_parameter: u32,
	caches: Mutex<AuthorityCaches>,
	/// Same handle and same sub-query labels as the db-sync source, so both backends
	/// report into one histogram and can be compared directly.
	metrics_opt: Option<MidnightDataSourceMetrics>,
}

impl BlockfrostAuthoritySelectionDataSource {
	pub fn new(
		client: Arc<BlockfrostClient>,
		security_parameter: u32,
		metrics_opt: Option<MidnightDataSourceMetrics>,
	) -> Self {
		let cap = NonZeroUsize::new(AUTHORITY_CACHE_SIZE).unwrap();
		Self {
			client,
			security_parameter,
			metrics_opt,
			caches: Mutex::new(AuthorityCaches {
				ariadne: LruCache::new(cap),
				candidates: LruCache::new(cap),
				nonce: LruCache::new(cap),
				highest_seen_stable_epoch: None,
			}),
		}
	}

	fn data_epoch_of(&self, epoch: McEpochNumber) -> Result<u32, BoxError> {
		let data_epoch = offset_data_epoch(&epoch).map_err(|offset| {
			format!(
				"Minimum supported epoch of data usage is {offset}, but {} was provided",
				epoch.0
			)
		})?;
		Ok(data_epoch.0)
	}

	/// Results for a stable data epoch are immutable and cacheable. Mirrors
	/// `get_latest_stable_epoch`: the epoch of the highest stable block, minus one.
	async fn can_cache(&self, data_epoch: u32) -> bool {
		if let Ok(caches) = self.caches.lock()
			&& let Some(highest) = caches.highest_seen_stable_epoch
			&& data_epoch <= highest
		{
			return true;
		}
		let stable_epoch = async {
			let latest =
				deadline("blocks/latest", self.client.api.blocks_latest()).await.ok()?.ok()?;
			let stable_height = block_height(&latest).ok()?.saturating_sub(self.security_parameter);
			let stable = self.client.block_by_id(&stable_height.to_string()).await.ok()??;
			u32::try_from(stable.epoch?).ok()?.checked_sub(1)
		}
		.await;
		match stable_epoch {
			Some(stable_epoch) => {
				if let Ok(mut caches) = self.caches.lock() {
					caches.highest_seen_stable_epoch = Some(stable_epoch);
				}
				data_epoch <= stable_epoch
			},
			None => false,
		}
	}

	/// Last block of `epoch` (or of the closest earlier non-empty epoch), mirroring
	/// the `epoch_no <= $1` semantics of `get_latest_block_for_epoch`.
	async fn last_block_of_epoch(&self, epoch: u32) -> Result<Option<BlockContent>, BoxError> {
		let _sq_timer =
			start_sub_query_timer(&self.metrics_opt, "candidates_get_latest_block_for_epoch");
		let mut n = i64::from(epoch);
		while n >= 0 {
			let _t = Timer::new(format!("GET epochs/{n}/blocks"));
			let hashes = match deadline(
				&format!("epochs/{n}/blocks"),
				self.client.api.epochs_blocks(n as i32, Pagination::new(Order::Desc, 1, 1)),
			)
			.await?
			{
				Ok(hashes) => hashes,
				Err(e) if is_404(&e) => vec![],
				Err(e) => return Err(box_err(e)),
			};
			if let Some(hash) = hashes.first() {
				return self.client.block_by_id(hash).await;
			}
			n -= 1;
		}
		Ok(None)
	}

	/// The most recent output carrying an asset of `unit` created at or before
	/// `boundary_block`, together with its creating tx hash. Mirrors
	/// `get_token_utxo_for_epoch`: ordered by (block, tx index) descending, spent-ness
	/// deliberately not checked.
	async fn latest_token_output_upto(
		&self,
		unit: &str,
		boundary_block: u32,
	) -> Result<Option<(RangeTx, TxContentUtxo)>, BoxError> {
		let _sq_timer =
			start_sub_query_timer(&self.metrics_opt, "candidates_get_token_utxo_for_epoch");
		for page in 1..=MAX_PAGES {
			let batch = self
				.client
				.range_txs_desc_page(TxSource::Asset(unit), boundary_block, page)
				.await?;
			let last_page = batch.len() < PAGE_SIZE;
			for row in batch {
				let utxos = self.client.tx_utxos(&row.tx_hash).await?;
				if utxos.outputs.iter().any(|o| !o.collateral && has_unit(&o.amount, unit)) {
					return Ok(Some((row, utxos)));
				}
			}
			if last_page {
				return Ok(None);
			}
		}
		Err(too_many_pages(&format!("asset {unit}/transactions")))
	}

	async fn get_ariadne_parameters_uncached(
		&self,
		data_epoch: u32,
		permissioned_candidate_policy: &PolicyId,
	) -> Result<AriadneParameters, BoxError> {
		// The permissioned-candidates token has an empty asset name: unit = policy id hex.
		let unit = hex::encode(permissioned_candidate_policy.0);

		// DParameter is now read from pallet_system_parameters storage, not from mainchain.
		// This hardcoded value is unused - the actual d_parameter comes from the runtime.
		let d_parameter =
			DParameter { num_permissioned_candidates: 0, num_registered_candidates: 0 };

		let boundary = self.last_block_of_epoch(data_epoch).await?;
		let candidates_output = match boundary {
			Some(boundary) => {
				self.latest_token_output_upto(&unit, block_height(&boundary)?).await?
			},
			None => None,
		};

		let permissioned_candidates = match candidates_output {
			None => None,
			Some((_row, utxos)) => {
				let output = utxos
					.outputs
					.iter()
					.filter(|o| !o.collateral && has_unit(&o.amount, &unit))
					.min_by_key(|o| o.output_index)
					.expect("latest_token_output_upto only returns txs with a matching output");
				let datum = self
					.client
					.datum(output.inline_datum.as_ref(), output.data_hash.as_ref())
					.await?
					.ok_or("Expected data was not found: Permissioned Candidates List Datum")?;
				Some(PermissionedCandidateDatums::try_from(datum)?.into())
			},
		};

		Ok(AriadneParameters { d_parameter, permissioned_candidates })
	}

	async fn get_candidates_uncached(
		&self,
		data_epoch: u32,
		committee_candidate_address: &MainchainAddress,
	) -> Result<Vec<CandidateRegistrations>, BoxError> {
		let address = committee_candidate_address.to_string();
		let Some(boundary) = self.last_block_of_epoch(data_epoch).await? else {
			return Ok(vec![]);
		};
		let address_scan_timer =
			start_sub_query_timer(&self.metrics_opt, "candidates_get_utxos_for_address");
		let boundary_height = block_height(&boundary)?;

		// TODO: this replays the address's entire transaction history on every uncached
		// epoch, and the address is public, so anyone can lengthen that history by sending
		// transactions to it. Make it incremental — keep the computed UTXO set and fetch
		// only what is new since the last boundary, as `governance_body_authorities` does.
		//
		// Rebuild the as-of-boundary UTXO set of the committee candidate address:
		// outputs created at block <= boundary minus outputs spent at block <= boundary.
		// All listed txs are <= boundary, so spends after the boundary don't remove
		// entries — the same semantics as db-sync's `get_utxos_for_address`.
		let rows = self
			.client
			.range_txs(
				TxSource::Address(&address),
				None,
				Some(BlockCursor::block(u64::from(boundary_height))),
			)
			.await?;

		struct ActiveUtxo {
			utxo_id: UtxoId,
			datum: Option<PlutusData>,
			tx_inputs: Vec<UtxoId>,
			row: RangeTx,
		}
		let mut active: HashMap<(McTxHash, u16), ActiveUtxo> = HashMap::new();
		for row in &rows {
			let utxos = self.client.tx_utxos(&row.tx_hash).await?;
			let tx_hash = McTxHash(decode_hash32(&row.tx_hash)?);
			let tx_inputs: Vec<UtxoId> = utxos
				.inputs
				.iter()
				.filter(|i| !i.collateral && !i.reference.unwrap_or(false))
				.map(|i| {
					Ok::<_, BoxError>(UtxoId {
						tx_hash: McTxHash(decode_hash32(&i.tx_hash)?),
						index: UtxoIndex(u16::try_from(i.output_index)?),
					})
				})
				.collect::<Result<_, _>>()?;
			for input in
				utxos.inputs.iter().filter(|i| !i.collateral && !i.reference.unwrap_or(false))
			{
				if input.address == address {
					active.remove(&(
						McTxHash(decode_hash32(&input.tx_hash)?),
						u16::try_from(input.output_index)?,
					));
				}
			}
			for output in utxos.outputs.iter().filter(|o| !o.collateral) {
				if output.address == address {
					let index = u16::try_from(output.output_index)?;
					let datum = self
						.client
						.datum(output.inline_datum.as_ref(), output.data_hash.as_ref())
						.await?;
					active.insert(
						(tx_hash, index),
						ActiveUtxo {
							utxo_id: UtxoId { tx_hash, index: UtxoIndex(index) },
							datum,
							tx_inputs: tx_inputs.clone(),
							row: row.clone(),
						},
					);
				}
			}
		}

		// Block info (epoch/slot) for each creating tx.
		let mut blocks: HashMap<u32, BlockContent> = HashMap::new();
		for utxo in active.values() {
			if let std::collections::hash_map::Entry::Vacant(entry) =
				blocks.entry(utxo.row.block_height)
			{
				let block = self
					.client
					.block_by_id(&utxo.row.block_height.to_string())
					.await?
					.ok_or_else(|| format!("block {} not found", utxo.row.block_height))?;
				entry.insert(block);
			}
		}

		// Parse registrations, mirroring `parse_candidates` (log and skip invalid datums).
		let mut candidates: Vec<(RegisterValidatorDatum, UtxoInfo, Vec<UtxoId>)> = Vec::new();
		let mut active: Vec<ActiveUtxo> = active.into_values().collect();
		active.sort_by_key(|u| (u.row.block_height, u.row.tx_index, u.utxo_id.index.0));
		for utxo in active {
			let Some(datum) = utxo.datum else {
				log::error!("Missing registration datum for {:?}", utxo.utxo_id);
				continue;
			};
			let Ok(datum) = RegisterValidatorDatum::try_from(datum) else {
				log::error!("Invalid registration datum for {:?}", utxo.utxo_id);
				continue;
			};
			let block = &blocks[&utxo.row.block_height];
			let utxo_info = UtxoInfo {
				utxo_id: utxo.utxo_id,
				epoch_number: McEpochNumber(u32::try_from(
					block.epoch.ok_or("block has no epoch")?,
				)?),
				block_number: McBlockNumber(utxo.row.block_height),
				slot_number: McSlotNumber(u64::try_from(block.slot.ok_or("block has no slot")?)?),
				tx_index_within_block: McTxIndexInBlock(utxo.row.tx_index),
			};
			candidates.push((datum, utxo_info, utxo.tx_inputs));
		}

		let registered = candidates
			.into_iter()
			.map(|(datum, utxo_info, tx_inputs)| {
				make_registered_candidate(datum, utxo_info, tx_inputs)
			})
			.collect::<Vec<_>>();

		// Stake distribution: per-pool sums for the data epoch. An entirely absent
		// `epoch_stake` snapshot yields `stake_delegation: None` for every candidate
		// (mirrors `stake_map.is_empty()`); a pool missing from a non-empty snapshot
		// yields Some(0).
		// Nothing to weight when no candidate registrations exist (the current state of
		// all networks: `num_registered_candidates` is 0), so skip the probe entirely.
		let epoch_has_stakes = if registered.is_empty() {
			false
		} else {
			let _t = Timer::new(format!("GET epochs/{data_epoch}/stakes (sentinel)"));
			match deadline(
				&format!("epochs/{data_epoch}/stakes"),
				self.client
					.api
					.epochs_stakes(data_epoch as i32, Pagination::new(Order::Asc, 1, 1)),
			)
			.await?
			{
				Ok(rows) => !rows.is_empty(),
				Err(e) if is_404(&e) => false,
				Err(e) => return Err(box_err(e)),
			}
		};

		// Shadowing would not drop the address-scan guard, so its histogram would also
		// cover this phase. The db-sync source drops at the same boundary.
		drop(address_scan_timer);
		let mut stake_by_pool: HashMap<MainchainKeyHash, StakeDelegation> = HashMap::new();
		let _sq_timer =
			start_sub_query_timer(&self.metrics_opt, "candidates_get_stake_distribution");
		if epoch_has_stakes {
			let pools: Vec<MainchainKeyHash> = registered
				.iter()
				.map(|c| MainchainKeyHash::from_vkey(&c.stake_pool_pub_key.0))
				.collect::<std::collections::HashSet<_>>()
				.into_iter()
				.collect();
			for pool in pools {
				let key_hash = Ed25519KeyHash::from_bytes(pool.0.to_vec())
					.map_err(|e| format!("invalid pool key hash: {e:?}"))?;
				let bech32 = key_hash
					.to_bech32("pool")
					.map_err(|e| format!("failed to bech32-encode pool hash: {e:?}"))?;
				// A pool's per-epoch `active_stake` equals db-sync's
				// `SUM(epoch_stake.amount) GROUP BY pool` for that epoch (verified against
				// the delegator listings). Reading it from the pool's history costs a few
				// requests per pool instead of paging every delegator — the delegator lists
				// run to hundreds of pages for large pools.
				//
				// The SDK honours `from`/`to` cursors only on the account, address and asset
				// transaction endpoints and silently ignores them everywhere else, so the
				// epoch cannot be selected server-side. Walk ascending and stop as soon as
				// the row is found or the history passes `data_epoch`: a pool's history
				// begins near the epochs a syncing node replays, so this is usually a
				// single request. Descending would need the whole history for any epoch
				// that is not close to the tip, which is the common case during sync.
				let _t = Timer::new(format!("GET pools/{bech32}/history (epoch {data_epoch})"));
				let mut entry = None;
				for page in 1..=MAX_PAGES {
					let rows = match deadline(
						&format!("pools/{bech32}/history page={page}"),
						self.client
							.api
							.pools_history(&bech32, Pagination::new(Order::Asc, page, PAGE_SIZE)),
					)
					.await?
					{
						Ok(rows) => rows,
						Err(e) if is_404(&e) => vec![],
						Err(e) => return Err(box_err(e)),
					};
					let last_page = rows.len() < PAGE_SIZE;
					// Ascending order: a row beyond the target proves the epoch is absent.
					let past_target = rows.iter().any(|r| r.epoch > data_epoch as i32);
					entry = rows.into_iter().find(|r| r.epoch == data_epoch as i32);
					if entry.is_some() || past_target || last_page {
						break;
					}
				}
				// An epoch missing from a pool's history means no stake in that epoch,
				// which is the same as its absence from the SQL `GROUP BY` — the caller
				// maps a missing entry to `Some(0)` when the epoch snapshot exists.
				if let Some(entry) = entry {
					let stake = entry
						.active_stake
						.parse::<u64>()
						.map_err(|e| format!("invalid active_stake for {bech32}: {e}"))?;
					stake_by_pool.insert(pool, StakeDelegation(stake));
				}
			}
		}

		let mut grouped: HashMap<StakePoolPublicKey, Vec<RegisteredCandidate>> = HashMap::new();
		for candidate in registered {
			grouped.entry(candidate.stake_pool_pub_key.clone()).or_default().push(candidate);
		}
		Ok(grouped
			.into_iter()
			.map(|(stake_pool_public_key, candidates)| CandidateRegistrations {
				stake_pool_public_key: stake_pool_public_key.clone(),
				registrations: candidates.into_iter().map(|c| c.registration_data).collect(),
				stake_delegation: if epoch_has_stakes {
					Some(
						stake_by_pool
							.get(&MainchainKeyHash::from_vkey(&stake_pool_public_key.0))
							.cloned()
							.unwrap_or(StakeDelegation(0)),
					)
				} else {
					None
				},
			})
			.collect())
	}
}

struct RegisteredCandidate {
	stake_pool_pub_key: StakePoolPublicKey,
	registration_data: RegistrationData,
}

/// Mirrors `convert_utxos_to_candidates` for the V0/V1 datum variants.
fn make_registered_candidate(
	datum: RegisterValidatorDatum,
	utxo_info: UtxoInfo,
	tx_inputs: Vec<UtxoId>,
) -> RegisteredCandidate {
	match datum {
		RegisterValidatorDatum::V0 {
			stake_ownership,
			sidechain_pub_key,
			sidechain_signature,
			registration_utxo,
			own_pkh: _own_pkh,
			aura_pub_key,
			grandpa_pub_key,
		} => RegisteredCandidate {
			stake_pool_pub_key: stake_ownership.pub_key,
			registration_data: RegistrationData {
				registration_utxo,
				sidechain_signature: sidechain_signature.clone(),
				mainchain_signature: stake_ownership.signature,
				// For now we use the same key for both cross chain and sidechain actions
				cross_chain_signature: CrossChainSignature(sidechain_signature.0),
				sidechain_pub_key: sidechain_pub_key.clone(),
				cross_chain_pub_key: CrossChainPublicKey(sidechain_pub_key.0),
				keys: CandidateKeys(vec![aura_pub_key.into(), grandpa_pub_key.into()]),
				utxo_info,
				tx_inputs,
			},
		},
		RegisterValidatorDatum::V1 {
			stake_ownership,
			sidechain_pub_key,
			sidechain_signature,
			registration_utxo,
			own_pkh: _own_pkh,
			keys,
		} => RegisteredCandidate {
			stake_pool_pub_key: stake_ownership.pub_key,
			registration_data: RegistrationData {
				registration_utxo,
				sidechain_signature: sidechain_signature.clone(),
				mainchain_signature: stake_ownership.signature,
				// For now we use the same key for both cross chain and sidechain actions
				cross_chain_signature: CrossChainSignature(sidechain_signature.0),
				sidechain_pub_key: sidechain_pub_key.clone(),
				cross_chain_pub_key: CrossChainPublicKey(sidechain_pub_key.0),
				keys,
				utxo_info,
				tx_inputs,
			},
		},
	}
}

#[async_trait::async_trait]
impl AuthoritySelectionDataSource for BlockfrostAuthoritySelectionDataSource {
	async fn get_ariadne_parameters(
		&self,
		epoch: McEpochNumber,
		_d_parameter_policy: PolicyId,
		permissioned_candidate_policy: PolicyId,
	) -> Result<AriadneParameters, BoxError> {
		let _t = Timer::new(format!("get_ariadne_parameters[{}]", epoch.0));
		let data_epoch = self.data_epoch_of(epoch)?;
		let key = (data_epoch, permissioned_candidate_policy.clone());
		let cacheable = self.can_cache(data_epoch).await;
		if cacheable
			&& let Ok(mut caches) = self.caches.lock()
			&& let Some(cached) = caches.ariadne.get(&key)
		{
			return Ok(cached.clone());
		}
		let result = self
			.get_ariadne_parameters_uncached(data_epoch, &permissioned_candidate_policy)
			.await?;
		if cacheable && let Ok(mut caches) = self.caches.lock() {
			caches.ariadne.put(key, result.clone());
		}
		Ok(result)
	}

	async fn get_candidates(
		&self,
		epoch: McEpochNumber,
		committee_candidate_address: MainchainAddress,
	) -> Result<Vec<CandidateRegistrations>, BoxError> {
		let _t = Timer::new(format!("get_candidates[{}]", epoch.0));
		let data_epoch = self.data_epoch_of(epoch)?;
		let key = (data_epoch, committee_candidate_address.to_string());
		let cacheable = self.can_cache(data_epoch).await;
		if cacheable
			&& let Ok(mut caches) = self.caches.lock()
			&& let Some(cached) = caches.candidates.get(&key)
		{
			return Ok(cached.clone());
		}
		let result = self.get_candidates_uncached(data_epoch, &committee_candidate_address).await?;
		if cacheable && let Ok(mut caches) = self.caches.lock() {
			caches.candidates.put(key, result.clone());
		}
		Ok(result)
	}

	async fn get_epoch_nonce(&self, epoch: McEpochNumber) -> Result<Option<EpochNonce>, BoxError> {
		let _t = Timer::new(format!("get_epoch_nonce[{}]", epoch.0));
		let nonce_timer = start_sub_query_timer(&self.metrics_opt, "candidates_get_epoch_nonce");
		let data_epoch = self.data_epoch_of(epoch)?;
		if let Ok(mut caches) = self.caches.lock()
			&& let Some(cached) = caches.nonce.get(&data_epoch)
		{
			return Ok(Some(cached.clone()));
		}
		let params = {
			let _t = Timer::new(format!("GET epochs/{data_epoch}/parameters"));
			match deadline(
				&format!("epochs/{data_epoch}/parameters"),
				self.client.api.epochs_parameters(data_epoch as i32),
			)
			.await?
			{
				Ok(params) => Some(params),
				Err(e) if is_404(&e) => None,
				Err(e) => return Err(box_err(e)),
			}
		};
		let nonce = params
			.map(|p| Ok::<_, BoxError>(EpochNonce(hex::decode(p.nonce)?)))
			.transpose()?;
		// Stops here: `can_cache()` below issues its own requests and is not part of
		// the db-sync query this label mirrors.
		drop(nonce_timer);
		// `None` nonces are never cached: the epoch data may simply not be there yet.
		if let Some(nonce) = &nonce
			&& self.can_cache(data_epoch).await
			&& let Ok(mut caches) = self.caches.lock()
		{
			caches.nonce.put(data_epoch, nonce.clone());
		}
		Ok(nonce)
	}

	async fn data_epoch(&self, for_epoch: McEpochNumber) -> Result<McEpochNumber, BoxError> {
		Ok(McEpochNumber(self.data_epoch_of(for_epoch)?))
	}
}

#[cfg(test)]
mod tests {
	use super::super::testing::{Reply, client_at, fake_server};
	use super::*;

	#[tokio::test]
	async fn sub_query_metrics_are_recorded_under_the_db_sync_label() {
		use midnight_primitives_mainchain_follower::data_source::metrics::MetricsRegistry;

		// A 404 for the epoch parameters is a valid "no data yet" answer, so this drives
		// the method to completion with a single request.
		let (url, _seen) = fake_server(vec![Reply::Json(404, "{}".into())]).await;
		let registry = MetricsRegistry::new();
		let metrics = MidnightDataSourceMetrics::register(&registry).expect("register");
		let source = BlockfrostAuthoritySelectionDataSource::new(
			Arc::new(client_at(&url)),
			432,
			Some(metrics),
		);

		let nonce = source.get_epoch_nonce(McEpochNumber(100)).await.expect("nonce query");
		assert!(nonce.is_none(), "404 means the epoch has no data yet");

		// The histogram must carry one observation under the same label the db-sync
		// source uses, otherwise the two backends cannot be compared in one panel.
		let families = registry.gather();
		let family = families
			.iter()
			.find(|f| f.get_name() == "midnight_data_source_query_time_elapsed")
			.expect("histogram registered");
		let sample = family
			.get_metric()
			.iter()
			.find(|m| {
				m.get_label().iter().any(|l| {
					l.get_name() == "query_name" && l.get_value() == "candidates_get_epoch_nonce"
				})
			})
			.expect("a sample labelled candidates_get_epoch_nonce");
		assert_eq!(sample.get_histogram().get_sample_count(), 1);
	}
}
