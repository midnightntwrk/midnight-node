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

//! Sliding-window cNIGHT observation data source.
//!
//! Holds a contiguous window of observation events in memory, sorted by
//! Cardano position. The cache starts empty: the first inherent query after
//! startup is served by the live db-backed source and kicks off a background
//! refresh anchored at the runtime's latest processed Cardano position — so a
//! node restarting after a full sync pulls only the window it needs, not
//! `[genesis, tip]`. Single-flight refreshes slide the window forward as the
//! chain advances (trimming behind the follower, extending toward the stable
//! tip). Queries outside the cached window delegate to the live source so the
//! node keeps importing.

use crate::data_source::candidates_data_source::observed_async_trait;
use crate::data_source::cnight_observation::{
	MidnightCNightObservationDataSourceError, MidnightCNightObservationDataSourceImpl,
};
use crate::data_source::metrics::MidnightDataSourceMetrics;
use crate::{MidnightCNightObservationDataSource, ObservedUtxo};
use cardano_serialization_lib::{Address, EnterpriseAddress};
use midnight_primitives_cnight_observation::{CNightAddresses, CardanoPosition, ObservedUtxos};
use sidechain_domain::McBlockHash;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Row ceiling for a "whole range" pull. Both the sliding window and the
/// consensus path pass this and rely on completeness (not a row count) for
/// determinism; a pull that actually returns this many rows is treated as
/// truncated (see `bulk_pull`).
pub const LARGE_LIMIT: usize = 5_000_000;

/// Default number of cardano blocks to keep in the sliding window when the
/// node config doesn't override it. Memory cost ≈ 5 KB × events-per-block,
/// so 100k blocks ≈ a few hundred MB on a busy chain.
pub const DEFAULT_WINDOW_SIZE: u32 = 100_000;

/// If the next-needed cardano position (`start_position`) is within this many
/// blocks of the cache's `end`, kick an async refresh that slides the window
/// forward.
const REFRESH_THRESHOLD: u32 = 10_000;

