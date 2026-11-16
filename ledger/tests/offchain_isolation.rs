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

//! Off-chain ledger host calls against block execution.
//!
//! A separate binary because `set_default_storage` is process-global and set-once.

use ledger_storage_ledger_8::{
	arena::ArenaHash,
	storage::{default_storage, try_get_default_storage},
};
use ledger_storage_ledger_8::{
	arena::{ArenaKey, TypedArenaKey},
	db::ParityDb,
};
use midnight_node_ledger::{
	ephemeral::EphemeralDb,
	host_api::ledger_9::ledger_9_bridge as host,
	ledger_8::api::ledger::Ledger as Ledger8,
	ledger_9::{BlockContext, api::ledger::Ledger, storage::init_storage_paritydb_separate},
};
use midnight_node_res::{
	networks::{MidnightNetwork, UndeployedNetwork},
	undeployed::transactions::DEPLOY_TX,
};
use midnight_primitives_ledger::{
	LedgerEphemeralExt, LedgerMetrics, LedgerMetricsExt, LedgerOnchainExt,
};
use parity_scale_codec::Encode;
use sp_externalities::{Externalities, ExternalitiesExt};
use sp_state_machine::BasicExternalities;
use std::sync::{Arc, Mutex};

/// Host calls without a `LedgerStorageExt` use the separate-db storage.
type Shared = ParityDb;

const RUNTIME_VERSION: u32 = 1;

fn hash_of(state_root: &[u8]) -> ArenaHash<storage_sha2::Sha256> {
	let key: TypedArenaKey<Ledger<Shared>, storage_sha2::Sha256> =
		midnight_serialize::tagged_deserialize(&mut &state_root[..]).unwrap();
	ArenaKey::from(key).hash().clone()
}

/// Whether `key` is known to the shared storage (its caches or its database), and its root
/// count there.
fn in_shared(key: &ArenaHash<storage_sha2::Sha256>) -> (bool, u32) {
	default_storage::<Shared>().with_backend(|backend| {
		(backend.get(key).is_some(), backend.get_roots().get(key).copied().unwrap_or(0))
	})
}

fn in_ephemeral(key: &ArenaHash<storage_sha2::Sha256>) -> bool {
	try_get_default_storage::<EphemeralDb<Shared>>()
		.is_some_and(|storage| storage.with_backend(|backend| backend.get(key).is_some()))
}

fn held(ext: &mut BasicExternalities) -> Option<usize> {
	let mut ext: &mut dyn Externalities = ext;
	ext.extension::<LedgerEphemeralExt>().map(|held| held.len())
}

fn flushes(metrics: &Arc<Mutex<Option<LedgerMetrics>>>) -> u64 {
	let metrics = metrics.lock().unwrap();
	let metrics = metrics.as_ref().unwrap();
	metrics
		.storage_flush_time
		.with_label_values(&["ledger_state"])
		.get_sample_count()
}

fn externalities(onchain: bool) -> (BasicExternalities, Arc<Mutex<Option<LedgerMetrics>>>) {
	let registry = prometheus_endpoint::Registry::new();
	let metrics = Arc::new(Mutex::new(Some(LedgerMetrics::register(&registry).unwrap())));
	let mut ext = BasicExternalities::default();
	ext.register_extension(LedgerMetricsExt::new(metrics.clone()));
	if onchain {
		ext.register_extension(LedgerOnchainExt::default());
	}
	(ext, metrics)
}

/// Everything a block with one user transaction and one system transaction hands back to the
/// runtime, plus reads at the states it produced and at a committed one.
#[derive(Debug, PartialEq)]
struct Outputs {
	applied: Vec<u8>,
	applied_system: Vec<u8>,
	post_block: Vec<u8>,
	apply_post_block: Vec<u8>,
	ledger_state_root: Vec<u8>,
	zswap_state_root: Vec<u8>,
	parameters: Vec<u8>,
	committed_parameters: Vec<u8>,
	cost: Vec<u8>,
	migrated: Vec<u8>,
	state_roots: Vec<Vec<u8>>,
}

fn run_block(
	genesis: &[u8],
	v8_state: &[u8],
	tx: &[u8],
	system_tx: &[u8],
	ctx: &BlockContext,
) -> Outputs {
	let applied = host::apply_transaction(genesis, tx, ctx.clone(), RUNTIME_VERSION).unwrap();
	let applied_system = host::apply_cnight_system_transaction(
		&applied.state_root,
		system_tx,
		ctx.clone(),
		RUNTIME_VERSION,
	)
	.unwrap();
	let post_block = host::post_block_update(&applied_system.state_root, ctx.clone()).unwrap();
	// During block execution the post-block update releases the block's intermediate states,
	// so the second post-block variant runs on a sibling block from the same parent.
	let sibling = host::apply_transaction(genesis, tx, ctx.clone(), RUNTIME_VERSION).unwrap();
	let sibling_system = host::apply_cnight_system_transaction(
		&sibling.state_root,
		system_tx,
		ctx.clone(),
		RUNTIME_VERSION,
	)
	.unwrap();
	assert_eq!(sibling_system.state_root, applied_system.state_root);
	let apply_post_block =
		host::apply_post_block_update(&sibling_system.state_root, ctx.clone()).unwrap();
	let (migrated, migration_cost) = host::migrate_state_v8_to_v9(v8_state).unwrap();
	host::flush_storage();

	Outputs {
		ledger_state_root: host::get_ledger_state_root(&apply_post_block).encode(),
		zswap_state_root: host::get_zswap_state_root(&apply_post_block).encode(),
		parameters: host::get_ledger_parameters(&apply_post_block).encode(),
		committed_parameters: host::get_ledger_parameters(genesis).encode(),
		cost: host::get_transaction_cost(&apply_post_block, tx, ctx.clone(), u64::MAX).encode(),
		state_roots: vec![
			applied.state_root.clone(),
			applied_system.state_root.clone(),
			post_block.clone(),
			apply_post_block.clone(),
			migrated.clone(),
		],
		applied: applied.encode(),
		applied_system: applied_system.encode(),
		post_block,
		apply_post_block,
		migrated: (migrated, migration_cost).encode(),
	}
}

