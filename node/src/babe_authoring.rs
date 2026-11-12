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

//! Block authoring across the AURA→BABE consensus migration.
//!
//! Two slot workers, of which at most one may author on a given best block. Both run for the
//! node's life, each behind an [`EngineGatedSyncOracle`] that makes it skip a slot unless the state
//! of the best block selects its engine. The choice follows the best block in both directions, so
//! authoring returns to AURA if the best chain moves to a branch that has not flipped.
//!
//! Each worker reads the best block on its own: the gate from the client's chain info, the worker
//! from its `SelectChain`. Both are the same while the node uses `LongestChain`. A block that
//! changes the engine, imported between those reads, costs a skipped slot or an ordinary one-block
//! fork.
//!
//! Seeding the epoch tree has two triggers, one implementation ([`seed_epoch_tree_if_needed`]):
//!
//! - **About to author a BABE block** (authorities only): the BABE worker's gate seeds at the
//!   best block before it lets a slot through. The validator that authors the very first BABE
//!   block has imported no BABE block yet and its own blocks bypass the import queue, so nothing
//!   else would seed before it proposes. A failed attempt is repeated at the next slot.
//! - **About to verify a BABE block** (every role): [`BabeEpochSeeder`], called by the import
//!   queue's dispatching verifier right before it hands a BABE block to the BABE verifier. It is
//!   what lets non-authorities import the first BABE block; they run no authoring gate.
//!
//! Seeding is idempotent and its check-and-import is atomic under the epoch-tree lock, so both
//! triggers may run concurrently on an authority.
//!
//! [`configuration_at_startup`] is the other start-of-day seam: `BabeBlockImport` is built in
//! `new_partial` even while the chain is still on AURA, but `sc_consensus_babe::configuration`
//! requires on-chain `BabeApi`. A node binary rolled out *before* the runtime upgrade that adds
//! `pallet-babe` would otherwise refuse to start. When the WASM at best/genesis has no `BabeApi`,
//! the helper synthesizes a placeholder from the AURA slot duration and sidechain epoch length
//! so the idle BABE pipeline can still be constructed. Real epoch descriptors are seeded from
//! `BabeApi` at the flip, not from that placeholder.

use crate::consensus_engine_dispatch::EpochSeeder;
use crate::engine_digests::{authoring_engine, slot_of};
use midnight_node_runtime::opaque::Block;
use midnight_primitives_consensus_engine::{ActiveEngine, ConsensusEngineApi};
use parity_scale_codec::Encode;
use sc_client_api::{AuxStore, UsageProvider};
use sc_consensus_babe::{
	BabeBlockWeight, BabeLink,
	aux_schema::{block_weight_key, load_block_weight},
};
use sc_consensus_epochs::{EpochChanges, IsDescendentOfBuilder, ViableEpoch, descendent_query};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::{HeaderBackend, HeaderMetadata};
use sp_consensus::SyncOracle;
use sp_consensus_babe::{BabeApi, BabeConfiguration, digests::NextEpochDescriptor};
use sp_consensus_slots::{Slot, SlotDuration};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, NumberFor};
use std::future::Future;
use std::sync::{
	Arc,
	atomic::{AtomicBool, Ordering},
};

const LOG_TARGET: &str = "babe-authoring";

type Hash = <Block as BlockT>::Hash;

/// Trait bundle for a client the authoring gates can query and write aux data on.
pub trait AuthoringClient:
	ProvideRuntimeApi<Block>
	+ HeaderBackend<Block>
	+ HeaderMetadata<Block, Error = sp_blockchain::Error>
	+ AuxStore
	+ Send
	+ Sync
	+ 'static
{
}

impl<C> AuthoringClient for C where
	C: ProvideRuntimeApi<Block>
		+ HeaderBackend<Block>
		+ HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ AuxStore
		+ Send
		+ Sync
		+ 'static
{
}

