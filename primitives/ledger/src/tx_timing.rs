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

//! Phase-level timing for Midnight transaction processing.
//!
//! Answers two questions that the existing Prometheus histograms cannot:
//!
//! 1. *Within* one transaction, where does the time go — deserialization, ledger
//!    state load, ZK proof verification (`well_formed`), the guaranteed-execution
//!    dry run, the ledger apply itself, or persistence?
//! 2. *Within* one block, what share of wall-clock time is Midnight transaction
//!    processing versus the surrounding Substrate machinery?
//!
//! # Design
//!
//! A span is opened by [`span`] at each ledger host-function entry point and
//! lives on a thread-local stack, so code deep in the call tree (the apply, the
//! `well_formed` call site) can attribute time to it with a bare [`mark`] /
//! [`mark_agg`] call instead of threading a timer through every signature. Host
//! calls run synchronously on the calling thread, so the thread-local stack is
//! always the current call's own.
//!
//! Two independent sinks:
//!
//! - **Per-span log line** — one `key=value` line per host call on the
//!   `midnight::tx_timing` target at `debug`, carrying every phase delta. Built
//!   only when that target is enabled, so it costs nothing when off.
//! - **Global counters** — process-wide totals per operation and per aggregated
//!   [`Phase`], always maintained (a handful of relaxed atomic adds). The node's
//!   block-import wrapper diffs [`Totals::snapshot`] across one block to report
//!   the ledger's share of block execution time.
//!
//! # Usage
//!
//! ```ignore
//! let _span = tx_timing::span(Op::ApplyTx);
//! tx_timing::note("tx", hex::encode(tx_hash));
//! let tx = deserialize(bytes)?;              // `?` still logs, with outcome=err
//! tx_timing::mark_agg(Phase::Deserialize);
//! ...
//! tx_timing::ok();                           // marks the span successful
//! ```
//!
//! Enable with `-l midnight::tx_timing=debug`.

use std::{
	cell::RefCell,
	fmt::Display,
	sync::atomic::{AtomicU64, Ordering},
	time::{Duration, Instant},
};

/// Log target for all timing output. Kept separate from `midnight::ledger_v2` so
/// that timing can be enabled without the rest of the ledger's debug chatter.
pub const LOG_TARGET: &str = "midnight::tx_timing";

/// A ledger host-function entry point — the unit one [`span`] measures end to end.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
	/// `apply_transaction`: executing a Midnight tx as part of a block.
	ApplyTx,
	/// `apply_system_transaction`: executing a system tx as part of a block.
	ApplySystemTx,
	/// `validate_transaction`: mempool admission / revalidation.
	ValidateTx,
	/// `validate_guaranteed_execution`: `pre_dispatch`, i.e. the last check
	/// before a tx is allowed into a block.
	PreDispatch,
	/// `post_block_update` / `apply_post_block_update`: the end-of-block ledger
	/// transition (DUST generation and friends).
	PostBlockUpdate,
}

const OP_COUNT: usize = 5;

impl Op {
	/// Stable identifier used in log lines and metric-ish output.
	pub const fn as_str(self) -> &'static str {
		match self {
			Op::ApplyTx => "apply_tx",
			Op::ApplySystemTx => "apply_system_tx",
			Op::ValidateTx => "validate_tx",
			Op::PreDispatch => "pre_dispatch",
			Op::PostBlockUpdate => "post_block_update",
		}
	}

	const fn index(self) -> usize {
		match self {
			Op::ApplyTx => 0,
			Op::ApplySystemTx => 1,
			Op::ValidateTx => 2,
			Op::PreDispatch => 3,
			Op::PostBlockUpdate => 4,
		}
	}
}

