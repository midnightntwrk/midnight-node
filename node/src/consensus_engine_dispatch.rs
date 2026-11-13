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

//! Consensus-engine dispatch for the AURA→BABE migration.
//!
//! During the migration the chain switches its block-production engine at the consensus flip, so
//! both engines' verification and import logic must coexist behind the node's single import queue.
//! [`EngineDispatchVerifier`] and [`EngineDispatchBlockImport`] route each block to the AURA or BABE
//! verifier / block import for the engine that **authored** it, read from the block's own header —
//! see [`engine_from_pre_runtime_digest`].
//!
//! # Routing key: the header, read by [`crate::engine_digests`]
//!
//! The key must come from the block being routed, not from chain state. Reading the engine from the
//! parent's runtime state (`ConsensusEngineApi::active_engine`) needs the parent to be *imported*,
//! and at the flip boundary it is not: a sync batch `[…, flip, flip+1, …]` carries the first BABE
//! block together with its parent, so the parent's state does not exist when the batch is queued.
//! The engine change is only visible in the flip block's post-state, so nothing derived from
//! earlier blocks in the batch can see it either. Warp and gap sync have no state or bodies at all.
//!
//! The header can. [`engine_digests::authoring_engine`](crate::engine_digests::authoring_engine)
//! names the engine that authored a block from its pre-runtime digests, trusting their layout only
//! for blocks executed by a runtime that contains `pallet-consensus-engine` (which enforces it) as
//! told by the `pallet-version` digest, and treating everything older as plain AURA. That module
//! is the single place in the node that makes this decision; this one only routes on its answer.
//!
//! Routing on the header does not weaken the migration guards: it decides *which verifier runs*,
//! not whether the block is valid. A block whose digests misstate its engine fails either the
//! receiving verifier's seal/author checks or the pallet's digest assertions at execution.
//!
//! # Ordering
//!
//! Both engines sit behind one `BasicQueue`, whose single worker verifies and imports blocks one at
//! a time in submission order. At the flip a sync batch `[…, flip, flip+1, …]` is therefore
//! processed in order — the first BABE block is verified only after the flip block is imported —
//! with no cross-engine coordination.
//!
//! # Seeding BABE's epoch tree on the import path
//!
//! Nothing is imported through the BABE pipeline before the flip, so `BabeLink`'s `EpochChanges`
//! is empty and the first BABE block has no epoch to be verified under ("Could not fetch epoch at
//! <flip block>"). The tree has to be seeded at the flip block *after* it is imported and *before*
//! its child reaches the BABE verifier. Block-import notifications cannot drive that: the client
//! emits none for `BlockOrigin::NetworkInitialSync` (and other sync origins), and even at the tip a
//! notification-driven task runs asynchronously to the import worker.
//!
//! The verifier sees every BABE block right after its parent was imported, so
//! [`EngineDispatchVerifier`] asks an [`EpochSeeder`] to cover the block's parent immediately before
//! handing the block to the BABE verifier. The seeder must be idempotent and cheap once the tree
//! covers the parent, since this runs for every BABE block.
//!
//! # Parent of a BABE block
//!
//! A BABE-routed block must follow its parent by number, and the parent must carry a BABE
//! pre-runtime digest. [`check_babe_parent`] checks both before the BABE verifier runs. A block
//! whose parent is not imported is rejected, unless it is imported with its state (a warp-sync
//! target), which has no imported parent.
//!
//! # BABE chain weight of AURA blocks
//!
//! BABE's fork choice compares cumulative chain weights. [`AuraWeightBlockImport`] records weight
//! 0 for the AURA blocks of the pallet's era, on the import queue and on the AURA authoring path,
//! so a BABE block can be weighed against an AURA parent or best block. AURA blocks themselves
//! keep AURA's longest-chain rule, so between a flipped and an unflipped branch the best block
//! depends on import order until one branch extends: a BABE block wins over a longer AURA best
//! block by weight, and a longer AURA block wins over a BABE best block by number. Blocks in the
//! block gap are skipped, as in BABE's own import, and so are warp-proof headers.
//!
//! Justifications are finality (GRANDPA) and engine-agnostic; the GRANDPA justification import is
//! registered on the queue directly and is not part of this dispatch.