/// BABE configuration used to construct `BabeBlockImport` at node start.
///
/// Prefers the on-chain [`BabeApi`] at best (or genesis, when there is no finalized state),
/// matching [`sc_consensus_babe::configuration`]. If that WASM does not implement `BabeApi`
/// yet — an AURA-only runtime, before the upgrade that introduces `pallet-babe` — returns a
/// placeholder built from the AURA slot duration and the sidechain epoch length instead of
/// failing start with `VersionInvalid("Unsupported or invalid BabeApi version")`.
///
/// The placeholder copies [`midnight_node_runtime::BABE_GENESIS_EPOCH_CONFIG`] so `c` and
/// `allowed_slots` match what `BabeApi::configuration` will report after the upgrade (the
/// pallet's `EpochConfig` is unset until the flip). Epoch descriptors used to verify/author
/// BABE blocks are seeded from `BabeApi` at the flip, not from this value.
pub fn configuration_at_startup<C>(
	client: &C,
	aura_slot_duration: SlotDuration,
	slots_per_epoch: u32,
) -> sp_blockchain::Result<BabeConfiguration>
where
	C: AuxStore + ProvideRuntimeApi<Block> + UsageProvider<Block>,
	C::Api: BabeApi<Block>,
{
	let at_hash = if client.usage_info().chain.finalized_state.is_some() {
		client.usage_info().chain.best_hash
	} else {
		client.usage_info().chain.genesis_hash
	};

	match client.runtime_api().api_version::<dyn BabeApi<Block>>(at_hash)? {
		None => {
			log::info!(
				target: LOG_TARGET,
				"BabeApi is not implemented at {at_hash:?}; constructing the BABE import pipeline \
				 from AURA slot duration and sidechain epoch length. BABE stays idle until the \
				 runtime upgrade exposes BabeApi.",
			);
			Ok(placeholder_configuration(aura_slot_duration, slots_per_epoch))
		},
		Some(_) => sc_consensus_babe::configuration(client),
	}
}

pub(crate) fn placeholder_configuration(
	aura_slot_duration: SlotDuration,
	slots_per_epoch: u32,
) -> BabeConfiguration {
	let epoch_config = midnight_node_runtime::BABE_GENESIS_EPOCH_CONFIG;
	BabeConfiguration {
		slot_duration: aura_slot_duration.as_millis(),
		epoch_length: u64::from(slots_per_epoch),
		c: epoch_config.c,
		authorities: Vec::new(),
		randomness: [0u8; 32],
		allowed_slots: epoch_config.allowed_slots,
	}
}

/// The engine active in the state of `hash`, defaulting to AURA when the query fails (the safe
/// pre-flip default, matching the import-queue dispatch).
pub(crate) fn active_engine_at<C>(client: &C, hash: Hash) -> ActiveEngine
where
	C: ProvideRuntimeApi<Block>,
	C::Api: ConsensusEngineApi<Block>,
{
	match client.runtime_api().active_engine(hash) {
		Ok(engine) => engine,
		Err(err) => {
			log::debug!(target: LOG_TARGET, "active_engine query at {hash:?} failed: {err}; assuming AURA");
			ActiveEngine::Aura
		},
	}
}

/// Whether `epoch_changes` can resolve an epoch for the children of the flip block `at`.
///
/// The children are queried at `flip_slot + 1`, the first BABE slot, **not** at the flip block's own
/// slot: the flip happens at the last AURA slot of an epoch and BABE's genesis slot is the one
/// after, so every epoch in a seeded tree starts *above* `flip_slot`. Querying at `flip_slot` finds
/// no epoch and reports an already-seeded tree as uncovered, which would make every subsequent call
/// seed again.
fn tree_covers_children_of<D>(
	epoch_changes: &EpochChanges<Hash, NumberFor<Block>, sc_consensus_babe::Epoch>,
	descendent_of_builder: D,
	at: &Hash,
	number: NumberFor<Block>,
	flip_slot: Slot,
) -> Result<bool, String>
where
	D: IsDescendentOfBuilder<Hash>,
{
	let first_babe_slot = flip_slot + 1;
	epoch_changes
		.epoch_descriptor_for_child_of(descendent_of_builder, at, number, first_babe_slot)
		.map(|descriptor| descriptor.is_some())
		.map_err(|e| e.to_string())
}

