// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Ledger GC worker: reclaims tagged-root wrappers whose block position is
//! provably dead, then runs an incremental arena `gc`.
//!
//! Governing invariant: never release a root that may still be needed. Every
//! uncertain or erroring path keeps. Over-release panics at the next storage
//! flush, so a release list is built from one slice's scan, consumed once and
//! never retried.
//!
//! # Shape
//!
//! The worker is stateless across slices apart from a two-slice debounce set and a
//! few flags. Every finality notification runs one slice (see [`reclaim_slice`]):
//!
//! 0. gate: a multi-block migration in flight (or an unreadable cursor) suspends
//!    *release*; scan, pin and arena gc still run. Major sync is deliberately NOT
//!    a gate: a resync from genesis must release tips as finality advances,
//!    otherwise ledger storage holds every tip of the whole chain until the sync
//!    ends (the pre-GC bloat, as a peak);
//! 1. scan (`gc::scan`): every root with its tag and persist count; the first root
//!    that is not a wrapper stops the worker (see *Legacy roots*);
//! 2. liveness, with no lock held: resolve each tag to its parent block, height
//!    gate, child-state gate, two-slice debounce;
//! 3. pin every scanned wrapper, run one arena gc slice (optional), drop the pins;
//! 4. release (`gc::release_to_zero`, this slice's counts, zero reads) — after the
//!    sweep, because a staged `unpersist` dirties the write cache and the sweep
//!    refuses a dirty cache;
//! 5. metrics.
//!
//! Per-slice work is proportional to the root set, which on a pruned node is the
//! pruning window plus forks (a few hundred roots): the scan reads each root once,
//! liveness does one header lookup per root and `children`/`have_state_at` only
//! for roots past the height gate. A very large `--state-pruning` window makes
//! this proportionally more expensive; the hold-time warnings show it.
//!
//! # Pins during the sweep
//!
//! The storage crate's sweep culls every DB node that is not in its mark set and
//! never consults root counts, so a wrapper flushed in the middle of a multi-slice
//! sweep must be marked before the sweep cursor moves on. storage-core guarantees
//! that itself: every flush that changes a root count forces a root rescan, and the
//! next `gc()` call greys and marks every unmarked root before it sweeps again
//! (`gc::wrapper_flushed_mid_sweep_survives_without_pins` is the regression test;
//! the guarantee is a `gc-v1` feature detail a dependency bump could drop). The
//! worker additionally pins every scanned wrapper across its gc call (`live_inserts`
//! is the other root source the sweep consults), which costs one lazy `Sp` per
//! wrapper in the pruning window and makes the slice safe even without that rescan.
//!
//! # Legacy roots
//!
//! Roots written before tagged roots are not wrappers and can never be released,
//! and an upgraded-in-place database holds the whole chain's history of them. GC
//! does not run on such a database. The decision is the META-column marker the
//! database layer writes when it creates a fresh ledger storage
//! (`custom_parity_db::ledger_gc_tagged_from_genesis`): one point read at startup,
//! never a root iteration, so the size of a legacy database is irrelevant. As a
//! defence the startup probe and every slice also stop the worker at the first
//! root that is not a wrapper. Either way: a resync message and
//! `ledger_gc_suspended{reason="legacy_roots"}`; the node runs exactly as without
//! GC until its ledger storage is resynced.
//!
//! # Sweep self-disable
//!
//! A panic inside the storage crate's sweep (for example a DB row it cannot decode)
//! disables the sweep for the rest of the process; release continues.
//!
//! # Liveness rule per pruning mode
//!
//! On a pruned node Substrate's own state pruning decides: a wrapper is dead once
//! its block is below the pruning window *and* no child of its parent block still
//! has state ([`Liveness::PrunedState`]). Modes with unbounded wrapper history
//! (archive pruning, unbounded retention) run no worker by default, because every
//! canonical block keeps its state forever and that gate would keep every root.
//! `--ledger-state-pruning <N>` opts such a node in with an explicit ledger window:
//! release is then by height only ([`Liveness::HeightOnly`], dead iff the block is
//! below `finalized - N`), stale forks included. Anything that loads the ledger
//! state of a block below that window (runtime APIs, warp ledger-sync requests)
//! fails on that node; the indexer no longer needs such states (it validates
//! against `pallet_midnight::StateKey`).

use std::{
	collections::HashSet,
	sync::{Arc, Mutex},
	time::Duration,
};

use futures::{FutureExt, StreamExt};
use log::{debug, error, info, warn};
use midnight_node_ledger::gc::{self, LegacyRoot, TaggedRootInfo};
use midnight_primitives_ledger::{LedgerMetrics, LedgerMetricsExt};
use sc_client_api::{Backend, BlockchainEvents, StorageProvider};
use sc_client_db::PruningMode;
use sc_service::SpawnTaskHandle;
use sp_blockchain::{Backend as BlockchainBackend, HeaderBackend};
use sp_core::{H256, storage::StorageKey};
use sp_crypto_hashing::twox_128;
use sp_runtime::{SaturatedConversion, traits::Header as HeaderT};

use crate::service::{FullBackend, FullClient};

const LOG_TARGET: &str = "midnight::ledger_gc";

/// Arena sweep budget per slice. 25 ms and 100 ms fell behind at ~10 tx/block on the
/// perfnet A/B of the storage bundle (garbage ~130 stored nodes per block); 200 ms kept
/// the ledger flat through a 14 h load with no import penalty.
const DEFAULT_SLICE_MS: u64 = 200;
/// Hold-time warning threshold for the scan and release holds.
const HOLD_WARN: Duration = Duration::from_millis(50);