/// Phases that are aggregated process-wide, in addition to appearing in the
/// per-span line. These are the ones worth attributing across a whole block;
/// everything else is recorded with [`mark`] and stays span-local.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
	/// Tagged-deserialization of the transaction blob.
	Deserialize,
	/// Loading the ledger state (arena lookup, possibly a parity-db read).
	LoadState,
	/// `well_formed()` — ZK proof verification and structural checks. Recorded
	/// only on a cache miss; a hit records [`Phase::ProofCacheHit`] instead.
	ProofVerification,
	/// A `VerifiedTransaction` served from the strict cache, skipping
	/// `well_formed()` entirely.
	ProofCacheHit,
	/// Dry run of the guaranteed segment against current state.
	GuaranteedDryRun,
	/// `LedgerState::apply` — the state transition itself, including on-chain
	/// (Impact) execution of contract calls.
	LedgerApply,
	/// Writing the new ledger state into the arena / to disk.
	Persist,
}

const PHASE_COUNT: usize = 7;

impl Phase {
	/// Stable identifier; becomes the `<name>_us` key in the per-span line.
	pub const fn as_str(self) -> &'static str {
		match self {
			Phase::Deserialize => "deserialize",
			Phase::LoadState => "load_state",
			Phase::ProofVerification => "proof_verify",
			Phase::ProofCacheHit => "proof_cache_hit",
			Phase::GuaranteedDryRun => "guaranteed_dry_run",
			Phase::LedgerApply => "ledger_apply",
			Phase::Persist => "persist",
		}
	}

	const fn index(self) -> usize {
		match self {
			Phase::Deserialize => 0,
			Phase::LoadState => 1,
			Phase::ProofVerification => 2,
			Phase::ProofCacheHit => 3,
			Phase::GuaranteedDryRun => 4,
			Phase::LedgerApply => 5,
			Phase::Persist => 6,
		}
	}

	/// All phases, in `index()` order — for iterating a [`Totals`].
	pub const ALL: [Phase; PHASE_COUNT] = [
		Phase::Deserialize,
		Phase::LoadState,
		Phase::ProofVerification,
		Phase::ProofCacheHit,
		Phase::GuaranteedDryRun,
		Phase::LedgerApply,
		Phase::Persist,
	];
}

static OP_NANOS: [AtomicU64; OP_COUNT] = [const { AtomicU64::new(0) }; OP_COUNT];
static OP_COUNTS: [AtomicU64; OP_COUNT] = [const { AtomicU64::new(0) }; OP_COUNT];
static PHASE_NANOS: [AtomicU64; PHASE_COUNT] = [const { AtomicU64::new(0) }; PHASE_COUNT];
static PHASE_COUNTS: [AtomicU64; PHASE_COUNT] = [const { AtomicU64::new(0) }; PHASE_COUNT];

/// An occurrence count and the total time it accounted for.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Counter {
	pub count: u64,
	pub nanos: u64,
}

impl Counter {
	/// Elapsed time in milliseconds, for display.
	pub fn millis(&self) -> f64 {
		self.nanos as f64 / 1_000_000.0
	}

	fn saturating_sub(self, earlier: Self) -> Self {
		Counter {
			count: self.count.saturating_sub(earlier.count),
			nanos: self.nanos.saturating_sub(earlier.nanos),
		}
	}
}

/// Process-wide timing counters, as of one point in time.
///
/// Diff two snapshots with [`Totals::since`] to attribute time to a window (a
/// block import, say). Note that counters are process-wide: on an authoring node
/// a snapshot window around block import can also pick up work done by a
/// concurrent proposal or mempool validation on another thread. [`Op::ApplyTx`],
/// [`Op::ApplySystemTx`], [`Op::PreDispatch`] and [`Op::PostBlockUpdate`] all run
/// as part of block execution; [`Op::ValidateTx`] is the mempool-only one and is
/// worth reporting separately for exactly that reason.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Totals {
	ops: [Counter; OP_COUNT],
	phases: [Counter; PHASE_COUNT],
}

