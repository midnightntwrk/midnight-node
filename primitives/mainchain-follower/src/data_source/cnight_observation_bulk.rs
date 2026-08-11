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
//! Holds a contiguous window of observation events in memory, grouped by
//! Cardano transaction and sorted by position ([`CNightGroupedUtxos`]). The
//! cache starts empty: the first inherent query after
//! startup is served by the live db-backed source and kicks off a background
//! refresh anchored at the runtime's latest processed Cardano position — so a
//! node restarting after a full sync pulls only the window it needs, not
//! `[genesis, tip]`. Single-flight refreshes slide the window forward as the
//! chain advances (trimming behind the follower, extending toward the stable
//! tip). Queries outside the cached window delegate to the live source so the
//! node keeps importing.

use crate::data_source::candidates_data_source::observed_async_trait;
use crate::data_source::cnight_grouped::CNightGroupedUtxos;
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

/// Pull every cnight observation event in `[start, end]` (inclusive), grouped
/// by transaction and sorted by `tx_position`.
///
/// The result is **gap-free by construction**: each category query is
/// position-ordered and row-limited, so a query that hit `limit` covers its
/// category only up to a position frontier. The merged set is cut back to the
/// earliest such frontier (see [`merge_gap_free`]), and that cut is returned
/// as `Some(position)` — the point from which the caller's cursor must resume.
/// `None` means no query was row-limited and the whole requested range is
/// covered. Advancing the cursor past a returned cut would silently skip the
/// unfetched rows; at [`LARGE_LIMIT`] a cut is pathological and is logged at
/// error level.
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
) -> Result<(CNightGroupedUtxos, Option<CardanoPosition>), BulkPullError> {
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

	let mut categories: Vec<Vec<ObservedUtxo>> = Vec::with_capacity(4);
	let mut counts = (0usize, 0usize, 0usize, 0usize);
	if let Some(ident) = auth_token_ident {
		let v = data_source
			.get_registration_utxos(cardano_network, ident, &cfg.mapping_validator_address, &paged)
			.await?;
		counts.0 = v.len();
		categories.push(v);
	}
	let v = data_source
		.get_deregistration_utxos(cardano_network, &cfg.mapping_validator_address, &paged)
		.await?;
	counts.1 = v.len();
	categories.push(v);
	if let Some(ident) = cnight_ident {
		let v = data_source.get_asset_create_utxos(cardano_network, ident, &paged).await?;
		counts.2 = v.len();
		categories.push(v);
		let v = data_source.get_asset_spend_utxos(cardano_network, ident, &paged).await?;
		counts.3 = v.len();
		categories.push(v);
	}
	let (all, cut) = merge_gap_free(categories, limit);
	log::info!(
		target: "cnight::sliding-window",
		"bulk_pull [{}/{}, {}/{}] -> reg={} dereg={} create={} spend={} complete={} (auth_ident={:?} cnight_ident={:?})",
		start.block_number, start.tx_index_in_block,
		end.block_number, end.tx_index_in_block,
		counts.0, counts.1, counts.2, counts.3, cut.is_none(), auth_token_ident, cnight_ident,
	);
	if let Some(cut) = &cut {
		log::error!(
			target: "cnight::sliding-window",
			"bulk_pull hit the {limit}-row limit in [{}, {}] (reg={} dereg={} create={} spend={}); \
			 results are truncated to the covered prefix and the cursor will resume at {}/{}",
			start.block_number, end.block_number, counts.0, counts.1, counts.2, counts.3,
			cut.block_number, cut.tx_index_in_block,
		);
	}
	Ok((all, cut))
}

