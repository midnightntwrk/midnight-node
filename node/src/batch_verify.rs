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

//! Native-direct batch ZK-proof verification.
//!
//! Shared machinery for the two batch-verification ingress points (the block-import wrapper in
//! [`crate::batch_block_import`] and — once landed — the mempool worker pool). It builds the
//! inputs the ledger's native batch entry point needs (ledger `state_key`, `BlockContext`,
//! `runtime_version`) from the client backend at a target block and calls
//! `midnight_node_ledger::host_api::ledger_9::batch_verify_transactions` **directly as native
//! Rust**, so the expensive aggregate crypto never crosses the WASM boundary.
//!
//! ## Correctness note
//!
//! Everything here is a best-effort *optimisation*: the caches it warms are re-checked downstream.
//! `get_verified_transaction` always re-runs the non-crypto `well_formed` checks with the
//! authoritative runtime `BlockContext` and only skips the (`BlockContext`-independent) ZK crypto
//! on a proof-cache hit. So an imperfect natively-assembled `BlockContext`, a stale `state_key`, or
//! a wrong ledger-version guess can only *reduce the hit-rate* (downstream falls back to inline
//! verification) — it can never cause an invalid transaction to be accepted.

use crate::service::FullClient;
use midnight_node_ledger::{ledger_9::BlockContext, types::active_version::LedgerApiError};
use midnight_node_runtime::opaque::Block;
use midnight_primitives_ledger::{
	LedgerMetrics, LedgerMetricsExt, LedgerStorage, LedgerStorageExt,
};
use parity_scale_codec::Decode;
use prometheus_endpoint::{
	Counter, CounterVec, Gauge, Histogram, HistogramOpts, Opts, Registry, U64, register,
};
use sc_client_api::StorageProvider;
use sp_api::{Core, ProvideRuntimeApi};
use sp_core::storage::StorageKey;
use sp_crypto_hashing::twox_128;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use sp_state_machine::BasicExternalities;
use std::sync::{Arc, Mutex};

const LOG_TARGET: &str = "midnight::batch_verify";

/// Minimum runtime `spec_version` at which the chain runs ledger 9 — the only ledger version with
/// the cross-transaction proof-batching primitives. Mirrors the toolkit's
/// `LedgerVersion::from_spec_version` mapping (`>= 2_000_000` ⇒ ledger 9). Blocks below this run an
/// older ledger and are never routed to the native (ledger-9) batch entry point; they fall back to
/// per-transaction inline verification downstream.
pub const LEDGER_9_MIN_SPEC_VERSION: u32 = 2_000_000;

/// Timestamp drift (seconds) the runtime allows; mirrors `pallet_midnight::get_block_context`.
const TBLOCK_DRIFT_SECS: u32 = 30;

/// Computes the storage key of a `StorageValue` given its pallet and item names.
fn storage_value_key(pallet: &[u8], item: &[u8]) -> StorageKey {
	let mut key = twox_128(pallet).to_vec();
	key.extend_from_slice(&twox_128(item));
	StorageKey(key)
}

type BlockHash = <Block as BlockT>::Hash;

/// Builds the native batch-verification inputs from the client backend and invokes the ledger's
/// native batch entry point.
#[derive(Clone)]
pub struct BatchVerifier {
	client: Arc<FullClient>,
	ledger_storage: LedgerStorage,
	ledger_metrics: Arc<Mutex<Option<LedgerMetrics>>>,
	metrics: BatchVerifyMetrics,
}

impl BatchVerifier {
	pub fn new(
		client: Arc<FullClient>,
		ledger_storage: LedgerStorage,
		ledger_metrics: Arc<Mutex<Option<LedgerMetrics>>>,
		metrics: BatchVerifyMetrics,
	) -> Self {
		Self { client, ledger_storage, ledger_metrics, metrics }
	}

	pub fn metrics(&self) -> &BatchVerifyMetrics {
		&self.metrics
	}