/// Errors that can arise while bulk-pulling cNIGHT observation events.
#[derive(thiserror::Error, Debug)]
pub enum BulkPullError {
	#[error("invalid mapping validator address: {0}")]
	InvalidMappingValidatorAddress(String),
	#[error("failed to extract network id from mapping validator address: {0}")]
	NetworkId(String),
	#[error("mapping validator address is not an EnterpriseAddress")]
	NotEnterpriseAddress,
	#[error("mapping validator address has no script hash")]
	MissingScriptHash,
	#[error("get_low_bounds({0}) returned None")]
	MissingLowBounds(u32),
	#[error("get_high_bounds({0}) returned None")]
	MissingHighBounds(u32),
	#[error(transparent)]
	Db(#[from] sqlx::Error),
	#[error(transparent)]
	Observation(#[from] MidnightCNightObservationDataSourceError),
}

/// Pull every cnight observation event in `[start, end]` (inclusive), sorted by
/// `tx_position`, with `complete = true` iff no query hit `limit` (so the result
/// is the whole range, not a row-limited prefix).
///
/// `complete == false` means a query held >`limit` rows. At [`LARGE_LIMIT`] that
/// is pathological, so it is logged at error level; callers must not advance the
/// cursor to the tip on it (that would skip the unfetched rows).
///
/// Both endpoints are full `CardanoPosition`s so callers can pass exact
/// `(block, tx_index)` boundaries or whole-block ranges via
/// `CardanoPosition::{min,max}_for_block`.
pub async fn bulk_pull(
	pool: &PgPool,
	cfg: &CNightAddresses,
	start: &CardanoPosition,
	end: &CardanoPosition,
	// Per-query SQL row limit; callers pass `LARGE_LIMIT` for a whole-range pull.
	limit: usize,
) -> Result<(Vec<ObservedUtxo>, bool), BulkPullError> {
	let data_source = MidnightCNightObservationDataSourceImpl::new(pool.clone(), None, 0);

	let mapping_validator_address = Address::from_bech32(&cfg.mapping_validator_address)
		.map_err(|e| BulkPullError::InvalidMappingValidatorAddress(e.to_string()))?;
	let cardano_network = mapping_validator_address
		.network_id()
		.map_err(|e| BulkPullError::NetworkId(e.to_string()))?;
	let mapping_validator_policy_id = EnterpriseAddress::from_address(&mapping_validator_address)
		.ok_or(BulkPullError::NotEnterpriseAddress)?
		.payment_cred()
		.to_scripthash()
		.ok_or(BulkPullError::MissingScriptHash)?;

	// One-shot id lookups: there's no caching benefit within a single pull, so
	// query directly instead of allocating a throwaway `MultiAssetCache`.
	let auth_token_ident = crate::db::resolve_multi_asset_id(
		pool,
		&mapping_validator_policy_id.to_bytes(),
		cfg.auth_token_asset_name.as_bytes(),
	)
	.await?;
	let cnight_ident = crate::db::resolve_multi_asset_id(
		pool,
		&cfg.cnight_policy_id,
		cfg.cnight_asset_name.as_bytes(),
	)
	.await?;

	let (low_bounds, high_bounds) = tokio::try_join!(
		crate::db::get_low_bounds(pool, start.block_number.into()),
		crate::db::get_high_bounds(pool, end.block_number.into()),
	)?;
	let low_bounds = low_bounds.ok_or(BulkPullError::MissingLowBounds(start.block_number))?;
	let high_bounds = high_bounds.ok_or(BulkPullError::MissingHighBounds(end.block_number))?;

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
		let v = data_source
			.get_registration_utxos(cardano_network, ident, &cfg.mapping_validator_address, &paged)
			.await?;
		counts.0 = v.len();
		all.extend(v);
	}
	let v = data_source
		.get_deregistration_utxos(cardano_network, &cfg.mapping_validator_address, &paged)
		.await?;
	counts.1 = v.len();
	all.extend(v);
	if let Some(ident) = cnight_ident {
		let v = data_source.get_asset_create_utxos(cardano_network, ident, &paged).await?;
		counts.2 = v.len();
		all.extend(v);
		let v = data_source.get_asset_spend_utxos(cardano_network, ident, &paged).await?;
		counts.3 = v.len();
		all.extend(v);
	}
	all.sort();
	// A query that returned exactly `limit` rows may have more behind it, so the
	// pull is not provably complete.
	let complete = counts.0 < limit && counts.1 < limit && counts.2 < limit && counts.3 < limit;
	log::info!(
		target: "cnight::sliding-window",
		"bulk_pull [{}/{}, {}/{}] -> reg={} dereg={} create={} spend={} complete={} (auth_ident={:?} cnight_ident={:?})",
		start.block_number, start.tx_index_in_block,
		end.block_number, end.tx_index_in_block,
		counts.0, counts.1, counts.2, counts.3, complete, auth_token_ident, cnight_ident,
	);
	if !complete {
		log::error!(
			target: "cnight::sliding-window",
			"bulk_pull hit the {limit}-row limit in [{}, {}] (reg={} dereg={} create={} spend={}); \
			 the cNIGHT observation window may be incomplete and the cursor will not advance to the tip",
			start.block_number, end.block_number, counts.0, counts.1, counts.2, counts.3,
		);
	}
	Ok((all, complete))
}

/// Build the observation inherent from the **complete** sorted event list for
/// `[start, fallback_end]`. Caps at `tx_capacity` whole transactions and
/// `max_utxos` UTXOs (the runtime `process_tokens` envelope).
///
/// Consensus invariants:
/// - Transactions are admitted whole, so `end` lands on a tx boundary; resuming
///   there cannot skip a counted tx's UTXOs.
/// - The cursor reaches `fallback_end` (the tip) ONLY when `complete` (the fetch
///   wasn't row-limited); otherwise it stops at the last fully-observed tx.
///
/// Every node feeds this the complete range and the same `max_utxos`, so there is
/// no fetch-size input left to disagree on — inherents are byte-identical.
pub fn truncate_to_tx_capacity(
	events: Vec<ObservedUtxo>,
	tx_capacity: usize,
	max_utxos: usize,
	complete: bool,
	start_position: &CardanoPosition,
	fallback_end: CardanoPosition,
) -> ObservedUtxos {
	let n = events.len();
	let mut truncated: Vec<ObservedUtxo> = Vec::with_capacity(n.min(max_utxos));
	let mut num_txs: usize = 0;
	let mut capped = false;
	let mut i = 0usize;
	while i < n {
		// Extent [i, j) of the whole tx at `i` — its UTXOs share a position.
		let mut j = i + 1;
		while j < n
			&& events[j].header.tx_position.partial_cmp(&events[i].header.tx_position)
				== Some(core::cmp::Ordering::Equal)
		{
			j += 1;
		}
		let tx_len = j - i;
		// A lone tx bigger than the whole envelope is still admitted (cap check
		// skipped while empty) so the runtime's bound rejects it loudly instead
		// of us stalling forever.
		let exceeds_tx_cap = num_txs + 1 > tx_capacity;
		let exceeds_max_utxos = !truncated.is_empty() && truncated.len() + tx_len > max_utxos;
		if exceeds_tx_cap || exceeds_max_utxos {
			capped = true;
			break;
		}
		truncated.extend_from_slice(&events[i..j]);
		num_txs += 1;
		i = j;
	}

	let end = if capped {
		// Only whole txs are admitted, so resuming just past the last one is
		// safe. That holds even for a row-limited fetch (`limit = max_utxos + 1`):
		// a query that hit the row limit contributed more rows than the
		// `max_utxos` cap admits, so the cap fires inside the proven-complete
		// prefix. Sole exception: the lone-oversized-tx admission above, whose
		// fetched rows may themselves be truncated — that needs one tx with more
		// than `max_utxos` events, beyond what a Cardano tx can physically carry.
		boundary_after(&truncated, start_position)
	} else if complete {
		fallback_end
	} else {
		// Row-limited: the final fetched tx may be missing UTXOs past the limit,
		// so drop it and resume only as far as we proved complete.
		if let Some(last) = truncated.last().map(|u| u.header.tx_position.clone()) {
			truncated.retain(|u| u.header.tx_position < last);
		}
		boundary_after(&truncated, start_position)
	};

	ObservedUtxos { start: start_position.clone(), end, utxos: truncated }
}

/// Position just past the last accepted event. When nothing was accepted this
/// still increments past `start`, skipping the tx at the cursor — reachable
/// only if the sole fetched tx was dropped as row-truncated, i.e. a single tx
/// with more than `max_utxos` events at the cursor, beyond what a Cardano tx
/// can physically carry.
fn boundary_after(truncated: &[ObservedUtxo], start_position: &CardanoPosition) -> CardanoPosition {
	truncated
		.last()
		.map(|u| u.header.tx_position.clone())
		.unwrap_or_else(|| start_position.clone())
		.increment()
}

/// Cached result of the previous `get_utxos_up_to_capacity` call. During
/// initial sync many consecutive Midnight blocks share the same Cardano tip,
/// so recomputing the window each time is wasted work.
#[derive(Clone)]
struct LastObservation {
	start_position: CardanoPosition,
	current_tip: McBlockHash,
	result: ObservedUtxos,
}

/// A `MidnightCNightObservationDataSource` backed by an in-memory event vector
/// built once at startup, with an async sliding-window refresh and a live
/// db-backed fallback for queries past the current horizon.
pub struct BulkCachedCNightObservationDataSource {
	/// Sorted events. Readers take the read lock for the (cheap) slice+copy of
	/// their window; the refresh task takes the write lock briefly to mutate
	/// the vec in place (trim the front, append the extension).
	all_events: Arc<std::sync::RwLock<Vec<ObservedUtxo>>>,
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
	/// Single-flight gate for sliding-window refreshes. The owned lock guard is
	/// held by the in-flight refresh task; `try_lock_owned` failing means a
	/// refresh is already running, so a new trigger is a no-op.
	refresh_in_flight: Arc<tokio::sync::Mutex<()>>,
	#[allow(dead_code)]
	metrics_opt: Option<MidnightDataSourceMetrics>,
}

/// Configuration and dependencies for [`BulkCachedCNightObservationDataSource::new`].
///
/// The initial `events` are passed to `new` separately (they're bulk data, not
/// configuration); everything the cache needs to bootstrap and run its
/// sliding-window refresh lives here.
pub struct BulkCacheConfig {
	/// Cardano block range the initial events cover: `[window_start_block, window_end_block]`.
	pub window_start_block: u32,
	pub window_end_block: u32,
	/// Cardano blocks to keep in the sliding window.
	pub window_size: u32,
	/// Cardano blocks to leave un-fetched past the requested target (re-org
	/// safety). Equals `cardano_security_parameter + block_stability_margin`.
	pub stability_margin: u32,
	/// db-sync connection used by the refresh and per-call block lookups.
	pub pool: PgPool,
	/// Live source consulted for queries past the cached window.
	pub db_fallback: Arc<MidnightCNightObservationDataSourceImpl>,
	/// cNIGHT addresses the refresh re-runs the observation queries against.
	pub cnight_addresses: CNightAddresses,
	pub metrics_opt: Option<MidnightDataSourceMetrics>,
}

impl BulkCachedCNightObservationDataSource {
	/// Build a cache seeded with `events` covering
	/// `[config.window_start_block, config.window_end_block]`. The caller is
	/// responsible for having bulk-pulled that range; we just record the
	/// bookkeeping.
	pub fn new(events: Vec<ObservedUtxo>, config: BulkCacheConfig) -> Self {
		let BulkCacheConfig {
			window_start_block,
			window_end_block,
			window_size,
			stability_margin,
			pool,
			db_fallback,
			cnight_addresses,
			metrics_opt,
		} = config;
		Self {
			all_events: Arc::new(std::sync::RwLock::new(events)),
			pool,
			block_position_cache: Arc::new(Mutex::new(HashMap::new())),
			last_observation: Arc::new(Mutex::new(None)),
			snapshot_start_block: Arc::new(std::sync::RwLock::new(Some(window_start_block))),
			snapshot_end_block: Arc::new(std::sync::RwLock::new(Some(window_end_block))),
			db_fallback,
			cnight_addresses,
			stability_margin,
			window_size,
			refresh_in_flight: Arc::new(tokio::sync::Mutex::new(())),
			metrics_opt,
		}
	}

