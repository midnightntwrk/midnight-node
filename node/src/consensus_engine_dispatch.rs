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
//! # Routing key: the first AURA/BABE pre-runtime digest
//!
//! The key must come from the block being routed, not from chain state. Reading the engine from the
//! parent's runtime state (`ConsensusEngineApi::active_engine`) needs the parent to be *imported*,
//! and at the flip boundary it is not: a sync batch `[…, flip, flip+1, …]` carries the first BABE
//! block together with its parent, so the parent's state does not exist when the batch is queued.
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
//! Justifications are finality (GRANDPA) and engine-agnostic; the GRANDPA justification import is
//! registered on the queue directly and is not part of this dispatch.

use async_trait::async_trait;
use midnight_primitives_consensus_engine::ActiveEngine;
use sc_consensus::{BlockCheckParams, BlockImport, BlockImportParams, ImportResult, Verifier};
use sp_consensus::Error as ConsensusError;
use sp_consensus_aura::AURA_ENGINE_ID;
use sp_consensus_babe::BABE_ENGINE_ID;
use sp_runtime::traits::{Block as BlockT, Header as _};
use std::{marker::PhantomData, sync::Arc};

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

/// The pipeline a block is routed to: the engine that authored it. A header without an AURA/BABE
/// pre-runtime digest can't be routed and defaults to AURA, whose verifier produces the clearer
/// error for it.
fn route<Block: BlockT>(header: &Block::Header) -> ActiveEngine {
	engine_from_pre_runtime_digest::<Block>(header).unwrap_or(ActiveEngine::Aura)
}

/// Makes BABE's epoch tree able to resolve epochs for the children of a given parent block.
///
/// Called by [`EngineDispatchVerifier`] right before a block is handed to the BABE verifier (see the
/// module docs). Implementations must be idempotent, cheap when the tree already covers `parent`,
/// and must refuse to seed at a block whose state has not flipped to BABE — the parent is taken from
/// a peer-supplied header, so this is what stops a peer from resetting the tree at an arbitrary
/// block.
pub trait EpochSeeder<Block: BlockT>: Send + Sync {
	/// Best-effort: `parent` may not be imported (the BABE block import then rejects the child with
	/// `UnknownParent` and sync re-offers it later).
	fn ensure_seeded_for_child_of(&self, parent: Block::Hash);
}

/// [`Verifier`] that hands each block to the AURA or BABE verifier by the engine that authored it,
/// seeding BABE's epoch tree at the block's parent before a BABE verification.
pub struct EngineDispatchVerifier<Block: BlockT, Aura, Babe> {
	aura: Aura,
	babe: Babe,
	seeder: Arc<dyn EpochSeeder<Block>>,
}

impl<Block: BlockT, Aura, Babe> EngineDispatchVerifier<Block, Aura, Babe> {
	pub fn new(seeder: Arc<dyn EpochSeeder<Block>>, aura: Aura, babe: Babe) -> Self {
		Self { aura, babe, seeder }
	}
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
				self.seeder.ensure_seeded_for_child_of(*block.header.parent_hash());
				self.babe.verify(block).await
			},
		}
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
	use midnight_node_runtime::opaque::{Block, Header};
	use sp_consensus::BlockOrigin;
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

	/// Header of block `number` whose parent hash starts with `number - 1`.
	fn header_with(number: u32, logs: Vec<DigestItem>) -> Header {
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

	/// A pre-arming AURA block: AURA pre-runtime digest only (plus the mc-hash one), AURA seal.
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

	/// An armed-phase AURA block: AURA pre-runtime digest first, then the BABE `SecondaryPlain`
	/// one, in the order `pallet-consensus-engine` enforces. AURA seal.
	fn armed_aura_block(number: u32) -> Header {
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
	fn engine_is_read_from_the_first_aura_or_babe_pre_runtime_digest() {
		let engine = engine_from_pre_runtime_digest::<Block>;
		assert_eq!(engine(&aura_block(1)), Some(ActiveEngine::Aura));
		assert_eq!(engine(&babe_block(1)), Some(ActiveEngine::Babe));
	}

	#[test]
	fn armed_aura_block_with_both_pre_digests_is_aura() {
		// From arming to the flip every AURA block also carries a BABE pre-runtime digest; the
		// pallet guarantees the AURA one comes first, and that order is what decides.
		assert_eq!(
			engine_from_pre_runtime_digest::<Block>(&armed_aura_block(1)),
			Some(ActiveEngine::Aura)
		);
	}

	#[test]
	fn other_engines_pre_runtime_digests_are_skipped() {
		let header =
			header_with(1, vec![pre_runtime(OTHER_ENGINE_ID), pre_runtime(BABE_ENGINE_ID)]);
		assert_eq!(engine_from_pre_runtime_digest::<Block>(&header), Some(ActiveEngine::Babe));
	}

	#[test]
	fn header_without_an_aura_or_babe_pre_runtime_digest_has_no_engine() {
		let header = header_with(
			1,
			vec![pre_runtime(OTHER_ENGINE_ID), DigestItem::Seal(BABE_ENGINE_ID, vec![])],
		);
		assert_eq!(engine_from_pre_runtime_digest::<Block>(&header), None);
	}

	#[test]
	fn verifier_routes_by_authoring_engine() {
		let h = Harness::new();

		h.verify(armed_aura_block(10));
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
		h.verify(armed_aura_block(96));
		h.verify(armed_aura_block(97));
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
			Recorder::default(),
			Failing,
		);
		assert_eq!(
			block_on(verifier.verify(params(babe_block(5)))).err(),
			Some("bad seal".to_string())
		);
	}

	#[test]
	fn block_import_routes_by_authoring_engine() {
		let h = Harness::new();

		h.import(armed_aura_block(10));
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
