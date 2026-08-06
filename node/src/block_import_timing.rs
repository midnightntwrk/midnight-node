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

//! A `BlockImport` wrapper that reports how much of each block's import time was
//! spent in the Midnight ledger.
//!
//! `midnight_primitives_ledger::tx_timing` says where the time inside one
//! transaction goes; this says how much of a *block* that adds up to. Wrapping
//! the import queue's block import gives a wall-clock denominator that includes
//! everything Substrate does around the ledger — WASM execution, weight
//! accounting, state root computation, database commit — so the residual
//! (`total_ms` minus the ledger lines) is the non-Midnight machinery.
//!
//! Because this sits on the import path it measures the same code that block
//! *sync* exercises: syncing a chain with these logs enabled is a repeatable
//! profile of block execution.
//!
//! Enable with `-l midnight::tx_timing=debug`.

use midnight_primitives_ledger::tx_timing::{LOG_TARGET, Op, Phase, Totals};
use sc_consensus::{
	BlockCheckParams, BlockImport, BlockImportParams, ImportResult, import_queue::BoxBlockImport,
};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use std::time::Instant;

/// Wraps a block import, logging a per-block timing summary.
pub struct TimingBlockImport<B: BlockT> {
	inner: BoxBlockImport<B>,
}

impl<B: BlockT> TimingBlockImport<B> {
	/// Wraps `inner`. Cheap when the timing log target is disabled: the wrapper
	/// then does nothing beyond delegating.
	pub fn new(inner: BoxBlockImport<B>) -> Self {
		Self { inner }
	}
}

#[async_trait::async_trait]
impl<B: BlockT> BlockImport<B> for TimingBlockImport<B> {
	type Error = sp_consensus::error::Error;

	async fn check_block(&self, block: BlockCheckParams<B>) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).await
	}

	async fn import_block(&self, block: BlockImportParams<B>) -> Result<ImportResult, Self::Error> {
		if !log::log_enabled!(target: LOG_TARGET, log::Level::Debug) {
			return self.inner.import_block(block).await;
		}

		let number = *block.header.number();
		let hash = block.post_hash();
		let extrinsics = block.body.as_ref().map(|body| body.len()).unwrap_or(0);

		let totals_before = Totals::snapshot();
		let started = Instant::now();
		let result = self.inner.import_block(block).await;
		let total = started.elapsed();
		let delta = Totals::snapshot().since(&totals_before);

		// `pre_dispatch` is part of block execution, not just authoring: the Bare
		// extrinsic path runs `ValidateUnsigned::pre_dispatch` as the transaction is
		// dispatched, so on a syncing node that is where `well_formed()` proof
		// verification is actually paid (`apply_tx` then hits the strict cache).
		// `validate_tx` is the mempool-only op and is reported separately — the
		// counters are process-wide, so on an authoring node it can pick up work
		// from another thread inside this window.
		let ledger_nanos = delta.op(Op::ApplyTx).nanos
			+ delta.op(Op::ApplySystemTx).nanos
			+ delta.op(Op::PreDispatch).nanos
			+ delta.op(Op::PostBlockUpdate).nanos;
		let total_nanos = total.as_nanos() as u64;
		let pct = |nanos: u64| -> f64 {
			if total_nanos == 0 { 0.0 } else { nanos as f64 * 100.0 / total_nanos as f64 }
		};

		let mut line = format!(
			"op=block_import outcome={} number={number} hash={hash} extrinsics={extrinsics} \
			 total_ms={:.3} ledger_ms={:.3} ledger_pct={:.1} mn_txs={} system_txs={} \
			 apply_tx_ms={:.3} pre_dispatch_ms={:.3} post_block_update_ms={:.3}",
			if result.is_ok() { "ok" } else { "err" },
			total.as_secs_f64() * 1_000.0,
			ledger_nanos as f64 / 1_000_000.0,
			pct(ledger_nanos),
			delta.op(Op::ApplyTx).count,
			delta.op(Op::ApplySystemTx).count,
			delta.op(Op::ApplyTx).millis(),
			delta.op(Op::PreDispatch).millis(),
			delta.op(Op::PostBlockUpdate).millis(),
		);
		for phase in Phase::ALL {
			let counter = delta.phase(phase);
			line.push_str(&format!(
				" {}_ms={:.3} {}_pct={:.1}",
				phase.as_str(),
				counter.millis(),
				phase.as_str(),
				pct(counter.nanos),
			));
		}
		// Mempool validation is not part of block import; surfaced so that a
		// window where it dominates the CPU is visible rather than confusing.
		line.push_str(&format!(
			" validate_tx_ms={:.3} validate_tx_count={}",
			delta.op(Op::ValidateTx).millis(),
			delta.op(Op::ValidateTx).count,
		));
		log::debug!(target: LOG_TARGET, "{line}");

		result
	}
}