use crate::engine_digests::{
	authoring_engine, babe_pre_digest_is_authoritative, has_authoritative_babe_pre_digest,
};
use async_trait::async_trait;
use midnight_primitives_consensus_engine::ActiveEngine;
use parity_scale_codec::Encode;
use sc_consensus::{BlockCheckParams, BlockImport, BlockImportParams, ImportResult, Verifier};
use sc_consensus_babe::{BabeBlockWeight, aux_schema::block_weight_key};
use sp_blockchain::HeaderBackend;
use sp_consensus::{BlockOrigin, Error as ConsensusError};
use sp_runtime::traits::{Block as BlockT, Header as _, NumberFor, One};
use std::{marker::PhantomData, sync::Arc};

/// The pipeline a block is routed to: the engine that authored it. A header without an AURA/BABE
/// pre-runtime digest can't be routed and defaults to AURA, whose verifier produces the clearer
/// error for it.
fn route<Block: BlockT>(header: &Block::Header) -> ActiveEngine {
	authoring_engine::<Block>(header).unwrap_or(ActiveEngine::Aura)
}

/// Makes BABE's epoch tree able to resolve epochs for the children of a given parent block.
///
/// Called by [`EngineDispatchVerifier`] right before a block is handed to the BABE verifier (see the
/// module docs). Implementations must be idempotent, cheap when the tree already covers `parent`,
/// and must refuse to seed at a block whose state has not flipped to BABE — the parent is taken from
/// a peer-supplied header, so this is what stops a peer from adding epochs at an arbitrary block.
pub trait EpochSeeder<Block: BlockT>: Send + Sync {
	/// Best-effort: `parent` is not imported when the child is imported with its state.
	fn ensure_seeded_for_child_of(&self, parent: Block::Hash);
}

/// [`Verifier`] that hands each block to the AURA or BABE verifier by the engine that authored it,
/// seeding BABE's epoch tree at the block's parent before a BABE verification.
pub struct EngineDispatchVerifier<Block: BlockT, Aura, Babe> {
	aura: Aura,
	babe: Babe,
	seeder: Arc<dyn EpochSeeder<Block>>,
	parent_header: ParentHeaderLookup<Block>,
}

/// Looks an imported header up by hash; an error when it is not imported or cannot be read.
pub type ParentHeaderLookup<Block> =
	Box<dyn Fn(<Block as BlockT>::Hash) -> Result<<Block as BlockT>::Header, String> + Send + Sync>;

/// [`ParentHeaderLookup`] answered from the client's header backend.
pub fn parent_header_lookup<Block, C>(client: Arc<C>) -> ParentHeaderLookup<Block>
where
	Block: BlockT,
	C: HeaderBackend<Block> + Send + Sync + 'static,
{
	Box::new(move |hash| match client.header(hash) {
		Ok(Some(header)) => Ok(header),
		Ok(None) => Err(format!("parent {hash:?} of a BABE block is not imported")),
		Err(err) => Err(format!("parent {hash:?} of a BABE block cannot be read: {err}")),
	})
}

impl<Block: BlockT, Aura, Babe> EngineDispatchVerifier<Block, Aura, Babe> {
	pub fn new(
		seeder: Arc<dyn EpochSeeder<Block>>,
		parent_header: ParentHeaderLookup<Block>,
		aura: Aura,
		babe: Babe,
	) -> Self {
		Self { aura, babe, seeder, parent_header }
	}
}

/// Checks that a BABE-routed block follows its parent by number and that the parent carries a
/// readable BABE pre-runtime digest. Genesis is accepted as a parent.
fn check_babe_parent<Block: BlockT>(
	parent: &Block::Header,
	header: &Block::Header,
) -> Result<(), String> {
	if *header.number() != *parent.number() + One::one() {
		return Err(format!(
			"BABE block claims number {:?} but its parent {:?} is #{:?}",
			header.number(),
			header.parent_hash(),
			parent.number(),
		));
	}
	if !babe_pre_digest_is_authoritative::<Block>(parent)
		|| sc_consensus_babe::find_pre_digest::<Block>(parent).is_err()
	{
		return Err(format!(
			"parent {:?} (#{:?}) of a BABE block has no readable BABE pre-runtime digest",
			header.parent_hash(),
			parent.number(),
		));
	}
	Ok(())
}

#[async_trait]
impl<Block, Aura, Babe> Verifier<Block> for EngineDispatchVerifier<Block, Aura, Babe>
where
	Block: BlockT,
	Aura: Verifier<Block>,
	Babe: Verifier<Block>,
{
	async fn verify(
		&self,
		block: BlockImportParams<Block>,
	) -> Result<BlockImportParams<Block>, String> {
		match route::<Block>(&block.header) {
			ActiveEngine::Aura => self.aura.verify(block).await,
			ActiveEngine::Babe => {
				// A block imported with its state (a warp-sync target) has no imported parent. Any
				// other block reaches the verifier only after the queue found its parent, so the
				// lookup failing here is a safeguard.
				if !block.with_state() {
					let parent = (self.parent_header)(*block.header.parent_hash())?;
					check_babe_parent::<Block>(&parent, &block.header)?;
				}
				self.seeder.ensure_seeded_for_child_of(*block.header.parent_hash());
				self.babe.verify(block).await
			},
		}
	}
}

