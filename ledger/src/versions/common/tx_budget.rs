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

//! Per-transaction block-budget calculator.
//!
//! A block is bounded by the ledger's `block_limits`: a five-dimensional
//! [`SyntheticCost`] (read time, compute time, block usage, bytes written,
//! bytes churned). Every applied transaction accrues its own `cost` into the
//! block's running fullness, and the *fraction of a limit* the largest
//! dimension reaches is what ultimately decides how many transactions fit in a
//! block — and, via `scale_normalized_cost`, what Substrate weight the
//! transaction is charged.
//!
//! This module turns that single number into an itemised bill: it takes the
//! per-aspect costs computed by the version-specific `tx_budget` module (proof
//! verification, signature checks, verifier-key reads, state application, …)
//! and emits one machine-readable line per transaction, plus one per block, on
//! the `midnight::tx_budget` target. Nothing is computed unless that target is
//! enabled at `debug`, so the calculator is free when it is off:
//!
//! ```text
//! -lmidnight::tx_budget=debug
//! ```
//!
//! ## Line format
//!
//! One JSON object per line. Keys are terse because these lines are produced at
//! transaction rate over hours-long load runs; `scripts/perf/tx-budget-report.py`
//! parses them.
//!
//! Cost objects (`c`, `lim`) and the fullness arrays (`fb`, `fa`) use the
//! dimension keys / order `rt` (read time, ps), `ct` (compute time, ps), `bu`
//! (block usage, bytes), `bw` (bytes written), `bc` (bytes churned). Share
//! objects (`s`) hold the same dimensions as a fraction of the block limit.
//!
//! Transaction line:
//!
//! ```text
//! {"k":"tx","p":"<parent block hash, hex>","tb":<tblock>,"tx":"<tx hash, hex>",
//!  "sz":<serialized bytes>,"c":{…},"s":{…},"bind":"ct","bs":<block share>,
//!  "fb":[…],"fa":[…],
//!  "a":[{"n":"<aspect>","q":<count>,"ct":<ps>,…,"s":<share>},…]}
//! ```
//!
//! Block line, emitted from the end-of-block update:
//!
//! ```text
//! {"k":"blk","p":"<parent block hash, hex>","tb":<tblock>,
//!  "c":{…},"s":{…},"bind":"ct","bs":<block share>,"lim":{…}}
//! ```
//!
//! `bind` names the dimension that is closest to its limit and `bs` is that
//! fraction — the transaction's (or block's) actual share of the block budget.
//!
//! ## Reading the aspects
//!
//! The aspects partition the transaction's total cost, so `sum(a) == c` up to
//! fixed-point rounding. The version-specific builder emits an explicit
//! `validation.other` residual for whatever it could not attribute; a residual
//! that is more than a rounding error means this crate's reconstruction has
//! drifted from the ledger's cost model and the per-aspect split should not be
//! trusted until it is re-synced (the totals always remain exact — they come
//! from the ledger itself).

#![cfg(feature = "std")]

use super::super::{BlockContext, base_crypto_local};
use crate::common::types::Hash;
use alloc::{borrow::Cow, string::String};
use base_crypto_local::cost_model::SyntheticCost;
use core::fmt::Write;

/// Log target for the per-transaction budget lines. Enable at `debug`.
pub const TX_BUDGET_TARGET: &str = "midnight::tx_budget";

/// Which kind of transaction a line describes. System transactions accrue into
/// the same block fullness as user ones, so a report that ignored them could not
/// reconcile a block's fill with the transactions in it.
#[derive(Debug, Clone, Copy)]
pub enum Kind {
	/// A user transaction, submitted through `send_mn_transaction`.
	User,
	/// A system transaction, applied by the runtime itself.
	System,
}

impl Kind {
	fn as_str(self) -> &'static str {
		match self {
			Kind::User => "tx",
			Kind::System => "sys",
		}
	}
}

/// Dimension keys, in the order used by the `fb` / `fa` arrays.
const DIMS: [&str; 5] = ["rt", "ct", "bu", "bw", "bc"];