const ENV_SLICE_MS: &str = "MIDNIGHT_LEDGER_GC_SLICE_MS";
const ENV_ARENA_SWEEP: &str = "MIDNIGHT_LEDGER_GC_ARENA_SWEEP";
/// Maximum nodes one sweep slice may delete; `0` or unset = no cap. Experiment knob:
/// bounds the burst of deletions parity-db enacts after a backlog (perfnet saw the disk
/// 100% busy for 6–30 s while a backlog drained at ~3,000 writes/s).
const ENV_MAX_DELETES: &str = "MIDNIGHT_LEDGER_GC_MAX_DELETES";

/// Which gates decide that a tagged root's block position is dead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Liveness {
	/// Height gate plus child-state gate: Substrate's state pruning is the authority
	/// (pruned nodes).
	PrunedState,
	/// Height gate only, against an explicit ledger window (`--ledger-state-pruning`
	/// on a node that keeps Substrate state for every block).
	HeightOnly,
}

/// The release window of one worker: lag in blocks and the liveness rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
	pub lag: u32,
	pub liveness: Liveness,
}

/// Worker configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GcConfig {
	/// Time budget of one arena gc slice.
	pub slice: Duration,
	/// Run the arena mark/sweep (escape hatch: `false` = tip release only).
	pub arena_sweep: bool,
	/// Cap on nodes deleted per sweep slice (`None` = unbounded).
	pub max_deletes: Option<usize>,
	/// Release lag (the `--state-pruning` window, or the explicit
	/// `--ledger-state-pruning` window); `None` = no worker.
	pub lag: Option<u32>,
	/// Which gates decide that a wrapper is dead.
	pub liveness: Liveness,
	/// The ledger storage was created by a node that tags every root (the META
	/// marker written on first open); `false` = legacy roots, no worker.
	pub tagged_from_genesis: bool,
}

fn env_parse<T: std::str::FromStr>(name: &str, default: T) -> T {
	match std::env::var(name) {
		Ok(v) => v.trim().parse().unwrap_or_else(|_| {
			warn!(target: LOG_TARGET, "⚠️  Ignoring unparsable {name}={v:?}; using the default");
			default
		}),
		Err(_) => default,
	}
}

fn env_bool(name: &str, default: bool) -> bool {
	match std::env::var(name) {
		Ok(v) => match v.trim() {
			"1" => true,
			"0" => false,
			_ => {
				warn!(target: LOG_TARGET, "⚠️  Ignoring unparsable {name}={v:?}; using the default");
				default
			},
		},
		Err(_) => default,
	}
}

/// Release window for the configured pruning mode and the optional explicit
/// ledger window (`--ledger-state-pruning`). `Err` is a configuration the node
/// must refuse; check it before any service task is spawned, otherwise the
/// reason is lost in the shutdown of the half-built service.
///
/// A `None` flag does not mean archive: sc-client-db then prunes at
/// `PruningMode::default()` (constrained, 256) on a fresh DB, but reuses the
/// stored mode on an existing one, which `requires_full_sync()` reveals as
/// archive. Without the explicit window, `Constrained(None)` (unbounded
/// retention) and both archive modes run no worker. With it, those modes release
/// by height only; a constrained node keeps the child-state gate and must not set
/// a ledger window smaller than its state window (Substrate would then hold state
/// for blocks whose ledger root is gone, and every host call there would fail).
pub fn window_for(
	state_pruning: &Option<PruningMode>,
	backend_requires_full_sync: bool,
	ledger_window: Option<u32>,
) -> Result<Option<Window>, String> {
	let mode = match state_pruning {
		Some(mode) => Some(mode.clone()),
		None if backend_requires_full_sync => None,
		None => Some(PruningMode::default()),
	};
	let state_window = match mode {
		Some(PruningMode::Constrained(c)) => c.max_blocks,
		Some(PruningMode::ArchiveCanonical | PruningMode::ArchiveAll) | None => None,
	};
	Ok(match (state_window, ledger_window) {
		(Some(state), Some(ledger)) if ledger < state => {
			return Err(format!(
				"--ledger-state-pruning {ledger} is smaller than the state pruning window \
				 {state}; the ledger window must be at least the state window"
			));
		},
		(Some(_), Some(ledger)) => Some(Window { lag: ledger, liveness: Liveness::PrunedState }),
		(Some(state), None) => Some(Window { lag: state, liveness: Liveness::PrunedState }),
		(None, Some(ledger)) => Some(Window { lag: ledger, liveness: Liveness::HeightOnly }),
		(None, None) => None,
	})
}

impl GcConfig {
	/// Build the config from the pruning flags and the `MIDNIGHT_LEDGER_GC_*`
	/// environment overrides. `Err` is a configuration the node must refuse to
	/// start with (see [`window_for`]).
	pub fn from_env(
		state_pruning: &Option<PruningMode>,
		backend_requires_full_sync: bool,
		ledger_window: Option<u32>,
		tagged_from_genesis: bool,
	) -> Result<Self, String> {
		let window = window_for(state_pruning, backend_requires_full_sync, ledger_window)?;
		Ok(Self {
			slice: Duration::from_millis(env_parse(ENV_SLICE_MS, DEFAULT_SLICE_MS)),
			arena_sweep: env_bool(ENV_ARENA_SWEEP, true),
			max_deletes: match env_parse::<usize>(ENV_MAX_DELETES, 0) {
				0 => None,
				n => Some(n),
			},
			lag: window.map(|w| w.lag),
			liveness: window.map(|w| w.liveness).unwrap_or(Liveness::PrunedState),
			tagged_from_genesis,
		})
	}
}

