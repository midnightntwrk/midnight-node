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

//! Cross-block proof-verification lookahead for the import queue.
//!
//! [`BasicQueue`](sc_consensus::import_queue::BasicQueue) imports blocks strictly sequentially:
//! `import_many_blocks` awaits each block's `import_block` before starting the next, and
//! [`crate::batch_block_import`] batch-verifies a block's proofs *inside* that call. Verification
//! is therefore serialized with execution — a syncing node pays `verify(N) + execute(N)` per block
//! even though the two are independent and the verification inputs for block N+1 are already on
//! hand.
//!
//! The sync engine does not hand the queue one block at a time; it hands it a whole chunk. This
//! module intercepts at that point ([`ImportQueueService::import_blocks`]), splits the chunk into
//! groups, and dispatches each group's Midnight transactions to a blocking worker pool as a single
//! aggregate verification. The sequential importer then runs as before, and by the time it reaches
//! block N the proofs are usually already verified: the import path awaits the group's result
//! instead of doing the work itself.
//!
//! Two effects, in order of size:
//!
//! 1. **Overlap.** Verification of later blocks happens while earlier blocks execute, so sync time
//!    approaches `max(verify, execute)` rather than their sum.
//! 2. **Larger batches.** One aggregate call spanning several blocks amortises the fixed cost of a
//!    batch over more proofs, which is where cross-transaction batching's advantage comes from.
//!
//! ## Verifying against a stale state
//!
//! Every block in a chunk is verified against one reference state: the parent of the chunk's first
//! block. It has to be — the parents of the later blocks are precisely the blocks that have not
//! been imported yet, so their states do not exist. That reference is therefore *stale* by up to a
//! chunk for the last block in it.
//!
//! This cannot cause a valid block to be rejected, which is the property that makes the whole
//! thing safe:
//!
//! - Jobs run with `isolate_on_failure = false`. On an aggregate failure the ledger's batch entry
//!   point returns early **without writing to the revalidation cache**, so a lookahead can never
//!   record `ProofOutcome::Invalid`.
//! - A transaction that fails the *non-crypto* `well_formed` checks against the stale state (its
//!   contract was deployed by a block in this very chunk, say) is dropped from the batch as
//!   `Prep::Failed`, which likewise writes nothing.
//!
//! So the worst a stale reference can do is lose the optimisation: the transaction is verified
//! inline during execution exactly as it would have been without this module. Only `VerifiedAt`
//! is ever recorded, and `get_verified_transaction` re-checks that record against the real state
//! through the ledger's `RevalidationReference` before trusting it.
//!
//! The drift window here (a chunk) is smaller than one the node already accepts: a transaction
//! verified on the mempool path carries its `VerifiedAt` record for the cache's whole TTL, and may
//! be revalidated many blocks later.

use crate::batch_verify::{BatchVerifier, BatchVerifyError, BatchVerifyMetrics};
use futures::{
	channel::oneshot,
	future::{FutureExt, Shared},
};
use midnight_node_runtime::opaque::Block;
use sc_consensus::import_queue::{
	ImportQueue, ImportQueueService, IncomingBlock, Link, RuntimeOrigin,
};
use sp_consensus::BlockOrigin;
use sp_core::traits::SpawnEssentialNamed;
use sp_runtime::{
	Justifications,
	traits::{Block as BlockT, Header as HeaderT, NumberFor},
};
use std::{
	collections::{BTreeMap, HashMap, VecDeque},
	sync::{Arc, Mutex},
};

const LOG_TARGET: &str = "midnight::batch_verify";

/// Seconds per AURA slot. Used only to estimate the `tblock` of blocks that have not been imported
/// yet; see [`LookaheadScheduler::schedule`] for why an estimate is good enough.
const SLOT_SECS: u64 = 6;

type BlockHash = <Block as BlockT>::Hash;

/// What a lookahead job concluded about the group of blocks it covered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookaheadOutcome {
	/// The aggregate check passed: every transaction in the group that was well-formed against the
	/// reference state has its proof verdict recorded, and the import path can skip its own
	/// verification.
	Verified,
	/// Nothing can be concluded — the reference state was unavailable, the runtime predates
	/// ledger 9, or the aggregate check failed without attribution. The import path must verify
	/// the block itself. Never a reason to reject: see the module docs.
	Unavailable,
}

