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

//! Mempool batch-verification ingress: a custom [`ChainApi`] plus a bounded queue and blocking
//! worker pool that batches external Midnight submissions.
//!
//! [`MidnightChainApi`] wraps the stock [`FullChainApi`] and pins its associated types to it, so a
//! [`sc_transaction_pool::BasicPool`] built over it behaves byte-identically to the stock pool for
//! every projection. It overrides only `validate_transaction`: for **external** Midnight
//! `send_mn_transaction` extrinsics it routes the submission through a queue that a pool of blocking
//! workers drains in batches, calling the native batch-verification entry point
//! ([`crate::batch_verify::BatchVerifier`]) once per batch to warm the process-global proof/soft/
//! strict caches. Every other case (batching disabled, `Local`/`InBlock` source, non-Midnight
//! extrinsic) is delegated verbatim to the inner `FullChainApi`, so with the feature off the pool is
//! exactly the stock pool.
//!
//! ## Safety
//!
//! The batch path only ever *warms caches* and *builds the same validity tags the runtime would*.
//! On any batch failure, unavailability, or a per-transaction rejection, the worker asks the caller
//! to [`WorkerOutcome::Delegate`] to the runtime, which produces the authoritative result (fast,
//! since the caches are warm). So batching can only turn an expensive per-tx runtime validation into
//! a cheap one — it can never accept a transaction the runtime would reject.

use crate::batch_verify::{BatchVerifier, BatchVerifyError, BatchVerifyMetrics};
use async_trait::async_trait;
use midnight_node_ledger::types::active_version::LedgerApiError;
use parity_scale_codec::{Decode, Encode};
use sc_transaction_pool::{ChainApi, FullChainApi, ValidateTransactionPriority};
use sc_transaction_pool_api::error::Error as TxPoolError;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::{HeaderMetadata, TreeRoute};
use sp_core::traits::SpawnEssentialNamed;
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, BlockIdTo, NumberFor},
	transaction_validity::{TransactionSource, TransactionValidity, ValidTransaction},
};
use sp_transaction_pool::runtime_api::TaggedTransactionQueue;
use std::sync::Arc;
use tokio::{
	sync::{Mutex, mpsc, oneshot},
	time::{Duration, Instant, timeout},
};

const LOG_TARGET: &str = "midnight::batch_verify";

/// Seconds added to the parent block's timestamp when the mempool worker assembles the batch
/// `BlockContext`. Mirrors `pallet_midnight::validate_unsigned`, which bumps `tblock` by one slot
/// plus the skipped-slots margin (`SLOT_DURATION`/1000 · (1 + MaxSkippedSlots)) so a transaction
/// near the edge of its dust-validity window is not falsely rejected while blocks are being
/// produced. With a 6 s slot and the default `MaxSkippedSlots = 1` this is `6 · (1 + 1) = 12`. Only
/// affects the non-crypto `well_formed` checks, which the runtime re-runs authoritatively, so an
/// approximation only shifts the mempool accept/reject boundary — never block validity.
const MEMPOOL_TBLOCK_EXTRA_SECS: u64 = 12;

/// Injectable batch-verification backend, so the worker pool can be unit-tested with a stub instead
/// of a real client + ledger. `H` is the block-hash type (`<Block as BlockT>::Hash`).
///
/// The real implementation is [`BatchVerifier`]; it computes proof results natively and warms the
/// process-global caches. Tests supply a stub that returns scripted results.
pub trait BatchVerify<H>: Send + Sync {
	/// Batch-verifies `txs` (serialized Midnight transactions) against the ledger state at `at`,
	/// warming the caches. Returns one result per input transaction on success (see
	/// [`BatchVerifier::batch_verify`]).
	fn verify(
		&self,
		at: H,
		txs: Vec<Vec<u8>>,
		isolate_on_failure: bool,
		extra_secs: u64,
	) -> Result<Vec<Result<(), LedgerApiError>>, BatchVerifyError>;

	/// Runtime `spec_version` at `at`, needed to build the `and_provides` validity tag. `None` when
	/// unavailable (the worker then delegates that transaction to the runtime).
	fn runtime_version(&self, at: H) -> Option<u32>;
}