/// Storage key of `pallet_migrations::Cursor` (`MultiBlockMigrations::Cursor`).
pub fn mbm_cursor_key() -> Vec<u8> {
	let mut key = twox_128(b"MultiBlockMigrations").to_vec();
	key.extend_from_slice(&twox_128(b"Cursor"));
	key
}

/// Read-only chain queries the liveness decision needs.
pub trait ChainView {
	/// Number of the block with this hash; `Ok(None)` if unknown.
	fn block_number(&self, hash: &H256) -> Result<Option<u32>, String>;
	/// Known children of `hash`.
	fn children(&self, hash: &H256) -> Result<Vec<H256>, String>;
	/// Whether Substrate still holds state for `child` at `number`.
	fn have_state_at(&self, child: &H256, number: u32) -> bool;
}

struct ClientView<'a> {
	client: &'a FullClient,
	backend: &'a FullBackend,
}

impl ChainView for ClientView<'_> {
	fn block_number(&self, hash: &H256) -> Result<Option<u32>, String> {
		self.client
			.header(*hash)
			.map(|h| h.map(|h| (*h.number()).saturated_into::<u32>()))
			.map_err(|e| e.to_string())
	}
	fn children(&self, hash: &H256) -> Result<Vec<H256>, String> {
		self.backend.blockchain().children(*hash).map_err(|e| e.to_string())
	}
	fn have_state_at(&self, child: &H256, number: u32) -> bool {
		self.backend.have_state_at(*child, number.saturated_into())
	}
}

/// Cross-slice worker state (process lifetime, in memory only). Nothing here is a
/// count source; a restart reaches the same verdicts from the database alone.
#[derive(Debug, Default)]
pub struct WorkerState {
	/// Wrappers that passed the gates in the previous slice (debounce).
	pub previous_candidates: HashSet<Vec<u8>>,
	/// Wrappers whose unknown parent or malformed tag was already logged; pruned
	/// every slice to the wrappers that are still unknown.
	pub logged_unknown: HashSet<Vec<u8>>,
	/// Whether the last slice saw a migration in flight.
	pub mbm_suspended: bool,
	/// Why the arena sweep is off for the rest of the process, if it is:
	/// `"sweep_panic"` (a sweep panicked, e.g. the storage crate's `scan` failed to
	/// decode a DB row). Tip release continues; the released tips are reclaimed by
	/// a later process's sweep.
	pub sweep_off: Option<&'static str>,
}

impl WorkerState {
	pub fn new() -> Self {
		Self::default()
	}
}

/// Whether the arena sweep runs in this slice.
pub fn sweep_allowed(state: &WorkerState, config: &GcConfig) -> bool {
	config.arena_sweep && state.sweep_off.is_none()
}

/// Verdicts of one slice.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Classified {
	/// `(wrapper, count)` to release; counts are from this slice's scan.
	pub release: Vec<(Vec<u8>, u32)>,
	/// Wrappers that passed the height and child-state gates this slice.
	pub candidates: usize,
	/// Wrappers kept because tag/parent/children were unreadable or malformed.
	pub kept_unknown: usize,
}

/// Pure liveness decision over one scan.
///
/// `state.mbm_suspended` evaluates nothing and clears the debounce set, so a resumed
/// worker needs two fresh slices before its first release.
pub fn classify(
	finalized: u32,
	window: Window,
	roots: &[TaggedRootInfo],
	state: &mut WorkerState,
	view: &impl ChainView,
) -> Classified {
	let mut out = Classified::default();
	let mut candidates: HashSet<Vec<u8>> = HashSet::new();
	let mut unknown: HashSet<Vec<u8>> = HashSet::new();
	let evaluate = !state.mbm_suspended;

	for root in roots {
		if root.tag.len() != 32 {
			out.kept_unknown += 1;
			unknown.insert(root.wrapper.clone());
			if state.logged_unknown.insert(root.wrapper.clone()) {
				debug!(target: LOG_TARGET, "tagged root with a malformed tag; keeping");
			}
			continue;
		}
		if !evaluate || root.count == 0 {
			continue;
		}
		let parent_hash = H256::from_slice(&root.tag);
		let parent_number = match view.block_number(&parent_hash) {
			Ok(Some(n)) => n,
			Ok(None) => {
				out.kept_unknown += 1;
				unknown.insert(root.wrapper.clone());
				if state.logged_unknown.insert(root.wrapper.clone()) {
					debug!(
						target: LOG_TARGET,
						"parent {parent_hash:?} of a tagged root is unknown; keeping"
					);
				}
				continue;
			},
			Err(e) => {
				out.kept_unknown += 1;
				debug!(target: LOG_TARGET, "header lookup for {parent_hash:?} failed ({e}); keeping");
				continue;
			},
		};
		// The tip belongs to height n = parent + 1. Dead iff n < finalized - lag,
		// i.e. n + lag < finalized (computed in u64, no overflow).
		let n = u64::from(parent_number) + 1;
		if n + u64::from(window.lag) >= u64::from(finalized) {
			continue;
		}
		// Child-state gate: on a pruned node Substrate's pruning is the authority.
		// With an explicit ledger window on a node that keeps every state the gate
		// would keep every canonical root forever, so the height gate alone decides.
		if window.liveness == Liveness::PrunedState {
			let children = match view.children(&parent_hash) {
				Ok(c) => c,
				Err(e) => {
					debug!(target: LOG_TARGET, "children({parent_hash:?}) failed ({e}); keeping");
					out.kept_unknown += 1;
					continue;
				},
			};
			if children
				.iter()
				.any(|c| view.have_state_at(c, u32::try_from(n).unwrap_or(u32::MAX)))
			{
				continue;
			}
		}
		out.candidates += 1;
		if state.previous_candidates.contains(&root.wrapper) {
			out.release.push((root.wrapper.clone(), root.count));
		}
		candidates.insert(root.wrapper.clone());
	}

	state.previous_candidates = candidates;
	state.logged_unknown.retain(|h| unknown.contains(h));
	out
}