/// Whether a block number lies in the node's block gap, the history still being fetched after a
/// warp sync.
pub type InBlockGap<Block> = Arc<dyn Fn(NumberFor<Block>) -> bool + Send + Sync>;

/// [`InBlockGap`] answered from the client's chain info.
pub fn in_block_gap<Block, C>(client: Arc<C>) -> InBlockGap<Block>
where
	Block: BlockT,
	C: HeaderBackend<Block> + Send + Sync + 'static,
{
	Arc::new(move |number| {
		client
			.info()
			.block_gap
			.is_some_and(|gap| gap.start <= number && number <= gap.end)
	})
}

/// [`BlockImport`] for AURA-authored blocks that records a zero BABE chain weight for the blocks of
/// the pallet's era, the value the epoch seeder gives the flip block.
///
/// Only matters while AURA and BABE blocks can compete, until the flip block is finalized; it can
/// be removed once that holds on every network.
pub struct AuraWeightBlockImport<Block: BlockT, Inner> {
	inner: Inner,
	in_block_gap: InBlockGap<Block>,
}

impl<Block: BlockT, Inner> AuraWeightBlockImport<Block, Inner> {
	pub fn new(inner: Inner, in_block_gap: InBlockGap<Block>) -> Self {
		Self { inner, in_block_gap }
	}
}

#[async_trait]
impl<Block, Inner> BlockImport<Block> for AuraWeightBlockImport<Block, Inner>
where
	Block: BlockT,
	Inner: BlockImport<Block, Error = ConsensusError> + Send + Sync,
{
	type Error = ConsensusError;

	async fn check_block(
		&self,
		block: BlockCheckParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).await
	}

	async fn import_block(
		&self,
		mut block: BlockImportParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		// Gap blocks and warp-proof headers are finalized history: fork choice never weighs them,
		// and no finality notification would prune their entry.
		if has_authoritative_babe_pre_digest::<Block>(&block.header)
			&& block.origin != BlockOrigin::WarpSync
			&& !(self.in_block_gap)(*block.header.number())
		{
			block
				.auxiliary
				.push((block_weight_key(block.post_hash()), Some((0 as BabeBlockWeight).encode())));
		}
		self.inner.import_block(block).await
	}
}

/// [`BlockImport`] that hands each block to the AURA or BABE block import by the engine that
/// authored it. Both must ultimately write to the same backend.
pub struct EngineDispatchBlockImport<Block, Aura, Babe> {
	aura: Aura,
	babe: Babe,
	_phantom: PhantomData<Block>,
}

impl<Block, Aura, Babe> EngineDispatchBlockImport<Block, Aura, Babe> {
	pub fn new(aura: Aura, babe: Babe) -> Self {
		Self { aura, babe, _phantom: PhantomData }
	}
}