	/// Runtime `spec_version` at `at`, via the `Core` runtime API.
	pub fn spec_version_at(&self, at: BlockHash) -> Option<u32> {
		match self.client.runtime_api().version(at) {
			Ok(v) => Some(v.spec_version),
			Err(e) => {
				log::debug!(target: LOG_TARGET, "could not read runtime version at {at:?}: {e:?}");
				None
			},
		}
	}

	/// Whether the block at `at` runs ledger 9 (the only version with batch verification).
	pub fn is_ledger_9(&self, at: BlockHash) -> bool {
		self.spec_version_at(at).is_some_and(|v| v >= LEDGER_9_MIN_SPEC_VERSION)
	}

	/// Reads and decodes a `StorageValue` at `at`.
	fn read_value<T: Decode>(&self, at: BlockHash, pallet: &[u8], item: &[u8]) -> Option<T> {
		match self.client.storage(at, &storage_value_key(pallet, item)) {
			Ok(Some(data)) => T::decode(&mut &data.0[..]).ok(),
			Ok(None) => None,
			Err(e) => {
				log::debug!(
					target: LOG_TARGET,
					"could not read storage {}::{} at {at:?}: {e:?}",
					String::from_utf8_lossy(pallet),
					String::from_utf8_lossy(item),
				);
				None
			},
		}
	}

	/// The ledger arena `state_key` (`pallet_midnight::StateKey`) at `at`.
	pub fn state_key_at(&self, at: BlockHash) -> Option<Vec<u8>> {
		self.read_value::<Vec<u8>>(at, b"Midnight", b"StateKey")
	}

	/// Assembles a `BlockContext` from the state at `at`, duplicating
	/// `pallet_midnight::get_block_context` outside the runtime.
	///
	/// `extra_secs` is added to `tblock` to simulate future block time — the block-import path
	/// nudges it forward by roughly one slot, and the mempool path applies the larger
	/// skipped-slots margin. Because the result is only used for the non-crypto `well_formed`
	/// checks and re-checked downstream with the authoritative context, approximation is safe.
	pub fn block_context_at(&self, at: BlockHash, extra_secs: u64) -> BlockContext {
		let now_ms = self.read_value::<u64>(at, b"Timestamp", b"Now").unwrap_or(0);
		let now_s = now_ms / 1_000;
		let last_block_time =
			self.read_value::<u64>(at, b"Midnight", b"ParentTimestamp").unwrap_or(0);
		let parent_block_hash = self
			.client
			.header(at)
			.ok()
			.flatten()
			.map(|h| h.parent_hash().as_ref().to_vec())
			.unwrap_or_else(|| vec![0u8; 32]);

		BlockContext {
			tblock: now_s.saturating_add(extra_secs),
			tblock_err: TBLOCK_DRIFT_SECS,
			parent_block_hash,
			last_block_time,
		}
	}

	/// Builds a minimal externalities carrying the node's `LedgerStorageExt` (and
	/// `LedgerMetricsExt`), which the ledger's `set_default_storage` needs to connect the
	/// process-global content-addressed arena. No trie state view at `at` is required — the ledger
	/// state is loaded from the arena by `state_key`, not from the Substrate trie.
	fn build_externalities(&self) -> BasicExternalities {
		let mut ext = BasicExternalities::new_empty();
		ext.register_extension(LedgerStorageExt::new(self.ledger_storage.clone()));
		ext.register_extension(LedgerMetricsExt::new(self.ledger_metrics.clone()));
		ext
	}