/// The verification a lookahead job performs, behind a trait so the queue can be tested without a
/// client, a ledger, or proofs.
pub trait LookaheadVerify: Send + Sync {
	/// Verifies `txs` against the state at `at`. `extra_secs` is added to the reference block's
	/// timestamp to approximate the `tblock` of the blocks being verified.
	fn verify(&self, at: BlockHash, txs: Vec<Vec<u8>>, extra_secs: u64) -> LookaheadOutcome;
}

impl LookaheadVerify for BatchVerifier {
	fn verify(&self, at: BlockHash, txs: Vec<Vec<u8>>, extra_secs: u64) -> LookaheadOutcome {
		// `isolate_on_failure = false` is load-bearing, not a performance choice: it is what keeps
		// a failure against the reference state from being attributed to individual transactions
		// and cached as `Invalid`. See the module docs.
		match self.batch_verify(at, txs, /* isolate_on_failure */ false, extra_secs) {
			// An outer `Ok` only means the aggregate crypto call itself completed. Transactions
			// that failed the *non-crypto* checks against the reference state are reported as
			// inner `Err`s and were never verified, so claiming the group is done would make the
			// import path skip verification that did not happen — every one of those transactions
			// would then be verified inline during execution, at full per-transaction cost, on top
			// of the work this job already did. Only a clean sweep is worth reporting.
			Ok(results) if results.iter().all(|r| r.is_ok()) => LookaheadOutcome::Verified,
			Ok(results) => {
				let failed = results.iter().filter(|r| r.is_err()).count();
				log::debug!(
					target: LOG_TARGET,
					"lookahead at {at:?}: {failed}/{} transaction(s) not verifiable against the \
					 reference state; deferring the group",
					results.len(),
				);
				LookaheadOutcome::Unavailable
			},
			Err(BatchVerifyError::ProofInvalid) => {
				// Unattributable against a state that is legitimately stale, so this is not
				// evidence that any particular block is bad. Hand it back for per-block
				// verification, which runs against the right state and can reject properly.
				log::debug!(
					target: LOG_TARGET,
					"lookahead: aggregate check failed at {at:?}; deferring to per-block verification",
				);
				LookaheadOutcome::Unavailable
			},
			Err(BatchVerifyError::Unavailable(reason)) => {
				log::debug!(target: LOG_TARGET, "lookahead unavailable at {at:?}: {reason}");
				LookaheadOutcome::Unavailable
			},
		}
	}
}

/// A shared, cloneable handle to a job's result. Resolves to `Err(Canceled)` if the job was shed
/// or its worker died, which callers treat exactly like [`LookaheadOutcome::Unavailable`].
type LookaheadHandle = Shared<oneshot::Receiver<LookaheadOutcome>>;

/// Maps a block to the in-flight (or finished) lookahead covering it.
///
/// Bounded and FIFO-evicting: blocks on a fork the node never imports would otherwise leave their
/// entries behind forever. Eviction only costs the optimisation for that block.
pub struct LookaheadRegistry {
	inner: Mutex<RegistryInner>,
	capacity: usize,
}

#[derive(Default)]
struct RegistryInner {
	slots: HashMap<BlockHash, LookaheadHandle>,
	order: VecDeque<BlockHash>,
}

impl LookaheadRegistry {
	pub fn new(capacity: usize) -> Self {
		Self { inner: Mutex::new(RegistryInner::default()), capacity: capacity.max(1) }
	}

	fn insert(&self, hash: BlockHash, handle: LookaheadHandle) {
		let mut inner = self.inner.lock().expect("lookahead registry mutex poisoned");
		if inner.slots.insert(hash, handle).is_none() {
			inner.order.push_back(hash);
		}
		while inner.order.len() > self.capacity {
			if let Some(oldest) = inner.order.pop_front() {
				inner.slots.remove(&oldest);
			}
		}
	}

	/// Removes and returns the handle covering `hash`, if any. Taking rather than peeking keeps a
	/// re-import of the same block from awaiting a handle whose result was already consumed.
	pub fn take(&self, hash: &BlockHash) -> Option<LookaheadHandle> {
		let mut inner = self.inner.lock().expect("lookahead registry mutex poisoned");
		let handle = inner.slots.remove(hash)?;
		if let Some(pos) = inner.order.iter().position(|h| h == hash) {
			inner.order.remove(pos);
		}
		Some(handle)
	}

	#[cfg(test)]
	fn len(&self) -> usize {
		self.inner.lock().unwrap().slots.len()
	}
}