/// One costed component of a transaction, as attributed by the version-specific
/// `tx_budget` module.
#[derive(Debug, Clone)]
pub struct Aspect {
	/// Dotted name, e.g. `validation.zswap_input_proof`. Borrowed for the fixed
	/// user-transaction aspects; owned for system transactions, whose aspect is
	/// named after the variant.
	pub name: Cow<'static, str>,
	/// How many things of this kind the transaction contains (proofs, signatures,
	/// verifier-key reads, …). `1` for aspects that are not per-item.
	pub count: u64,
	/// What those items cost, in the same units as the block limits.
	pub cost: SyntheticCost,
}

impl Aspect {
	pub fn new(name: impl Into<Cow<'static, str>>, count: u64, cost: SyntheticCost) -> Self {
		Self { name: name.into(), count, cost }
	}
}

/// Whether the calculator should run at all. Every entry point is gated on this,
/// so a node without `midnight::tx_budget=debug` does no extra work.
pub fn enabled() -> bool {
	log::log_enabled!(target: TX_BUDGET_TARGET, log::Level::Debug)
}

/// The five dimensions of a cost, in `DIMS` order.
fn dims(cost: &SyntheticCost) -> [u64; 5] {
	[
		cost.read_time.into_picoseconds(),
		cost.compute_time.into_picoseconds(),
		cost.block_usage,
		cost.bytes_written,
		cost.bytes_churned,
	]
}

/// Saturating per-dimension difference, for computing residuals.
/// (`CostDuration`'s own `Sub` already saturates.)
pub fn saturating_sub(lhs: &SyntheticCost, rhs: &SyntheticCost) -> SyntheticCost {
	SyntheticCost {
		read_time: lhs.read_time - rhs.read_time,
		compute_time: lhs.compute_time - rhs.compute_time,
		block_usage: lhs.block_usage.saturating_sub(rhs.block_usage),
		bytes_written: lhs.bytes_written.saturating_sub(rhs.bytes_written),
		bytes_churned: lhs.bytes_churned.saturating_sub(rhs.bytes_churned),
	}
}

/// Each dimension as a fraction of its block limit. A zero limit yields `0.0`
/// rather than a NaN, so a malformed parameter set cannot poison the report.
fn shares(cost: &SyntheticCost, limits: &SyntheticCost) -> [f64; 5] {
	let c = dims(cost);
	let l = dims(limits);
	let mut out = [0.0f64; 5];
	for i in 0..5 {
		out[i] = if l[i] == 0 { 0.0 } else { c[i] as f64 / l[i] as f64 };
	}
	out
}

/// The dimension closest to its limit, and how close: the transaction's real
/// share of the block budget.
fn binding(share: &[f64; 5]) -> (&'static str, f64) {
	let mut idx = 0;
	for i in 1..5 {
		if share[i] > share[idx] {
			idx = i;
		}
	}
	(DIMS[idx], share[idx])
}

fn write_cost(out: &mut String, cost: &SyntheticCost) {
	let d = dims(cost);
	let _ = write!(
		out,
		"{{\"rt\":{},\"ct\":{},\"bu\":{},\"bw\":{},\"bc\":{}}}",
		d[0], d[1], d[2], d[3], d[4]
	);
}

fn write_share(out: &mut String, share: &[f64; 5]) {
	let _ = write!(
		out,
		"{{\"rt\":{:.6},\"ct\":{:.6},\"bu\":{:.6},\"bw\":{:.6},\"bc\":{:.6}}}",
		share[0], share[1], share[2], share[3], share[4]
	);
}

fn write_dim_array(out: &mut String, cost: &SyntheticCost) {
	let d = dims(cost);
	let _ = write!(out, "[{},{},{},{},{}]", d[0], d[1], d[2], d[3], d[4]);
}