/// Merge the per-category query results into one gap-free grouped set.
///
/// Each element of `categories` is one category's rows, position-ascending
/// (the queries are `ORDER BY block_no, block_index ... LIMIT limit`). A
/// category that returned `limit` rows may have more behind the limit, so it
/// is only proven complete *below* the position of its last returned row (its
/// frontier) — and the frontier transaction itself may have been sliced
/// mid-tx by the row limit.
///
/// The merged set is therefore cut at the earliest frontier across all
/// row-limited categories, dropping the frontier tx too. Every event below
/// the cut is provably fetched for **all** categories, so the result carries
/// no hidden gaps: a category's truncation can never be masked by other
/// categories' events beyond its frontier. Returns the cut position
/// (`None` when no category was row-limited).
fn merge_gap_free(
	categories: Vec<Vec<ObservedUtxo>>,
	limit: usize,
) -> (CNightGroupedUtxos, Option<CardanoPosition>) {
	let mut cut: Option<CardanoPosition> = None;
	for rows in &categories {
		// The frontier is exclusive: the row limit may have sliced the tx at the
		// last row's position mid-tx, so that tx is dropped and the cursor
		// resumes exactly there.
		if rows.len() >= limit
			&& let Some(frontier) = rows.last().map(|u| u.header.tx_position.clone())
		{
			cut = Some(match cut {
				Some(c) if c < frontier => c,
				_ => frontier,
			});
		}
	}
	// One sort over the concatenated categories: `from_unsorted` merges events
	// sharing a transaction position whichever category they came from, so
	// per-category accumulation (and its repeated re-sorts) buys nothing.
	let mut all = CNightGroupedUtxos::from_unsorted(categories.into_iter().flatten().collect());
	if let Some(cut) = &cut {
		all.truncate_at_position(cut);
	}
	(all, cut)
}

/// Build the observation inherent from a **gap-free** grouped event set
/// covering `[start, covered_end)` (what `bulk_pull` returns — a row-limited
/// pull is already cut back to its proven-complete prefix, with `covered_end`
/// the cut). Caps at `tx_capacity` whole transactions and `max_utxos` UTXOs
/// (the runtime `process_tokens` envelope).
///
/// Consensus invariants:
/// - Transactions are admitted whole, so `end` lands on a tx boundary; resuming
///   there cannot skip a counted tx's UTXOs.
/// - The cursor reaches `covered_end` ONLY when every event was admitted;
///   otherwise it stops just past the last admitted tx — safe because the set
///   is gap-free, so everything not admitted is at or ahead of that boundary.
/// - The cursor never advances past an event we did not admit, and never at all
///   when nothing was admitted (see [`boundary_after`]).
///
/// Every node feeds this the same gap-free range and the same `max_utxos`, so
/// there is no fetch-size input left to disagree on — inherents are
/// byte-identical.
pub fn truncate_to_tx_capacity(
	events: CNightGroupedUtxos,
	tx_capacity: usize,
	max_utxos: usize,
	start_position: &CardanoPosition,
	covered_end: CardanoPosition,
) -> ObservedUtxos {
	// Whole transactions only, up to both caps; the lone-oversized-tx admission
	// lives in `take_envelope_prefix`.
	let (admitted, capped) = events.take_envelope_prefix(tx_capacity, max_utxos);

	let end = if capped { boundary_after(&admitted, start_position) } else { covered_end };

	ObservedUtxos { start: start_position.clone(), end, utxos: admitted.into_utxos() }
}