/// A downloaded block awaiting a reference state: its hash and Midnight transactions, keyed
/// elsewhere by block number.
type PendingBlock = (BlockHash, Vec<Vec<u8>>);

/// A block placed into a group, without the parent (only the group's reference state matters).
type GroupMember = (BlockHash, NumberFor<Block>, Vec<Vec<u8>>);

/// One aggregate verification covering a contiguous group of blocks.
struct LookaheadJob {
	/// Reference state: the parent of the chunk's first block.
	at: BlockHash,
	/// Seconds past the reference block's timestamp to verify at.
	extra_secs: u64,
	txs: Vec<Vec<u8>>,
	blocks: usize,
	/// One per block in the group; all receive the same outcome.
	replies: Vec<oneshot::Sender<LookaheadOutcome>>,
}

/// Tuning for the lookahead pipeline.
#[derive(Clone, Copy, Debug)]
pub struct LookaheadConfig {
	/// Blocks per aggregate verification. Larger groups amortise the batch's fixed cost over more
	/// proofs but make the first block of a group wait for the whole group.
	pub blocks_per_job: usize,
	/// Upper bound on transactions in one aggregate call, so a chunk of full blocks cannot build
	/// an unboundedly large batch.
	pub max_txs_per_job: usize,
	/// Blocking workers running jobs concurrently.
	pub workers: usize,
	/// In-flight jobs allowed before new ones are shed (and their blocks verified per-block).
	pub queue_capacity: usize,
	/// Blocks tracked in the registry at once.
	pub registry_capacity: usize,
	/// How far past the reference block a job may reach. Bounds how stale the reference can be for
	/// the last block in a group, and so how often the non-crypto checks miss.
	pub max_stale_blocks: u64,
	/// Blocks held waiting to be scheduled. Bounded so a long download run cannot grow it without
	/// limit; the excess is simply verified per-block.
	pub pending_capacity: usize,
}

impl Default for LookaheadConfig {
	fn default() -> Self {
		Self {
			blocks_per_job: 4,
			max_txs_per_job: 256,
			workers: 2,
			queue_capacity: 16,
			registry_capacity: 512,
			max_stale_blocks: 8,
			pending_capacity: 4096,
		}
	}
}

/// Splits incoming chunks into jobs and hands them to the worker pool.
pub struct LookaheadScheduler {
	jobs: async_channel::Sender<LookaheadJob>,
	registry: Arc<LookaheadRegistry>,
	/// Downloaded blocks awaiting a reference state to verify against, keyed by block number so
	/// the lowest — the one the import cursor reaches next — is always the one scheduled first.
	pending: Mutex<BTreeMap<NumberFor<Block>, PendingBlock>>,
	cfg: LookaheadConfig,
	metrics: BatchVerifyMetrics,
}

impl LookaheadScheduler {
	/// Builds the queue and spawns `cfg.workers` blocking verification workers on `spawner`.
	pub fn new(
		spawner: &impl SpawnEssentialNamed,
		verifier: Arc<dyn LookaheadVerify>,
		cfg: LookaheadConfig,
		metrics: BatchVerifyMetrics,
	) -> (Arc<Self>, Arc<LookaheadRegistry>) {
		let workers = cfg.workers.max(1);
		let (jobs_tx, jobs_rx) = async_channel::bounded::<LookaheadJob>(cfg.queue_capacity.max(1));
		let registry = Arc::new(LookaheadRegistry::new(cfg.registry_capacity));

		for _ in 0..workers {
			spawner.spawn_essential_blocking(
				"midnight-lookahead-verify",
				Some("block-import"),
				Box::pin(run_worker(jobs_rx.clone(), verifier.clone(), metrics.clone())),
			);
		}

		let scheduler = Arc::new(Self {
			jobs: jobs_tx,
			registry: registry.clone(),
			pending: Mutex::new(BTreeMap::new()),
			cfg,
			metrics,
		});
		(scheduler, registry)
	}

	/// Builds a scheduler around an existing job channel, so tests can inspect the jobs the
	/// grouping produces without running any verification.
	#[cfg(test)]
	fn with_channel(
		jobs: async_channel::Sender<LookaheadJob>,
		registry: Arc<LookaheadRegistry>,
		cfg: LookaheadConfig,
	) -> Self {
		Self {
			jobs,
			registry,
			pending: Mutex::new(BTreeMap::new()),
			cfg,
			metrics: BatchVerifyMetrics::new(None),
		}
	}

