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

//! Import-queue dispatch for the AURA→BABE consensus migration.
//!
//! During the migration the chain switches its block-production engine at the consensus flip, so
//! both engines' import pipelines must coexist. `sc_consensus_babe` doesn't expose a composable
//! verifier (only a whole `import_queue`), so rather than dispatch at the verifier/block-import
//! layer we dispatch one level up: [`DispatchImportQueue`] wraps the AURA import queue and a full
//! BABE import queue, and routes each incoming block to the queue for the engine that **authored**
//! it, read from the block's own header — see [`engine_from_pre_runtime_digest`].
//!
//! # Routing key: the first AURA/BABE pre-runtime digest
//!
//! The key must come from the block being routed, not from chain state. Reading the engine from the
//! parent's runtime state (`ConsensusEngineApi::active_engine`) needs the parent to be *imported*,
//! and at the flip boundary it is not: a sync batch `[…, flip, flip+1, …]` carries the first BABE
//! block together with its parent, so the parent's state does not exist when the batch is routed.
//! The engine change is only visible in the flip block's post-state, so nothing derived from
//! earlier blocks in the batch can see it either.
//!
//! The header can. `pallet-consensus-engine` asserts in `on_initialize`, for every block that
//! executes, that:
//! - before arming, no BABE pre-runtime digest is present;
//! - from arming to the flip, exactly one AURA and exactly one BABE pre-runtime digest are present
//!   and the AURA one comes **first**;
//! - after the flip, no AURA pre-runtime digest is present.
//!
//! So for any valid block the first pre-runtime digest with an AURA or BABE engine id names the
//! engine that authored it. That is the same invariant the runtime enforces (the seal, by contrast,
//! is only checked by the node-side verifiers), which keeps routing keyed to what the chain itself
//! guarantees. Pre-runtime digests from other engines (e.g. the partner-chains main-chain hash) are
//! skipped.
//!
//! Routing on the header does not weaken the migration guards: it decides *which verifier runs*,
//! not whether the block is valid. A block whose digests misstate its engine fails either the
//! receiving verifier's seal/author checks or the pallet's digest assertions at execution.
//!
//! # Ordering across the two queues
//!
//! Each wrapped queue is a `BasicQueue` with its own background worker, and submitting a batch to a
//! queue only enqueues it on that worker's channel. Two workers give no ordering between them: at
//! the flip a sync batch `[…, flip, flip+1, …]` is split into an AURA part ending at the flip block
//! and a BABE part starting at its child, and the BABE worker can pick up `flip+1` before the AURA
//! worker has imported `flip`. The same happens across consecutive submissions — sync keeps
//! queueing batches without waiting for results, so an all-AURA batch followed by an all-BABE batch
//! races the same way. The BABE import then fails with `UnknownParent`, which makes sync restart
//! (and, once the missing parent is in, re-request the same blocks).
//!
//! The dispatcher therefore orders the BABE queue behind the AURA queue: it counts AURA batches whose
//! results have not yet come back through the [`Link`], and while that count is non-zero any BABE
//! batch is held in the dispatcher instead of being submitted. Held batches are released, in
//! submission order, when the AURA queue has nothing in flight. AURA never has to wait for BABE,
//! since the flip is one-way and no AURA block can descend from a BABE block. Pre-flip and post-flip
//! only one side is ever non-empty, so the gate is inert outside the transition.
//!
//! Held batches are always eventually submitted — never dropped — because sync's bookkeeping
//! (`queue_blocks`, import backpressure) expects a result for every block it handed to the queue.
//!
//! Justifications are finality (GRANDPA) and engine-agnostic; they are routed to the AURA queue,
//! whose block import owns the GRANDPA justification import.

use async_trait::async_trait;
use midnight_primitives_consensus_engine::ActiveEngine;
use sc_consensus::import_queue::{
	BlockImportError, BlockImportStatus, ImportQueue, ImportQueueService, IncomingBlock,
	JustificationImportResult, Link, RuntimeOrigin,
};
use sp_consensus::BlockOrigin;
use sp_consensus_aura::AURA_ENGINE_ID;
use sp_consensus_babe::BABE_ENGINE_ID;
use sp_runtime::Justifications;
use sp_runtime::traits::{Block as BlockT, Header as _, NumberFor};
use std::{
	collections::VecDeque,
	sync::{Arc, Mutex, MutexGuard, PoisonError},
};

