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
//! Full nodes (no authoring) still need the epoch tree to *import* the first BABE block;
//! [`bootstrap_babe_at_flip`] is therefore also spawned for non-authority roles.

use futures::StreamExt;
use midnight_node_runtime::opaque::Block;
use midnight_primitives_consensus_engine::{ActiveEngine, ConsensusEngineApi};
use parity_scale_codec::Encode;
use sc_client_api::{AuxStore, BlockchainEvents};
use sc_consensus_babe::{BabeBlockWeight, BabeLink, aux_schema::block_weight_key};
use sc_consensus_epochs::descendent_query;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::{HeaderBackend, HeaderMetadata};
use sp_consensus_babe::BabeApi;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
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

/// Resolve once the best chain has flipped to BABE, yielding the best block hash at that point.
///
/// Returns immediately if the chain is already on BABE (restart after the flip); otherwise watches
/// best-block import notifications until one lands whose state selects BABE.
pub async fn wait_for_flip<C>(client: &Arc<C>) -> Hash
where
	C: SupervisorClient,
	C::Api: ConsensusEngineApi<Block>,
{
	let best = client.info().best_hash;
	if active_engine_at(&**client, best) == ActiveEngine::Babe {
		log::info!(target: LOG_TARGET, "chain already on BABE at startup (best {best:?})");
		return best;
	}

	let mut notifications = client.import_notification_stream();
	while let Some(notification) = notifications.next().await {
		if !notification.is_new_best {
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
/// (restart after the flip). Import of the first BABE block needs this even on non-authorities.
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

/// Seed BABE's epoch tree so authoring/verification can resolve epochs for children of `at`.
///
/// Before the flip nothing is imported through the BABE pipeline, so its `EpochChanges` is empty
/// and the first BABE block (a child of the flip block) has no epoch to be authored/verified under.
/// This mirrors the warp-sync bootstrap (`import_state`): it resets the tree to the `current`/`next`
/// epochs the runtime reports at `at`. `migrate_to_babe` makes those runtime APIs return the
/// epoch-0 genesis, and the flip block carries a BABE pre-digest (from the ArmedBabe proposer) so
/// its slot is readable.
///
/// It is a no-op when the tree already covers `at` — e.g. a restart after the flip, where
/// `block_import` reloaded a populated tree from the aux DB that must not be clobbered.
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

	// Don't clobber a tree already loaded from the aux DB (restart after the flip).
	{
		let epoch_changes = babe_link.epoch_changes().shared_data();
		let already_covered = epoch_changes
			.epoch_descriptor_for_child_of(descendent_query(&**client), &at, number, slot)
			.map(|descriptor| descriptor.is_some())
			.unwrap_or(false);
		if already_covered {
			log::debug!(target: LOG_TARGET, "BABE epoch tree already covers {at:?}; not seeding");
			return Ok(());
		}
	}

	let current = client.runtime_api().current_epoch(at).map_err(|e| e.to_string())?;
	let next = client.runtime_api().next_epoch(at).map_err(|e| e.to_string())?;
	let (current_index, next_index) = (current.epoch_index, next.epoch_index);

	babe_link.epoch_changes().shared_data().reset(
		parent_hash,
		at,
		number,
		current.into(),
		next.into(),
	);

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

	log::info!(
		target: LOG_TARGET,
		"seeded BABE epoch tree and zero chain-weight at flip block #{number} ({at:?}): epochs {current_index} and {next_index}",
	);
	Ok(())
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
