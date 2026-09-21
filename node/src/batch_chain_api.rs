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
	sync::{mpsc, oneshot},
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
/// An opaque prepared transaction as it travels through the queue.
///
/// `BatchVerifier::prepare` returns a ledger type parameterised by the storage-mode DB, which the
/// node picks at runtime; boxing it here keeps [`MempoolBatcher`] and [`MidnightChainApi`] free of
/// a type parameter that would otherwise ripple through the pool and the service wiring — and lets
/// the unit tests below drive the pool with a stub that has no ledger behind it at all.
pub type PreparedHandle = Box<dyn std::any::Any + Send>;

pub trait BatchVerify<H>: Send + Sync {
	/// Runs the per-transaction half of batch verification: everything that does not depend on
	/// which other transactions share the batch. Called as each submission arrives, so the work is
	/// done during the queue's accumulation window rather than at dispatch.
	fn prepare(
		&self,
		at: H,
		tx_bytes: &[u8],
		extra_secs: u64,
	) -> Result<PreparedHandle, BatchVerifyError>;

	/// Decides a batch built by [`Self::prepare`]: one fold plus a single pairing check, at a cost
	/// essentially independent of the batch size.
	fn finalize(
		&self,
		at: H,
		prepared: Vec<PreparedHandle>,
		isolate_on_failure: bool,
		extra_secs: u64,
	) -> Result<Vec<Result<(), LedgerApiError>>, BatchVerifyError>;

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

	/// Whether the runtime's mempool soft cache already holds a successful validation for
	/// `tx_bytes` under `runtime_version`.
	///
	/// The pool revalidates everything it holds on every block import. The inline path answers
	/// those from this cache on its first line; the batch path has no such check and re-runs the
	/// full per-proof preparation each time, so without this its cost scales with how long
	/// transactions sit in the pool rather than with how many were submitted.
	fn soft_cache_hit(&self, runtime_version: u32, tx_bytes: &[u8]) -> bool;
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

	fn prepare(
		&self,
		at: <midnight_node_runtime::opaque::Block as BlockT>::Hash,
		tx_bytes: &[u8],
		extra_secs: u64,
	) -> Result<PreparedHandle, BatchVerifyError> {
		self.prepare(at, tx_bytes, extra_secs).map(|p| Box::new(p) as PreparedHandle)
	}

	fn finalize(
		&self,
		at: <midnight_node_runtime::opaque::Block as BlockT>::Hash,
		prepared: Vec<PreparedHandle>,
		isolate_on_failure: bool,
		extra_secs: u64,
	) -> Result<Vec<Result<(), LedgerApiError>>, BatchVerifyError> {
		let mut items = Vec::with_capacity(prepared.len());
		for p in prepared {
			match p.downcast::<midnight_node_ledger::host_api::ledger_9::PreparedTransaction>() {
				Ok(p) => items.push(*p),
				// Defensive: only this impl ever puts values in, so a mismatch is a bug rather
				// than a condition. Fall back rather than reject a possibly-valid transaction.
				Err(_) => {
					return Err(BatchVerifyError::Unavailable(
						"prepared transaction of unexpected type".into(),
					));
				},
			}
		}
		self.finalize(at, items, isolate_on_failure, extra_secs)
	}

	fn runtime_version(
		&self,
		at: <midnight_node_runtime::opaque::Block as BlockT>::Hash,
	) -> Option<u32> {
		self.spec_version_at(at)
	}