impl Totals {
	/// Read the current global counters.
	pub fn snapshot() -> Self {
		let mut totals = Totals::default();
		for i in 0..OP_COUNT {
			totals.ops[i] = Counter {
				count: OP_COUNTS[i].load(Ordering::Relaxed),
				nanos: OP_NANOS[i].load(Ordering::Relaxed),
			};
		}
		for i in 0..PHASE_COUNT {
			totals.phases[i] = Counter {
				count: PHASE_COUNTS[i].load(Ordering::Relaxed),
				nanos: PHASE_NANOS[i].load(Ordering::Relaxed),
			};
		}
		totals
	}

	/// Counters accumulated between `earlier` and `self`.
	pub fn since(&self, earlier: &Totals) -> Totals {
		let mut delta = Totals::default();
		for i in 0..OP_COUNT {
			delta.ops[i] = self.ops[i].saturating_sub(earlier.ops[i]);
		}
		for i in 0..PHASE_COUNT {
			delta.phases[i] = self.phases[i].saturating_sub(earlier.phases[i]);
		}
		delta
	}

	/// Counter for one operation.
	pub fn op(&self, op: Op) -> Counter {
		self.ops[op.index()]
	}

	/// Counter for one aggregated phase.
	pub fn phase(&self, phase: Phase) -> Counter {
		self.phases[phase.index()]
	}
}