	/// Trigger an async sliding-window refresh if not already in flight.
	/// Returns immediately. Single-flight: concurrent triggers are no-ops.
	/// `follower_anchor` is the runtime's latest processed Cardano block (the
	/// query's `start_position`) — the refresh restarts the window there when
	/// the cache has fallen behind it (see [`plan_refresh`]).
	fn maybe_kick_refresh(&self, follower_anchor: u32, target_end: u32) {
		// Single-flight: if a refresh already holds the gate, do nothing. The
		// guard is moved into the spawned task and released on completion.
		let Ok(guard) = self.refresh_in_flight.clone().try_lock_owned() else {
			return;
		};

		// Snapshot the shared state the refresh needs (cheap — mostly `Arc`s) so
		// it can run in the spawned task independent of `&self`.
		let ctx = RefreshContext {
			pool: self.pool.clone(),
			cnight_addresses: self.cnight_addresses.clone(),
			all_events: Arc::clone(&self.all_events),
			last_observation: Arc::clone(&self.last_observation),
			snapshot_start_block: Arc::clone(&self.snapshot_start_block),
			snapshot_end_block: Arc::clone(&self.snapshot_end_block),
			window_size: self.window_size,
			stability_margin: self.stability_margin,
		};

		tokio::spawn(async move {
			// Hold the gate for the lifetime of the refresh; dropped (unlocked)
			// when this task ends.
			let _guard = guard;
			if let Err(e) = ctx.refresh(follower_anchor, target_end).await {
				log::warn!(
					target: "cnight::sliding-window",
					"refresh failed (ignored, db_fallback continues to serve): {e}"
				);
			}
		});
	}
}

/// Shared state a sliding-window refresh operates on. Built in
/// `maybe_kick_refresh` by cloning the relevant fields out of the data source
/// (cheap — mostly `Arc`s) so the refresh can run in a spawned task.
struct RefreshContext {
	pool: PgPool,
	cnight_addresses: CNightAddresses,
	all_events: Arc<std::sync::RwLock<Vec<ObservedUtxo>>>,
	last_observation: Arc<Mutex<Option<LastObservation>>>,
	snapshot_start_block: Arc<std::sync::RwLock<Option<u32>>>,
	snapshot_end_block: Arc<std::sync::RwLock<Option<u32>>>,
	window_size: u32,
	stability_margin: u32,
}

impl RefreshContext {
	/// Extend the cache forward to `target_end`, pulling events in `(old_end,
	/// target_end]` — or, when the follower has moved past the window
	/// entirely, restart the window at `follower_anchor` (see
	/// [`plan_refresh`]). New events sort strictly after every retained
	/// event, so no global re-sort is needed.
	async fn refresh(
		&self,
		follower_anchor: u32,
		target_end: u32,
	) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		// Clamp refreshes to the highest stable db-sync block at or below the
		// requested target. This keeps proactive lookahead from caching
		// rollback-prone Cardano blocks while still tolerating sparse snapshots or
		// a db-sync instance that has not reached the exact requested block.
		let target_end = match crate::db::get_highest_stable_block_le(
			&self.pool,
			target_end,
			self.stability_margin,
		)
		.await?
		{
			Some(highest) => highest,
			None => return Ok(()), // no stable block at or below the target yet
		};