type SharedMetrics = Arc<Mutex<Option<LedgerMetrics>>>;

/// Run `f` against the shared ledger metrics; a no-op when metrics are disabled.
/// `LedgerMetricsExt` locks the mutex only for the duration of each update.
fn with_metrics(metrics: &SharedMetrics, f: impl FnOnce(&mut LedgerMetricsExt)) {
	f(&mut LedgerMetricsExt::new(metrics.clone()));
}

/// Whether a multi-block migration may be in flight at the finalized block.
/// Present cursor or read error both count as "in flight".
fn mbm_in_flight(client: &FullClient, finalized_hash: H256) -> bool {
	match client.storage(finalized_hash, &StorageKey(mbm_cursor_key())) {
		Ok(None) => false,
		Ok(Some(_)) => true,
		Err(e) => {
			debug!(target: LOG_TARGET, "MBM cursor unreadable ({e}); treating as in flight");
			true
		},
	}
}

/// One root scan with its hold logged and recorded.
fn timed_scan(metrics: &SharedMetrics) -> Result<Vec<TaggedRootInfo>, LegacyRoot> {
	let (scan, hold) = gc::scan();
	if hold > HOLD_WARN {
		warn!(target: LOG_TARGET, "⏱️  Ledger root scan held the arena for {hold:?}");
	}
	with_metrics(metrics, |m| m.observe_gc_phase("scan", hold.as_secs_f64()));
	scan
}

/// Log and record that the worker will not run because of a legacy root.
fn refuse_legacy(metrics: &SharedMetrics) {
	error!(
		target: LOG_TARGET,
		"🛑 Ledger GC disabled: the ledger storage holds roots that predate tagged roots and \
		 can never be reclaimed. Resync the ledger storage to enable it."
	);
	with_metrics(metrics, |m| m.set_gc_suspended("legacy_roots", true));
}

/// Why a slice asked the worker to stop.
#[derive(Debug, PartialEq, Eq)]
enum Stop {
	/// The scan hit a root that is not a wrapper.
	LegacyRoot,
}

/// One reclaim slice. See the module docs for the step list.
fn reclaim_slice(
	client: &FullClient,
	backend: &FullBackend,
	metrics: &SharedMetrics,
	config: &GcConfig,
	lag: u32,
	state: &mut WorkerState,
) -> Result<(), Stop> {
	// Step 0: gate. (Major sync is not one: see the module docs.)
	let info = client.info();
	let finalized: u32 = info.finalized_number.saturated_into();

	{
		let suspended = mbm_in_flight(client, info.finalized_hash);
		if suspended != state.mbm_suspended {
			if suspended {
				info!(
					target: LOG_TARGET,
					"⏸️  Multi-block migration in flight; ledger root release suspended"
				);
			} else {
				info!(target: LOG_TARGET, "▶️  Multi-block migrations idle; ledger root release resumed");
			}
		}
		state.mbm_suspended = suspended;
		with_metrics(metrics, |m| m.set_gc_suspended("mbm", suspended));
	}

	// Step 1: scan.
	let wrappers = timed_scan(metrics).map_err(|LegacyRoot| Stop::LegacyRoot)?;
	with_metrics(metrics, |m| m.set_gc_tagged_roots("total", wrappers.len() as u64));

	// Step 2: liveness, no lock held.
	let view = ClientView { client, backend };
	let window = Window { lag, liveness: config.liveness };
	let verdict = classify(finalized, window, &wrappers, state, &view);
	with_metrics(metrics, |m| m.set_gc_tagged_roots("candidates", verdict.candidates as u64));

	// Step 3: pin, then arena gc. The sweep runs BEFORE the release: a staged
	// `unpersist` is a pending write-cache entry, and `collect_garbage` refuses to
	// sweep a dirty cache. Releasing first would skip the sweep in every slice that
	// releases something, which on a pruned node is every slice.
	if sweep_allowed(state, config) {
		let hashes: Vec<Vec<u8>> = wrappers.iter().map(|r| r.wrapper.clone()).collect();
		let pins = gc::pin_roots(&hashes);
		with_metrics(metrics, |m| m.set_gc_tagged_roots("pinned", pins.len() as u64));
		match guarded_sweep(config.slice, config.max_deletes) {
			Some(Ok((culled, hold))) => {
				if hold > config.slice * 2 {
					warn!(target: LOG_TARGET, "⏱️  Arena gc held the arena for {hold:?}");
				}
				with_metrics(metrics, |m| {
					m.observe_gc_phase("arena_gc", hold.as_secs_f64());
					m.inc_gc_culled(culled as u64);
				});
				if culled > 0 {
					debug!(target: LOG_TARGET, "🗑️  Arena gc culled {culled} node(s) in {hold:?}");
				}
			},
			Some(Err(e)) => debug!(target: LOG_TARGET, "⏸️  Arena gc deferred: {e}"),
			None => {
				state.sweep_off = Some("sweep_panic");
				with_metrics(metrics, |m| m.set_gc_suspended("sweep_panic", true));
				error!(
					target: LOG_TARGET,
					"💥 Arena gc slice panicked; arena sweep disabled for this process \
					 (tip release continues; released tips are reclaimed by a later sweep)"
				);
			},
		}
		drop(pins);
	}

	// Step 4: release (the list is consumed once and never retried). Durable at the
	// next block flush; the next sweep that rescans its roots reclaims the tips.
	if !verdict.release.is_empty() {
		let (released, hold) = gc::release_to_zero(&verdict.release);
		if hold > HOLD_WARN {
			warn!(target: LOG_TARGET, "⏱️  Ledger release held the arena for {hold:?}");
		}
		with_metrics(metrics, |m| {
			m.observe_gc_phase("release", hold.as_secs_f64());
			m.inc_gc_released(released as u64);
		});
		if released > 0 {
			info!(target: LOG_TARGET, "🧹 Released {released} dead ledger tip(s)");
		}
	}
	Ok(())
}