/// A single in-flight measurement. Created by [`span`]; emitted on drop.
struct Span {
	op: Op,
	start: Instant,
	/// End of the most recent phase — the origin for the next [`mark`].
	last: Instant,
	/// `(name, micros)` per phase. Only populated when logging is enabled.
	phases: Vec<(&'static str, u128)>,
	/// Extra context (`tx=`, `size=`, …). Only populated when logging is enabled.
	fields: Vec<(&'static str, String)>,
	/// Set by [`ok`]; anything else means we unwound through a `?`.
	outcome: &'static str,
	detailed: bool,
}

thread_local! {
	/// Stack of open spans on this thread. A stack rather than a single slot so
	/// that a host call which opens a nested span (or re-enters the ledger API)
	/// attributes marks to the innermost one.
	static SPANS: RefCell<Vec<Span>> = const { RefCell::new(Vec::new()) };
}

/// Whether per-span log lines should be built. When false, spans still maintain
/// the global counters but skip all formatting and allocation.
fn detailed_enabled() -> bool {
	log::log_enabled!(target: LOG_TARGET, log::Level::Debug)
}

/// Opens a timing span for `op`. The returned guard emits the log line and
/// updates the global counters when dropped, including on an error unwind — so a
/// transaction rejected halfway through still reports where its time went.
#[must_use = "the span is measured until the guard is dropped"]
pub fn span(op: Op) -> SpanGuard {
	let now = Instant::now();
	let detailed = detailed_enabled();
	// `try_with` throughout: instrumentation must never be the thing that panics,
	// including on a thread whose TLS is already being torn down.
	let _ = SPANS.try_with(|spans| {
		if let Ok(mut spans) = spans.try_borrow_mut() {
			spans.push(Span {
				op,
				start: now,
				last: now,
				phases: Vec::new(),
				fields: Vec::new(),
				outcome: "err",
				detailed,
			})
		}
	});
	SpanGuard { _private: () }
}

/// Guard returned by [`span`]. Dropping it closes the span.
pub struct SpanGuard {
	_private: (),
}

impl Drop for SpanGuard {
	fn drop(&mut self) {
		let popped = SPANS
			.try_with(|spans| spans.try_borrow_mut().ok().and_then(|mut spans| spans.pop()))
			.ok()
			.flatten();
		let Some(span) = popped else {
			return;
		};
		let total = span.start.elapsed();

		let idx = span.op.index();
		OP_NANOS[idx].fetch_add(total.as_nanos() as u64, Ordering::Relaxed);
		OP_COUNTS[idx].fetch_add(1, Ordering::Relaxed);

		if !span.detailed {
			return;
		}

		let mut line = format!(
			"op={} outcome={} total_us={}",
			span.op.as_str(),
			span.outcome,
			total.as_micros()
		);
		for (key, value) in &span.fields {
			line.push_str(&format!(" {key}={value}"));
		}
		for (name, micros) in &span.phases {
			line.push_str(&format!(" {name}_us={micros}"));
		}
		log::debug!(target: LOG_TARGET, "{line}");
	}
}

/// Runs `f` against the innermost open span, if any.
fn with_current<F: FnOnce(&mut Span)>(f: F) {
	let _ = SPANS.try_with(|spans| {
		if let Ok(mut spans) = spans.try_borrow_mut()
			&& let Some(span) = spans.last_mut()
		{
			f(span)
		}
	});
}

/// Closes the phase that ended now, naming it `name`, and starts the next one.
///
/// Span-local: use [`mark_agg`] for phases that should also be aggregated
/// process-wide.
pub fn mark(name: &'static str) {
	with_current(|span| {
		let now = Instant::now();
		let elapsed = now.duration_since(span.last);
		span.last = now;
		if span.detailed {
			span.phases.push((name, elapsed.as_micros()));
		}
	});
}

/// [`mark`], and additionally add the phase to the global counters.
pub fn mark_agg(phase: Phase) {
	let now = Instant::now();
	let mut elapsed = None;
	with_current(|span| {
		let delta = now.duration_since(span.last);
		span.last = now;
		elapsed = Some(delta);
		if span.detailed {
			span.phases.push((phase.as_str(), delta.as_micros()));
		}
	});
	// A phase recorded outside any span (e.g. a ledger read served over RPC)
	// still counts towards the process totals; it just has no line to appear on.
	record_phase(phase, elapsed.unwrap_or_default());
}

/// Adds `elapsed` to a phase's global counters without touching the current
/// span's cursor. For phases measured with their own timer.
pub fn record_phase(phase: Phase, elapsed: Duration) {
	let idx = phase.index();
	PHASE_NANOS[idx].fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
	PHASE_COUNTS[idx].fetch_add(1, Ordering::Relaxed);
}

/// Attaches a `key=value` field to the current span's log line.
///
/// Formatting is skipped when timing output is disabled, but the value itself is
/// still evaluated at the call site — use [`note_with`] when producing the value
/// costs something.
pub fn note(key: &'static str, value: impl Display) {
	with_current(|span| {
		if span.detailed {
			span.fields.push((key, value.to_string()));
		}
	});
}

/// [`note`], with the value computed lazily — for fields whose formatting costs
/// something (hex-encoding a hash, say).
pub fn note_with<F, V>(key: &'static str, value: F)
where
	F: FnOnce() -> V,
	V: Display,
{
	with_current(|span| {
		if span.detailed {
			span.fields.push((key, value().to_string()));
		}
	});
}

/// Marks the current span as successful. Call it just before returning `Ok`;
/// a span dropped without it reports `outcome=err`.
pub fn ok() {
	with_current(|span| span.outcome = "ok");
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::Mutex;

	#[test]
	fn span_accumulates_op_totals() {
		let before = Totals::snapshot();
		{
			let _span = span(Op::ValidateTx);
			ok();
		}
		let delta = Totals::snapshot().since(&before);
		assert_eq!(delta.op(Op::ValidateTx).count, 1);
		assert_eq!(delta.op(Op::ApplyTx).count, 0);
	}

	#[test]
	fn marks_aggregate_into_phase_totals() {
		let before = Totals::snapshot();
		{
			let _span = span(Op::ApplyTx);
			mark("uninteresting");
			mark_agg(Phase::ProofVerification);
			mark_agg(Phase::ProofVerification);
			ok();
		}
		let delta = Totals::snapshot().since(&before);
		assert_eq!(delta.phase(Phase::ProofVerification).count, 2);
		assert_eq!(delta.phase(Phase::LedgerApply).count, 0);
	}

	#[test]
	fn marks_outside_a_span_are_ignored_but_phases_still_count() {
		let before = Totals::snapshot();
		mark("orphan");
		mark_agg(Phase::LoadState);
		let delta = Totals::snapshot().since(&before);
		assert_eq!(delta.phase(Phase::LoadState).count, 1);
		assert_eq!(delta.phase(Phase::LoadState).nanos, 0);
	}

	#[test]
	fn nested_spans_pop_in_order() {
		let before = Totals::snapshot();
		{
			let _outer = span(Op::ValidateTx);
			{
				let _inner = span(Op::PreDispatch);
				mark_agg(Phase::Deserialize);
			}
			mark_agg(Phase::LedgerApply);
			ok();
		}
		let delta = Totals::snapshot().since(&before);
		assert_eq!(delta.op(Op::ValidateTx).count, 1);
		assert_eq!(delta.op(Op::PreDispatch).count, 1);
		assert_eq!(delta.phase(Phase::Deserialize).count, 1);
		assert_eq!(delta.phase(Phase::LedgerApply).count, 1);
		// The stack must be empty again, or later spans would leak into it.
		SPANS.with(|spans| assert!(spans.borrow().is_empty()));
	}

	/// Captures every record on our target so the emitted line can be asserted on.
	/// Installed once for the whole test binary; concurrent tests are filtered out
	/// by looking for the marker field each assertion adds.
	struct CapturingLogger;

	static CAPTURED: Mutex<Vec<String>> = Mutex::new(Vec::new());

	impl log::Log for CapturingLogger {
		fn enabled(&self, metadata: &log::Metadata) -> bool {
			metadata.target() == LOG_TARGET
		}

		fn log(&self, record: &log::Record) {
			if self.enabled(record.metadata()) {
				CAPTURED.lock().unwrap().push(record.args().to_string());
			}
		}

		fn flush(&self) {}
	}

	fn install_logger() {
		static INIT: std::sync::Once = std::sync::Once::new();
		INIT.call_once(|| {
			log::set_logger(&CapturingLogger).expect("no other logger in this test binary");
			log::set_max_level(log::LevelFilter::Debug);
		});
	}

	/// Lines captured so far that carry `marker`.
	fn captured_with(marker: &str) -> Vec<String> {
		CAPTURED
			.lock()
			.unwrap()
			.iter()
			.filter(|l| l.contains(marker))
			.cloned()
			.collect()
	}

	#[test]
	fn emits_one_line_with_fields_and_phases() {
		install_logger();
		{
			let _span = span(Op::ApplyTx);
			note("marker", "emits-one-line");
			note("size", 4096);
			mark("deserialize_ish");
			mark_agg(Phase::ProofVerification);
			ok();
		}

		let lines = captured_with("emits-one-line");
		assert_eq!(lines.len(), 1, "expected exactly one line, got {lines:?}");
		let line = &lines[0];
		assert!(line.starts_with("op=apply_tx outcome=ok total_us="), "got {line}");
		assert!(line.contains(" size=4096"), "got {line}");
		assert!(line.contains(" deserialize_ish_us="), "got {line}");
		assert!(line.contains(" proof_verify_us="), "got {line}");
	}

	#[test]
	fn error_unwind_still_reports_progress() {
		install_logger();

		fn failing() -> Result<(), ()> {
			let _span = span(Op::ValidateTx);
			note("marker", "error-unwind");
			mark_agg(Phase::Deserialize);
			Err(())?;
			ok();
			Ok(())
		}
		assert!(failing().is_err());

		let lines = captured_with("error-unwind");
		assert_eq!(lines.len(), 1, "expected exactly one line, got {lines:?}");
		assert!(lines[0].contains("outcome=err"), "got {}", lines[0]);
		// The phase completed before the failure must still be attributed.
		assert!(lines[0].contains(" deserialize_us="), "got {}", lines[0]);
	}

	#[test]
	fn since_is_saturating() {
		let later = Totals::snapshot();
		let mut earlier = later;
		earlier.ops[Op::ApplyTx.index()].count += 5;
		assert_eq!(later.since(&earlier).op(Op::ApplyTx).count, 0);
	}
}