const LOG_TARGET: &str = "consensus-engine-dispatch";

/// The consensus engine that authored `header`: the engine id of its first AURA or BABE
/// pre-runtime digest, or `None` if it carries neither.
///
/// See the module docs for why the *first* such digest is decisive: `pallet-consensus-engine`
/// requires the AURA pre-runtime digest to precede the BABE one on every armed-phase AURA block, and
/// forbids an AURA pre-runtime digest on every post-flip BABE block.
pub fn engine_from_pre_runtime_digest<Block: BlockT>(
	header: &Block::Header,
) -> Option<ActiveEngine> {
	header.digest().logs().iter().find_map(|log| match log.as_pre_runtime() {
		Some((id, _)) if id == AURA_ENGINE_ID => Some(ActiveEngine::Aura),
		Some((id, _)) if id == BABE_ENGINE_ID => Some(ActiveEngine::Babe),
		_ => None,
	})
}

/// Split an ordered batch of incoming blocks by authoring engine, preserving order within each
/// side. Blocks without a header, or without an AURA/BABE pre-runtime digest (which can't be
/// routed), default to the AURA queue — its verifier produces the clearer error for them.
fn split_by_engine<Block: BlockT>(
	blocks: Vec<IncomingBlock<Block>>,
) -> (Vec<IncomingBlock<Block>>, Vec<IncomingBlock<Block>>) {
	let mut aura = Vec::new();
	let mut babe = Vec::new();
	for block in blocks {
		let engine = block
			.header
			.as_ref()
			.and_then(engine_from_pre_runtime_digest::<Block>)
			.unwrap_or(ActiveEngine::Aura);
		match engine {
			ActiveEngine::Aura => aura.push(block),
			ActiveEngine::Babe => babe.push(block),
		}
	}
	(aura, babe)
}

/// Orders the BABE queue behind the AURA queue (see the module docs).
///
/// Owns the single handle through which every BABE batch is submitted, so held and direct
/// submissions cannot overtake each other. Shared by all [`DispatchImportQueueService`] handles and
/// by the [`Link`] wrapper that observes the AURA queue's results.
struct BabeGate<Block: BlockT> {
	/// AURA batches submitted whose `blocks_processed` has not yet been observed.
	aura_in_flight: usize,
	/// BABE batches held until `aura_in_flight` drops to zero, in submission order.
	held: VecDeque<(BlockOrigin, Vec<IncomingBlock<Block>>)>,
	babe: Box<dyn ImportQueueService<Block>>,
}

impl<Block: BlockT> BabeGate<Block> {
	fn new(babe: Box<dyn ImportQueueService<Block>>) -> Self {
		Self { aura_in_flight: 0, held: VecDeque::new(), babe }
	}

	/// Record that a batch is about to be submitted to the AURA queue.
	fn aura_submitted(&mut self) {
		self.aura_in_flight += 1;
	}

	/// The AURA queue reported the results of one batch.
	fn aura_processed(&mut self) {
		self.aura_in_flight = self.aura_in_flight.saturating_sub(1);
		if self.aura_in_flight == 0 && !self.held.is_empty() {
			log::debug!(
				target: LOG_TARGET,
				"AURA queue drained; releasing {} held BABE batch(es)",
				self.held.len(),
			);
			for (origin, blocks) in self.held.drain(..) {
				self.babe.import_blocks(origin, blocks);
			}
		}
	}

	/// Submit a BABE batch now, or hold it while the AURA queue still has work in flight.
	fn submit_babe(&mut self, origin: BlockOrigin, blocks: Vec<IncomingBlock<Block>>) {
		if self.aura_in_flight == 0 && self.held.is_empty() {
			self.babe.import_blocks(origin, blocks);
		} else {
			log::debug!(
				target: LOG_TARGET,
				"Holding {} BABE block(s) behind {} in-flight AURA batch(es)",
				blocks.len(),
				self.aura_in_flight,
			);
			self.held.push_back((origin, blocks));
		}
	}
}