/// Run one arena gc slice, converting a panic inside the storage crate into
/// `None`. The arena mutex and the `RefCell` borrow are released on unwind and
/// the sweep's resume handle is only advanced after a batch succeeds, so a
/// panicking batch deletes nothing; retrying it every finality notification
/// would only repeat the panic, hence the caller disables the sweep.
///
/// The node's panic hook (`sp_panic_handler`) exits the process from inside the hook
/// unless the panicking thread holds an [`sp_panic_handler::AbortGuard`] that forces
/// unwinding, so every `catch_unwind` here holds one; without it the catch never runs.
fn guarded_sweep(
	slice: Duration,
	max_deletes: Option<usize>,
) -> Option<Result<(usize, Duration), String>> {
	let _unwind = sp_panic_handler::AbortGuard::force_unwind();
	std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
		gc::collect_garbage(slice, max_deletes)
	}))
	.ok()
}

async fn run(
	client: Arc<FullClient>,
	backend: Arc<FullBackend>,
	metrics: SharedMetrics,
	config: GcConfig,
	lag: u32,
) {
	let mut finality = client.finality_notification_stream();
	let mut state = WorkerState::new();

	loop {
		let slice_state = std::mem::take(&mut state);
		let (c, b, m, cfg) = (client.clone(), backend.clone(), metrics.clone(), config.clone());
		state = match tokio::task::spawn_blocking(move || {
			// A panic anywhere in the slice must surface as a `JoinError` below, not
			// exit the process (see `guarded_sweep`).
			let _unwind = sp_panic_handler::AbortGuard::force_unwind();
			let mut st = slice_state;
			let outcome = reclaim_slice(&c, &b, &m, &cfg, lag, &mut st);
			(st, outcome)
		})
		.await
		{
			Ok((_, Err(Stop::LegacyRoot))) => {
				refuse_legacy(&metrics);
				return;
			},
			Ok((st, Ok(()))) => st,
			Err(e) => {
				// The debounce set is lost with the panicked slice: starting fresh only
				// delays releases, it never causes one. A panic here is outside the
				// guarded sweep (scan/release), so keep the sweep off as well: something
				// in the arena is not healthy.
				error!(target: LOG_TARGET, "💥 GC slice task failed: {e}");
				with_metrics(&metrics, |m| m.set_gc_suspended("sweep_panic", true));
				WorkerState { sweep_off: Some("sweep_panic"), ..WorkerState::new() }
			},
		};

		let Some(_note) = finality.next().await else {
			warn!(target: LOG_TARGET, "⚠️  Finality stream closed; stopping ledger GC");
			return;
		};
		while let Some(Some(_)) = finality.next().now_or_never() {}
	}
}