	/// Records a downloaded chunk. Nothing is verified yet: a block can only be verified once
	/// there is a state to verify it against, and at this point the whole chunk is still waiting
	/// behind the import cursor.
	///
	/// Blocks with nothing to verify — no body, no header, no Midnight transactions, or execution
	/// skipped — are passed over; they cost the import path nothing to begin with.
	pub fn enqueue(&self, blocks: &[IncomingBlock<Block>]) {
		let mut pending = self.pending.lock().expect("lookahead pending mutex poisoned");
		for block in blocks.iter().filter(|b| !b.skip_execution) {
			if pending.len() >= self.cfg.pending_capacity {
				break;
			}
			let (Some(header), Some(body)) = (block.header.as_ref(), block.body.as_ref()) else {
				continue;
			};
			let txs = crate::batch_block_import::extract_midnight_txs(body);
			if txs.is_empty() {
				continue;
			}
			pending.insert(*header.number(), (block.hash, txs));
		}
	}

	/// Schedules the next group now that `reference` has been imported and its state exists.
	///
	/// This is the point the whole module turns on. At ingress the right reference state does not
	/// exist — the sync engine downloads well ahead of the import cursor, so the parents of the
	/// queued blocks are themselves still queued. Immediately after block N is imported, though,
	/// state N is exactly the reference block N+1's transactions were built against, and is at
	/// most `max_stale_blocks` out of date for the rest of the group. Verifying against a state
	/// that far off is what the revalidation path is built to absorb; verifying against one
	/// hundreds of blocks old is not, and silently verifies nothing.
	pub fn advance(&self, reference: BlockHash, reference_number: NumberFor<Block>) {
		let horizon = u64::from(reference_number).saturating_add(self.cfg.max_stale_blocks);
		let mut group: Vec<GroupMember> = Vec::new();
		let mut group_txs = 0usize;
		{
			let mut pending = self.pending.lock().expect("lookahead pending mutex poisoned");
			while group.len() < self.cfg.blocks_per_job {
				let Some((&number, _)) = pending.iter().next() else { break };
				if number <= reference_number || u64::from(number) > horizon {
					break;
				}
				let (_, (hash, txs)) = pending.pop_first().expect("peeked above");
				if !group.is_empty() && group_txs + txs.len() > self.cfg.max_txs_per_job {
					// Put it back: it belongs to the next group, not this one.
					pending.insert(number, (hash, txs));
					break;
				}
				group_txs += txs.len();
				group.push((hash, number, txs));
			}
		}
		if !group.is_empty() {
			self.dispatch(reference, reference_number, group);
		}
	}

	/// Sends one group as a job, registering its blocks only if the send succeeds.
	fn dispatch(
		&self,
		reference: BlockHash,
		reference_number: NumberFor<Block>,
		group: Vec<GroupMember>,
	) {
		if group.is_empty() {
			return;
		}
		// Verify at the group's midpoint in time. `tblock` feeds only the non-crypto `well_formed`
		// checks (ttl), which the runtime re-runs authoritatively during execution, so an estimate
		// that is off merely drops a transaction from the batch — it cannot admit a bad one.
		let first = group.first().expect("non-empty").1;
		let last = group.last().expect("non-empty").1;
		let midpoint =
			(Self::offset(first, reference_number) + Self::offset(last, reference_number)) / 2;
		let extra_secs = SLOT_SECS * midpoint.max(1);

		let mut txs = Vec::new();
		let mut replies = Vec::with_capacity(group.len());
		let mut handles = Vec::with_capacity(group.len());
		for (hash, _, mut block_txs) in group {
			txs.append(&mut block_txs);
			let (tx, rx) = oneshot::channel();
			replies.push(tx);
			handles.push((hash, rx.shared()));
		}
		let blocks = handles.len();
		let job = LookaheadJob { at: reference, extra_secs, txs, blocks, replies };

		match self.jobs.try_send(job) {
			Ok(()) => {
				for (hash, handle) in handles {
					self.registry.insert(hash, handle);
				}
				self.metrics.observe_lookahead_scheduled(blocks);
			},
			Err(e) => {
				// Shed: the pool is saturated or shutting down. Nothing is registered, so every
				// block in this group takes the ordinary per-block path.
				log::debug!(
					target: LOG_TARGET,
					"lookahead queue full, skipping {blocks} block(s): {e}",
				);
				self.metrics.observe_lookahead_shed(blocks);
			},
		}
	}