/// Lock the gate, tolerating poisoning: the state is simple counters and a queue, and a panic in a
/// holder leaves it consistent enough to keep routing.
fn lock_gate<Block: BlockT>(gate: &Mutex<BabeGate<Block>>) -> MutexGuard<'_, BabeGate<Block>> {
	gate.lock().unwrap_or_else(PoisonError::into_inner)
}

/// [`Link`] handed to the AURA queue: forwards every callback to the real link, and after each
/// `blocks_processed` lets the gate release BABE batches that were waiting on the AURA queue.
struct AuraLink<'a, Block: BlockT> {
	inner: &'a dyn Link<Block>,
	gate: Arc<Mutex<BabeGate<Block>>>,
}

impl<Block: BlockT> Link<Block> for AuraLink<'_, Block> {
	fn blocks_processed(
		&self,
		imported: usize,
		count: usize,
		results: Vec<(Result<BlockImportStatus<NumberFor<Block>>, BlockImportError>, Block::Hash)>,
	) {
		self.inner.blocks_processed(imported, count, results);
		lock_gate(&self.gate).aura_processed();
	}

	fn justification_imported(
		&self,
		who: RuntimeOrigin,
		hash: &Block::Hash,
		number: NumberFor<Block>,
		import_result: JustificationImportResult,
	) {
		self.inner.justification_imported(who, hash, number, import_result);
	}

	fn request_justification(&self, hash: &Block::Hash, number: NumberFor<Block>) {
		self.inner.request_justification(hash, number);
	}
}

/// Service handle for [`DispatchImportQueue`]: routes submitted blocks to the AURA or BABE queue.
pub struct DispatchImportQueueService<Block: BlockT> {
	aura: Box<dyn ImportQueueService<Block>>,
	gate: Arc<Mutex<BabeGate<Block>>>,
}

impl<Block: BlockT> ImportQueueService<Block> for DispatchImportQueueService<Block> {
	fn import_blocks(&mut self, origin: BlockOrigin, blocks: Vec<IncomingBlock<Block>>) {
		let (aura, babe) = split_by_engine(blocks);
		// Hold the gate across both submissions: the AURA batch must be counted before the BABE
		// part of the same call is considered for release.
		let mut gate = lock_gate(&self.gate);
		if !aura.is_empty() {
			gate.aura_submitted();
			self.aura.import_blocks(origin, aura);
		}
		if !babe.is_empty() {
			gate.submit_babe(origin, babe);
		}
	}

	fn import_justifications(
		&mut self,
		who: RuntimeOrigin,
		hash: Block::Hash,
		number: NumberFor<Block>,
		justifications: Justifications,
	) {
		// Finality (GRANDPA) is engine-agnostic; the GRANDPA justification import lives under the
		// AURA queue.
		self.aura.import_justifications(who, hash, number, justifications);
	}
}

/// Import queue that dispatches blocks to the AURA or BABE import queue based on the engine that
/// authored them. Both queues must ultimately write to the same backend.
pub struct DispatchImportQueue<Block: BlockT, Aura, Babe> {
	aura: Aura,
	babe: Babe,
	gate: Arc<Mutex<BabeGate<Block>>>,
	service: DispatchImportQueueService<Block>,
}

impl<Block, Aura, Babe> DispatchImportQueue<Block, Aura, Babe>
where
	Block: BlockT,
	Aura: ImportQueue<Block>,
	Babe: ImportQueue<Block>,
{
	pub fn new(aura: Aura, babe: Babe) -> Self {
		let gate = Arc::new(Mutex::new(BabeGate::new(babe.service())));
		let service = DispatchImportQueueService { aura: aura.service(), gate: gate.clone() };
		Self { aura, babe, gate, service }
	}
}