impl BatchVerify<<midnight_node_runtime::opaque::Block as BlockT>::Hash> for BatchVerifier {
	fn verify(
		&self,
		at: <midnight_node_runtime::opaque::Block as BlockT>::Hash,
		txs: Vec<Vec<u8>>,
		isolate_on_failure: bool,
		extra_secs: u64,
	) -> Result<Vec<Result<(), LedgerApiError>>, BatchVerifyError> {
		self.batch_verify(at, txs, isolate_on_failure, extra_secs)
	}

	fn runtime_version(
		&self,
		at: <midnight_node_runtime::opaque::Block as BlockT>::Hash,
	) -> Option<u32> {
		self.spec_version_at(at)
	}
}

/// Tunables for the mempool batch-verification queue + worker pool (from `MidnightCfg`).
#[derive(Debug, Clone, Copy)]
pub struct MempoolBatchConfig {
	/// Number of blocking worker tasks (N).
	pub workers: usize,
	/// Dispatch a batch as soon as this many transactions are queued (k_target).
	pub target_batch_size: usize,
	/// Maximum number of transactions verified in one aggregate call (M).
	pub max_batch_size: usize,
	/// Maximum time a transaction waits before a partial batch is dispatched (tau).
	pub max_age: Duration,
	/// Bounded queue capacity; submissions beyond this are shed.
	pub queue_capacity: usize,
}

/// The worker's verdict for one queued transaction, delivered back to the parked
/// `validate_transaction` call via a `oneshot`.
enum WorkerOutcome {
	/// Native validity built from the batch result (equivalent to the runtime's).
	Validated(TransactionValidity),
	/// The worker could not batch-verify this transaction (unavailable, rejected, or missing
	/// runtime version); the caller must delegate to the runtime for the authoritative result.
	Delegate,
}

/// A submission parked on the batch-verification queue.
struct QueueItem<Block: BlockT> {
	at: <Block as BlockT>::Hash,
	/// The `send_mn_transaction` payload (the Midnight transaction bytes).
	tx_bytes: Vec<u8>,
	enqueued_at: Instant,
	reply: oneshot::Sender<WorkerOutcome>,
}

/// Why a batch was dispatched (recorded as the `trigger` metric label).
#[derive(Clone, Copy)]
enum Trigger {
	/// The target batch size was reached.
	KTarget,
	/// The oldest transaction hit the max-age timeout.
	Tau,
	/// The queue was closed (node shutting down); flush what we have.
	Closed,
}

impl Trigger {
	fn label(self) -> &'static str {
		match self {
			Trigger::KTarget => "k_target",
			Trigger::Tau | Trigger::Closed => "tau",
		}
	}
}

/// Immutable per-worker parameters derived from [`MempoolBatchConfig`].
#[derive(Clone, Copy)]
struct BatchParams {
	k_target: usize,
	max_batch: usize,
	tau: Duration,
}

/// Owns the bounded queue and spawns the blocking worker pool.
pub struct MempoolBatcher<Block: BlockT> {
	queue_tx: mpsc::Sender<QueueItem<Block>>,
	metrics: BatchVerifyMetrics,
}

impl<Block: BlockT> MempoolBatcher<Block> {
	/// Builds the queue and spawns `cfg.workers` blocking worker tasks on `spawner`.
	pub fn new(
		spawner: &impl SpawnEssentialNamed,
		verifier: Arc<dyn BatchVerify<<Block as BlockT>::Hash>>,
		cfg: MempoolBatchConfig,
		metrics: BatchVerifyMetrics,
	) -> Self {
		let (queue_tx, rx) = mpsc::channel::<QueueItem<Block>>(cfg.queue_capacity.max(1));
		let rx = Arc::new(Mutex::new(rx));
		let params = BatchParams {
			k_target: cfg.target_batch_size.max(1),
			max_batch: cfg.max_batch_size.max(1),
			tau: cfg.max_age,
		};
		for _ in 0..cfg.workers.max(1) {
			let rx = rx.clone();
			let verifier = verifier.clone();
			let metrics = metrics.clone();
			spawner.spawn_essential_blocking(
				"midnight-mempool-verify",
				Some("transaction-pool"),
				Box::pin(run_worker::<Block>(rx, verifier, params, metrics)),
			);
		}
		Self { queue_tx, metrics }
	}