	/// Batch-verifies the proofs of `txs` (serialized Midnight transactions) against the ledger
	/// state at `at`, warming the process-global proof/soft/strict caches on success.
	///
	/// Returns one result per input transaction (in order) on success. On failure it distinguishes
	/// two cases the callers must treat differently (see [`BatchVerifyError`]): a genuine invalid
	/// proof (the caller may reject the block) versus a setup/availability issue (the caller must
	/// fall back to inline verification, never reject).
	pub fn batch_verify(
		&self,
		at: BlockHash,
		txs: Vec<Vec<u8>>,
		isolate_on_failure: bool,
		extra_secs: u64,
	) -> Result<Vec<Result<(), LedgerApiError>>, BatchVerifyError> {
		if txs.is_empty() {
			return Ok(Vec::new());
		}

		let Some(spec_version) = self.spec_version_at(at) else {
			return Err(BatchVerifyError::Unavailable("could not resolve runtime version".into()));
		};
		if spec_version < LEDGER_9_MIN_SPEC_VERSION {
			return Err(BatchVerifyError::Unavailable(format!(
				"block at {at:?} runs a pre-ledger-9 runtime (spec_version {spec_version}); \
				 batch verification unsupported"
			)));
		}

		let Some(state_key) = self.state_key_at(at) else {
			return Err(BatchVerifyError::Unavailable("could not read ledger state_key".into()));
		};
		let block_context = self.block_context_at(at, extra_secs);

		let tx_count = txs.len();
		let mut ext = self.build_externalities();

		// Time the aggregate crypto call itself (not the node-side state setup above). Recording
		// here covers BOTH ingress paths, since the mempool's `BatchVerify::verify` delegates to
		// this method and block import calls it directly.
		let start = std::time::Instant::now();
		let result = midnight_node_ledger::host_api::ledger_9::batch_verify_transactions(
			&mut ext,
			&state_key,
			&txs,
			block_context,
			spec_version,
			isolate_on_failure,
		);
		self.metrics.observe_batch_duration(start.elapsed().as_secs_f64());

		match result {
			Ok(results) => {
				self.metrics.observe_batch(tx_count, true);
				Ok(results)
			},
			Err(e) => {
				self.metrics.observe_batch(tx_count, false);
				// A `Transaction` error is a genuine invalid/malformed proof surfaced by the
				// fail-fast aggregate check — the block contains a bad proof and may be rejected.
				// Any other error is a setup/availability issue (missing ledger state, decode
				// failure, etc.): the caller must fall back to inline verification, never reject a
				// possibly-valid block on it.
				match e {
					LedgerApiError::Transaction(_) => Err(BatchVerifyError::ProofInvalid),
					other => Err(BatchVerifyError::Unavailable(format!("{other:?}"))),
				}
			},
		}
	}
}

/// Outcome of a failed batch verification, distinguishing a genuine proof failure (safe to reject
/// the block) from a setup/availability issue (must fall back to inline verification).
#[derive(Debug)]
pub enum BatchVerifyError {
	/// The aggregate proof check failed: the batch contains an invalid or malformed proof.
	ProofInvalid,
	/// Batch verification could not be performed (missing state, unsupported version, etc.).
	/// Callers must fall back to per-transaction inline verification, not reject the block.
	Unavailable(String),
}

/// Prometheus metrics for batch verification, mirroring the `FilteringMetrics` pattern.
#[derive(Clone)]
pub struct BatchVerifyMetrics {
	batch_size: Option<Histogram>,
	batches_total: Option<CounterVec<U64>>,
	txs_total: Option<Counter<U64>>,
	queue_depth: Option<Gauge<U64>>,
	queue_rejected_total: Option<Counter<U64>>,
	/// Batches dispatched by the mempool worker, labelled by dispatch trigger (`k_target`/`tau`).
	dispatch_reason: Option<CounterVec<U64>>,
	/// Wall-clock time of a single aggregate batch-verification call (seconds).
	batch_duration: Option<Histogram>,
	/// Mempool batches that fell back to per-transaction runtime validation (unavailable).
	fallback_total: Option<Counter<U64>>,
}

const OUTCOME_SUCCESS: &str = "success";
const OUTCOME_FAILURE: &str = "failure";
const OUTCOMES: &[&str] = &[OUTCOME_SUCCESS, OUTCOME_FAILURE];