#[async_trait]
impl<Block, Aura, Babe> ImportQueue<Block> for DispatchImportQueue<Block, Aura, Babe>
where
	Block: BlockT,
	Aura: ImportQueue<Block>,
	Babe: ImportQueue<Block>,
{
	fn service(&self) -> Box<dyn ImportQueueService<Block>> {
		Box::new(DispatchImportQueueService { aura: self.aura.service(), gate: self.gate.clone() })
	}

	fn service_ref(&mut self) -> &mut dyn ImportQueueService<Block> {
		&mut self.service
	}

	fn poll_actions(&mut self, cx: &mut futures::task::Context, link: &dyn Link<Block>) {
		let aura_link = AuraLink { inner: link, gate: self.gate.clone() };
		self.aura.poll_actions(cx, &aura_link);
		self.babe.poll_actions(cx, link);
	}

	async fn run(self, link: &dyn Link<Block>) {
		// Drive both engines' queue workers; each forwards its results to the shared link. The
		// AURA side goes through `AuraLink` so its results also release gated BABE batches.
		let aura_link = AuraLink { inner: link, gate: self.gate.clone() };
		futures::future::join(self.aura.run(&aura_link), self.babe.run(link)).await;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use midnight_node_runtime::opaque::{Block, Header};
	use sp_core::H256;
	use sp_runtime::{ConsensusEngineId, DigestItem, traits::Header as HeaderT};

	/// Records the hashes of blocks submitted to it.
	#[derive(Clone, Default)]
	struct Recorder(Arc<Mutex<Vec<u8>>>);
	impl Recorder {
		fn hashes(&self) -> Vec<u8> {
			self.0.lock().unwrap().clone()
		}
	}
	impl ImportQueueService<Block> for Recorder {
		fn import_blocks(&mut self, _origin: BlockOrigin, blocks: Vec<IncomingBlock<Block>>) {
			self.0.lock().unwrap().extend(blocks.iter().map(|b| b.hash.as_ref()[0]));
		}
		fn import_justifications(
			&mut self,
			_who: RuntimeOrigin,
			_hash: <Block as BlockT>::Hash,
			_number: NumberFor<Block>,
			_justifications: Justifications,
		) {
		}
	}

	/// Link with the trait's no-op defaults, standing in for the sync engine.
	struct NoopLink;
	impl Link<Block> for NoopLink {}

	/// Harness: a service handle plus the AURA-side link the real queue runner would use.
	struct Harness {
		service: DispatchImportQueueService<Block>,
		gate: Arc<Mutex<BabeGate<Block>>>,
		aura: Recorder,
		babe: Recorder,
	}

	impl Harness {
		fn new() -> Self {
			let (aura, babe) = (Recorder::default(), Recorder::default());
			let gate = Arc::new(Mutex::new(BabeGate::new(Box::new(babe.clone()))));
			let service =
				DispatchImportQueueService { aura: Box::new(aura.clone()), gate: gate.clone() };
			Self { service, gate, aura, babe }
		}

		fn import(&mut self, blocks: Vec<IncomingBlock<Block>>) {
			self.service.import_blocks(BlockOrigin::NetworkInitialSync, blocks);
		}

		/// Simulate the AURA queue's worker reporting one batch through the link.
		fn aura_batch_done(&self) {
			AuraLink { inner: &NoopLink, gate: self.gate.clone() }.blocks_processed(0, 0, vec![]);
		}
	}

	fn hash_with_first_byte(byte: u8) -> H256 {
		let mut bytes = [0u8; 32];
		bytes[0] = byte;
		H256::from(bytes)
	}

	/// Some other engine's pre-runtime digest, as the partner-chains main-chain hash digest is.
	const OTHER_ENGINE_ID: ConsensusEngineId = *b"mcsh";

	fn pre_runtime(id: ConsensusEngineId) -> DigestItem {
		DigestItem::PreRuntime(id, vec![0])
	}

	fn header_with(logs: Vec<DigestItem>) -> Header {
		let mut header = Header::new(
			1,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		);
		for log in logs {
			header.digest_mut().push(log);
		}
		header
	}

	fn incoming(header: Option<Header>, hash_first_byte: u8) -> IncomingBlock<Block> {
		IncomingBlock {
			hash: hash_with_first_byte(hash_first_byte),
			header,
			body: None,
			indexed_body: None,
			justifications: None,
			origin: None,
			allow_missing_state: false,
			skip_execution: false,
			import_existing: false,
			state: None,
		}
	}

	/// A pre-arming AURA block: AURA pre-runtime digest only (plus the mc-hash one), AURA seal.
	fn aura_block(hash: u8) -> IncomingBlock<Block> {
		incoming(
			Some(header_with(vec![
				pre_runtime(OTHER_ENGINE_ID),
				pre_runtime(AURA_ENGINE_ID),
				DigestItem::Seal(AURA_ENGINE_ID, vec![1]),
			])),
			hash,
		)
	}

	/// An armed-phase AURA block: AURA pre-runtime digest first, then the BABE `SecondaryPlain`
	/// one, in the order `pallet-consensus-engine` enforces. AURA seal.
	fn armed_aura_block(hash: u8) -> IncomingBlock<Block> {
		incoming(
			Some(header_with(vec![
				pre_runtime(OTHER_ENGINE_ID),
				pre_runtime(AURA_ENGINE_ID),
				pre_runtime(BABE_ENGINE_ID),
				DigestItem::Seal(AURA_ENGINE_ID, vec![1]),
			])),
			hash,
		)
	}

	/// A post-flip BABE block: BABE pre-runtime digest only, BABE seal.
	fn babe_block(hash: u8) -> IncomingBlock<Block> {
		incoming(
			Some(header_with(vec![
				pre_runtime(OTHER_ENGINE_ID),
				pre_runtime(BABE_ENGINE_ID),
				DigestItem::Seal(BABE_ENGINE_ID, vec![2]),
			])),
			hash,
		)
	}

	fn engine_of(block: &IncomingBlock<Block>) -> Option<ActiveEngine> {
		engine_from_pre_runtime_digest::<Block>(block.header.as_ref().unwrap())
	}

	#[test]
	fn engine_is_read_from_the_first_aura_or_babe_pre_runtime_digest() {
		assert_eq!(engine_of(&aura_block(1)), Some(ActiveEngine::Aura));
		assert_eq!(engine_of(&babe_block(1)), Some(ActiveEngine::Babe));
	}

	#[test]
	fn armed_aura_block_with_both_pre_digests_is_aura() {
		// From arming to the flip every AURA block also carries a BABE pre-runtime digest; the
		// pallet guarantees the AURA one comes first, and that order is what decides.
		assert_eq!(engine_of(&armed_aura_block(1)), Some(ActiveEngine::Aura));
	}

	#[test]
	fn other_engines_pre_runtime_digests_are_skipped() {
		let header = header_with(vec![pre_runtime(OTHER_ENGINE_ID), pre_runtime(BABE_ENGINE_ID)]);
		assert_eq!(engine_from_pre_runtime_digest::<Block>(&header), Some(ActiveEngine::Babe));
	}

	#[test]
	fn header_without_an_aura_or_babe_pre_runtime_digest_has_no_engine() {
		let header = header_with(vec![
			pre_runtime(OTHER_ENGINE_ID),
			DigestItem::Seal(BABE_ENGINE_ID, vec![]),
		]);
		assert_eq!(engine_from_pre_runtime_digest::<Block>(&header), None);
	}

	#[test]
	fn service_splits_batch_by_authoring_engine() {
		let mut h = Harness::new();

		h.import(vec![armed_aura_block(10), babe_block(20), aura_block(30)]);
		h.aura_batch_done();

		assert_eq!(h.aura.hashes(), vec![10, 30]);
		assert_eq!(h.babe.hashes(), vec![20]);
	}

	#[test]
	fn service_routes_headerless_block_to_aura() {
		let mut h = Harness::new();

		let mut block = babe_block(42);
		block.header = None; // no header, so it can't be routed → AURA
		h.import(vec![block]);

		assert_eq!(h.aura.hashes(), vec![42]);
		assert!(h.babe.hashes().is_empty());
	}

	#[test]
	fn service_routes_block_without_engine_digest_to_aura() {
		let mut h = Harness::new();

		h.import(vec![incoming(Some(header_with(vec![pre_runtime(OTHER_ENGINE_ID)])), 43)]);

		assert_eq!(h.aura.hashes(), vec![43]);
		assert!(h.babe.hashes().is_empty());
	}

	#[test]
	fn babe_part_of_a_straddling_batch_waits_for_the_aura_part() {
		let mut h = Harness::new();

		// The flip-boundary sync batch: the last AURA block (97), then the first BABE blocks.
		h.import(vec![armed_aura_block(96), armed_aura_block(97), babe_block(98), babe_block(99)]);

		// AURA got its part immediately; BABE gets nothing until the AURA worker reports.
		assert_eq!(h.aura.hashes(), vec![96, 97]);
		assert!(h.babe.hashes().is_empty());

		h.aura_batch_done();
		assert_eq!(h.babe.hashes(), vec![98, 99]);
	}

	#[test]
	fn babe_batch_waits_for_an_earlier_aura_batch() {
		let mut h = Harness::new();

		// Consecutive submissions, as sync does without waiting for results.
		h.import(vec![armed_aura_block(96), armed_aura_block(97)]);
		h.import(vec![babe_block(98)]);

		assert!(h.babe.hashes().is_empty());
		h.aura_batch_done();
		assert_eq!(h.babe.hashes(), vec![98]);
	}

	#[test]
	fn babe_waits_for_every_in_flight_aura_batch_and_keeps_order() {
		let mut h = Harness::new();

		h.import(vec![aura_block(10)]);
		h.import(vec![aura_block(11)]);
		h.import(vec![babe_block(20)]);
		h.import(vec![babe_block(21)]);

		h.aura_batch_done();
		assert!(h.babe.hashes().is_empty(), "one AURA batch is still in flight");

		h.aura_batch_done();
		assert_eq!(h.babe.hashes(), vec![20, 21]);
	}

	#[test]
	fn a_new_aura_batch_extends_the_wait() {
		let mut h = Harness::new();

		h.import(vec![aura_block(10)]);
		h.import(vec![babe_block(20)]);
		// A further AURA batch arrives while BABE is already held (e.g. a late fork).
		h.import(vec![aura_block(11)]);

		h.aura_batch_done();
		assert!(h.babe.hashes().is_empty());
		h.aura_batch_done();
		assert_eq!(h.babe.hashes(), vec![20]);
	}

	#[test]
	fn babe_goes_straight_through_when_aura_is_idle() {
		let mut h = Harness::new();

		h.import(vec![babe_block(20)]);
		assert_eq!(h.babe.hashes(), vec![20]);

		// Once released, later BABE-only batches are not held either.
		h.import(vec![aura_block(10)]);
		h.import(vec![babe_block(21)]);
		h.aura_batch_done();
		h.import(vec![babe_block(22)]);
		assert_eq!(h.babe.hashes(), vec![20, 21, 22]);
	}

	#[test]
	fn aura_link_forwards_to_the_inner_link() {
		struct Counting(Mutex<(usize, usize, usize)>);
		impl Link<Block> for Counting {
			fn blocks_processed(
				&self,
				_: usize,
				_: usize,
				_: Vec<(Result<BlockImportStatus<u32>, BlockImportError>, H256)>,
			) {
				self.0.lock().unwrap().0 += 1;
			}
			fn justification_imported(
				&self,
				_: RuntimeOrigin,
				_: &H256,
				_: u32,
				_: JustificationImportResult,
			) {
				self.0.lock().unwrap().1 += 1;
			}
			fn request_justification(&self, _: &H256, _: u32) {
				self.0.lock().unwrap().2 += 1;
			}
		}

		let inner = Counting(Mutex::new((0, 0, 0)));
		let gate = Arc::new(Mutex::new(BabeGate::new(Box::new(Recorder::default()))));
		let link = AuraLink { inner: &inner, gate };

		link.blocks_processed(0, 0, vec![]);
		link.justification_imported(
			RuntimeOrigin::random(),
			&H256::zero(),
			1,
			JustificationImportResult::Success,
		);
		link.request_justification(&H256::zero(), 1);

		assert_eq!(*inner.0.lock().unwrap(), (1, 1, 1));
	}
}
