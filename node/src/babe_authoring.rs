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

//! Block-authoring supervision for the AURA→BABE consensus migration.
//!
//! `BabeBlockImport` is constructed at node start (safe now that `prune_finalized` skips
//! headers with no BABE pre-digest). The two slot workers still cannot run concurrently:
//! the inactive one would spam failed aux-data fetches every slot, and if both authored they
//! would fork the chain. [`run_authoring_supervisor`] is the single authoring gate: it polls
//! AURA until the flip, bootstraps BABE's epoch tree, then polls BABE for the rest of the
//! node's life. The switch is one-directional, so a restart after the flip skips AURA.
//!
//! Seeding the epoch tree has two triggers, one implementation ([`seed_epoch_tree_if_needed`]):
//!
//! - **About to author a BABE block** (authorities only): the supervisor seeds after
//!   [`wait_for_flip`] and before starting the BABE worker. The validator that authors the very
//!   first BABE block has imported no BABE block yet and its own blocks bypass the import queue, so
//!   nothing else would seed before it proposes. `wait_for_flip` watches the origin-independent
//!   every-import stream so the hand-over also happens while syncing across the flip.
//! - **About to verify a BABE block** (every role): [`BabeEpochSeeder`], called by the import-queue
//!   dispatcher right before it hands a BABE batch to the BABE queue. This is the only trigger that
//!   works while *syncing* across the flip — the client emits no block-import notifications for
//!   sync-origin imports, so a notification watcher never fires there — and it is what lets
//!   non-authorities import the first BABE block; they run no flip watcher at all.
//!
//! Seeding is idempotent and its check-and-reset is atomic under the epoch-tree lock, so both
//! triggers may run concurrently on an authority.

use crate::consensus_engine_dispatch::EpochSeeder;
use futures::StreamExt;
use midnight_node_runtime::opaque::Block;
use midnight_primitives_consensus_engine::{ActiveEngine, ConsensusEngineApi};
use parity_scale_codec::Encode;
use sc_client_api::{AuxStore, BlockchainEvents};
use sc_consensus_babe::{BabeBlockWeight, BabeLink, aux_schema::block_weight_key};
use sc_consensus_epochs::{EpochChanges, IsDescendentOfBuilder, descendent_query};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::{HeaderBackend, HeaderMetadata};
use sp_consensus_babe::{BABE_ENGINE_ID, BabeApi};
use sp_consensus_slots::Slot;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, NumberFor};
use std::future::Future;
use std::sync::Arc;

const LOG_TARGET: &str = "babe-authoring";

type Hash = <Block as BlockT>::Hash;

/// Trait bundle for a client the supervisor can query, subscribe to, and write aux data on.
pub trait SupervisorClient:
	ProvideRuntimeApi<Block>
	+ HeaderBackend<Block>
	+ HeaderMetadata<Block, Error = sp_blockchain::Error>
	+ BlockchainEvents<Block>
	+ AuxStore
	+ Send
	+ Sync
	+ 'static
{
}