/// Spawn the ledger GC worker, unless the pruning mode has unbounded wrapper
/// history, the ledger columns cannot be iterated, or the database holds legacy
/// roots (each logged once; the node then runs without GC).
pub fn try_spawn(
	spawn: &SpawnTaskHandle,
	client: Arc<FullClient>,
	backend: Arc<FullBackend>,
	metrics: SharedMetrics,
	config: GcConfig,
) {
	let Some(lag) = config.lag else {
		info!(
			target: LOG_TARGET,
			"ℹ️  ledger GC disabled: archive or unbounded state pruning; set \
			 --ledger-state-pruning <N> to release ledger states below finalized - N"
		);
		return;
	};
	// A ledger storage created by an older node holds roots that were never tagged.
	// Decided by the META marker written on first open: one point read, no root
	// iteration, so a multi-million-root legacy database costs nothing here.
	if !config.tagged_from_genesis {
		refuse_legacy(&metrics);
		return;
	}
	// Fail closed on a database whose ledger columns cannot be iterated (unified DBs
	// created before the column options were applied in unified mode, issue #2294):
	// storage-core panics inside `get_roots`/`scan` there, which would otherwise kill
	// every slice. One probe, before anything is spawned; it also refuses a database
	// with a legacy root (the marker is the authority, this is a defence).
	let probe = {
		let _unwind = sp_panic_handler::AbortGuard::force_unwind();
		std::panic::catch_unwind(|| timed_scan(&metrics))
	};
	let wrappers = match probe {
		Ok(Ok(wrappers)) => wrappers,
		Ok(Err(LegacyRoot)) => {
			refuse_legacy(&metrics);
			return;
		},
		Err(_) => {
			error!(
				target: LOG_TARGET,
				"🛑 Ledger GC disabled: the ledger storage columns cannot be iterated (database \
				 created without a btree index on the GC root and node columns). Resync the \
				 ledger storage to enable it."
			);
			with_metrics(&metrics, |m| m.set_gc_suspended("unindexed_columns", true));
			return;
		},
	};
	info!(
		target: LOG_TARGET,
		"♻️  Ledger GC enabled: release lag {lag} blocks ({}), slice {:?}, arena sweep {}, max \
		 deletes per slice {}, {} tagged root(s) at startup",
		match config.liveness {
			Liveness::PrunedState => "state pruning decides",
			Liveness::HeightOnly => "height only, explicit ledger window",
		},
		config.slice,
		config.arena_sweep,
		config.max_deletes.map_or("unbounded".to_string(), |n| n.to_string()),
		wrappers.len()
	);
	spawn.spawn("ledger-gc", Some("ledger"), run(client, backend, metrics, config, lag));
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{cell::Cell, collections::HashMap};

	static ENV_LOCK: Mutex<()> = Mutex::new(());

	const PRUNED: Window = Window { lag: 256, liveness: Liveness::PrunedState };
	const HEIGHT_ONLY: Window = Window { lag: 256, liveness: Liveness::HeightOnly };

	#[test]
	fn window_table() {
		let c = |n| PruningMode::blocks_pruning(n);
		let pruned = |lag| Some(Window { lag, liveness: Liveness::PrunedState });
		let height = |lag| Some(Window { lag, liveness: Liveness::HeightOnly });
		let mut unbounded = PruningMode::default();
		if let PruningMode::Constrained(c) = &mut unbounded {
			c.max_blocks = None;
		}
		// No explicit ledger window: as before.
		assert_eq!(window_for(&None, true, None), Ok(None));
		assert_eq!(window_for(&None, false, None), Ok(pruned(256)));
		assert_eq!(window_for(&Some(c(1024)), false, None), Ok(pruned(1024)));
		assert_eq!(window_for(&Some(PruningMode::ArchiveAll), false, None), Ok(None));
		assert_eq!(window_for(&Some(PruningMode::ArchiveCanonical), false, None), Ok(None));
		assert_eq!(window_for(&Some(unbounded.clone()), false, None), Ok(None));
		// Explicit ledger window on modes with unbounded state: height only.
		assert_eq!(window_for(&None, true, Some(512)), Ok(height(512)));
		assert_eq!(window_for(&Some(PruningMode::ArchiveAll), false, Some(512)), Ok(height(512)));
		assert_eq!(
			window_for(&Some(PruningMode::ArchiveCanonical), false, Some(512)),
			Ok(height(512))
		);
		assert_eq!(window_for(&Some(unbounded), false, Some(512)), Ok(height(512)));
		// Explicit ledger window on a pruned node: keeps the child-state gate, must
		// not be smaller than the state window.
		assert_eq!(window_for(&Some(c(256)), false, Some(1024)), Ok(pruned(1024)));
		assert_eq!(window_for(&Some(c(256)), false, Some(256)), Ok(pruned(256)));
		assert!(window_for(&Some(c(256)), false, Some(255)).is_err());
		assert!(window_for(&None, false, Some(100)).is_err());
	}

	#[test]
	fn height_only_releases_roots_whose_children_keep_state() {
		// Archive node: every child has state. The pruned rule keeps the root
		// forever; the height-only rule releases it after the debounce.
		let mut f = Fake::default();
		f.numbers.insert(h(1), 500);
		f.children.insert(h(1), Ok(vec![h(101)]));
		f.with_state.insert(h(101));
		let roots = [root(1, h(1), 1)];
		let mut st = WorkerState::new();
		for _ in 0..3 {
			assert!(classify(1000, PRUNED, &roots, &mut st, &f).release.is_empty());
		}
		let mut st = WorkerState::new();
		let first = classify(1000, HEIGHT_ONLY, &roots, &mut st, &f);
		assert_eq!(first.candidates, 1);
		assert!(first.release.is_empty());
		let second = classify(1000, HEIGHT_ONLY, &roots, &mut st, &f);
		assert_eq!(second.release, vec![(w(1), 1)]);
		// Still inside the window: kept under either rule.
		f.numbers.insert(h(1), 800);
		let mut st = WorkerState::new();
		for _ in 0..3 {
			let out = classify(1000, HEIGHT_ONLY, &roots, &mut st, &f);
			assert!(out.release.is_empty());
			assert_eq!(out.candidates, 0);
		}
	}

	#[test]
	fn env_overrides_and_bad_values_fall_back() {
		let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
		let keys = [ENV_SLICE_MS, ENV_ARENA_SWEEP, ENV_MAX_DELETES];
		for k in keys {
			unsafe { std::env::remove_var(k) };
		}
		let d = GcConfig::from_env(&None, false, None, true).unwrap();
		assert_eq!(d.slice, Duration::from_millis(200));
		assert!(d.arena_sweep);
		assert_eq!(d.max_deletes, None);
		assert_eq!(d.lag, Some(256));

		unsafe {
			std::env::set_var(ENV_SLICE_MS, "40");
			std::env::set_var(ENV_ARENA_SWEEP, "0");
			std::env::set_var(ENV_MAX_DELETES, "5000");
		}
		let c = GcConfig::from_env(&None, false, None, true).unwrap();
		assert_eq!(c.slice, Duration::from_millis(40));
		assert!(!c.arena_sweep);
		assert_eq!(c.max_deletes, Some(5000));
		unsafe { std::env::set_var(ENV_MAX_DELETES, "0") };
		assert_eq!(GcConfig::from_env(&None, false, None, true).unwrap().max_deletes, None);

		unsafe { std::env::set_var(ENV_SLICE_MS, "banana") };
		assert_eq!(
			GcConfig::from_env(&None, false, None, true).unwrap().slice,
			Duration::from_millis(200)
		);
		unsafe { std::env::set_var(ENV_ARENA_SWEEP, "yes") };
		assert!(GcConfig::from_env(&None, false, None, true).unwrap().arena_sweep);
		for k in keys {
			unsafe { std::env::remove_var(k) };
		}
	}

	#[test]
	fn mbm_cursor_key_matches_pallet() {
		use frame_support::storage::generator::StorageValue;
		type Cursor = pallet_migrations::Cursor<midnight_node_runtime::Runtime>;
		assert_eq!(mbm_cursor_key(), Cursor::storage_value_final_key().to_vec());
	}

	#[derive(Default)]
	struct Fake {
		numbers: HashMap<H256, u32>,
		children: HashMap<H256, Result<Vec<H256>, String>>,
		with_state: HashSet<H256>,
		/// Parents whose header lookup errors.
		number_errors: HashSet<H256>,
		number_lookups: Cell<usize>,
	}
	impl ChainView for Fake {
		fn block_number(&self, h: &H256) -> Result<Option<u32>, String> {
			self.number_lookups.set(self.number_lookups.get() + 1);
			if self.number_errors.contains(h) {
				return Err("header db".into());
			}
			Ok(self.numbers.get(h).copied())
		}
		fn children(&self, h: &H256) -> Result<Vec<H256>, String> {
			self.children.get(h).cloned().unwrap_or(Ok(vec![]))
		}
		fn have_state_at(&self, c: &H256, _n: u32) -> bool {
			self.with_state.contains(c)
		}
	}

	fn h(i: u8) -> H256 {
		H256::repeat_byte(i)
	}
	fn w(i: u8) -> Vec<u8> {
		vec![i; 32]
	}
	/// Wrapper `i` tagged with parent `p`.
	fn root(i: u8, p: H256, count: u32) -> TaggedRootInfo {
		TaggedRootInfo { wrapper: w(i), tag: p.0.to_vec(), count }
	}

	/// Parent `p` (number `pn`) with one child lacking state.
	fn dead_parent(f: &mut Fake, p: u8, pn: u32) {
		f.numbers.insert(h(p), pn);
		f.children.insert(h(p), Ok(vec![h(p + 100)]));
	}

	#[test]
	fn inside_window_is_kept() {
		let mut f = Fake::default();
		dead_parent(&mut f, 1, 990);
		let mut st = WorkerState::new();
		let roots = [root(1, h(1), 1)];
		for _ in 0..3 {
			let out = classify(1000, PRUNED, &roots, &mut st, &f);
			assert!(out.release.is_empty());
			assert_eq!(out.candidates, 0);
		}
	}

	#[test]
	fn child_with_state_keeps() {
		let mut f = Fake::default();
		dead_parent(&mut f, 1, 100);
		f.with_state.insert(h(101));
		let mut st = WorkerState::new();
		let roots = [root(1, h(1), 1)];
		for _ in 0..3 {
			let out = classify(1000, PRUNED, &roots, &mut st, &f);
			assert!(out.release.is_empty());
			assert_eq!(out.candidates, 0);
		}
	}

	#[test]
	fn candidate_twice_is_released_with_the_current_count() {
		let mut f = Fake::default();
		dead_parent(&mut f, 1, 100);
		let mut st = WorkerState::new();
		let out = classify(1000, PRUNED, &[root(1, h(1), 2)], &mut st, &f);
		assert!(out.release.is_empty());
		assert_eq!(out.candidates, 1);
		// The count grew between the two slices (a concurrent persist): the newer
		// scan's count is what gets released.
		let out = classify(1000, PRUNED, &[root(1, h(1), 3)], &mut st, &f);
		assert_eq!(out.release, vec![(w(1), 3)]);
		// A wrapper that drops out of the candidate set restarts the debounce.
		f.with_state.insert(h(101));
		classify(1000, PRUNED, &[root(1, h(1), 3)], &mut st, &f);
		f.with_state.clear();
		let out = classify(1000, PRUNED, &[root(1, h(1), 3)], &mut st, &f);
		assert!(out.release.is_empty());
	}

	#[test]
	fn bad_tag_is_kept_and_never_looked_up() {
		let f = Fake::default();
		let mut st = WorkerState::new();
		let roots = [TaggedRootInfo { wrapper: w(9), tag: vec![1, 2, 3], count: 1 }];
		for _ in 0..3 {
			let out = classify(1000, PRUNED, &roots, &mut st, &f);
			assert!(out.release.is_empty());
			assert_eq!(out.kept_unknown, 1);
		}
		assert_eq!(f.number_lookups.get(), 0, "no lookup for a malformed tag");
		assert_eq!(st.logged_unknown, HashSet::from([w(9)]));
	}

	#[test]
	fn unknown_parent_is_kept_logged_once_and_forgotten_when_gone() {
		let mut f = Fake::default();
		let mut st = WorkerState::new();
		let roots = [root(1, h(1), 1)];
		for _ in 0..3 {
			let out = classify(1000, PRUNED, &roots, &mut st, &f);
			assert!(out.release.is_empty());
			assert_eq!(out.kept_unknown, 1);
			assert_eq!(st.logged_unknown, HashSet::from([w(1)]));
		}
		assert_eq!(f.number_lookups.get(), 3, "looked up every slice");
		// The parent shows up: resolved on the next slice, no longer "unknown".
		dead_parent(&mut f, 1, 100);
		let out = classify(1000, PRUNED, &roots, &mut st, &f);
		assert_eq!(out.kept_unknown, 0);
		assert_eq!(out.candidates, 1);
		assert!(st.logged_unknown.is_empty(), "pruned to the roots still unknown");
		// A root that leaves the scan leaves the logged set too.
		let f2 = Fake::default();
		classify(1000, PRUNED, &[root(2, h(2), 1)], &mut st, &f2);
		assert_eq!(st.logged_unknown, HashSet::from([w(2)]));
		classify(1000, PRUNED, &[], &mut st, &f2);
		assert!(st.logged_unknown.is_empty());
	}

	#[test]
	fn lookup_errors_keep() {
		let mut f = Fake::default();
		f.numbers.insert(h(1), 100);
		f.children.insert(h(1), Err("db".into()));
		f.number_errors.insert(h(0xfe));
		let mut st = WorkerState::new();
		let roots = [root(1, h(1), 1), root(2, h(0xfe), 1)];
		for _ in 0..3 {
			let out = classify(1000, PRUNED, &roots, &mut st, &f);
			assert!(out.release.is_empty());
			assert_eq!(out.kept_unknown, 2);
		}
	}

	#[test]
	fn only_dead_confirmed_wrappers_are_released() {
		let mut f = Fake::default();
		dead_parent(&mut f, 1, 100); // dead: candidate, then released
		f.numbers.insert(h(2), 995); // in window
		dead_parent(&mut f, 3, 100); // candidate this slice only (not in previous set)
		let mut st = WorkerState::new();
		let roots = [
			root(1, h(1), 1),
			root(2, h(2), 1),
			root(3, h(3), 1),
			TaggedRootInfo { wrapper: w(4), tag: vec![1], count: 1 },
			root(5, h(55), 1), // unknown parent
		];
		st.previous_candidates.insert(w(1));
		let out = classify(1000, PRUNED, &roots, &mut st, &f);
		assert_eq!(out.release, vec![(w(1), 1)]);
		assert_eq!(out.candidates, 2);
		assert_eq!(out.kept_unknown, 2);
		assert_eq!(st.previous_candidates, HashSet::from([w(1), w(3)]));
	}

	#[test]
	fn released_wrapper_absent_from_the_next_scan_is_not_retried() {
		let mut f = Fake::default();
		dead_parent(&mut f, 1, 100);
		let mut st = WorkerState::new();
		classify(1000, PRUNED, &[root(1, h(1), 1)], &mut st, &f);
		assert_eq!(classify(1000, PRUNED, &[root(1, h(1), 1)], &mut st, &f).release.len(), 1);
		// Released and flushed: the next scan no longer lists it, so nothing is
		// released again (a replay would over-release and panic at flush).
		let out = classify(1000, PRUNED, &[], &mut st, &f);
		assert!(out.release.is_empty() && st.previous_candidates.is_empty());
	}

	#[test]
	fn lag_boundary() {
		// finalized 1000, lag 256: dead iff n < 744, i.e. parent < 743.
		let mut f = Fake::default();
		dead_parent(&mut f, 1, 743); // n = 744 == finalized - lag -> keep
		dead_parent(&mut f, 2, 742); // n = 743 -> candidate
		let mut st = WorkerState::new();
		let out = classify(1000, PRUNED, &[root(1, h(1), 1), root(2, h(2), 1)], &mut st, &f);
		assert_eq!(out.candidates, 1);
		assert_eq!(st.previous_candidates, HashSet::from([w(2)]));
	}

	#[test]
	fn suspended_never_releases() {
		let mut f = Fake::default();
		dead_parent(&mut f, 1, 100);
		let mut st = WorkerState::new();
		let roots = [root(1, h(1), 1)];
		classify(1000, PRUNED, &roots, &mut st, &f);
		st.mbm_suspended = true;
		let out = classify(1000, PRUNED, &roots, &mut st, &f);
		assert!(out.release.is_empty() && st.previous_candidates.is_empty());
		// Resuming needs two fresh slices.
		st.mbm_suspended = false;
		assert!(classify(1000, PRUNED, &roots, &mut st, &f).release.is_empty());
		assert_eq!(classify(1000, PRUNED, &roots, &mut st, &f).release.len(), 1);
	}

	#[test]
	fn sweep_allowed_table() {
		let cfg = GcConfig {
			slice: Duration::from_millis(25),
			arena_sweep: true,
			max_deletes: None,
			lag: Some(256),
			liveness: Liveness::PrunedState,
			tagged_from_genesis: true,
		};
		let mut st = WorkerState::new();
		assert!(sweep_allowed(&st, &cfg));
		st.sweep_off = Some("sweep_panic");
		assert!(!sweep_allowed(&st, &cfg));
		let off = GcConfig { arena_sweep: false, ..cfg };
		assert!(!sweep_allowed(&WorkerState::new(), &off));
	}
}