/// Position just past the last accepted transaction — or `start` **unchanged**
/// when nothing was accepted, so the cursor never moves past unobserved data.
///
/// Accepting nothing takes a misconfigured `tx_capacity == 0` (the caps are
/// only checked against a non-empty admission otherwise). Incrementing there
/// would skip the tx sitting at the cursor permanently, so we hold and log: a
/// stalled cNIGHT cursor is visible and recoverable, silently dropped
/// mint/burn events are neither.
fn boundary_after(
	admitted: &CNightGroupedUtxos,
	start_position: &CardanoPosition,
) -> CardanoPosition {
	match admitted.last_position() {
		Some(last) => last.clone().increment(),
		None => {
			log::error!(
				target: "cnight::observation",
				"no whole transaction fit the acceptance envelope at cardano {}/{}; \
				 holding the cursor (check CardanoTxCapacityPerBlock and UtxoPerTxOverestimate)",
				start_position.block_number, start_position.tx_index_in_block,
			);
			start_position.clone()
		},
	}
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

/// A `MidnightCNightObservationDataSource` backed by an in-memory grouped
/// event window built once at startup, with an async sliding-window refresh
/// and a live db-backed fallback for queries past the current horizon.
pub struct BulkCachedCNightObservationDataSource {
	/// The cached window, grouped by transaction and sorted. Readers take the
	/// read lock for the (cheap) slice+copy of their range; the refresh task
	/// takes the write lock briefly to mutate the window in place (trim the
	/// front, append the extension).
	all_events: Arc<std::sync::RwLock<CNightGroupedUtxos>>,
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
	pub fn new(events: CNightGroupedUtxos, config: BulkCacheConfig) -> Self {
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
	all_events: Arc<std::sync::RwLock<CNightGroupedUtxos>>,
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
		// Whole multi-block window, so `LARGE_LIMIT`; a cut here is pathological
		// (bulk_pull error-logs it). If it does happen, claim coverage only
		// through the last *whole* block actually pulled: the cut block is
		// partial, so drop its events and let the next refresh re-pull it from
		// scratch — a shorter window is benign, a window with a hidden gap is a
		// consensus split waiting to be served.
		let (mut extension, cut) =
			bulk_pull(&self.pool, &self.cnight_addresses, &start, &end, LARGE_LIMIT).await?;
		let covered_end = match &cut {
			None => target_end,
			Some(cut_pos) => {
				extension
					.truncate_at_position(&CardanoPosition::min_for_block(cut_pos.block_number));
				cut_pos.block_number.saturating_sub(1)
			},
		};
		{
			let mut events_guard =
				self.all_events.write().map_err(|e| format!("all_events write poisoned: {e}"))?;
			// Slide the window in place: trim the front, then append the
			// extension, which sorts strictly after every retained tx
			// (`append` checks that claim).
			events_guard.trim_before_block(new_window_start);
			events_guard.append(extension);
		}
		*self
			.snapshot_start_block
			.write()
			.map_err(|e| format!("snapshot_start_block write poisoned: {e}"))? = Some(new_window_start);
		*self
			.snapshot_end_block
			.write()
			.map_err(|e| format!("snapshot_end_block write poisoned: {e}"))? = Some(covered_end);
		log::info!(
			target: "cnight::sliding-window",
			"refresh done: window now [{new_window_start}, {covered_end}] (took {:?})",
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
		let window: CNightGroupedUtxos = match self.all_events.read() {
			Ok(guard) => guard.slice_range(start_position, &end),
			Err(_) => CNightGroupedUtxos::default(),
		};
		// The window is gap-free through `snapshot_end_block` (a truncated
		// refresh shortens its coverage claim instead of storing a gap), so the
		// slice covers the whole queried range — same inputs as the db
		// fallback, hence the same inherent.
		let result = truncate_to_tx_capacity(window, tx_capacity, max_utxos, start_position, end);

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
	use crate::{
		ObservedUtxo, ObservedUtxoData, ObservedUtxoHeader, RegistrationData, UtxoIndexInTx,
	};
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

	// The window-mechanics tests (slice/trim/append) live with
	// `CNightGroupedUtxos` in `cnight_grouped.rs`.

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

	/// Group raw test events the way `bulk_pull` would.
	fn grouped(events: Vec<ObservedUtxo>) -> CNightGroupedUtxos {
		CNightGroupedUtxos::from_unsorted(events)
	}

	/// No category hit its row limit: nothing is cut and the whole merge is
	/// covered (`cut == None`).
	#[test]
	fn merge_gap_free_without_row_limit_covers_everything() {
		let a: Vec<_> = (0..3u32).map(|b| utxo(b, 0)).collect();
		let b = vec![utxo(1, 1)];
		let (merged, cut) = merge_gap_free(vec![a, b], 100);
		assert!(cut.is_none());
		assert_eq!(merged.num_utxos(), 4);
		assert_eq!(merged.num_transactions(), 4);
	}

	/// The review scenario (Lech): one category (A) is row-limited while
	/// another (B) has events beyond A's frontier. B's later events must not
	/// mask A's truncation — the merge cuts everything at A's frontier, so the
	/// gap in A can never be silently skipped over.
	#[test]
	fn category_truncation_cannot_be_masked_by_later_events() {
		// A: 10 single-UTXO txs at blocks 0..10, exactly hitting limit = 10,
		// so its frontier is its last returned row (block 9).
		let a: Vec<_> = (0..10u32).map(|b| utxo(b, 0)).collect();
		// B: one event below the frontier, one far beyond it.
		let b = vec![utxo(3, 1), utxo(50, 0)];
		let (merged, cut) = merge_gap_free(vec![a, b], 10);
		let cut = cut.expect("A hit the row limit");
		assert_eq!(cut, pos(9, 0), "cut at A's frontier");
		// Kept: A's blocks 0..9 (9 events) + B's (3,1). Dropped: A's frontier
		// tx (possibly mid-sliced) and B's block-50 event beyond the frontier.
		assert_eq!(merged.num_utxos(), 10);
		assert!(merged.last_position().unwrap() < &cut);
		// The cursor resumes exactly at the cut — A's unfetched events are
		// ahead of it and get re-pulled by the next inherent.
		let obs = truncate_to_tx_capacity(merged, 1000, 100_000, &pos(0, 0), cut.clone());
		assert_eq!(obs.end, cut);
	}

	/// The cNIGHT observation skip bug, fixed structurally: a row-limited
	/// fetch is cut back to its proven-complete prefix and the cursor resumes
	/// at the cut — NEVER at the tip. The rows between the cut and the tip
	/// would otherwise be skipped forever, and a node that DID fetch them
	/// would build a different inherent (check_inherent split).
	#[test]
	fn row_limited_fetch_must_not_advance_to_tip() {
		// The range holds 50 txs (5 UTXOs each) but the fetch was row-limited
		// to the first 200 rows = 40 txs, the last of which may be mid-sliced.
		let fetched: Vec<ObservedUtxo> = (0..40u32)
			.flat_map(|tx| (0..5u16).map(move |u| utxo_with_index(tx, u)))
			.collect();
		let tip = pos(100, 0);

		let (merged, cut) = merge_gap_free(vec![fetched], 200);
		let cut = cut.expect("the fetch hit the row limit");
		assert_eq!(cut, pos(39, 0), "frontier = position of the last fetched row");
		assert_eq!(merged.num_utxos(), 39 * 5, "the frontier tx is dropped whole");

		let obs = truncate_to_tx_capacity(merged, 1000, 100_000, &pos(0, 0), cut.clone());
		assert_ne!(obs.end, tip, "advanced to tip on a row-limited fetch -> skips unfetched txs");
		assert_eq!(obs.end, cut, "resume exactly at the frontier");
		assert!(
			obs.utxos.iter().all(|u| u.header.tx_position < cut),
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
		let obs = truncate_to_tx_capacity(
			grouped(events.clone()),
			1000,
			100_000,
			&pos(0, 0),
			tip.clone(),
		);
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
			grouped(fifty_txs_five_utxos()),
			1000,
			22,
			&pos(0, 0),
			pos(100, 0),
		);
		assert_eq!(obs.utxos.len(), 20, "did not cut on a whole-tx boundary");
		assert!(obs.utxos.len() <= 22);
		assert_eq!(obs.end, pos(3, 0).increment(), "must resume past the last whole tx");
	}

	/// Nothing admitted ⟹ the cursor does not move. Here a row limit sliced
	/// the sole fetched tx mid-transaction: the merge cut drops the partial tx
	/// entirely and the cursor holds at its position, never stepping over it.
	#[test]
	fn row_limited_lone_tx_holds_the_cursor() {
		// One tx (5 UTXOs sharing a position) at the cursor, fetch row-limited.
		let events: Vec<_> = (0..5u16).map(|u| utxo_with_index(7, u)).collect();
		let start = pos(7, 0);
		let (merged, cut) = merge_gap_free(vec![events], 5);
		assert!(merged.is_empty(), "a possibly-partial tx must not be admitted");
		let cut = cut.expect("the fetch hit the row limit");
		let obs = truncate_to_tx_capacity(merged, 1000, 100_000, &start, cut);
		assert!(obs.utxos.is_empty());
		assert_eq!(obs.end, start, "cursor stepped over an unobserved transaction");
	}

	/// Same rule via the other route into "nothing admitted": a misconfigured
	/// `tx_capacity == 0`. Observation stalls loudly instead of the cursor
	/// walking the chain one tx per block, dropping every event as it goes.
	#[test]
	fn zero_tx_capacity_holds_the_cursor() {
		let start = pos(0, 0);
		let obs = truncate_to_tx_capacity(
			grouped(fifty_txs_five_utxos()),
			0,
			100_000,
			&start,
			pos(100, 0),
		);
		assert!(obs.utxos.is_empty());
		assert_eq!(obs.end, start, "cursor advanced with zero capacity — events lost");
	}

	/// The transaction-count cap admits at most `tx_capacity` whole transactions.
	#[test]
	fn tx_capacity_cap_truncates_at_whole_tx() {
		let obs = truncate_to_tx_capacity(
			grouped(fifty_txs_five_utxos()),
			10,
			100_000,
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