	/// Enqueues a submission, returning the `oneshot` the caller awaits. `Err(())` on a full queue
	/// (the caller sheds the submission with `ImmediatelyDropped`).
	fn enqueue(
		&self,
		at: <Block as BlockT>::Hash,
		tx_bytes: Vec<u8>,
	) -> Result<oneshot::Receiver<WorkerOutcome>, ()> {
		let (reply, reply_rx) = oneshot::channel();
		let item = QueueItem { at, tx_bytes, enqueued_at: Instant::now(), reply };
		match self.queue_tx.try_send(item) {
			Ok(()) => {
				let depth =
					self.queue_tx.max_capacity().saturating_sub(self.queue_tx.capacity()) as u64;
				self.metrics.set_queue_depth(depth);
				Ok(reply_rx)
			},
			Err(_) => {
				self.metrics.inc_queue_rejected();
				Err(())
			},
		}
	}
}

/// One blocking worker: forms batches from the shared queue and verifies each.
async fn run_worker<Block: BlockT>(
	rx: Arc<Mutex<mpsc::Receiver<QueueItem<Block>>>>,
	verifier: Arc<dyn BatchVerify<<Block as BlockT>::Hash>>,
	params: BatchParams,
	metrics: BatchVerifyMetrics,
) {
	loop {
		// Phase 0: block (holding the lock only while idle-waiting with an empty batch) for the
		// first item. A closed channel ends the worker.
		let first = {
			let mut guard = rx.lock().await;
			match guard.recv().await {
				Some(item) => item,
				None => return,
			}
		};
		let deadline = first.enqueued_at + params.tau;
		let mut batch = vec![first];

		// Phase 1: accumulate until the target size is reached or the oldest item hits tau. Each
		// wait is bounded by the remaining time to the deadline, and the lock is released between
		// attempts so other workers make progress.
		let trigger = loop {
			if batch.len() >= params.k_target {
				break Trigger::KTarget;
			}
			let now = Instant::now();
			if now >= deadline {
				break Trigger::Tau;
			}
			let remaining = deadline - now;
			let recv_one = async {
				let mut guard = rx.lock().await;
				guard.recv().await
			};
			match timeout(remaining, recv_one).await {
				Ok(Some(item)) => batch.push(item),
				Ok(None) => break Trigger::Closed,
				Err(_) => break Trigger::Tau,
			}
		};

		// Phase 2: greedily drain any further immediately-available items, capped at M.
		{
			let mut guard = rx.lock().await;
			while batch.len() < params.max_batch {
				match guard.try_recv() {
					Ok(item) => batch.push(item),
					Err(_) => break,
				}
			}
		}

		metrics.observe_dispatch(trigger.label());
		process_batch::<Block>(&*verifier, batch, &metrics);

		if matches!(trigger, Trigger::Closed) {
			return;
		}
	}
}

/// Verifies one drained batch: groups by target block, runs one aggregate verification per group,
/// and resolves each parked `oneshot`.
fn process_batch<Block: BlockT>(
	verifier: &dyn BatchVerify<<Block as BlockT>::Hash>,
	batch: Vec<QueueItem<Block>>,
	metrics: &BatchVerifyMetrics,
) {
	for (at, items) in group_by_at::<Block>(batch) {
		let runtime_version = verifier.runtime_version(at);
		let txs: Vec<Vec<u8>> = items.iter().map(|i| i.tx_bytes.clone()).collect();

		// Duration is recorded inside `BatchVerifier::batch_verify` (which `verify` delegates to),
		// so it is captured here for the mempool and equally on the block-import path.
		let outcome =
			verifier.verify(at, txs, /* isolate_on_failure */ true, MEMPOOL_TBLOCK_EXTRA_SECS);

		match outcome {
			Ok(results) => {
				for (idx, item) in items.into_iter().enumerate() {
					// A verified proof with a known runtime version → native validity tag. Anything
					// else (per-tx rejection, or no runtime version to tag with) → delegate to the
					// runtime for the authoritative result.
					let out = match (results.get(idx), runtime_version) {
						(Some(Ok(())), Some(v)) => {
							WorkerOutcome::Validated(success_validity(v, &item.tx_bytes))
						},
						_ => WorkerOutcome::Delegate,
					};
					let _ = item.reply.send(out);
				}
			},
			Err(reason) => {
				// Setup/availability failure (or, defensively, a fail-fast proof error): never
				// reject — delegate every parked submission to the runtime.
				log::debug!(
					target: LOG_TARGET,
					"mempool batch verification unavailable for {} tx(s), delegating to runtime: {reason:?}",
					items.len(),
				);
				metrics.inc_fallback();
				for item in items {
					let _ = item.reply.send(WorkerOutcome::Delegate);
				}
			},
		}
	}
}