/// Add the flip block `at`'s epoch pair to the tree, as the pair a genesis block announces: its
/// children resolve to `current`, and to `next` from `next`'s start slot.
///
/// The pair is built through the tree's own increment of `current`, the shape every later epoch
/// change extends, and checked against the `next` the runtime reports so a disagreement between
/// the two is an error rather than a silently diverging epoch 1.
fn seed_tree_at_flip_block<D>(
	epoch_changes: &mut EpochChanges<Hash, NumberFor<Block>, sc_consensus_babe::Epoch>,
	descendent_of_builder: D,
	parent_hash: Hash,
	at: Hash,
	number: NumberFor<Block>,
	current: sc_consensus_babe::Epoch,
	next: &sc_consensus_babe::Epoch,
) -> Result<(), String>
where
	D: IsDescendentOfBuilder<Hash>,
{
	let descriptor =
		NextEpochDescriptor { authorities: next.authorities.clone(), randomness: next.randomness };
	let pair = ViableEpoch::<sc_consensus_babe::Epoch>::UnimportedGenesis(current)
		.increment((descriptor, next.config.clone()));
	if pair.as_ref() != next {
		return Err(format!(
			"the next epoch the runtime reports at {at:?} does not follow its current epoch",
		));
	}
	epoch_changes
		.import(descendent_of_builder, at, number, parent_hash, pair)
		.map_err(|e| e.to_string())
}

/// Seed BABE's epoch tree so authoring/verification can resolve epochs for children of `at`.
///
/// Before the flip nothing is imported through the BABE pipeline, so its `EpochChanges` is empty
/// and the first BABE block (a child of the flip block) has no epoch to be authored/verified under.
/// It records the `current`/`next` epochs the runtime reports at `at` under `at` itself, as the
/// pair a genesis block announces. `migrate_to_babe` makes those runtime APIs return the epoch-0
/// genesis, and the flip block carries a BABE pre-digest (from the BABE pre-digest proposer
/// wrapper) so its slot is readable.
///
/// It is a no-op when the tree already covers children of `at` — the other trigger got there first,
/// or a restart after the flip reloaded a populated tree from the aux DB. The pair is added to the
/// tree, never reset into it: a fork across the flip has a flip block per branch, and a node that
/// seeded at the losing one must still verify the winning one, so each flip block keeps its own
/// pair and finality prunes the loser's.
pub fn seed_epoch_tree_if_needed<C>(
	client: &Arc<C>,
	babe_link: &BabeLink<Block>,
	at: Hash,
) -> Result<(), String>
where
	C: AuthoringClient,
	C::Api: BabeApi<Block>,
{
	let header = client
		.header(at)
		.map_err(|e| e.to_string())?
		.ok_or_else(|| format!("header for flip block {at:?} not found"))?;
	let number = *header.number();
	let parent_hash = *header.parent_hash();

	// The flip block is the last AURA block; its slot is the AURA one, read by the shared rule.
	let slot = slot_of::<Block>(&header).map_err(|e| format!("flip block {at:?}: {e}"))?;

	// Hold the tree lock from the coverage check through the weight write. Seeding has two triggers
	// (the BABE authoring gate and the import-path seeder) that can run concurrently around the
	// flip; under one lock the second caller sees the first one's complete result.
	let mut epoch_changes = babe_link.epoch_changes().shared_data();
	let covered =
		tree_covers_children_of(&epoch_changes, descendent_query(&**client), &at, number, slot)
			.map_err(|e| format!("epoch-tree lookup for children of {at:?} failed: {e}"))?;
	if covered {
		log::debug!(target: LOG_TARGET, "BABE epoch tree already covers {at:?}; not seeding");
	} else {
		let current = client.runtime_api().current_epoch(at).map_err(|e| e.to_string())?;
		let next = client.runtime_api().next_epoch(at).map_err(|e| e.to_string())?;
		let (current_index, next_index) = (current.epoch_index, next.epoch_index);
		seed_tree_at_flip_block(
			&mut epoch_changes,
			descendent_query(&**client),
			parent_hash,
			at,
			number,
			current.into(),
			&next.into(),
		)?;
		log::info!(
			target: LOG_TARGET,
			"seeded BABE epoch tree at flip block #{number} ({at:?}): epochs {current_index} and {next_index}",
		);
	}
	ensure_zero_weight_at(&**client, at)
}