	/// How many slots `number` sits past the reference block. Saturating, because a chunk that is
	/// not anchored where we expect should degrade to "one slot ahead", not panic.
	fn offset(number: NumberFor<Block>, reference: NumberFor<Block>) -> u64 {
		let n: u64 = number.into();
		let r: u64 = reference.into();
		n.saturating_sub(r)
	}
}

/// Drains jobs, running each aggregate verification and broadcasting its outcome.
async fn run_worker(
	jobs: async_channel::Receiver<LookaheadJob>,
	verifier: Arc<dyn LookaheadVerify>,
	metrics: BatchVerifyMetrics,
) {
	while let Ok(job) = jobs.recv().await {
		let LookaheadJob { at, extra_secs, txs, blocks, replies } = job;
		let tx_count = txs.len();
		let started = std::time::Instant::now();
		let outcome = verifier.verify(at, txs, extra_secs);
		metrics.observe_lookahead_job(outcome == LookaheadOutcome::Verified, started.elapsed());
		log::debug!(
			target: LOG_TARGET,
			"lookahead verified {tx_count} tx(s) across {blocks} block(s) at {at:?}: {outcome:?} \
			 ({}ms)",
			started.elapsed().as_millis(),
		);
		for reply in replies {
			// A dropped receiver just means that block was imported without waiting.
			let _ = reply.send(outcome);
		}
	}
}

/// [`ImportQueueService`] that schedules lookahead before forwarding a chunk to the inner queue.
pub struct LookaheadService {
	inner: Box<dyn ImportQueueService<Block>>,
	/// `None` when lookahead is disabled, making the wrapper a pass-through.
	scheduler: Option<Arc<LookaheadScheduler>>,
}

impl ImportQueueService<Block> for LookaheadService {
	fn import_blocks(&mut self, origin: BlockOrigin, blocks: Vec<IncomingBlock<Block>>) {
		// Scheduling first is what creates the overlap: the jobs are already with the workers by
		// the time the inner queue starts importing the first block.
		if let Some(scheduler) = &self.scheduler {
			scheduler.enqueue(&blocks);
		}
		self.inner.import_blocks(origin, blocks);
	}

	fn import_justifications(
		&mut self,
		who: RuntimeOrigin,
		hash: BlockHash,
		number: NumberFor<Block>,
		justifications: Justifications,
	) {
		self.inner.import_justifications(who, hash, number, justifications);
	}
}

/// [`ImportQueue`] wrapper that adds cross-block verification lookahead.
///
/// Everything about importing is delegated unchanged; the only addition is on the ingress side,
/// where a chunk is dispatched for verification before it is queued.
pub struct LookaheadImportQueue<Q> {
	inner: Q,
	service: LookaheadService,
	scheduler: Option<Arc<LookaheadScheduler>>,
}

impl<Q: ImportQueue<Block>> LookaheadImportQueue<Q> {
	/// Wraps `inner`. A `None` scheduler makes this a pass-through, so the queue type stays the
	/// same whether or not the feature is switched on.
	pub fn new(inner: Q, scheduler: Option<Arc<LookaheadScheduler>>) -> Self {
		let service = LookaheadService { inner: inner.service(), scheduler: scheduler.clone() };
		Self { inner, service, scheduler }
	}
}

#[async_trait::async_trait]
impl<Q: ImportQueue<Block>> ImportQueue<Block> for LookaheadImportQueue<Q> {
	fn service(&self) -> Box<dyn ImportQueueService<Block>> {
		Box::new(LookaheadService {
			inner: self.inner.service(),
			scheduler: self.scheduler.clone(),
		})
	}

	fn service_ref(&mut self) -> &mut dyn ImportQueueService<Block> {
		&mut self.service
	}

	fn poll_actions(&mut self, cx: &mut futures::task::Context, link: &dyn Link<Block>) {
		self.inner.poll_actions(cx, link)
	}

	async fn run(self, link: &dyn Link<Block>) {
		self.inner.run(link).await
	}
}