/// Groups a batch by target block hash, preserving first-seen order. Batches are small (≤ M) and
/// almost always share one `at`, so the linear grouping is cheap.
fn group_by_at<Block: BlockT>(
	batch: Vec<QueueItem<Block>>,
) -> Vec<(<Block as BlockT>::Hash, Vec<QueueItem<Block>>)> {
	let mut groups: Vec<(<Block as BlockT>::Hash, Vec<QueueItem<Block>>)> = Vec::new();
	for item in batch {
		if let Some(group) = groups.iter_mut().find(|(hash, _)| *hash == item.at) {
			group.1.push(item);
		} else {
			groups.push((item.at, vec![item]));
		}
	}
	groups
}

/// Builds the `TransactionValidity` for a successfully batch-verified transaction, byte-for-byte
/// matching `pallet_midnight::validate_unsigned` (`with_tag_prefix("Midnight").longevity(600)
/// .and_provides(tx_hash)`), where `tx_hash` is the state-independent `tx_validation_cache_key`.
fn success_validity(runtime_version: u32, tx_bytes: &[u8]) -> TransactionValidity {
	let tx_hash = tx_validation_cache_key(runtime_version, tx_bytes);
	ValidTransaction::with_tag_prefix("Midnight")
		.longevity(600)
		.and_provides(tx_hash)
		.build()
}

/// Recomputes the ledger's `tx_validation_cache_key` natively: `Twox128(runtime_version_le ++
/// tx_bytes)` zero-extended to 32 bytes. Must stay in sync with
/// `Bridge::tx_validation_cache_key` in the ledger crate (and the `and_provides` tag the runtime
/// builds), or the pool's provides-tag would not match across the native and runtime paths.
fn tx_validation_cache_key(runtime_version: u32, tx_bytes: &[u8]) -> [u8; 32] {
	let mut input = runtime_version.to_le_bytes().to_vec();
	input.extend_from_slice(tx_bytes);
	let hash16 = sp_crypto_hashing::twox_128(&input);
	let mut out = [0u8; 32];
	out[..16].copy_from_slice(&hash16);
	out
}

/// Extracts the `send_mn_transaction` payload from an extrinsic, mirroring the decode/match in
/// `filtering_pool`. Returns `None` for any non-Midnight extrinsic (which is delegated verbatim).
fn extract_send_mn_transaction<Block: BlockT>(
	uxt: &<Block as BlockT>::Extrinsic,
) -> Option<Vec<u8>> {
	let decoded = midnight_node_runtime::UncheckedExtrinsic::decode(&mut &uxt.encode()[..]).ok()?;
	match decoded.function {
		midnight_node_runtime::RuntimeCall::Midnight(
			midnight_node_runtime::MidnightCall::send_mn_transaction { midnight_tx },
		) => Some(midnight_tx),
		_ => None,
	}
}

/// A [`ChainApi`] that batch-verifies external Midnight submissions, wrapping the stock
/// [`FullChainApi`]. See the module docs.
pub struct MidnightChainApi<Client, Block: BlockT> {
	inner: Arc<FullChainApi<Client, Block>>,
	/// `Some` when mempool batching is enabled; `None` → always delegate to `inner`.
	batcher: Option<MempoolBatcher<Block>>,
}

impl<Client, Block: BlockT> MidnightChainApi<Client, Block> {
	pub fn new(
		inner: Arc<FullChainApi<Client, Block>>,
		batcher: Option<MempoolBatcher<Block>>,
	) -> Self {
		Self { inner, batcher }
	}
}