impl<C> SupervisorClient for C where
	C: ProvideRuntimeApi<Block>
		+ HeaderBackend<Block>
		+ HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ BlockchainEvents<Block>
		+ AuxStore
		+ Send
		+ Sync
		+ 'static
{
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

fn has_babe_pre_runtime_digest(header: &<Block as BlockT>::Header) -> bool {
	header
		.digest()
		.logs()
		.iter()
		.any(|log| matches!(log.as_pre_runtime(), Some((id, _)) if id == BABE_ENGINE_ID))
}

/// Resolve once the best chain has flipped to BABE, yielding the best block hash at that point.
///
/// Returns immediately if the chain is already on BABE (restart after the flip); otherwise watches
/// new-best block imports until one lands whose state selects BABE.
///
/// This uses `every_import_notification_stream`, not `import_notification_stream`: the latter is
/// silent for blocks imported with a sync origin, so a validator syncing across the flip would keep
/// running the AURA worker until some *other* node's block arrived at the tip — each slot in between
/// it would attempt an AURA proposal that the runtime rejects ("AURA pre-runtime digest present in
/// state 'Babe'"), and if every validator were in that state nobody would ever produce that block.
/// The every-import stream fires for sync-origin imports too, so the hand-over happens at the flip
/// block wherever the node is relative to the tip. The BABE worker then simply idles (slot workers
/// skip slots while major-syncing) until sync completes.
///
/// The stream is subscribed *before* reading the best block so a flip imported in between is not
/// missed. Every-import sinks only receive imports that happen after they exist.
pub async fn wait_for_flip<C>(client: &Arc<C>) -> Hash
where
	C: SupervisorClient,
	C::Api: ConsensusEngineApi<Block>,
{
	let mut notifications = client.every_import_notification_stream();

	let best = client.info().best_hash;
	if active_engine_at(&**client, best) == ActiveEngine::Babe {
		log::info!(target: LOG_TARGET, "chain already on BABE at startup (best {best:?})");
		return best;
	}

	while let Some(notification) = notifications.next().await {
		if !notification.is_new_best {
			continue;
		}
		// Cheap pre-filter for the (possibly long) pre-arming history: the flip block, like every
		// block from arming onward, carries a BABE pre-runtime digest. Blocks without one cannot be
		// past the flip, so skip the runtime query for them.
		if !has_babe_pre_runtime_digest(&notification.header) {
			continue;
		}
		if active_engine_at(&**client, notification.hash) == ActiveEngine::Babe {
			log::info!(
				target: LOG_TARGET,
				"consensus flip to BABE observed at block #{} ({:?})",
				notification.header.number(),
				notification.hash,
			);
			return notification.hash;
		}
	}

	// The notification stream only ends when the node is shutting down.
	client.info().best_hash
}

/// Wait for the consensus flip, then seed BABE's epoch tree (and chain-weight) at that block.
///
/// Returns the flip-block hash. Seeding is a no-op when the tree already covers the block
/// (restart after the flip). Non-authorities don't use this; their tree is seeded on the import path.
pub async fn bootstrap_babe_at_flip<C>(client: Arc<C>, babe_link: &BabeLink<Block>) -> Hash
where
	C: SupervisorClient,
	C::Api: BabeApi<Block> + ConsensusEngineApi<Block>,
{
	let at = wait_for_flip(&client).await;
	if let Err(err) = seed_epoch_tree_if_needed(&client, babe_link, at) {
		log::error!(
			target: LOG_TARGET,
			"failed to seed BABE epoch tree at {at:?}: {err}; BABE import/authoring may stall",
		);
	}
	at
}

/// Whether `epoch_changes` can resolve an epoch for the children of the flip block `at`.
///
/// The children are queried at `flip_slot + 1`, the first BABE slot, **not** at the flip block's own
/// slot: the flip happens at the last AURA slot of an epoch and BABE's genesis slot is the one
/// after, so every epoch in a seeded tree starts *above* `flip_slot`. Querying at `flip_slot` finds
/// no epoch and reports an already-seeded tree as uncovered, which would make every subsequent call
/// re-seed — and `reset` wipes the tree.
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

/// Seed BABE's epoch tree so authoring/verification can resolve epochs for children of `at`.
///
/// Before the flip nothing is imported through the BABE pipeline, so its `EpochChanges` is empty
/// and the first BABE block (a child of the flip block) has no epoch to be authored/verified under.
/// This mirrors the warp-sync bootstrap (`import_state`): it resets the tree to the `current`/`next`
/// epochs the runtime reports at `at`. `migrate_to_babe` makes those runtime APIs return the
/// epoch-0 genesis, and the flip block carries a BABE pre-digest (from the ArmedBabe proposer) so
/// its slot is readable.
///
/// It is a no-op when the tree already covers children of `at` — the other trigger got there first,
/// or a restart after the flip reloaded a populated tree from the aux DB. It refuses (with an error)
/// to reset a non-empty tree that does not cover `at`: `reset` discards everything BABE has recorded
/// since the flip, so that is never the right move for a block that is not the flip block.
pub fn seed_epoch_tree_if_needed<C>(
	client: &Arc<C>,
	babe_link: &BabeLink<Block>,
	at: Hash,
) -> Result<(), String>
where
	C: SupervisorClient,
	C::Api: BabeApi<Block>,
{
	let header = client
		.header(at)
		.map_err(|e| e.to_string())?
		.ok_or_else(|| format!("header for flip block {at:?} not found"))?;
	let number = *header.number();
	let parent_hash = *header.parent_hash();

	let slot = sc_consensus_babe::find_pre_digest::<Block>(&header)
		.map_err(|e| format!("flip block {at:?} has no BABE pre-digest: {e}"))?
		.slot();

	// Hold the tree lock from the coverage check through the reset. Seeding has two triggers (the
	// authoring supervisor and the import-path seeder) that can run concurrently around the flip;
	// with check and reset under one lock the second caller sees the first one's result instead of
	// racing it.
	{
		let mut epoch_changes = babe_link.epoch_changes().shared_data();
		let covered =
			tree_covers_children_of(&epoch_changes, descendent_query(&**client), &at, number, slot)
				.map_err(|e| format!("epoch-tree lookup for children of {at:?} failed: {e}"))?;
		if covered {
			log::debug!(target: LOG_TARGET, "BABE epoch tree already covers {at:?}; not seeding");
			return Ok(());
		}

		// `reset` discards the whole tree. A tree that has content but does not cover the flip
		// block's children is not a seeding situation — it is either a bug in the coverage check
		// or a block that is not the flip block — and wiping it would destroy the epochs BABE has
		// recorded from block digests since the flip (which is exactly what a stale seed did once:
		// the node kept the flip-time `next_epoch` for epoch 1 while the rest of the network used
		// the epoch-1 descriptor announced at the first BABE block, and forked at the epoch
		// boundary). Refuse loudly instead.
		if epoch_changes.tree().iter().next().is_some() {
			return Err(format!(
				"BABE epoch tree is non-empty but does not cover children of {at:?}; refusing to \
				 reset it",
			));
		}

		let current = client.runtime_api().current_epoch(at).map_err(|e| e.to_string())?;
		let next = client.runtime_api().next_epoch(at).map_err(|e| e.to_string())?;
		let (current_index, next_index) = (current.epoch_index, next.epoch_index);
		epoch_changes.reset(parent_hash, at, number, current.into(), next.into());
		log::info!(
			target: LOG_TARGET,
			"seeded BABE epoch tree at flip block #{number} ({at:?}): epochs {current_index} and {next_index}",
		);
	}

	// Bootstrap the flip block's cumulative BABE chain weight to 0. The flip block was imported
	// through the AURA pipeline, which records no BABE block weight, so the first BABE block (its
	// child) would fail to import with "Parent block ... has no associated weight". This mirrors the
	// warp-sync bootstrap (`import_state`), which likewise resets the weight to 0 at the sync point.
	let weight_key = block_weight_key(at);
	let weight_value = (0 as BabeBlockWeight).encode();
	let no_delete: &[&[u8]] = &[];
	client
		.insert_aux(&[(weight_key.as_slice(), weight_value.as_slice())], no_delete)
		.map_err(|e| e.to_string())?;

	log::debug!(target: LOG_TARGET, "bootstrapped zero BABE chain-weight at flip block {at:?}");
	Ok(())
}

/// [`EpochSeeder`] for the import-queue dispatcher: seeds BABE's epoch tree at `parent` when the
/// first BABE block is about to be verified and `parent` is the flip block.
///
/// Seeds only when the runtime state at `parent` has flipped to BABE. The parent hash comes from a
/// peer-supplied header, so without that check a peer could make us reset the tree at an
/// arbitrary imported block; with it, the only block that is both post-flip *and* not yet covered by
/// the tree is the flip block itself (every later block is imported through the BABE pipeline,
/// which records its epoch).
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
	C: SupervisorClient,
	C::Api: BabeApi<Block> + ConsensusEngineApi<Block>,
{
	fn ensure_seeded_for_child_of(&self, parent: Hash) {
		// Not imported (yet): nothing to seed at; the BABE queue will report `UnknownParent` and
		// sync re-offers the block later.
		if !matches!(self.client.header(parent), Ok(Some(_))) {
			log::debug!(target: LOG_TARGET, "parent {parent:?} of a BABE block is not imported; not seeding");
			return;
		}
		if active_engine_at(&*self.client, parent) != ActiveEngine::Babe {
			log::debug!(target: LOG_TARGET, "state at {parent:?} has not flipped to BABE; not seeding");
			return;
		}
		if let Err(err) = seed_epoch_tree_if_needed(&self.client, &self.babe_link, parent) {
			log::error!(
				target: LOG_TARGET,
				"failed to seed BABE epoch tree at {parent:?} on the import path: {err}; BABE import may stall",
			);
		}
	}
}

/// Drive AURA authoring until the consensus flip, bootstrap BABE's epoch tree, then drive BABE
/// authoring for the remainder of the node's life.
///
/// `aura_worker` and `babe_worker` are the futures returned by `start_aura`/`start_babe`. The AURA
/// future is polled only until the flip is observed and the epoch tree is seeded, then dropped;
/// the BABE future runs terminally.
pub async fn run_authoring_supervisor<C>(
	client: Arc<C>,
	babe_link: BabeLink<Block>,
	aura_worker: impl Future<Output = ()>,
	babe_worker: impl Future<Output = ()>,
) where
	C: SupervisorClient,
	C::Api: BabeApi<Block> + ConsensusEngineApi<Block>,
{
	let flip_at = {
		let bootstrap = bootstrap_babe_at_flip(client, &babe_link);
		futures::pin_mut!(aura_worker, bootstrap);
		match futures::future::select(aura_worker, bootstrap).await {
			// The AURA worker is spawned as essential; if it returns first the service is going
			// down anyway, so there is nothing to hand over to.
			futures::future::Either::Left(((), _bootstrap)) => {
				log::warn!(target: LOG_TARGET, "AURA authoring worker exited before the consensus flip");
				return;
			},
			futures::future::Either::Right((flip_at, _aura)) => flip_at,
		}
	};

	log::info!(target: LOG_TARGET, "handing block authoring over from AURA to BABE at {flip_at:?}");

	babe_worker.await;
	log::warn!(target: LOG_TARGET, "BABE authoring worker exited");
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

			#[advanced]
			fn should_emit_babe_preruntime_digest(&self, _: Hash) -> Result<bool, sp_api::ApiError> {
				unimplemented!("not read by active_engine_at")
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

		tree.reset(parent, flip, 56, epoch(0, 810), epoch(1, 815));
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
	fn active_engine_at_returns_the_runtime_value() {
		let api = TestApi { engine: Some(ActiveEngine::Babe) };
		assert_eq!(active_engine_at(&api, Default::default()), ActiveEngine::Babe);
	}

	#[test]
	fn active_engine_at_defaults_to_aura_when_the_runtime_query_fails() {
		let api = TestApi { engine: None };
		assert_eq!(active_engine_at(&api, Default::default()), ActiveEngine::Aura);
	}
}