/// Give the flip block `at` a cumulative BABE chain weight of 0 unless it has one, which its first
/// BABE child needs as its parent weight. `AuraWeightBlockImport` records it at import; this covers
/// a flip block imported without it (by an earlier binary, or inside the block gap). It mirrors the
/// warp-sync bootstrap (`import_state`), which likewise sets the weight to 0 at the sync point.
fn ensure_zero_weight_at<C: AuxStore>(client: &C, at: Hash) -> Result<(), String> {
	if load_block_weight(client, at).map_err(|e| e.to_string())?.is_some() {
		return Ok(());
	}
	let weight_key = block_weight_key(at);
	let weight_value = (0 as BabeBlockWeight).encode();
	let no_delete: &[&[u8]] = &[];
	client
		.insert_aux(&[(weight_key.as_slice(), weight_value.as_slice())], no_delete)
		.map_err(|e| e.to_string())?;
	log::debug!(target: LOG_TARGET, "recorded zero BABE chain weight at flip block {at:?}");
	Ok(())
}

/// [`EpochSeeder`] for the import queue's dispatching verifier: seeds BABE's epoch tree at `parent`
/// when the first BABE block is about to be verified and `parent` is the flip block.
///
/// Called for every BABE block, so the common case must be cheap: a BABE-authored parent had its
/// epoch recorded by the BABE import that brought it in and is skipped on its header alone. For any
/// other parent it seeds only when the runtime state at `parent` has flipped to BABE. The parent
/// hash comes from a peer-supplied header, so without that check a peer could make us add an epoch
/// pair at an arbitrary imported block; with it, the only block that is both post-flip *and* not
/// yet covered by the tree is the flip block itself.
pub struct BabeEpochSeeder<C> {
	client: Arc<C>,
	babe_link: BabeLink<Block>,
}

impl<C> BabeEpochSeeder<C> {
	pub fn new(client: Arc<C>, babe_link: BabeLink<Block>) -> Self {
		Self { client, babe_link }
	}
}

impl<C> EpochSeeder<Block> for BabeEpochSeeder<C>
where
	C: AuthoringClient,
	C::Api: BabeApi<Block> + ConsensusEngineApi<Block>,
{
	fn ensure_seeded_for_child_of(&self, parent: Hash) {
		if let Err(err) = self.seed_for_child_of(parent) {
			log::error!(
				target: LOG_TARGET,
				"failed to seed BABE epoch tree at {parent:?} on the import path: {err}; BABE import may stall",
			);
		}
	}
}

impl<C> BabeEpochSeeder<C>
where
	C: AuthoringClient,
	C::Api: BabeApi<Block> + ConsensusEngineApi<Block>,
{
	/// Seed at `parent` if it is the flip block; `Ok` when a BABE child of `parent` can resolve its
	/// epoch or `parent` is not a block to seed at.
	fn seed_for_child_of(&self, parent: Hash) -> Result<(), String> {
		// Not imported: nothing to seed at. Only a block imported with its state gets here.
		let Ok(Some(parent_header)) = self.client.header(parent) else {
			log::debug!(target: LOG_TARGET, "parent {parent:?} of a BABE block is not imported; not seeding");
			return Ok(());
		};
		// A BABE-authored parent is already covered by the tree (see the type docs); only the flip
		// block — an AURA-authored parent of a BABE block — can need seeding.
		if authoring_engine::<Block>(&parent_header) == Some(ActiveEngine::Babe) {
			return Ok(());
		}
		if active_engine_at(&*self.client, parent) != ActiveEngine::Babe {
			log::debug!(target: LOG_TARGET, "state at {parent:?} has not flipped to BABE; not seeding");
			return Ok(());
		}
		seed_epoch_tree_if_needed(&self.client, &self.babe_link, parent)
	}
}

/// Decides, at the start of a slot, whether a slot worker may author in it.
pub type AuthoringGate = Arc<dyn Fn() -> bool + Send + Sync>;

/// Whether the AURA worker authors the child of `best`.
fn aura_authors_child_of<C>(client: &C, best: Hash) -> bool
where
	C: ProvideRuntimeApi<Block>,
	C::Api: ConsensusEngineApi<Block>,
{
	active_engine_at(client, best) == ActiveEngine::Aura
}

/// Whether the BABE worker authors the child of `best`: the state there selects BABE and `seed`
/// did not fail.
fn babe_authors_child_of<C>(
	client: &C,
	best: Hash,
	seed: impl FnOnce(Hash) -> Result<(), String>,
) -> bool
where
	C: ProvideRuntimeApi<Block>,
	C::Api: ConsensusEngineApi<Block>,
{
	if active_engine_at(client, best) != ActiveEngine::Babe {
		return false;
	}
	match seed(best) {
		Ok(()) => true,
		Err(err) => {
			log::error!(
				target: LOG_TARGET,
				"failed to seed BABE epoch tree at {best:?}: {err}; not authoring in this slot",
			);
			false
		},
	}
}