		let old_end = self
			.snapshot_end_block
			.read()
			.map_err(|e| format!("snapshot_end_block read poisoned: {e}"))?
			.unwrap_or(0);
		if target_end <= old_end {
			return Ok(());
		}
		let existing_start = self
			.snapshot_start_block
			.read()
			.map_err(|e| format!("snapshot_start_block read poisoned: {e}"))?
			.unwrap_or_else(|| old_end.saturating_add(1));
		let trim_anchor = self
			.last_observation
			.lock()
			.ok()
			.and_then(|g| g.as_ref().map(|last| last.start_position.block_number));
		let (from_block, new_window_start) =
			plan_refresh(old_end, follower_anchor, existing_start, trim_anchor, self.window_size);
		// The stable clamp above can land below a jumped-forward `from_block`
		// when db-sync lags; nothing useful to pull yet.
		if target_end < from_block {
			return Ok(());
		}
		log::info!(
			target: "cnight::sliding-window",
			"refresh kicked off: pulling [{from_block}, {target_end}] (was end={old_end}); trim behind {new_window_start}"
		);
		let t0 = std::time::Instant::now();
		let (start, end) = (
			CardanoPosition::min_for_block(from_block),
			CardanoPosition::max_for_block(target_end),
		);
		// Whole multi-block window, so `LARGE_LIMIT`. `_complete`: bulk_pull
		// already error-logs a truncated pull (which would leave a gap here).
		let (extension, _complete) =
			bulk_pull(&self.pool, &self.cnight_addresses, &start, &end, LARGE_LIMIT).await?;
		{
			let mut events_guard =
				self.all_events.write().map_err(|e| format!("all_events write poisoned: {e}"))?;
			slide_events(&mut events_guard, extension, new_window_start);
		}
		*self
			.snapshot_start_block
			.write()
			.map_err(|e| format!("snapshot_start_block write poisoned: {e}"))? = Some(new_window_start);
		*self
			.snapshot_end_block
			.write()
			.map_err(|e| format!("snapshot_end_block write poisoned: {e}"))? = Some(target_end);
		log::info!(
			target: "cnight::sliding-window",
			"refresh done: window now [{new_window_start}, {target_end}] (took {:?})",
			t0.elapsed()
		);
		Ok(())
	}
}