#[async_trait]
impl<Client, Block> ChainApi for MidnightChainApi<Client, Block>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>
		+ sc_client_api::BlockBackend<Block>
		+ BlockIdTo<Block>
		+ sc_client_api::blockchain::HeaderBackend<Block>
		+ HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ Send
		+ Sync
		+ 'static,
	Client::Api: TaggedTransactionQueue<Block>,
{
	type Block = Block;
	type Error = <FullChainApi<Client, Block> as ChainApi>::Error;

	async fn validate_transaction(
		&self,
		at: <Block as BlockT>::Hash,
		source: TransactionSource,
		uxt: Arc<<Block as BlockT>::Extrinsic>,
		validation_priority: ValidateTransactionPriority,
	) -> Result<TransactionValidity, Self::Error> {
		// Only external submissions are batched. Local/InBlock sources and the disabled case
		// delegate verbatim (the stock behaviour).
		let batcher = match &self.batcher {
			Some(batcher) if source == TransactionSource::External => batcher,
			_ => {
				return self.inner.validate_transaction(at, source, uxt, validation_priority).await;
			},
		};

		// Non-Midnight extrinsics are delegated verbatim.
		let Some(tx_bytes) = extract_send_mn_transaction::<Block>(&uxt) else {
			return self.inner.validate_transaction(at, source, uxt, validation_priority).await;
		};

		match batcher.enqueue(at, tx_bytes) {
			Ok(reply_rx) => match reply_rx.await {
				Ok(WorkerOutcome::Validated(validity)) => Ok(validity),
				// Worker asked us to delegate, or died before replying: fall back to the runtime.
				Ok(WorkerOutcome::Delegate) | Err(_) => {
					self.inner.validate_transaction(at, source, uxt, validation_priority).await
				},
			},
			// Bounded queue full: shed the submission (retriable).
			Err(()) => Err(TxPoolError::ImmediatelyDropped.into()),
		}
	}

	fn validate_transaction_blocking(
		&self,
		at: <Block as BlockT>::Hash,
		source: TransactionSource,
		uxt: Arc<<Block as BlockT>::Extrinsic>,
	) -> Result<TransactionValidity, Self::Error> {
		self.inner.validate_transaction_blocking(at, source, uxt)
	}

	fn block_id_to_number(
		&self,
		at: &BlockId<Block>,
	) -> Result<Option<NumberFor<Block>>, Self::Error> {
		self.inner.block_id_to_number(at)
	}

	fn block_id_to_hash(
		&self,
		at: &BlockId<Block>,
	) -> Result<Option<<Block as BlockT>::Hash>, Self::Error> {
		self.inner.block_id_to_hash(at)
	}

	fn hash_and_length(
		&self,
		uxt: &<Block as BlockT>::Extrinsic,
	) -> (<Block as BlockT>::Hash, usize) {
		self.inner.hash_and_length(uxt)
	}

	async fn block_body(
		&self,
		at: <Block as BlockT>::Hash,
	) -> Result<Option<Vec<<Block as BlockT>::Extrinsic>>, Self::Error> {
		self.inner.block_body(at).await
	}

	fn block_header(
		&self,
		at: <Block as BlockT>::Hash,
	) -> Result<Option<<Block as BlockT>::Header>, Self::Error> {
		self.inner.block_header(at)
	}

	fn tree_route(
		&self,
		from: <Block as BlockT>::Hash,
		to: <Block as BlockT>::Hash,
	) -> Result<TreeRoute<Block>, Self::Error> {
		self.inner.tree_route(from, to)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use midnight_node_runtime::opaque::Block as OpaqueBlock;
	use sp_core::H256;
	use std::sync::Mutex as StdMutex;

	type Hash = <OpaqueBlock as BlockT>::Hash;

	/// Scripted stub: maps each transaction (by its bytes) to a proof result and returns them in
	/// order. Records the batch sizes it was called with.
	struct StubVerifier {
		/// `tx_bytes -> Ok(())` (valid) or `Err(..)` (invalid). Missing → treated as valid.
		results: std::collections::HashMap<Vec<u8>, Result<(), LedgerApiError>>,
		/// If set, `verify` returns this availability error instead of per-tx results.
		unavailable: bool,
		runtime_version: Option<u32>,
		batch_sizes: Arc<StdMutex<Vec<usize>>>,
	}

	impl StubVerifier {
		fn new() -> Self {
			Self {
				results: Default::default(),
				unavailable: false,
				runtime_version: Some(2_000_000),
				batch_sizes: Arc::new(StdMutex::new(Vec::new())),
			}
		}
	}

	impl BatchVerify<Hash> for StubVerifier {
		fn verify(
			&self,
			_at: Hash,
			txs: Vec<Vec<u8>>,
			_isolate_on_failure: bool,
			_extra_secs: u64,
		) -> Result<Vec<Result<(), LedgerApiError>>, BatchVerifyError> {
			self.batch_sizes.lock().unwrap().push(txs.len());
			if self.unavailable {
				return Err(BatchVerifyError::Unavailable("stub".into()));
			}
			Ok(txs.iter().map(|tx| self.results.get(tx).cloned().unwrap_or(Ok(()))).collect())
		}

		fn runtime_version(&self, _at: Hash) -> Option<u32> {
			self.runtime_version
		}
	}

	fn params(k_target: usize, max_batch: usize, tau_ms: u64) -> BatchParams {
		BatchParams { k_target, max_batch, tau: Duration::from_millis(tau_ms) }
	}

	fn queue_item(
		at: Hash,
		tx_bytes: Vec<u8>,
	) -> (QueueItem<OpaqueBlock>, oneshot::Receiver<WorkerOutcome>) {
		let (reply, reply_rx) = oneshot::channel();
		(QueueItem { at, tx_bytes, enqueued_at: Instant::now(), reply }, reply_rx)
	}

	fn is_validated(outcome: &WorkerOutcome) -> bool {
		matches!(outcome, WorkerOutcome::Validated(Ok(_)))
	}

	#[test]
	fn success_validity_matches_runtime_tag_shape() {
		let v = success_validity(2_000_000, b"some-tx-bytes").expect("valid tx must build Ok");
		assert_eq!(v.longevity, 600, "longevity must mirror the pallet");
		assert_eq!(v.provides.len(), 1, "exactly one provides tag");
		// The provides tag is the "Midnight" prefix ++ the 32-byte cache key.
		assert!(v.provides[0].ends_with(&tx_validation_cache_key(2_000_000, b"some-tx-bytes")));
	}

	#[test]
	fn group_by_at_preserves_order_and_partitions() {
		let a = H256::repeat_byte(0xAA);
		let b = H256::repeat_byte(0xBB);
		let batch =
			vec![queue_item(a, vec![1]).0, queue_item(b, vec![2]).0, queue_item(a, vec![3]).0];
		let groups = group_by_at::<OpaqueBlock>(batch);
		assert_eq!(groups.len(), 2, "two distinct target blocks");
		assert_eq!(groups[0].0, a);
		assert_eq!(groups[0].1.len(), 2, "both `a` items grouped");
		assert_eq!(groups[1].0, b);
		assert_eq!(groups[1].1.len(), 1);
	}

	#[tokio::test]
	async fn worker_dispatches_at_k_target() {
		let at = H256::repeat_byte(1);
		let verifier = Arc::new(StubVerifier::new());
		let sizes = verifier.batch_sizes.clone();
		let (tx, rx) = mpsc::channel::<QueueItem<OpaqueBlock>>(64);
		// tau is long; the k_target=3 trigger must fire well before it.
		let worker = tokio::spawn(run_worker::<OpaqueBlock>(
			Arc::new(Mutex::new(rx)),
			verifier,
			params(3, 64, 60_000),
			BatchVerifyMetrics::new(None),
		));

		let mut replies = Vec::new();
		for i in 0..3u8 {
			let (item, reply_rx) = queue_item(at, vec![i]);
			tx.send(item).await.unwrap();
			replies.push(reply_rx);
		}

		for reply_rx in replies {
			let outcome = reply_rx.await.expect("worker must resolve the oneshot");
			assert!(is_validated(&outcome), "all three valid txs must be Validated");
		}
		// One aggregate call for all three (k_target reached).
		assert_eq!(sizes.lock().unwrap().as_slice(), &[3]);
		drop(tx);
		let _ = worker.await;
	}

	#[tokio::test]
	async fn worker_dispatches_at_tau() {
		let at = H256::repeat_byte(2);
		let verifier = Arc::new(StubVerifier::new());
		let sizes = verifier.batch_sizes.clone();
		let (tx, rx) = mpsc::channel::<QueueItem<OpaqueBlock>>(64);
		// k_target is unreachable with one tx; it must dispatch on the (short, real) tau timeout.
		let worker = tokio::spawn(run_worker::<OpaqueBlock>(
			Arc::new(Mutex::new(rx)),
			verifier,
			params(100, 64, 30),
			BatchVerifyMetrics::new(None),
		));

		let (item, reply_rx) = queue_item(at, vec![7]);
		tx.send(item).await.unwrap();

		let outcome = reply_rx.await.expect("worker must resolve on tau");
		assert!(is_validated(&outcome), "the single valid tx must be Validated");
		assert_eq!(sizes.lock().unwrap().as_slice(), &[1], "one tx dispatched on tau");
		drop(tx);
		let _ = worker.await;
	}

	#[tokio::test]
	async fn worker_delegates_invalid_and_unavailable() {
		let at = H256::repeat_byte(3);

		// A batch with one bad tx: the bad one delegates, the good ones validate.
		let mut verifier = StubVerifier::new();
		verifier.results.insert(
			vec![9],
			Err(LedgerApiError::Transaction(
				midnight_node_ledger::types::active_version::TransactionError::Invalid(
					midnight_node_ledger::types::active_version::InvalidError::UnknownError,
				),
			)),
		);
		let verifier = Arc::new(verifier);
		let (tx, rx) = mpsc::channel::<QueueItem<OpaqueBlock>>(64);
		let worker = tokio::spawn(run_worker::<OpaqueBlock>(
			Arc::new(Mutex::new(rx)),
			verifier,
			params(2, 64, 60_000),
			BatchVerifyMetrics::new(None),
		));

		let (good, good_rx) = queue_item(at, vec![8]);
		let (bad, bad_rx) = queue_item(at, vec![9]);
		tx.send(good).await.unwrap();
		tx.send(bad).await.unwrap();

		assert!(is_validated(&good_rx.await.unwrap()), "good tx validates");
		assert!(
			matches!(bad_rx.await.unwrap(), WorkerOutcome::Delegate),
			"rejected tx delegates to the runtime"
		);
		drop(tx);
		let _ = worker.await;
	}

	#[tokio::test]
	async fn worker_unavailable_delegates_all() {
		let at = H256::repeat_byte(4);
		let mut verifier = StubVerifier::new();
		verifier.unavailable = true;
		let verifier = Arc::new(verifier);
		let (tx, rx) = mpsc::channel::<QueueItem<OpaqueBlock>>(64);
		let worker = tokio::spawn(run_worker::<OpaqueBlock>(
			Arc::new(Mutex::new(rx)),
			verifier,
			params(1, 64, 60_000),
			BatchVerifyMetrics::new(None),
		));

		let (item, reply_rx) = queue_item(at, vec![5]);
		tx.send(item).await.unwrap();
		assert!(
			matches!(reply_rx.await.unwrap(), WorkerOutcome::Delegate),
			"availability failure must delegate, never reject"
		);
		drop(tx);
		let _ = worker.await;
	}

	#[tokio::test(flavor = "current_thread")]
	async fn enqueue_sheds_on_full_queue() {
		// Capacity 1, no workers draining: the second enqueue must be shed.
		let (queue_tx, _rx) = mpsc::channel::<QueueItem<OpaqueBlock>>(1);
		let batcher: MempoolBatcher<OpaqueBlock> =
			MempoolBatcher { queue_tx, metrics: BatchVerifyMetrics::new(None) };

		let at = H256::repeat_byte(6);
		assert!(batcher.enqueue(at, vec![1]).is_ok(), "first fits");
		assert!(batcher.enqueue(at, vec![2]).is_err(), "second is shed (queue full)");
	}
}