	fn soft_cache_hit(&self, runtime_version: u32, tx_bytes: &[u8]) -> bool {
		let key = midnight_node_ledger::host_api::ledger_9::tx_validation_cache_key(
			runtime_version,
			tx_bytes,
		);
		midnight_node_ledger::host_api::ledger_9::soft_validation_hit(&key)
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

/// A queued submission whose per-transaction verification work is already done, waiting only for
/// the batch it will be folded into.
struct PreparedItem<Block: BlockT> {
	at: <Block as BlockT>::Hash,
	tx_bytes: Vec<u8>,
	prepared: PreparedHandle,
	reply: oneshot::Sender<WorkerOutcome>,
}

/// A fully-formed batch of prepared submissions, handed from the dispatcher to a folding worker.
type Batch<Block> = Vec<PreparedItem<Block>>;

/// Owns the bounded queue and spawns the batch dispatcher plus its blocking worker pool.
pub struct MempoolBatcher<Block: BlockT> {
	queue_tx: mpsc::Sender<QueueItem<Block>>,
	metrics: BatchVerifyMetrics,
}

impl<Block: BlockT> MempoolBatcher<Block> {
	/// Builds the queue and spawns one dispatcher plus `cfg.workers` blocking verification
	/// workers on `spawner`.
	pub fn new(
		spawner: &impl SpawnEssentialNamed,
		verifier: Arc<dyn BatchVerify<<Block as BlockT>::Hash>>,
		cfg: MempoolBatchConfig,
		metrics: BatchVerifyMetrics,
	) -> Self {
		let (queue_tx, queue_rx) = mpsc::channel::<QueueItem<Block>>(cfg.queue_capacity.max(1));
		let workers = cfg.workers.max(1);
		// Formed batches, dispatcher → workers. `async_channel` is MPMC, so each worker owns its
		// own receiver clone and polls it directly; nothing is shared behind a lock, which is what
		// makes the hand-off deadlock-free (see `run_dispatcher`).
		let (batch_tx, batch_rx) = async_channel::bounded::<Batch<Block>>(workers);
		let params = BatchParams {
			k_target: cfg.target_batch_size.max(1),
			max_batch: cfg.max_batch_size.max(1),
			tau: cfg.max_age,
		};

		spawner.spawn_essential_blocking(
			"midnight-mempool-batcher",
			Some("transaction-pool"),
			Box::pin(run_dispatcher::<Block>(
				queue_rx,
				batch_tx,
				verifier.clone(),
				params,
				metrics.clone(),
			)),
		);
		for _ in 0..workers {
			spawner.spawn_essential_blocking(
				"midnight-mempool-verify",
				Some("transaction-pool"),
				Box::pin(run_worker::<Block>(batch_rx.clone(), verifier.clone(), metrics.clone())),
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

/// Forms batches from the submission queue and hands each to a verification worker.
///
/// Owns `queue_rx` outright, and that exclusive ownership is the point. The previous design gave
/// every worker a clone of an `Arc<Mutex<Receiver>>` and held that lock across the *idle*
/// `recv().await`; a worker that had already claimed a batch then blocked forever in its drain
/// phase on a lock held by an idle sibling, so with the shipped default of four workers no batch
/// was ever dispatched and Midnight transactions never became ready in the pool.
///
/// Keeping batch formation in a single task is also the right shape on its own terms: there is one
/// queue, so accumulation is inherently serial, and N workers each accumulating their own batch
/// would split the same arrivals into N smaller batches — the opposite of what batching is for.
/// Workers are left to do the part that actually parallelises: the aggregate crypto.
async fn run_dispatcher<Block: BlockT>(
	mut queue_rx: mpsc::Receiver<QueueItem<Block>>,
	batch_tx: async_channel::Sender<Batch<Block>>,
	verifier: Arc<dyn BatchVerify<<Block as BlockT>::Hash>>,
	params: BatchParams,
	metrics: BatchVerifyMetrics,
) {
	// Prepares one submission the moment it arrives. On failure the submission is resolved
	// immediately as `Delegate` — the runtime is authoritative, so an unpreparable transaction just
	// takes the ordinary path instead of holding up the batch.
	fn prepare_or_delegate<Block: BlockT>(
		item: QueueItem<Block>,
		verifier: &dyn BatchVerify<<Block as BlockT>::Hash>,
		metrics: &BatchVerifyMetrics,
	) -> Option<PreparedItem<Block>> {
		// The pool revalidates everything it still holds on every block import. Those are the same
		// transactions the runtime already accepted, and its soft cache will answer them without
		// touching a proof -- so preparing them here is pure waste, and waste that grows with how
		// long the pool stays congested rather than with how many transactions were submitted.
		// Measured without this check: 3 preparations per submission and rising, against exactly
		// one inline validation per submission on the unbatched path.
		//
		// Delegating rather than synthesising a validity keeps the runtime authoritative and costs
		// only the cache lookup it was going to do anyway.
		if let Some(version) = verifier.runtime_version(item.at)
			&& verifier.soft_cache_hit(version, &item.tx_bytes)
		{
			metrics.observe_soft_cache_short_circuit();
			let _ = item.reply.send(WorkerOutcome::Delegate);
			return None;
		}
		match verifier.prepare(item.at, &item.tx_bytes, MEMPOOL_TBLOCK_EXTRA_SECS) {
			Ok(prepared) => Some(PreparedItem {
				at: item.at,
				tx_bytes: item.tx_bytes,
				prepared,
				reply: item.reply,
			}),
			Err(reason) => {
				log::debug!(
					target: LOG_TARGET,
					"could not prepare submission, delegating to the runtime: {reason:?}",
				);
				metrics.inc_fallback();
				let _ = item.reply.send(WorkerOutcome::Delegate);
				None
			},
		}
	}

	loop {
		// Phase 0: wait for the first submission. A closed queue ends the dispatcher.
		let Some(first) = queue_rx.recv().await else { break };
		let deadline = first.enqueued_at + params.tau;
		// Prepare as it arrives: this is the expensive, per-transaction half, and doing it here
		// spends the accumulation window on it instead of paying it all at dispatch.
		let mut batch: Batch<Block> =
			prepare_or_delegate::<Block>(first, &*verifier, &metrics).into_iter().collect();

		// Phase 1: accumulate until the target size is reached or the oldest item hits tau.
		let trigger = loop {
			if batch.len() >= params.k_target {
				break Trigger::KTarget;
			}
			let now = Instant::now();
			if now >= deadline {
				break Trigger::Tau;
			}
			match timeout(deadline - now, queue_rx.recv()).await {
				Ok(Some(item)) => {
					batch.extend(prepare_or_delegate::<Block>(item, &*verifier, &metrics))
				},
				Ok(None) => break Trigger::Closed,
				Err(_) => break Trigger::Tau,
			}
		};

		// Phase 2: greedily take anything else already queued, capped at M.
		while batch.len() < params.max_batch {
			match queue_rx.try_recv() {
				Ok(item) => batch.extend(prepare_or_delegate::<Block>(item, &*verifier, &metrics)),
				Err(_) => break,
			}
		}

		// Everything in the batch may have failed preparation and already been answered.
		if !batch.is_empty() {
			metrics.observe_dispatch(trigger.label());
			// Backpressure: when every worker is busy this waits rather than forming ever more
			// batches. An error means every worker is gone, so there is nothing to dispatch to.
			if batch_tx.send(batch).await.is_err() {
				break;
			}
		}
		if matches!(trigger, Trigger::Closed) {
			break;
		}
	}
	// Stop accepting new batches; workers drain what is already queued, then exit.
	batch_tx.close();
}

/// One verification worker: runs the aggregate crypto for whole batches formed by the dispatcher.
///
/// Spawned on the blocking pool because [`process_batch`] verifies synchronously. Each worker polls
/// its own `async_channel` receiver clone, so an idle worker can never hold up a busy one.
async fn run_worker<Block: BlockT>(
	batches: async_channel::Receiver<Batch<Block>>,
	verifier: Arc<dyn BatchVerify<<Block as BlockT>::Hash>>,
	metrics: BatchVerifyMetrics,
) {
	while let Ok(batch) = batches.recv().await {
		process_batch::<Block>(&*verifier, batch, &metrics);
	}
}

/// Decides one drained batch: groups by target block, runs one fold per group, and resolves each
/// parked `oneshot`. The expensive per-transaction work already happened in the dispatcher, so all
/// that remains here is the aggregate check.
fn process_batch<Block: BlockT>(
	verifier: &dyn BatchVerify<<Block as BlockT>::Hash>,
	batch: Batch<Block>,
	metrics: &BatchVerifyMetrics,
) {
	for (at, items) in group_by_at::<Block>(batch) {
		let runtime_version = verifier.runtime_version(at);
		let (prepared, rest): (Vec<_>, Vec<_>) =
			items.into_iter().map(|i| (i.prepared, (i.tx_bytes, i.reply))).unzip();

		let outcome = verifier.finalize(
			at,
			prepared,
			/* isolate_on_failure */ true,
			MEMPOOL_TBLOCK_EXTRA_SECS,
		);

		match outcome {
			Ok(results) => {
				for (idx, (tx_bytes, reply)) in rest.into_iter().enumerate() {
					// A verified proof with a known runtime version → native validity tag. Anything
					// else (per-tx rejection, or no runtime version to tag with) → delegate to the
					// runtime for the authoritative result.
					let out = match (results.get(idx), runtime_version) {
						(Some(Ok(())), Some(v)) => {
							WorkerOutcome::Validated(success_validity(v, &tx_bytes))
						},
						_ => WorkerOutcome::Delegate,
					};
					let _ = reply.send(out);
				}
			},
			Err(reason) => {
				// Setup/availability failure (or, defensively, a fail-fast proof error): never
				// reject — delegate every parked submission to the runtime.
				log::debug!(
					target: LOG_TARGET,
					"mempool batch fold unavailable for {} tx(s), delegating to runtime: {reason:?}",
					rest.len(),
				);
				metrics.inc_fallback();
				for (_, reply) in rest {
					let _ = reply.send(WorkerOutcome::Delegate);
				}
			},
		}
	}
}

/// Groups a batch by target block hash, preserving first-seen order. Batches are small (≤ M) and
/// almost always share one `at`, so the linear grouping is cheap.
fn group_by_at<Block: BlockT>(
	batch: Batch<Block>,
) -> Vec<(<Block as BlockT>::Hash, Vec<PreparedItem<Block>>)> {
	let mut groups: Vec<(<Block as BlockT>::Hash, Vec<PreparedItem<Block>>)> = Vec::new();
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
		/// Transactions preparation was called for, in arrival order.
		prepared: Arc<StdMutex<Vec<Vec<u8>>>>,
		/// Transactions the stub reports as already held by the runtime's soft cache, standing in
		/// for a pool revalidation of something the runtime has already accepted.
		soft_cached: Arc<StdMutex<Vec<Vec<u8>>>>,
	}

	impl StubVerifier {
		fn new() -> Self {
			Self {
				results: Default::default(),
				unavailable: false,
				runtime_version: Some(2_000_000),
				batch_sizes: Arc::new(StdMutex::new(Vec::new())),
				prepared: Arc::new(StdMutex::new(Vec::new())),
				soft_cached: Arc::new(StdMutex::new(Vec::new())),
			}
		}
	}

	impl BatchVerify<Hash> for StubVerifier {
		fn soft_cache_hit(&self, _runtime_version: u32, tx_bytes: &[u8]) -> bool {
			self.soft_cached.lock().unwrap().iter().any(|t| t == tx_bytes)
		}

		/// The stub has no ledger, so "preparing" just carries the transaction bytes through the
		/// queue in the opaque handle; `finalize` reads them back to score the batch.
		fn prepare(
			&self,
			_at: Hash,
			tx_bytes: &[u8],
			_extra_secs: u64,
		) -> Result<PreparedHandle, BatchVerifyError> {
			self.prepared.lock().unwrap().push(tx_bytes.to_vec());
			Ok(Box::new(tx_bytes.to_vec()))
		}

		fn finalize(
			&self,
			_at: Hash,
			prepared: Vec<PreparedHandle>,
			_isolate_on_failure: bool,
			_extra_secs: u64,
		) -> Result<Vec<Result<(), LedgerApiError>>, BatchVerifyError> {
			let txs: Vec<Vec<u8>> = prepared
				.into_iter()
				.map(|p| *p.downcast::<Vec<u8>>().expect("stub only boxes tx bytes"))
				.collect();
			self.batch_sizes.lock().unwrap().push(txs.len());
			if self.unavailable {
				return Err(BatchVerifyError::Unavailable("stub".into()));
			}
			Ok(txs.iter().map(|tx| self.results.get(tx).cloned().unwrap_or(Ok(()))).collect())
		}

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

	/// Spawns one dispatcher plus `workers` verification workers over a fresh queue — the same
	/// wiring as [`MempoolBatcher::new`], without needing a `SpawnEssentialNamed`.
	fn spawn_pool(
		verifier: Arc<dyn BatchVerify<Hash>>,
		params: BatchParams,
		workers: usize,
	) -> (mpsc::Sender<QueueItem<OpaqueBlock>>, Vec<tokio::task::JoinHandle<()>>) {
		let (tx, rx) = mpsc::channel::<QueueItem<OpaqueBlock>>(64);
		let (batch_tx, batch_rx) = async_channel::bounded::<Batch<OpaqueBlock>>(workers.max(1));
		let metrics = BatchVerifyMetrics::new(None);
		let mut handles = vec![tokio::spawn(run_dispatcher::<OpaqueBlock>(
			rx,
			batch_tx,
			verifier.clone(),
			params,
			metrics.clone(),
		))];
		for _ in 0..workers {
			handles.push(tokio::spawn(run_worker::<OpaqueBlock>(
				batch_rx.clone(),
				verifier.clone(),
				metrics.clone(),
			)));
		}
		(tx, handles)
	}

	/// Awaits every spawned task after the queue is closed, so a test fails loudly on a hang
	/// instead of leaking tasks.
	async fn join_all(handles: Vec<tokio::task::JoinHandle<()>>) {
		for h in handles {
			let _ = h.await;
		}
	}

	#[test]
	fn success_validity_matches_runtime_tag_shape() {
		let v = success_validity(2_000_000, b"some-tx-bytes").expect("valid tx must build Ok");
		assert_eq!(v.longevity, 600, "longevity must mirror the pallet");
		assert_eq!(v.provides.len(), 1, "exactly one provides tag");
		// The provides tag is the "Midnight" prefix ++ the 32-byte cache key.
		assert!(v.provides[0].ends_with(&tx_validation_cache_key(2_000_000, b"some-tx-bytes")));
	}

	/// Wraps a queue item as an already-prepared one, as the dispatcher would.
	fn prepared_item(at: Hash, tx: Vec<u8>) -> PreparedItem<OpaqueBlock> {
		let (item, _rx) = queue_item(at, tx);
		PreparedItem {
			at: item.at,
			tx_bytes: item.tx_bytes.clone(),
			prepared: Box::new(item.tx_bytes),
			reply: item.reply,
		}
	}

	#[test]
	fn group_by_at_preserves_order_and_partitions() {
		let a = H256::repeat_byte(0xAA);
		let b = H256::repeat_byte(0xBB);
		let batch =
			vec![prepared_item(a, vec![1]), prepared_item(b, vec![2]), prepared_item(a, vec![3])];
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
		// tau is long; the k_target=3 trigger must fire well before it. Four workers (the shipped
		// default) so this also pins that the three submissions form ONE batch rather than being
		// split across workers.
		let (tx, handles) = spawn_pool(verifier, params(3, 64, 60_000), 4);

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
		join_all(handles).await;
	}

	#[tokio::test]
	async fn worker_dispatches_at_tau() {
		let at = H256::repeat_byte(2);
		let verifier = Arc::new(StubVerifier::new());
		let sizes = verifier.batch_sizes.clone();
		// k_target is unreachable with one tx; it must dispatch on the (short, real) tau timeout.
		let (tx, handles) = spawn_pool(verifier, params(100, 64, 30), 1);

		let (item, reply_rx) = queue_item(at, vec![7]);
		tx.send(item).await.unwrap();

		let outcome = reply_rx.await.expect("worker must resolve on tau");
		assert!(is_validated(&outcome), "the single valid tx must be Validated");
		assert_eq!(sizes.lock().unwrap().as_slice(), &[1], "one tx dispatched on tau");
		drop(tx);
		join_all(handles).await;
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
		let (tx, handles) = spawn_pool(verifier, params(2, 64, 60_000), 1);

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
		join_all(handles).await;
	}

	#[tokio::test]
	async fn worker_unavailable_delegates_all() {
		let at = H256::repeat_byte(4);
		let mut verifier = StubVerifier::new();
		verifier.unavailable = true;
		let verifier = Arc::new(verifier);
		let (tx, handles) = spawn_pool(verifier, params(1, 64, 60_000), 1);

		let (item, reply_rx) = queue_item(at, vec![5]);
		tx.send(item).await.unwrap();
		assert!(
			matches!(reply_rx.await.unwrap(), WorkerOutcome::Delegate),
			"availability failure must delegate, never reject"
		);
		drop(tx);
		join_all(handles).await;
	}

	/// The pool revalidates everything it holds on every block import. Those transactions are
	/// already in the runtime's soft cache, which the inline path answers from for free — so the
	/// batcher must not prepare their proofs again. Without this the batch path's cost scales with
	/// pool residency rather than with submissions: measured at 3 preparations per submission and
	/// climbing, against exactly one inline validation per submission unbatched.
	#[tokio::test]
	async fn a_revalidation_the_runtime_has_cached_is_not_prepared_again() {
		let at = H256::repeat_byte(4);
		let verifier = Arc::new(StubVerifier::new());
		// Stands in for a transaction the runtime already accepted and cached.
		verifier.soft_cached.lock().unwrap().push(vec![7u8]);
		let prepared = verifier.prepared.clone();
		let (tx, handles) = spawn_pool(verifier, params(1, 64, 60_000), 1);

		let (item, reply_rx) = queue_item(at, vec![7u8]);
		tx.send(item).await.unwrap();

		assert!(
			matches!(reply_rx.await.unwrap(), WorkerOutcome::Delegate),
			"a cached transaction must go back to the runtime, which answers from that same cache",
		);
		assert!(
			prepared.lock().unwrap().is_empty(),
			"no proof preparation may run for a transaction the runtime has already validated",
		);
		drop(tx);
		join_all(handles).await;
	}

	/// The short-circuit must not swallow first-time submissions.
	#[tokio::test]
	async fn an_uncached_submission_is_still_prepared() {
		let at = H256::repeat_byte(5);
		let verifier = Arc::new(StubVerifier::new());
		verifier.soft_cached.lock().unwrap().push(vec![1u8]);
		let prepared = verifier.prepared.clone();
		let (tx, handles) = spawn_pool(verifier, params(1, 64, 60_000), 1);

		let (item, reply_rx) = queue_item(at, vec![2u8]);
		tx.send(item).await.unwrap();

		assert!(is_validated(&reply_rx.await.unwrap()));
		assert_eq!(
			prepared.lock().unwrap().as_slice(),
			&[vec![2u8]],
			"a transaction the cache does not hold must still be prepared",
		);
		drop(tx);
		join_all(handles).await;
	}

	/// The point of the incremental design: each submission is prepared as it arrives, and the
	/// batch costs one fold no matter how many transactions it holds.
	///
	/// Preparation is the half whose cost grows with the batch (per-proof transcript replay and
	/// deferred MSM); folding is the half that is essentially constant. Doing the former during the
	/// accumulation window is what turns dispatch latency from "prepare n + fold" into "fold".
	#[tokio::test]
	async fn each_submission_is_prepared_on_arrival_and_the_batch_folds_once() {
		let at = H256::repeat_byte(9);
		let verifier = Arc::new(StubVerifier::new());
		let prepared = verifier.prepared.clone();
		let sizes = verifier.batch_sizes.clone();
		// k_target = 3 so the batch dispatches once all three have arrived.
		let (tx, handles) = spawn_pool(verifier, params(3, 64, 60_000), 1);

		let mut replies = Vec::new();
		for i in 0..3u8 {
			let (item, reply_rx) = queue_item(at, vec![i]);
			tx.send(item).await.unwrap();
			replies.push(reply_rx);
		}
		for reply_rx in replies {
			assert!(is_validated(&reply_rx.await.expect("worker must resolve the oneshot")));
		}

		assert_eq!(
			prepared.lock().unwrap().as_slice(),
			&[vec![0u8], vec![1u8], vec![2u8]],
			"every submission must be prepared individually, in arrival order",
		);
		assert_eq!(
			sizes.lock().unwrap().as_slice(),
			&[3],
			"and the three prepared transactions must be decided by a single fold",
		);
		drop(tx);
		join_all(handles).await;
	}

	/// Regression: a batch must still be dispatched when several workers sit idle.
	///
	/// The original pool shared one `mpsc::Receiver` between all workers behind a `Mutex` and held
	/// that lock across the idle `recv().await`. A worker that had claimed a batch then blocked
	/// forever on the same lock in its drain phase — held by an idle sibling — so at the shipped
	/// default of four workers no batch was ever verified and Midnight transactions never became
	/// ready in the pool. Every other test in this module spawns exactly one worker, which is why
	/// none of them caught it; this one uses the real default.
	#[tokio::test]
	async fn pool_dispatches_with_several_idle_workers() {
		let at = H256::repeat_byte(7);
		let verifier = Arc::new(StubVerifier::new());
		let sizes = verifier.batch_sizes.clone();
		// Short tau so a lone transaction dispatches on the timeout rather than via k_target.
		let (tx, handles) = spawn_pool(verifier, params(16, 64, 30), 4);

		let (item, reply_rx) = queue_item(at, vec![1]);
		tx.send(item).await.unwrap();

		let outcome = tokio::time::timeout(Duration::from_secs(5), reply_rx)
			.await
			.expect("the batch must dispatch even while other workers idle on the queue")
			.expect("worker must resolve the oneshot");
		assert!(is_validated(&outcome));
		assert_eq!(sizes.lock().unwrap().as_slice(), &[1]);
		drop(tx);
		join_all(handles).await;
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