/// Aspects are written with their zero dimensions elided — most of them touch
/// only `ct` — and aspects the transaction did not incur at all are dropped
/// entirely, which roughly halves the line at transaction rate. The dropped ones
/// carry no information: a missing aspect is a zero one.
fn write_aspects(out: &mut String, aspects: &[Aspect], limits: &SyntheticCost) {
	out.push('[');
	for (written, aspect) in aspects
		.iter()
		.filter(|a| a.count != 0 || a.cost != SyntheticCost::ZERO)
		.enumerate()
	{
		if written > 0 {
			out.push(',');
		}
		let _ = write!(out, "{{\"n\":\"{}\",\"q\":{}", aspect.name, aspect.count);
		let d = dims(&aspect.cost);
		for (key, value) in DIMS.iter().zip(d.iter()) {
			if *value != 0 {
				let _ = write!(out, ",\"{key}\":{value}");
			}
		}
		let (_, share) = binding(&shares(&aspect.cost, limits));
		let _ = write!(out, ",\"s\":{share:.6}}}");
	}
	out.push(']');
}

/// One line per applied transaction: what it cost, what fraction of each block
/// limit that is, which aspects it went to, and where the block stood before and
/// after it.
///
/// `fullness_before` / `fullness_after` bracket the transaction within its block,
/// so a run can be re-sequenced offline even when two block executions interleave
/// in the log (group by `p`, order by `fb`).
#[allow(clippy::too_many_arguments)]
pub fn log_tx(
	kind: Kind,
	tx_hash: &Hash,
	tx_bytes: usize,
	block_context: &BlockContext,
	cost: &SyntheticCost,
	aspects: &[Aspect],
	fullness_before: &SyntheticCost,
	fullness_after: &SyntheticCost,
	limits: &SyntheticCost,
) {
	let share = shares(cost, limits);
	let (bind, block_share) = binding(&share);

	let mut line = String::with_capacity(512 + aspects.len() * 64);
	let _ = write!(
		line,
		"{{\"k\":\"{}\",\"p\":\"{}\",\"tb\":{},\"tx\":\"{}\",\"sz\":{},\"c\":",
		kind.as_str(),
		hex::encode(&block_context.parent_block_hash),
		block_context.tblock,
		hex::encode(tx_hash),
		tx_bytes,
	);
	write_cost(&mut line, cost);
	line.push_str(",\"s\":");
	write_share(&mut line, &share);
	let _ = write!(line, ",\"bind\":\"{bind}\",\"bs\":{block_share:.6},\"fb\":");
	write_dim_array(&mut line, fullness_before);
	line.push_str(",\"fa\":");
	write_dim_array(&mut line, fullness_after);
	line.push_str(",\"a\":");
	write_aspects(&mut line, aspects, limits);
	line.push('}');

	log::debug!(target: TX_BUDGET_TARGET, "{line}");
}

/// One line per block, emitted from the end-of-block update: the accrued
/// fullness, its share of each limit, and the limits themselves (which are
/// governance-settable, so they are restated rather than assumed).
pub fn log_block(block_context: &BlockContext, fullness: &SyntheticCost, limits: &SyntheticCost) {
	let share = shares(fullness, limits);
	let (bind, block_share) = binding(&share);

	let mut line = String::with_capacity(512);
	let _ = write!(
		line,
		"{{\"k\":\"blk\",\"p\":\"{}\",\"tb\":{},\"c\":",
		hex::encode(&block_context.parent_block_hash),
		block_context.tblock,
	);
	write_cost(&mut line, fullness);
	line.push_str(",\"s\":");
	write_share(&mut line, &share);
	let _ = write!(line, ",\"bind\":\"{bind}\",\"bs\":{block_share:.6},\"lim\":");
	write_cost(&mut line, limits);
	line.push('}');

	log::debug!(target: TX_BUDGET_TARGET, "{line}");
}

/// The residual aspect: whatever the version-specific builder could not
/// attribute. Callers pass the exact total from the ledger and the sum of the
/// aspects they built; a non-trivial result means the reconstruction has drifted.
pub fn residual(
	name: impl Into<Cow<'static, str>>,
	total: &SyntheticCost,
	attributed: &SyntheticCost,
) -> Aspect {
	Aspect::new(name, 1, saturating_sub(total, attributed))
}

/// Sum of a slice of aspects, for computing the residual.
pub fn sum(aspects: &[Aspect]) -> SyntheticCost {
	aspects.iter().fold(SyntheticCost::ZERO, |acc, a| acc + a.cost)
}