impl BatchVerifyMetrics {
	pub fn new(registry: Option<&Registry>) -> Self {
		let batch_size = registry.map(|r| {
			register(
				Histogram::with_opts(HistogramOpts::new(
					"midnight_batch_verify_batch_size",
					"Number of transactions per batch verification call",
				))
				.unwrap(),
				r,
			)
			.unwrap()
		});
		let batches_total = {
			let opts = Opts::new(
				"midnight_batch_verify_batches_total",
				"Total batch verification calls by outcome",
			);
			registry.map(|r| register(CounterVec::new(opts, &["outcome"]).unwrap(), r).unwrap())
		};
		let txs_total = registry.map(|r| {
			register(
				Counter::new(
					"midnight_batch_verify_txs_total",
					"Total transactions submitted to batch verification",
				)
				.unwrap(),
				r,
			)
			.unwrap()
		});
		let queue_depth = registry.map(|r| {
			register(
				Gauge::new(
					"midnight_batch_verify_queue_depth",
					"Current depth of the mempool batch-verification queue",
				)
				.unwrap(),
				r,
			)
			.unwrap()
		});
		let queue_rejected_total = registry.map(|r| {
			register(
				Counter::new(
					"midnight_batch_verify_queue_rejected_total",
					"Submissions shed because the bounded batch queue was full",
				)
				.unwrap(),
				r,
			)
			.unwrap()
		});
		let dispatch_reason = {
			let opts = Opts::new(
				"midnight_batch_verify_dispatch_reason_total",
				"Mempool batches dispatched, by trigger (k_target or tau)",
			);
			registry.map(|r| register(CounterVec::new(opts, &["trigger"]).unwrap(), r).unwrap())
		};
		let batch_duration = registry.map(|r| {
			register(
				Histogram::with_opts(HistogramOpts::new(
					"midnight_batch_verify_duration_seconds",
					"Wall-clock time of a single aggregate batch-verification call",
				))
				.unwrap(),
				r,
			)
			.unwrap()
		});
		let fallback_total = registry.map(|r| {
			register(
				Counter::new(
					"midnight_batch_verify_fallback_total",
					"Mempool batches that fell back to per-transaction runtime validation",
				)
				.unwrap(),
				r,
			)
			.unwrap()
		});
		let _ = OUTCOMES;
		Self {
			batch_size,
			batches_total,
			txs_total,
			queue_depth,
			queue_rejected_total,
			dispatch_reason,
			batch_duration,
			fallback_total,
		}
	}

	/// Records a completed batch verification call.
	pub fn observe_batch(&self, size: usize, success: bool) {
		if let Some(h) = &self.batch_size {
			h.observe(size as f64);
		}
		if let Some(c) = &self.txs_total {
			c.inc_by(size as u64);
		}
		if let Some(c) = &self.batches_total {
			let outcome = if success { OUTCOME_SUCCESS } else { OUTCOME_FAILURE };
			let _ = c.get_metric_with_label_values(&[outcome]).map(|m| m.inc());
		}
	}

	/// Sets the current mempool queue depth gauge.
	pub fn set_queue_depth(&self, depth: u64) {
		if let Some(g) = &self.queue_depth {
			g.set(depth);
		}
	}

	/// Increments the count of submissions shed due to a full queue.
	pub fn inc_queue_rejected(&self) {
		if let Some(c) = &self.queue_rejected_total {
			c.inc();
		}
	}

	/// Records that a mempool batch was dispatched, labelled by its trigger (`k_target`/`tau`).
	pub fn observe_dispatch(&self, trigger: &str) {
		if let Some(c) = &self.dispatch_reason {
			let _ = c.get_metric_with_label_values(&[trigger]).map(|m| m.inc());
		}
	}

	/// Records the wall-clock duration (seconds) of one aggregate batch-verification call.
	pub fn observe_batch_duration(&self, secs: f64) {
		if let Some(h) = &self.batch_duration {
			h.observe(secs);
		}
	}

	/// Increments the count of mempool batches that fell back to per-transaction runtime validation.
	pub fn inc_fallback(&self) {
		if let Some(c) = &self.fallback_total {
			c.inc();
		}
	}
}