/// Gate for the AURA worker: open while the state of the best block selects AURA.
pub fn aura_authoring_gate<C>(client: Arc<C>) -> AuthoringGate
where
	C: ProvideRuntimeApi<Block> + HeaderBackend<Block> + Send + Sync + 'static,
	C::Api: ConsensusEngineApi<Block>,
{
	Arc::new(move || aura_authors_child_of(&*client, client.info().best_hash))
}

/// Gate for the BABE worker: open while the state of the best block selects BABE, seeding the
/// epoch tree at the flip block first.
pub fn babe_authoring_gate<C>(client: Arc<C>, babe_link: BabeLink<Block>) -> AuthoringGate
where
	C: AuthoringClient,
	C::Api: BabeApi<Block> + ConsensusEngineApi<Block>,
{
	let seeder = BabeEpochSeeder::new(client.clone(), babe_link);
	let (babe_client, aura_client) = (client.clone(), client.clone());
	logging_handover_gate(
		move || client.info().best_hash,
		move |best| babe_authors_child_of(&*babe_client, best, |at| seeder.seed_for_child_of(at)),
		move |best| aura_authors_child_of(&*aura_client, best),
	)
}

/// The BABE gate around `babe_authors`, read once per slot at `best_hash()`, logging when it opens
/// and when it closes with `aura_authors` taking over.
fn logging_handover_gate(
	best_hash: impl Fn() -> Hash + Send + Sync + 'static,
	babe_authors: impl Fn(Hash) -> bool + Send + Sync + 'static,
	aura_authors: impl Fn(Hash) -> bool + Send + Sync + 'static,
) -> AuthoringGate {
	let was_open = AtomicBool::new(false);
	Arc::new(move || {
		let best = best_hash();
		let open = babe_authors(best);
		if was_open.swap(open, Ordering::AcqRel) != open {
			if open {
				log::info!(target: LOG_TARGET, "handing block authoring over from AURA to BABE at {best:?}");
			} else if aura_authors(best) {
				// Not logged when the gate closed on a seeding failure: AURA does not author then.
				log::info!(target: LOG_TARGET, "handing block authoring back from BABE to AURA at {best:?}");
			}
		}
		open
	})
}

/// Wraps a slot worker's [`SyncOracle`] so the worker also skips every slot its gate is closed in.
#[derive(Clone)]
pub struct EngineGatedSyncOracle<Inner> {
	inner: Inner,
	gate: AuthoringGate,
}

impl<Inner> EngineGatedSyncOracle<Inner> {
	pub fn new(inner: Inner, gate: AuthoringGate) -> Self {
		Self { inner, gate }
	}
}

impl<Inner: SyncOracle> SyncOracle for EngineGatedSyncOracle<Inner> {
	fn is_major_syncing(&self) -> bool {
		self.inner.is_major_syncing() || !(self.gate)()
	}

	fn is_offline(&self) -> bool {
		self.inner.is_offline()
	}
}