#[async_trait]
impl<Block, Aura, Babe> BlockImport<Block> for EngineDispatchBlockImport<Block, Aura, Babe>
where
	Block: BlockT,
	Aura: BlockImport<Block, Error = ConsensusError> + Send + Sync,
	Babe: BlockImport<Block, Error = ConsensusError> + Send + Sync,
{
	type Error = ConsensusError;

	async fn check_block(
		&self,
		block: BlockCheckParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		// `BlockCheckParams` carries no header to route on. Neither engine's block import adds
		// checks of its own here — both forward to the same client-level preconditions beneath — so
		// the AURA side answers for both.
		self.aura.check_block(block).await
	}

	async fn import_block(
		&self,
		block: BlockImportParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		match route::<Block>(&block.header) {
			ActiveEngine::Aura => self.aura.import_block(block).await,
			ActiveEngine::Babe => self.babe.import_block(block).await,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use futures::executor::block_on;
	use midnight_node_runtime::VERSION_ID;
	use midnight_node_runtime::opaque::{Block, Header};
	use midnight_primitives_consensus_engine::ACTIVATION_SPEC_VERSION;
	use parity_scale_codec::Encode;
	use sp_consensus_aura::AURA_ENGINE_ID;
	use sp_consensus_babe::BABE_ENGINE_ID;
	use sp_core::H256;
	use sp_runtime::{ConsensusEngineId, DigestItem, traits::Header as HeaderT};
	use std::sync::Mutex;

	/// Records the numbers of the blocks handed to it, as a verifier or as a block import.
	#[derive(Clone, Default)]
	struct Recorder(Arc<Mutex<Vec<u32>>>);
	impl Recorder {
		fn seen(&self) -> Vec<u32> {
			self.0.lock().unwrap().clone()
		}
	}

	#[async_trait]
	impl Verifier<Block> for Recorder {
		async fn verify(
			&self,
			block: BlockImportParams<Block>,
		) -> Result<BlockImportParams<Block>, String> {
			self.0.lock().unwrap().push(*block.header.number());
			Ok(block)
		}
	}

	#[async_trait]
	impl BlockImport<Block> for Recorder {
		type Error = ConsensusError;
		async fn check_block(
			&self,
			block: BlockCheckParams<Block>,
		) -> Result<ImportResult, Self::Error> {
			self.0.lock().unwrap().push(block.number);
			Ok(ImportResult::imported(false))
		}
		async fn import_block(
			&self,
			block: BlockImportParams<Block>,
		) -> Result<ImportResult, Self::Error> {
			self.0.lock().unwrap().push(*block.header.number());
			Ok(ImportResult::imported(false))
		}
	}

	/// Records the parents it was asked to seed at (first byte of each hash).
	#[derive(Clone, Default)]
	struct SeedRecorder(Arc<Mutex<Vec<u8>>>);
	impl SeedRecorder {
		fn parents(&self) -> Vec<u8> {
			self.0.lock().unwrap().clone()
		}
	}
	impl EpochSeeder<Block> for SeedRecorder {
		fn ensure_seeded_for_child_of(&self, parent: H256) {
			self.0.lock().unwrap().push(parent.as_ref()[0]);
		}
	}

	/// Harness: a dispatching verifier and block import over recording AURA/BABE sides.
	struct Harness {
		verifier: EngineDispatchVerifier<Block, Recorder, Recorder>,
		block_import: EngineDispatchBlockImport<Block, Recorder, Recorder>,
		aura: Recorder,
		babe: Recorder,
		seeder: SeedRecorder,
	}

	impl Harness {
		fn new() -> Self {
			let (aura, babe, seeder) =
				(Recorder::default(), Recorder::default(), SeedRecorder::default());
			Self {
				verifier: EngineDispatchVerifier::new(
					Arc::new(seeder.clone()),
					Box::new(|hash| Ok(babe_parent_stub(hash))),
					aura.clone(),
					babe.clone(),
				),
				block_import: EngineDispatchBlockImport::new(aura.clone(), babe.clone()),
				aura,
				babe,
				seeder,
			}
		}

		fn verify(&self, header: Header) {
			block_on(self.verifier.verify(params(header))).unwrap();
		}

		fn import(&self, header: Header) {
			block_on(self.block_import.import_block(params(header))).unwrap();
		}
	}

	fn params(header: Header) -> BlockImportParams<Block> {
		BlockImportParams::new(BlockOrigin::NetworkInitialSync, header)
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

	/// The `pallet-version` consensus digest of a block executed by runtime `spec_version`.
	fn version_digest(spec_version: u32) -> DigestItem {
		DigestItem::Consensus(VERSION_ID, spec_version.encode())
	}

	/// A runtime from before `pallet-consensus-engine` existed.
	const PRE_ACTIVATION_SPEC_VERSION: u32 = ACTIVATION_SPEC_VERSION - 1;

	/// Header of block `number` whose parent hash starts with `number - 1`, carrying `logs`
	/// verbatim (no version digest unless `logs` has one).
	fn raw_header_with(number: u32, logs: Vec<DigestItem>) -> Header {
		let mut header = Header::new(
			number,
			Default::default(),
			Default::default(),
			hash_with_first_byte((number - 1) as u8),
			Default::default(),
		);
		for log in logs {
			header.digest_mut().push(log);
		}
		header
	}

	/// Header of a block executed by the runtime that introduced the pallet.
	fn header_with(number: u32, logs: Vec<DigestItem>) -> Header {
		let mut all = vec![version_digest(ACTIVATION_SPEC_VERSION)];
		all.extend(logs);
		raw_header_with(number, all)
	}

	/// Header of a block executed by a runtime from before the pallet existed.
	fn pre_activation_header_with(number: u32, logs: Vec<DigestItem>) -> Header {
		let mut all = vec![version_digest(PRE_ACTIVATION_SPEC_VERSION)];
		all.extend(logs);
		raw_header_with(number, all)
	}

	/// An AURA block from a pre-migration author: AURA pre-runtime digest only (plus the mc-hash
	/// one), AURA seal.
	fn aura_block(number: u32) -> Header {
		header_with(
			number,
			vec![
				pre_runtime(OTHER_ENGINE_ID),
				pre_runtime(AURA_ENGINE_ID),
				DigestItem::Seal(AURA_ENGINE_ID, vec![1]),
			],
		)
	}

	/// A pre-flip AURA block from a migration-aware author: AURA pre-runtime digest first, then the
	/// BABE `SecondaryPlain` one, in the order `pallet-consensus-engine` enforces. AURA seal.
	fn dual_digest_aura_block(number: u32) -> Header {
		header_with(
			number,
			vec![
				pre_runtime(OTHER_ENGINE_ID),
				pre_runtime(AURA_ENGINE_ID),
				pre_runtime(BABE_ENGINE_ID),
				DigestItem::Seal(AURA_ENGINE_ID, vec![1]),
			],
		)
	}

	/// A post-flip BABE block: BABE pre-runtime digest only, BABE seal.
	fn babe_block(number: u32) -> Header {
		header_with(
			number,
			vec![
				pre_runtime(OTHER_ENGINE_ID),
				pre_runtime(BABE_ENGINE_ID),
				DigestItem::Seal(BABE_ENGINE_ID, vec![2]),
			],
		)
	}

	/// A block with no AURA/BABE pre-runtime digest at all; it can't be routed.
	fn engineless_block(number: u32) -> Header {
		header_with(number, vec![pre_runtime(OTHER_ENGINE_ID)])
	}

	#[test]
	fn verifier_routes_a_pre_activation_block_with_a_leading_babe_digest_to_aura() {
		let h = Harness::new();
		h.verify(pre_activation_header_with(
			5,
			vec![
				pre_runtime(BABE_ENGINE_ID),
				pre_runtime(AURA_ENGINE_ID),
				DigestItem::Seal(AURA_ENGINE_ID, vec![1]),
			],
		));
		assert_eq!(h.aura.seen(), vec![5]);
		assert!(h.babe.seen().is_empty());
		assert!(h.seeder.parents().is_empty());
	}

	#[test]
	fn verifier_routes_by_authoring_engine() {
		let h = Harness::new();

		h.verify(dual_digest_aura_block(10));
		h.verify(babe_block(20));
		h.verify(aura_block(30));

		assert_eq!(h.aura.seen(), vec![10, 30]);
		assert_eq!(h.babe.seen(), vec![20]);
	}

	#[test]
	fn verifier_routes_block_without_engine_digest_to_aura() {
		let h = Harness::new();
		h.verify(engineless_block(43));
		assert_eq!(h.aura.seen(), vec![43]);
		assert!(h.babe.seen().is_empty());
	}

	#[test]
	fn verifier_seeds_at_the_parent_before_every_babe_verification_and_never_for_aura() {
		let h = Harness::new();

		// The flip-boundary sequence: the last AURA blocks, then the first BABE blocks.
		h.verify(dual_digest_aura_block(96));
		h.verify(dual_digest_aura_block(97));
		assert!(h.seeder.parents().is_empty());

		h.verify(babe_block(98));
		assert_eq!(h.seeder.parents(), vec![97], "seeded at the flip block before its child");
		h.verify(babe_block(99));
		assert_eq!(h.seeder.parents(), vec![97, 98]);
		assert_eq!(h.babe.seen(), vec![98, 99]);
	}

	#[test]
	fn verifier_surfaces_the_inner_error() {
		struct Failing;
		#[async_trait]
		impl Verifier<Block> for Failing {
			async fn verify(
				&self,
				_: BlockImportParams<Block>,
			) -> Result<BlockImportParams<Block>, String> {
				Err("bad seal".into())
			}
		}
		let verifier = EngineDispatchVerifier::<Block, _, _>::new(
			Arc::new(SeedRecorder::default()),
			Box::new(|hash| Ok(babe_parent_stub(hash))),
			Recorder::default(),
			Failing,
		);
		assert_eq!(
			block_on(verifier.verify(params(babe_block(5)))).err(),
			Some("bad seal".to_string())
		);
	}

	/// A decodable BABE `SecondaryPlain` pre-runtime digest for `slot`.
	fn babe_slot(slot: u64) -> DigestItem {
		use sp_consensus_babe::digests::{PreDigest, SecondaryPlainPreDigest};
		DigestItem::PreRuntime(
			BABE_ENGINE_ID,
			PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
				authority_index: 0,
				slot: slot.into(),
			})
			.encode(),
		)
	}

	/// A parent that passes the BABE parent check for the harness headers, whose parent hash
	/// starts with the parent's number.
	fn babe_parent_stub(hash: H256) -> Header {
		let number = hash.as_ref()[0] as u32;
		let mut header = Header::new(
			number,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		);
		header.digest_mut().push(version_digest(ACTIVATION_SPEC_VERSION));
		header.digest_mut().push(babe_slot(1));
		header
	}

	/// Verifier whose parent lookup knows exactly `parent`.
	fn verifier_knowing(
		parent: Header,
	) -> (EngineDispatchVerifier<Block, Recorder, Recorder>, Recorder, SeedRecorder) {
		let (babe, seeder) = (Recorder::default(), SeedRecorder::default());
		let parent_hash = parent.hash();
		let verifier = EngineDispatchVerifier::new(
			Arc::new(seeder.clone()),
			Box::new(move |hash| {
				(hash == parent_hash).then(|| parent.clone()).ok_or("not imported".to_string())
			}),
			Recorder::default(),
			babe.clone(),
		);
		(verifier, babe, seeder)
	}

	/// A BABE block claiming `number` on top of `parent`.
	fn babe_child_of(parent: &Header, number: u32) -> Header {
		let mut header = Header::new(
			number,
			Default::default(),
			Default::default(),
			parent.hash(),
			Default::default(),
		);
		header.digest_mut().push(version_digest(ACTIVATION_SPEC_VERSION));
		header.digest_mut().push(babe_slot(100));
		header
	}

	#[test]
	fn verifier_accepts_a_babe_block_on_a_flip_block_or_genesis_parent() {
		let flip_block = header_with(97, vec![pre_runtime(AURA_ENGINE_ID), babe_slot(99)]);
		// A chain that is BABE from its first block: genesis carries no digest at all.
		let genesis = Header::new(
			0,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		);
		for parent in [flip_block, genesis] {
			let (verifier, babe, seeder) = verifier_knowing(parent.clone());
			let child = babe_child_of(&parent, parent.number + 1);
			block_on(verifier.verify(params(child))).unwrap();
			assert_eq!(babe.seen(), vec![parent.number + 1]);
			assert_eq!(seeder.parents().len(), 1);
		}
	}

	#[test]
	fn verifier_rejects_a_babe_block_that_does_not_fit_its_parent() {
		let good_parent = header_with(97, vec![pre_runtime(AURA_ENGINE_ID), babe_slot(99)]);
		let bad_parents = [
			// Pallet-era block from a pre-migration author: AURA digest only.
			header_with(97, vec![pre_runtime(AURA_ENGINE_ID)]),
			// Pre-pallet block: its BABE digest, if any, is not authoritative.
			pre_activation_header_with(97, vec![pre_runtime(AURA_ENGINE_ID), babe_slot(99)]),
			// Pallet-era block whose BABE item does not decode.
			header_with(97, vec![pre_runtime(AURA_ENGINE_ID), pre_runtime(BABE_ENGINE_ID)]),
			// Two decodable BABE items.
			header_with(97, vec![pre_runtime(AURA_ENGINE_ID), babe_slot(99), babe_slot(99)]),
		];
		let cases = bad_parents
			.into_iter()
			.map(|parent| {
				(parent.clone(), babe_child_of(&parent, 98), "no readable BABE pre-runtime digest")
			})
			.chain([
				(good_parent.clone(), babe_child_of(&good_parent, 1), "claims number"),
				(good_parent, babe_block(50), "not imported"),
			]);
		for (parent, child, expected) in cases {
			let (verifier, babe, seeder) = verifier_knowing(parent);
			let err = block_on(verifier.verify(params(child))).err().unwrap();
			assert!(err.contains(expected), "{err}");
			assert!(babe.seen().is_empty());
			assert!(seeder.parents().is_empty());
		}
	}

	#[test]
	fn verifier_skips_the_parent_check_for_a_block_imported_with_its_state() {
		let known = header_with(97, vec![pre_runtime(AURA_ENGINE_ID), babe_slot(99)]);
		let (verifier, babe, _) = verifier_knowing(known);
		let mut block = params(babe_block(50));
		block.state_action = sc_consensus::StateAction::ApplyChanges(
			sc_consensus::StorageChanges::Import(sc_consensus::ImportedState {
				block: block.header.hash(),
				state: sp_state_machine::KeyValueStates(vec![]),
			}),
		);
		block_on(verifier.verify(block)).unwrap();
		assert_eq!(babe.seen(), vec![50]);
	}

	/// Block import that records the auxiliary entries of the blocks handed to it.
	#[derive(Clone, Default)]
	struct AuxRecorder(Arc<Mutex<AuxEntries>>);
	type AuxEntries = Vec<(Vec<u8>, Option<Vec<u8>>)>;

	#[async_trait]
	impl BlockImport<Block> for AuxRecorder {
		type Error = ConsensusError;
		async fn check_block(
			&self,
			_: BlockCheckParams<Block>,
		) -> Result<ImportResult, Self::Error> {
			Ok(ImportResult::imported(false))
		}
		async fn import_block(
			&self,
			block: BlockImportParams<Block>,
		) -> Result<ImportResult, Self::Error> {
			self.0.lock().unwrap().extend(block.auxiliary);
			Ok(ImportResult::imported(false))
		}
	}

	#[test]
	fn aura_weight_import_records_zero_weight_only_for_pallet_era_blocks_outside_the_gap() {
		let pallet_era = dual_digest_aura_block(11);
		let weight =
			vec![(block_weight_key(pallet_era.hash()), Some((0 as BabeBlockWeight).encode()))];
		let warp_header = BlockImportParams::new(BlockOrigin::WarpSync, dual_digest_aura_block(11));
		let pre_activation = pre_activation_header_with(
			11,
			vec![pre_runtime(AURA_ENGINE_ID), pre_runtime(BABE_ENGINE_ID)],
		);
		for (block, gap_end, expected) in [
			(params(pallet_era), 10, weight),
			(params(dual_digest_aura_block(10)), 10, vec![]),
			(warp_header, 0, vec![]),
			(params(aura_block(11)), 0, vec![]),
			(params(pre_activation), 0, vec![]),
		] {
			let inner = AuxRecorder::default();
			let import = AuraWeightBlockImport::new(inner.clone(), Arc::new(move |n| n <= gap_end));
			block_on(import.import_block(block)).unwrap();
			assert_eq!(*inner.0.lock().unwrap(), expected);
		}
	}

	#[test]
	fn block_import_routes_by_authoring_engine() {
		let h = Harness::new();

		h.import(dual_digest_aura_block(10));
		h.import(babe_block(20));
		h.import(engineless_block(30));

		assert_eq!(h.aura.seen(), vec![10, 30]);
		assert_eq!(h.babe.seen(), vec![20]);
	}

	#[test]
	fn block_import_checks_preconditions_on_the_aura_side() {
		let h = Harness::new();
		let check = BlockCheckParams {
			hash: hash_with_first_byte(7),
			number: 7,
			parent_hash: hash_with_first_byte(6),
			allow_missing_state: false,
			allow_missing_parent: false,
			import_existing: false,
		};
		block_on(h.block_import.check_block(check)).unwrap();
		assert_eq!(h.aura.seen(), vec![7]);
		assert!(h.babe.seen().is_empty());
	}
}

/// [`AuraWeightBlockImport`] against the real BABE block import, on the substrate test runtime.
#[cfg(test)]
mod babe_fork_choice_tests {
	use super::*;
	use sc_block_builder::BlockBuilderBuilder;
	use sc_consensus::{BoxBlockImport, ForkChoiceStrategy};
	use sc_consensus_babe::{BabeIntermediate, INTERMEDIATE_KEY};
	use sc_consensus_epochs::descendent_query;
	use sc_transaction_pool_api::{OffchainTransactionPoolFactory, RejectAllTxPool};
	use sp_consensus_babe::{
		digests::{PreDigest, PrimaryPreDigest, SecondaryPlainPreDigest},
		inherents::InherentDataProvider,
		make_vrf_sign_data,
	};
	use sp_consensus_slots::Slot;
	use sp_core::{H256, crypto::VrfSecret};
	use sp_keyring::Sr25519Keyring;
	use sp_runtime::{Digest, DigestItem};
	use substrate_test_runtime_client::{
		DefaultTestClientBuilderExt, TestClient, TestClientBuilder, TestClientBuilderExt,
		runtime::Block as TestBlock,
	};

	struct Chain {
		client: Arc<TestClient>,
		babe: BoxBlockImport<TestBlock>,
		link: sc_consensus_babe::BabeLink<TestBlock>,
	}

	fn chain() -> Chain {
		let (client, longest_chain) = TestClientBuilder::new().build_with_longest_chain();
		let client = Arc::new(client);
		let config = sc_consensus_babe::configuration(&*client).unwrap();
		let (babe, link) = sc_consensus_babe::block_import(
			config,
			client.clone(),
			client.clone(),
			|_, _| async { Ok((InherentDataProvider::new(Slot::from(100)),)) },
			longest_chain,
			OffchainTransactionPoolFactory::new(RejectAllTxPool::default()),
		)
		.unwrap();
		Chain { client, babe: Box::new(babe), link }
	}

	fn pre_digest(pre_digest: PreDigest) -> Digest {
		Digest {
			logs: vec![<DigestItem as sc_consensus_babe::CompatibleDigestItem>::babe_pre_digest(
				pre_digest,
			)],
		}
	}

	fn build(chain: &Chain, parent: H256, digest: Digest) -> BlockImportParams<TestBlock> {
		let parent_number = chain.client.number(parent).unwrap().unwrap();
		let block = BlockBuilderBuilder::new(&*chain.client)
			.on_parent_block(parent)
			.with_parent_block_number(parent_number)
			.with_inherent_digests(digest)
			.build()
			.unwrap()
			.build()
			.unwrap()
			.block;
		let mut params = BlockImportParams::new(BlockOrigin::NetworkBroadcast, block.header);
		params.body = Some(block.extrinsics);
		params
	}

	/// An AURA block of the pallet's era: carries a BABE pre-runtime digest, imported with AURA's
	/// longest-chain rule.
	fn aura_block(chain: &Chain, parent: H256, slot: u64) -> BlockImportParams<TestBlock> {
		let digest = pre_digest(PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
			authority_index: 0,
			slot: slot.into(),
		}));
		let mut params = build(chain, parent, digest);
		params.fork_choice = Some(ForkChoiceStrategy::LongestChain);
		params
	}

	/// A primary BABE block (chain weight 1) on genesis, prepared as the BABE verifier would.
	fn babe_block_on_genesis(chain: &Chain, slot: u64) -> BlockImportParams<TestBlock> {
		let genesis = chain.client.info().genesis_hash;
		let randomness = chain.link.config().randomness;
		let vrf_signature =
			Sr25519Keyring::Alice
				.pair()
				.vrf_sign(&make_vrf_sign_data(&randomness, slot.into(), 0));
		let digest = pre_digest(PreDigest::Primary(PrimaryPreDigest {
			authority_index: 0,
			slot: slot.into(),
			vrf_signature,
		}));
		let mut params = build(chain, genesis, digest);
		let epoch_descriptor = chain
			.link
			.epoch_changes()
			.shared_data()
			.epoch_descriptor_for_child_of(
				descendent_query(&*chain.client),
				&genesis,
				0,
				slot.into(),
			)
			.unwrap()
			.unwrap();
		params.insert_intermediate(
			INTERMEDIATE_KEY,
			BabeIntermediate::<TestBlock> { epoch_descriptor },
		);
		params
	}

	fn import_aura(chain: &Chain, params: BlockImportParams<TestBlock>) -> H256 {
		let hash = params.post_hash();
		let aura = AuraWeightBlockImport::new(chain.client.clone(), Arc::new(|_| false));
		futures::executor::block_on(aura.import_block(params)).unwrap();
		hash
	}

	#[test]
	fn babe_block_outweighs_a_longer_aura_best_block() {
		let chain = chain();
		let genesis = chain.client.info().genesis_hash;
		let a1 = import_aura(&chain, aura_block(&chain, genesis, 1));
		let a2 = import_aura(&chain, aura_block(&chain, a1, 2));
		assert_eq!(chain.client.info().best_hash, a2);

		let b1 = babe_block_on_genesis(&chain, 3);
		let b1_hash = b1.post_hash();
		futures::executor::block_on(chain.babe.import_block(b1)).unwrap();
		assert_eq!(chain.client.info().best_hash, b1_hash, "weight 1 beats weight 0 at #2");

		// AURA blocks keep the longest-chain rule: a longer AURA branch takes the best block back.
		let a3 = import_aura(&chain, aura_block(&chain, a2, 4));
		assert_eq!(chain.client.info().best_hash, a3);
	}

	#[test]
	fn babe_import_fails_against_an_aura_best_block_without_a_weight() {
		let chain = chain();
		let genesis = chain.client.info().genesis_hash;
		let a1 = aura_block(&chain, genesis, 1);
		let a1_hash = a1.post_hash();
		futures::executor::block_on(chain.client.clone().import_block(a1)).unwrap();
		assert_eq!(chain.client.info().best_hash, a1_hash);

		let error =
			futures::executor::block_on(chain.babe.import_block(babe_block_on_genesis(&chain, 3)))
				.unwrap_err();
		assert!(error.to_string().contains("No block weight"), "{error}");
	}
}