/// Decide a refresh's pull start and new window start:
/// `(from_block, new_window_start)`.
///
/// Contiguous case (`follower_anchor <= old_end + 1`): extend from
/// `old_end + 1`. The trim point is anchored on the follower's last-seen
/// position, keeping `window_size` blocks behind it — during catchup the
/// follower can be hundreds of thousands of blocks behind tip and still
/// needs that history, so trimming behind `target_end - window_size` would
/// silently drop required events. With no follower call observed yet
/// (`trim_anchor` is `None`), keep the existing start — never move it
/// backward, otherwise we'd lie about coverage.
///
/// Jump case (`follower_anchor > old_end + 1`): the runtime has already
/// processed past the window's end, so extending contiguously would re-pull
/// history nobody needs — e.g. a node restarting after a full sync, where
/// the window is still anchored at the genesis observation position.
/// Restart the window at the follower's position instead; queries older than
/// that (competing forks) are served by `db_fallback`. The window start must
/// equal the pull start here: retaining the old (pre-gap) events while
/// claiming coverage from an older `new_window_start` would leave a hole in
/// `(old_end, follower_anchor)` that cache reads would silently miss.
fn plan_refresh(
	old_end: u32,
	follower_anchor: u32,
	existing_start: u32,
	trim_anchor: Option<u32>,
	window_size: u32,
) -> (u32, u32) {
	let contiguous_from = old_end.saturating_add(1);
	if follower_anchor > contiguous_from {
		return (follower_anchor, follower_anchor);
	}
	let new_window_start = match trim_anchor {
		Some(anchor) => existing_start.max(anchor.saturating_sub(window_size)),
		None => existing_start,
	};
	(contiguous_from, new_window_start)
}