/// Awaits the lookahead covering `hash`, if one was scheduled.
///
/// `None` means the import path should verify the block itself — either nothing was scheduled, or
/// what was scheduled could conclude nothing.
pub async fn await_lookahead(
	registry: &LookaheadRegistry,
	hash: &BlockHash,
) -> Option<LookaheadOutcome> {
	let handle = registry.take(hash)?;
	match handle.await {
		Ok(outcome) => Some(outcome),
		// The worker died or the job was dropped; fall back rather than stall.
		Err(_) => Some(LookaheadOutcome::Unavailable),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Records what it was asked to verify and answers with a scripted outcome.
	struct FakeVerifier {
		outcome: LookaheadOutcome,
		calls: Mutex<Vec<(BlockHash, usize, u64)>>,
	}

	impl FakeVerifier {
		fn new(outcome: LookaheadOutcome) -> Arc<Self> {
			Arc::new(Self { outcome, calls: Mutex::new(Vec::new()) })
		}
		fn calls(&self) -> Vec<(BlockHash, usize, u64)> {
			self.calls.lock().unwrap().clone()
		}
	}

	impl LookaheadVerify for FakeVerifier {
		fn verify(&self, at: BlockHash, txs: Vec<Vec<u8>>, extra_secs: u64) -> LookaheadOutcome {
			self.calls.lock().unwrap().push((at, txs.len(), extra_secs));
			self.outcome
		}
	}

	#[test]
	fn registry_evicts_oldest_beyond_capacity() {
		let registry = LookaheadRegistry::new(2);
		let mut keep = Vec::new();
		for i in 0u8..3 {
			let (tx, rx) = oneshot::channel::<LookaheadOutcome>();
			keep.push(tx);
			registry.insert(BlockHash::from([i; 32]), rx.shared());
		}
		assert_eq!(registry.len(), 2);
		assert!(registry.take(&BlockHash::from([0u8; 32])).is_none(), "oldest should be evicted");
		assert!(registry.take(&BlockHash::from([2u8; 32])).is_some());
	}

	#[test]
	fn registry_take_is_once_only() {
		let registry = LookaheadRegistry::new(4);
		let (_tx, rx) = oneshot::channel::<LookaheadOutcome>();
		let hash = BlockHash::from([7u8; 32]);
		registry.insert(hash, rx.shared());
		assert!(registry.take(&hash).is_some());
		assert!(registry.take(&hash).is_none());
		assert_eq!(registry.len(), 0);
	}

	/// A dropped sender must resolve the wait rather than hang it: an import that cannot learn the
	/// lookahead's answer has to fall back, not stall the queue.
	#[tokio::test]
	async fn dropped_job_resolves_to_unavailable() {
		let registry = LookaheadRegistry::new(4);
		let hash = BlockHash::from([9u8; 32]);
		{
			let (tx, rx) = oneshot::channel::<LookaheadOutcome>();
			registry.insert(hash, rx.shared());
			drop(tx);
		}
		assert_eq!(await_lookahead(&registry, &hash).await, Some(LookaheadOutcome::Unavailable));
	}

	#[tokio::test]
	async fn no_lookahead_scheduled_means_no_wait() {
		let registry = LookaheadRegistry::new(4);
		assert_eq!(await_lookahead(&registry, &BlockHash::from([1u8; 32])).await, None);
	}

	// --- scheduling ------------------------------------------------------------------------

	use midnight_node_runtime::opaque::Header;
	use parity_scale_codec::Encode;
	use sp_runtime::OpaqueExtrinsic;

	/// An extrinsic the production extractor will recognise as a Midnight transaction.
	fn midnight_xt(tag: u8) -> OpaqueExtrinsic {
		let call = midnight_node_runtime::RuntimeCall::Midnight(
			midnight_node_runtime::MidnightCall::send_mn_transaction { midnight_tx: vec![tag; 4] },
		);
		let xt = midnight_node_runtime::UncheckedExtrinsic::new_bare(call);
		OpaqueExtrinsic::try_from_encoded_extrinsic(&xt.encode()).expect("opaque extrinsic")
	}

	/// An extrinsic that is not a Midnight transaction.
	fn other_xt() -> OpaqueExtrinsic {
		OpaqueExtrinsic::try_from_encoded_extrinsic(&[0u8; 4].encode()).expect("opaque extrinsic")
	}

	fn block_at(
		number: u32,
		parent: BlockHash,
		body: Vec<OpaqueExtrinsic>,
	) -> IncomingBlock<Block> {
		let header =
			Header::new(number, Default::default(), Default::default(), parent, Default::default());
		IncomingBlock {
			hash: header.hash(),
			header: Some(header),
			body: Some(body),
			indexed_body: None,
			justifications: None,
			origin: None,
			allow_missing_state: false,
			import_existing: false,
			state: None,
			skip_execution: false,
		}
	}

	/// A contiguous chain of `n` blocks each carrying `txs_per_block` Midnight transactions,
	/// newest first — the order the sync engine hands them over in.
	fn chain(n: u32, txs_per_block: usize) -> (BlockHash, Vec<IncomingBlock<Block>>) {
		let root = BlockHash::from([42u8; 32]);
		let mut parent = root;
		let mut blocks = Vec::new();
		for number in 1..=n {
			let body = (0..txs_per_block).map(|i| midnight_xt(i as u8)).collect();
			let block = block_at(number, parent, body);
			parent = block.hash;
			blocks.push(block);
		}
		blocks.reverse();
		(root, blocks)
	}

	fn scheduler(
		cfg: LookaheadConfig,
	) -> (LookaheadScheduler, async_channel::Receiver<LookaheadJob>) {
		let (tx, rx) = async_channel::bounded(cfg.queue_capacity.max(1));
		let registry = Arc::new(LookaheadRegistry::new(cfg.registry_capacity));
		(LookaheadScheduler::with_channel(tx, registry, cfg), rx)
	}

	fn drain(rx: &async_channel::Receiver<LookaheadJob>) -> Vec<LookaheadJob> {
		let mut jobs = Vec::new();
		while let Ok(job) = rx.try_recv() {
			jobs.push(job);
		}
		jobs
	}

	/// Enqueuing must not verify anything: at ingress the blocks' parents are themselves still
	/// queued, so there is no state to verify against yet. Scheduling happens on `advance`.
	#[test]
	fn enqueue_alone_schedules_nothing() {
		let (sched, rx) = scheduler(LookaheadConfig::default());
		let (_, blocks) = chain(4, 1);

		sched.enqueue(&blocks);

		assert!(rx.try_recv().is_err(), "no job until a reference state exists");
	}

	/// A group is verified against the block just imported — the state its first member's
	/// transactions were actually built against.
	#[test]
	fn a_group_is_verified_against_the_block_just_imported() {
		let cfg = LookaheadConfig { blocks_per_job: 3, ..Default::default() };
		let (sched, rx) = scheduler(cfg);
		let (root, blocks) = chain(6, 1);
		sched.enqueue(&blocks);

		sched.advance(root, 0);

		let jobs = drain(&rx);
		assert_eq!(jobs.len(), 1, "one group per advance");
		assert_eq!(jobs[0].at, root);
		assert_eq!(jobs[0].blocks, 3);
	}

	/// The staleness bound is what keeps this design honest: a reference more than a few blocks
	/// old fails the non-crypto checks for most transactions, and the job then verifies nothing
	/// while still costing a worker. Blocks past the horizon wait for a closer reference.
	#[test]
	fn advance_does_not_reach_past_the_staleness_horizon() {
		let cfg =
			LookaheadConfig { blocks_per_job: 100, max_stale_blocks: 3, ..Default::default() };
		let (sched, rx) = scheduler(cfg);
		let (root, blocks) = chain(10, 1);
		sched.enqueue(&blocks);

		sched.advance(root, 0);

		let jobs = drain(&rx);
		assert_eq!(jobs.len(), 1);
		assert_eq!(jobs[0].blocks, 3, "blocks 1..=3 only; 4 is beyond the horizon");
	}

	/// Blocks at or behind the cursor have already been imported; re-verifying them would be pure
	/// waste, and registering them would leave handles nothing ever consumes.
	#[test]
	fn advance_ignores_blocks_at_or_behind_the_cursor() {
		let (sched, rx) = scheduler(LookaheadConfig::default());
		let (_, blocks) = chain(4, 1);
		let fourth = blocks[0].hash;
		sched.enqueue(&blocks);

		// Pretend the cursor is already past every enqueued block.
		sched.advance(fourth, 4);

		assert!(rx.try_recv().is_err(), "nothing left ahead of the cursor");
	}

	/// Blocks with nothing to verify cost the import path nothing, so they must not take up room
	/// in a group or leave a registry entry an import would then wait on.
	#[test]
	fn blocks_with_nothing_to_verify_are_skipped() {
		let (sched, rx) = scheduler(LookaheadConfig { blocks_per_job: 8, ..Default::default() });
		let root = BlockHash::from([42u8; 32]);
		let with_txs = block_at(1, root, vec![midnight_xt(1)]);
		let no_midnight_txs = block_at(2, with_txs.hash, vec![other_xt()]);
		let mut bodiless = block_at(3, no_midnight_txs.hash, vec![midnight_xt(2)]);
		bodiless.body = None;
		let mut skipped = block_at(4, bodiless.hash, vec![midnight_xt(3)]);
		skipped.skip_execution = true;
		let hashes = (with_txs.hash, no_midnight_txs.hash, bodiless.hash, skipped.hash);

		sched.enqueue(&[skipped, bodiless, no_midnight_txs, with_txs]);
		sched.advance(root, 0);

		let jobs = drain(&rx);
		assert_eq!(jobs.len(), 1);
		assert_eq!(jobs[0].blocks, 1, "only the block with Midnight transactions");
		assert!(sched.registry.take(&hashes.0).is_some());
		for skipped_hash in [hashes.1, hashes.2, hashes.3] {
			assert!(sched.registry.take(&skipped_hash).is_none(), "must not be registered");
		}
	}

	/// A group is closed by whichever limit binds first, so one chunk of full blocks cannot build
	/// an unboundedly large aggregate call.
	#[test]
	fn groups_are_capped_by_transaction_count() {
		let cfg = LookaheadConfig {
			blocks_per_job: 100,
			max_txs_per_job: 5,
			max_stale_blocks: 64,
			..Default::default()
		};
		let (sched, rx) = scheduler(cfg);
		let (root, blocks) = chain(4, 3);
		sched.enqueue(&blocks);

		sched.advance(root, 0);

		let jobs = drain(&rx);
		assert_eq!(jobs.len(), 1);
		assert_eq!(jobs[0].txs.len(), 3, "a second block's 3 txs would exceed 5");
		// The block that did not fit is still pending, not dropped.
		assert_eq!(sched.pending.lock().unwrap().len(), 3);
	}

	/// When the pool is saturated the group is dropped rather than queued, and — critically —
	/// nothing is registered, so those blocks take the ordinary per-block path instead of awaiting
	/// a result that will never arrive.
	#[test]
	fn a_shed_group_registers_nothing() {
		let cfg = LookaheadConfig {
			blocks_per_job: 1,
			queue_capacity: 1,
			max_stale_blocks: 64,
			..Default::default()
		};
		let (sched, _rx) = scheduler(cfg);
		let (root, blocks) = chain(3, 1);
		let hashes: Vec<BlockHash> = blocks.iter().map(|b| b.hash).collect();
		sched.enqueue(&blocks);

		// Fill the queue, then keep advancing: later groups have nowhere to go.
		sched.advance(root, 0);
		sched.advance(hashes[2], 1);
		sched.advance(hashes[1], 2);

		let registered = hashes.iter().filter(|h| sched.registry.take(h).is_some()).count();
		assert_eq!(registered, 1, "only the group that fit in the queue is registered");
	}

	/// One job covers several blocks, so its verdict has to reach every one of them; a block whose
	/// reply never arrives would wait and then verify itself, losing the overlap.
	#[tokio::test]
	async fn a_worker_broadcasts_its_verdict_to_every_block_in_the_group() {
		let verifier = FakeVerifier::new(LookaheadOutcome::Verified);
		let (jobs_tx, jobs_rx) = async_channel::bounded(4);
		let registry = Arc::new(LookaheadRegistry::new(64));
		let cfg = LookaheadConfig { blocks_per_job: 3, ..Default::default() };
		let sched = LookaheadScheduler::with_channel(jobs_tx, registry.clone(), cfg);
		let (root, blocks) = chain(3, 2);
		let hashes: Vec<BlockHash> = blocks.iter().map(|b| b.hash).collect();
		sched.enqueue(&blocks);
		sched.advance(root, 0);

		let worker = tokio::spawn(run_worker(
			jobs_rx,
			verifier.clone() as Arc<dyn LookaheadVerify>,
			BatchVerifyMetrics::new(None),
		));

		for hash in &hashes {
			assert_eq!(
				await_lookahead(&registry, hash).await,
				Some(LookaheadOutcome::Verified),
				"every block in the group learns the verdict",
			);
		}
		assert_eq!(verifier.calls().len(), 1, "one aggregate call for the whole group");
		let (at, txs, _) = verifier.calls()[0];
		assert_eq!(at, root);
		assert_eq!(txs, 6);

		drop(sched);
		worker.abort();
	}
}