/// Drive both authoring workers until either exits. Each skips the slots its
/// [`EngineGatedSyncOracle`] closes, so at most one of them authors on a given best block.
pub async fn run_authoring_workers(
	aura_worker: impl Future<Output = ()>,
	babe_worker: impl Future<Output = ()>,
) {
	futures::pin_mut!(aura_worker, babe_worker);
	match futures::future::select(aura_worker, babe_worker).await {
		futures::future::Either::Left(_) => {
			log::warn!(target: LOG_TARGET, "AURA authoring worker exited")
		},
		futures::future::Either::Right(_) => {
			log::warn!(target: LOG_TARGET, "BABE authoring worker exited")
		},
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_api::{ApiRef, ProvideRuntimeApi};
	use sp_core::H256;

	#[derive(Clone)]
	struct TestApi {
		engine: Option<ActiveEngine>,
	}

	impl ProvideRuntimeApi<Block> for TestApi {
		type Api = TestApi;

		fn runtime_api(&self) -> ApiRef<'_, Self::Api> {
			self.clone().into()
		}
	}

	fn api_error(msg: &'static str) -> sp_api::ApiError {
		sp_api::ApiError::Application(Box::<dyn std::error::Error + Send + Sync>::from(msg))
	}

	sp_api::mock_impl_runtime_apis! {
		impl ConsensusEngineApi<Block> for TestApi {
			#[advanced]
			fn active_engine(&self, _: Hash) -> Result<ActiveEngine, sp_api::ApiError> {
				self.engine.ok_or_else(|| api_error("active_engine unavailable"))
			}
		}
	}

	/// Ancestry for the coverage tests: a linear chain given as hashes from oldest to newest. The
	/// epoch tree's "fake head" (a fresh block whose parent is the queried block) is handled via the
	/// `current` hint the tree passes in.
	struct LinearChain(Vec<H256>);
	impl IsDescendentOfBuilder<H256> for &LinearChain {
		type Error = sp_blockchain::Error;
		type IsDescendentOf = Box<dyn Fn(&H256, &H256) -> Result<bool, sp_blockchain::Error>>;
		fn build_is_descendent_of(&self, current: Option<(H256, H256)>) -> Self::IsDescendentOf {
			let chain = self.0.clone();
			Box::new(move |base, block| {
				let pos = |h: &H256| chain.iter().position(|x| x == h);
				let block_pos = match current {
					Some((head, parent)) if *block == head => pos(&parent).map(|p| p + 1),
					_ => pos(block),
				};
				Ok(matches!((pos(base), block_pos), (Some(b), Some(x)) if b < x))
			})
		}
	}

	/// Ancestry for the fork tests: `(block, parent)` edges. The fake head is resolved via the
	/// `current` hint as in [`LinearChain`].
	struct ForkedChain(Vec<(H256, H256)>);
	impl IsDescendentOfBuilder<H256> for &ForkedChain {
		type Error = sp_blockchain::Error;
		type IsDescendentOf = Box<dyn Fn(&H256, &H256) -> Result<bool, sp_blockchain::Error>>;
		fn build_is_descendent_of(&self, current: Option<(H256, H256)>) -> Self::IsDescendentOf {
			let mut edges = self.0.clone();
			edges.extend(current);
			Box::new(move |base, block| {
				let mut cursor = *block;
				while let Some((_, parent)) = edges.iter().find(|(child, _)| *child == cursor) {
					if parent == base {
						return Ok(true);
					}
					cursor = *parent;
				}
				Ok(false)
			})
		}
	}

	fn epoch(index: u64, start_slot: u64) -> sc_consensus_babe::Epoch {
		sp_consensus_babe::Epoch {
			epoch_index: index,
			start_slot: start_slot.into(),
			duration: 5,
			authorities: vec![],
			randomness: [0; 32],
			config: sp_consensus_babe::BabeEpochConfiguration {
				c: (1, 4),
				allowed_slots: sp_consensus_babe::AllowedSlots::PrimaryAndSecondaryPlainSlots,
			},
		}
		.into()
	}

	#[test]
	fn seeded_tree_covers_the_flip_blocks_children() {
		// Flip block #56 at slot 809 (last AURA slot); BABE genesis slot 810 = epoch 0.
		let (parent, flip) = (H256::repeat_byte(55), H256::repeat_byte(56));
		let chain = LinearChain(vec![parent, flip]);
		let mut tree = EpochChanges::<Hash, NumberFor<Block>, sc_consensus_babe::Epoch>::new();
		let flip_slot = Slot::from(809u64);

		assert!(!tree_covers_children_of(&tree, &chain, &flip, 56, flip_slot).unwrap());

		seed_tree_at_flip_block(&mut tree, &chain, parent, flip, 56, epoch(0, 810), &epoch(1, 815))
			.unwrap();
		assert!(
			tree_covers_children_of(&tree, &chain, &flip, 56, flip_slot).unwrap(),
			"a seeded tree must report the flip block's children as covered"
		);
		// The pitfall the helper exists for: at the flip block's *own* slot nothing matches.
		assert!(
			tree.epoch_descriptor_for_child_of(&chain, &flip, 56, flip_slot)
				.unwrap()
				.is_none()
		);
	}

	#[test]
	fn seeding_at_a_second_flip_block_keeps_both_forks_covered() {
		// Two branches from #54 each commit the flip in their own #56 at slot 809.
		let root = H256::repeat_byte(54);
		let (a1, f1) = (H256::repeat_byte(0x51), H256::repeat_byte(0x56));
		let (a2, f2) = (H256::repeat_byte(0x61), H256::repeat_byte(0x66));
		let chain = ForkedChain(vec![(a1, root), (f1, a1), (a2, root), (f2, a2)]);
		let mut tree = EpochChanges::<Hash, NumberFor<Block>, sc_consensus_babe::Epoch>::new();
		let flip_slot = Slot::from(809u64);

		seed_tree_at_flip_block(&mut tree, &chain, a1, f1, 56, epoch(0, 810), &epoch(1, 815))
			.unwrap();
		assert!(tree_covers_children_of(&tree, &chain, &f1, 56, flip_slot).unwrap());
		assert!(!tree_covers_children_of(&tree, &chain, &f2, 56, flip_slot).unwrap());

		// The second flip block gets its own pair; the first branch stays verifiable.
		seed_tree_at_flip_block(&mut tree, &chain, a2, f2, 56, epoch(0, 810), &epoch(1, 815))
			.unwrap();
		assert!(tree_covers_children_of(&tree, &chain, &f1, 56, flip_slot).unwrap());
		assert!(tree_covers_children_of(&tree, &chain, &f2, 56, flip_slot).unwrap());
		assert_eq!(tree.tree().iter().count(), 2);
	}

	#[test]
	fn seeding_rejects_a_next_epoch_that_does_not_follow_the_current_one() {
		let (parent, flip) = (H256::repeat_byte(55), H256::repeat_byte(56));
		let chain = LinearChain(vec![parent, flip]);
		let mut tree = EpochChanges::<Hash, NumberFor<Block>, sc_consensus_babe::Epoch>::new();

		let error = seed_tree_at_flip_block(
			&mut tree,
			&chain,
			parent,
			flip,
			56,
			epoch(0, 810),
			&epoch(1, 900),
		)
		.unwrap_err();

		assert!(error.contains("does not follow"), "{error}");
		assert!(tree.tree().iter().next().is_none());
	}

	#[test]
	fn active_engine_at_returns_the_runtime_value() {
		let api = TestApi { engine: Some(ActiveEngine::Babe) };
		assert_eq!(active_engine_at(&api, Default::default()), ActiveEngine::Babe);
	}

	#[test]
	fn active_engine_at_defaults_to_aura_when_the_runtime_query_fails() {
		let api = TestApi { engine: None };
		assert_eq!(active_engine_at(&api, Default::default()), ActiveEngine::Aura);
	}

	#[test]
	fn babe_authors_only_while_the_best_state_selects_babe_and_the_tree_is_seeded() {
		let best = Hash::default();
		let aura = TestApi { engine: Some(ActiveEngine::Aura) };
		let babe = TestApi { engine: Some(ActiveEngine::Babe) };

		assert!(babe_authors_child_of(&babe, best, |_| Ok(())));
		assert!(!babe_authors_child_of(&babe, best, |_| Err("no epoch".into())));
		assert!(!babe_authors_child_of(&aura, best, |_| panic!("no seeding before the flip")));
	}

	#[test]
	fn neither_engine_authors_twice_for_the_same_best_state() {
		let best = Hash::default();
		for engine in [Some(ActiveEngine::Aura), Some(ActiveEngine::Babe), None] {
			let api = TestApi { engine };
			let aura = aura_authors_child_of(&api, best);
			let babe = babe_authors_child_of(&api, best, |_| Ok(()));
			assert!(aura != babe, "exactly one engine authors for {engine:?}");
			assert_eq!(aura, engine != Some(ActiveEngine::Babe));
		}
	}

	struct FixedOracle {
		major_syncing: bool,
		offline: bool,
	}
	impl SyncOracle for FixedOracle {
		fn is_major_syncing(&self) -> bool {
			self.major_syncing
		}
		fn is_offline(&self) -> bool {
			self.offline
		}
	}

	#[test]
	fn gated_oracle_follows_the_gate_in_both_directions() {
		let open = Arc::new(AtomicBool::new(false));
		let gate: AuthoringGate = {
			let open = open.clone();
			Arc::new(move || open.load(Ordering::Acquire))
		};
		let oracle =
			EngineGatedSyncOracle::new(FixedOracle { major_syncing: false, offline: false }, gate);

		assert!(oracle.is_major_syncing());
		open.store(true, Ordering::Release);
		assert!(!oracle.is_major_syncing());
		open.store(false, Ordering::Release);
		assert!(oracle.is_major_syncing());
	}

	#[test]
	fn gated_oracle_keeps_the_inner_answers_without_running_the_gate_during_major_sync() {
		let syncing = EngineGatedSyncOracle::new(
			FixedOracle { major_syncing: true, offline: true },
			Arc::new(|| panic!("the gate must not run during a major sync")),
		);
		assert!(syncing.is_major_syncing());
		assert!(syncing.is_offline());

		let idle = EngineGatedSyncOracle::new(
			FixedOracle { major_syncing: false, offline: false },
			Arc::new(|| false),
		);
		assert!(!idle.is_offline());
	}

	#[test]
	fn babe_gate_reads_the_best_block_once_per_slot_and_consults_aura_only_when_it_closes() {
		let (unflipped, flipped) = (H256::repeat_byte(1), H256::repeat_byte(2));
		let best = Arc::new(std::sync::Mutex::new(unflipped));
		let (reads, aura_asked) = (
			Arc::new(std::sync::atomic::AtomicUsize::new(0)),
			Arc::new(std::sync::atomic::AtomicUsize::new(0)),
		);
		let gate = logging_handover_gate(
			{
				let (best, reads) = (best.clone(), reads.clone());
				move || {
					reads.fetch_add(1, Ordering::SeqCst);
					*best.lock().unwrap()
				}
			},
			move |at| at == flipped,
			{
				let aura_asked = aura_asked.clone();
				move |at| {
					aura_asked.fetch_add(1, Ordering::SeqCst);
					at == unflipped
				}
			},
		);

		for (at, open, aura_calls) in
			[(unflipped, false, 0), (flipped, true, 0), (flipped, true, 0), (unflipped, false, 1)]
		{
			*best.lock().unwrap() = at;
			assert_eq!(gate(), open);
			assert_eq!(aura_asked.load(Ordering::SeqCst), aura_calls);
		}
		assert_eq!(reads.load(Ordering::SeqCst), 4);
	}

	#[derive(Default)]
	struct MemoryAux(std::sync::Mutex<std::collections::HashMap<Vec<u8>, Vec<u8>>>);

	impl AuxStore for MemoryAux {
		fn insert_aux<
			'a,
			'b: 'a,
			'c: 'a,
			I: IntoIterator<Item = &'a (&'c [u8], &'c [u8])>,
			D: IntoIterator<Item = &'a &'b [u8]>,
		>(
			&self,
			insert: I,
			delete: D,
		) -> sp_blockchain::Result<()> {
			let mut map = self.0.lock().unwrap();
			for (key, value) in insert {
				map.insert(key.to_vec(), value.to_vec());
			}
			for key in delete {
				map.remove(*key);
			}
			Ok(())
		}

		fn get_aux(&self, key: &[u8]) -> sp_blockchain::Result<Option<Vec<u8>>> {
			Ok(self.0.lock().unwrap().get(key).cloned())
		}
	}

	#[test]
	fn flip_block_gets_zero_weight_only_when_it_has_none() {
		let (aux, flip, recorded) =
			(MemoryAux::default(), H256::repeat_byte(56), H256::repeat_byte(57));
		let key = block_weight_key(recorded);
		aux.insert_aux(&[(key.as_slice(), (7 as BabeBlockWeight).encode().as_slice())], &[])
			.unwrap();

		for at in [flip, recorded] {
			ensure_zero_weight_at(&aux, at).unwrap();
		}
		assert_eq!(load_block_weight(&aux, flip).unwrap(), Some(0));
		assert_eq!(load_block_weight(&aux, recorded).unwrap(), Some(7));
	}

	#[test]
	fn placeholder_configuration_matches_runtime_genesis_epoch_and_aura_timing() {
		let slots_per_epoch = 10;
		let config = placeholder_configuration(
			SlotDuration::from_millis(midnight_node_runtime::SLOT_DURATION),
			slots_per_epoch,
		);
		let genesis = midnight_node_runtime::BABE_GENESIS_EPOCH_CONFIG;
		assert_eq!(config.slot_duration, midnight_node_runtime::SLOT_DURATION);
		assert_eq!(config.epoch_length, u64::from(slots_per_epoch));
		assert_eq!(config.c, genesis.c);
		assert_eq!(config.allowed_slots, genesis.allowed_slots);
		assert!(config.authorities.is_empty());
		assert_eq!(config.randomness, [0u8; 32]);
	}
}