/// Off-chain execution returns exactly what block execution returns, keeps its states out of
/// the shared storage (including while a shared flush runs), and releases them with its
/// extensions.
#[test]
fn offchain_calls_match_block_execution_without_touching_shared_storage() {
	let dir = tempfile::tempdir().unwrap();
	let genesis =
		init_storage_paritydb_separate(dir.path(), UndeployedNetwork.genesis_state(), 1024);
	let (tx, ctx) = midnight_ledger_unsafe_helpers::ledger_9::extract_tx_with_context(DEPLOY_TX);
	let ctx: BlockContext = ctx.into();
	let system_tx = {
		use base_crypto::{hash::HashOutput, time::Timestamp};
		use mn_ledger_9::{
			dust::{DustPublicKey, InitialNonce},
			structure::{
				CNightGeneratesDustActionType, CNightGeneratesDustEvent, SystemTransaction,
			},
		};
		use transient_crypto_ledger_9::curve::Fr;
		let event = CNightGeneratesDustEvent {
			value: 1_000,
			owner: DustPublicKey(Fr::from(7u64)),
			time: Timestamp::from_secs(ctx.tblock),
			action: CNightGeneratesDustActionType::Create,
			nonce: InitialNonce(HashOutput([9u8; 32])),
		};
		let mut bytes = Vec::new();
		midnight_serialize::tagged_serialize(
			&SystemTransaction::CNightGeneratesDustUpdate { events: vec![event] },
			&mut bytes,
		)
		.unwrap();
		bytes
	};
	let v8_state = {
		let ledger = Ledger8::new(mn_ledger_8::structure::LedgerState::new("undeployed"));
		let mut sp = default_storage::<Shared>().arena.alloc(ledger);
		let mut bytes = Vec::new();
		midnight_serialize::tagged_serialize(&sp.as_typed_key(), &mut bytes).unwrap();
		sp.persist();
		bytes
	};
	let roots_before = default_storage::<Shared>().with_backend(|backend| backend.get_roots());

	let (mut offchain, offchain_metrics) = externalities(false);
	let offchain_outputs = sp_externalities::set_and_run_with_externalities(&mut offchain, || {
		let outputs = run_block(&genesis, &v8_state, &tx, &system_tx, &ctx);
		// Flushing the shared storage now must not include the held states.
		default_storage::<Shared>().with_backend(|backend| backend.flush_all_changes_to_db());
		outputs
	});
	let produced: Vec<_> = offchain_outputs.state_roots.iter().map(|r| hash_of(r)).collect();
	for key in &produced {
		assert_eq!(in_shared(key), (false, 0), "off-chain state reached the shared storage");
		assert!(in_ephemeral(key), "off-chain state released while still held");
	}
	assert_eq!(
		default_storage::<Shared>().with_backend(|backend| backend.get_roots()),
		roots_before,
		"off-chain execution changed the persisted roots",
	);
	assert_eq!(flushes(&offchain_metrics), 0, "off-chain execution flushed");
	// The sibling block's two intermediates are held a second time.
	assert_eq!(held(&mut offchain), Some(produced.len() + 2));

	drop(offchain);
	for key in &produced {
		assert!(!in_ephemeral(key), "off-chain state outlived its execution");
	}

	let (mut onchain, onchain_metrics) = externalities(true);
	let onchain_outputs = sp_externalities::set_and_run_with_externalities(&mut onchain, || {
		run_block(&genesis, &v8_state, &tx, &system_tx, &ctx)
	});
	assert_eq!(offchain_outputs, onchain_outputs);
	// Block execution persists only the post-block tips and the migration output; the
	// intra-block states (the first two) are held, never made GC roots.
	let (intermediates, durable) = produced.split_at(2);
	for key in intermediates {
		assert_eq!(in_shared(key).1, 0, "block execution persisted an intermediate state");
	}
	for key in durable {
		let (known, roots) = in_shared(key);
		assert!(known && roots > 0, "block execution did not persist its state");
	}
	assert_eq!(flushes(&onchain_metrics), 1);
	assert_eq!(held(&mut onchain), None);
}