/// Slide the in-memory window forward, in place: drop events before
/// `new_window_start`, then append `extension` (events strictly after the
/// existing end). Mutates `existing` to avoid allocating a fresh vec.
fn slide_events(
	existing: &mut Vec<ObservedUtxo>,
	extension: Vec<ObservedUtxo>,
	new_window_start: u32,
) {
	// `existing` is sorted ascending by tx_position.block_number, so a
	// partition_point gives the first retained index in O(log n).
	let trim_at =
		existing.partition_point(|u| u.header.tx_position.block_number < new_window_start);
	existing.drain(..trim_at);
	existing.extend(extension);
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
		max_utxos: usize,
	) -> Result<ObservedUtxos, Box<dyn std::error::Error + Send + Sync>> {
		// Same-tip cache: if `current_tip` and `start_position` are both
		// unchanged, the Cardano window hasn't grown, so reuse the previous
		// result directly. (A `start_position` that advanced under the same tip
		// falls through to a recompute — the pallet consumes inherent data
		// all-or-nothing, so the previous-start case is the one that recurs.)
		if let Ok(guard) = self.last_observation.lock()
			&& let Some(last) = guard.as_ref()
			&& last.current_tip == current_tip
			&& last.start_position == *start_position
		{
			return Ok(last.result.clone());
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
		// serves through `tip_pos`. Refresh is separately clamped to the latest
		// stable db-sync block so proactive lookahead does not cache unstable
		// Cardano data.
		let snapshot_end_opt = self.snapshot_end_block.read().ok().and_then(|g| *g);
		let snapshot_start_opt = self.snapshot_start_block.read().ok().and_then(|g| *g);
		if let Some(snapshot_end_block) = snapshot_end_opt {
			// Refresh proactively when tip_pos is closing on the snapshot end.
			if tip_pos.block_number.saturating_add(REFRESH_THRESHOLD) >= snapshot_end_block {
				let target_end = tip_pos
					.block_number
					.saturating_add(REFRESH_THRESHOLD)
					.saturating_add(self.stability_margin);
				self.maybe_kick_refresh(start_position.block_number, target_end);
			}
			let tip_past_snapshot_end = tip_pos.block_number > snapshot_end_block;
			let start_below_snapshot_start = snapshot_start_opt
				.is_some_and(|ss| start_position.block_number < ss);
			if tip_past_snapshot_end || start_below_snapshot_start {
				log::debug!(
					"cNIGHT observation: query [{} .. {}] outside cache window [{:?} .. {}], delegating to DB",
					start_position.block_number, tip_pos.block_number, snapshot_start_opt, snapshot_end_block,
				);
				return self
					.db_fallback
					.get_utxos_up_to_capacity(
						config,
						start_position,
						current_tip,
						tx_capacity,
						max_utxos,
					)
					.await;
			}
		} else {
			// No snapshot end yet — cache hasn't been populated. Delegate while
			// we wait for the first refresh to complete.
			return self
				.db_fallback
				.get_utxos_up_to_capacity(
					config,
					start_position,
					current_tip,
					tx_capacity,
					max_utxos,
				)
				.await;
		}

		let end = tip_pos.increment();
		// Hold the read lock only for the (cheap) slice+copy of our window.
		// Readers share the lock, so they don't block each other; a concurrent
		// refresh's write lock waits for this copy to finish.
		let window: Vec<ObservedUtxo> = match self.all_events.read() {
			Ok(guard) => slice_range(&guard, start_position, &end).to_vec(),
			Err(_) => Vec::new(),
		};
		// Window holds the complete range, so `complete = true` — same inputs as
		// the db fallback, hence the same inherent.
		let result =
			truncate_to_tx_capacity(window, tx_capacity, max_utxos, true, start_position, end);

		if let Ok(mut guard) = self.last_observation.lock() {
			*guard = Some(LastObservation {
				start_position: start_position.clone(),
				current_tip: current_tip.clone(),
				result: result.clone(),
			});
		}

		Ok(result)
	}

	async fn get_utxos_v1(
		&self,
		config: &CNightAddresses,
		start_position: &CardanoPosition,
		current_tip: McBlockHash,
		tx_capacity: usize,
	) -> Result<ObservedUtxos, Box<dyn std::error::Error + Send + Sync>> {
		// v1 had no cache: derive directly against the pool so the inherent is
		// byte-identical to mainnet history regardless of this cache's state.
		crate::data_source::cnight_observation_v1::derive_inherent_v1(
			&self.pool,
			config,
			start_position,
			current_tip,
			tx_capacity,
		)
		.await
	}

	async fn get_utxos_v2(
		&self,
		config: &CNightAddresses,
		start_position: &CardanoPosition,
		current_tip: McBlockHash,
		tx_capacity: usize,
		max_utxos: usize,
	) -> Result<ObservedUtxos, Box<dyn std::error::Error + Send + Sync>> {
		// SKETCH: derive directly against the pool. The sliding-window cache can
		// later serve whole blocks into `select_one_block` as a pure-perf layer
		// without changing this output.
		crate::data_source::cnight_observation_v2::derive_inherent_v2(
			&self.pool,
			config,
			start_position,
			current_tip,
			tx_capacity,
			max_utxos,
		)
		.await
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
		let mut existing: Vec<_> = (10..30).map(|n| utxo(n, 0)).collect();
		let extension: Vec<_> = (30..35).map(|n| utxo(n, 0)).collect();
		slide_events(&mut existing, extension, 15);
		let block_numbers: Vec<u32> =
			existing.iter().map(|u| u.header.tx_position.block_number).collect();
		assert_eq!(block_numbers, (15..35).collect::<Vec<_>>());
	}

	#[test]
	fn slide_events_no_trim_when_start_below_existing() {
		let mut existing: Vec<_> = (10..15).map(|n| utxo(n, 0)).collect();
		let extension: Vec<_> = (15..18).map(|n| utxo(n, 0)).collect();
		slide_events(&mut existing, extension, 5);
		assert_eq!(existing.len(), 8);
		assert_eq!(existing[0].header.tx_position.block_number, 10);
	}

	#[test]
	fn slide_events_full_trim_when_start_above_existing() {
		let mut existing: Vec<_> = (10..15).map(|n| utxo(n, 0)).collect();
		let extension: Vec<_> = (20..25).map(|n| utxo(n, 0)).collect();
		slide_events(&mut existing, extension, 100);
		// Everything from `existing` is dropped; only extension survives.
		let block_numbers: Vec<u32> =
			existing.iter().map(|u| u.header.tx_position.block_number).collect();
		assert_eq!(block_numbers, vec![20, 21, 22, 23, 24]);
	}

	#[test]
	fn plan_refresh_contiguous_extends_and_trims_behind_follower() {
		// Window ends at 100, follower at 90: extend from 101, keep
		// window_size=30 blocks behind the follower.
		let (from, start) = plan_refresh(100, 90, 50, Some(90), 30);
		assert_eq!((from, start), (101, 60));
	}

	#[test]
	fn plan_refresh_contiguous_never_moves_start_backward() {
		// Existing start (80) is already ahead of follower - window_size (60).
		let (from, start) = plan_refresh(100, 90, 80, Some(90), 30);
		assert_eq!((from, start), (101, 80));
	}

	#[test]
	fn plan_refresh_contiguous_keeps_start_without_trim_anchor() {
		// follower_anchor == old_end + 1 is still contiguous, not a jump.
		let (from, start) = plan_refresh(100, 101, 50, None, 30);
		assert_eq!((from, start), (101, 50));
	}

	#[test]
	fn plan_refresh_jumps_forward_when_follower_past_window() {
		// Restart after a full sync: window still anchored at genesis
		// (old_end=99) while the runtime has processed up to block 570_000.
		// Pull and window both restart at the follower, not at genesis.
		let (from, start) = plan_refresh(99, 570_000, 0, None, 100_000);
		assert_eq!((from, start), (570_000, 570_000));
	}

	#[test]
	fn plan_refresh_jump_ignores_stale_trim_anchor() {
		// A stale last_observation must not pull the window start back
		// behind the jump target (which would claim coverage over a gap).
		let (from, start) = plan_refresh(99, 570_000, 0, Some(50), 100_000);
		assert_eq!((from, start), (570_000, 570_000));
	}

	#[test]
	fn slide_events_empty_extension_just_trims() {
		let mut existing: Vec<_> = (10..20).map(|n| utxo(n, 0)).collect();
		slide_events(&mut existing, vec![], 14);
		let block_numbers: Vec<u32> =
			existing.iter().map(|u| u.header.tx_position.block_number).collect();
		assert_eq!(block_numbers, (14..20).collect::<Vec<_>>());
	}

	/// `block_number`-th transaction, `utxo_index`-th UTXO within it. UTXOs that
	/// share `block_number` belong to the same Cardano transaction (one distinct
	/// `tx_position`).
	fn utxo_with_index(block_number: u32, utxo_index: u16) -> ObservedUtxo {
		let mut u = utxo(block_number, 0);
		u.header.utxo_index = UtxoIndexInTx(utxo_index);
		u
	}

	/// 50 transactions, 5 UTXOs each — distinct `tx_position` per transaction.
	fn fifty_txs_five_utxos() -> Vec<ObservedUtxo> {
		(0..50u32)
			.flat_map(|tx| (0..5u16).map(move |u| utxo_with_index(tx, u)))
			.collect()
	}

	/// The cNIGHT observation skip bug, fixed: a row-limited (incomplete) fetch
	/// must NEVER advance `next_cardano_position` to the tip. The rows between
	/// the last observed tx and the tip would be skipped forever, and a node
	/// that DID fetch them would build a different inherent (check_inherent
	/// split). Before the fix, "fewer than tx_capacity distinct txs" was treated
	/// as "range complete" and the cursor jumped to the tip regardless.
	#[test]
	fn incomplete_fetch_must_not_advance_to_tip() {
		// The range holds 50 txs but the fetch was row-limited to the first 40.
		let fetched: Vec<ObservedUtxo> = (0..40u32)
			.flat_map(|tx| (0..5u16).map(move |u| utxo_with_index(tx, u)))
			.collect();
		let tip = pos(100, 0);

		let obs = truncate_to_tx_capacity(
			fetched,
			/* tx_capacity */ 1000,
			/* max_utxos */ 100_000,
			/* complete */ false,
			&pos(0, 0),
			tip.clone(),
		);

		assert_ne!(obs.end, tip, "advanced to tip on an incomplete fetch -> skips unfetched txs");
		// The last fetched tx may be partial, so it is dropped; resume at the last
		// provably-whole tx (tx 38), never past observed data.
		assert_eq!(obs.end, pos(38, 0).increment());
		assert!(
			obs.utxos.iter().all(|u| u.header.tx_position < pos(39, 0)),
			"retained a tx at/after the truncation frontier",
		);
	}

	/// A complete fetch under both caps reports the whole range and advances to
	/// the tip — the steady-state case, and what both the in-memory cache slice
	/// and the LARGE_LIMIT db fetch produce for the same block (hence no
	/// cache-vs-fallback divergence).
	#[test]
	fn complete_fetch_advances_to_tip() {
		let events = fifty_txs_five_utxos();
		let tip = pos(100, 0);
		let obs =
			truncate_to_tx_capacity(events.clone(), 1000, 100_000, true, &pos(0, 0), tip.clone());
		assert_eq!(obs.end, tip);
		assert_eq!(obs.utxos.len(), events.len());
	}

	/// The UTXO envelope cap truncates at a whole-tx boundary: never
	/// mid-transaction (resuming would skip the rest of that tx's UTXOs) and
	/// never above the runtime acceptance bound.
	#[test]
	fn utxo_envelope_cap_truncates_at_whole_tx() {
		// 250 events. Cap 22 -> 4 whole txs (20 UTXOs); the 5th (would reach 25)
		// is held back.
		let obs = truncate_to_tx_capacity(
			fifty_txs_five_utxos(),
			1000,
			22,
			true,
			&pos(0, 0),
			pos(100, 0),
		);
		assert_eq!(obs.utxos.len(), 20, "did not cut on a whole-tx boundary");
		assert!(obs.utxos.len() <= 22);
		assert_eq!(obs.end, pos(3, 0).increment(), "must resume past the last whole tx");
	}

	/// The transaction-count cap admits at most `tx_capacity` whole transactions.
	#[test]
	fn tx_capacity_cap_truncates_at_whole_tx() {
		let obs = truncate_to_tx_capacity(
			fifty_txs_five_utxos(),
			10,
			100_000,
			true,
			&pos(0, 0),
			pos(100, 0),
		);
		let distinct: std::collections::BTreeSet<u32> =
			obs.utxos.iter().map(|u| u.header.tx_position.block_number).collect();
		assert_eq!(distinct.len(), 10);
		assert_eq!(obs.utxos.len(), 50, "10 txs x 5 UTXOs");
		assert_eq!(obs.end, pos(9, 0).increment());
	}
}
