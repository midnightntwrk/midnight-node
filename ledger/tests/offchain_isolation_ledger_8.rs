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

//! Off-chain ledger-8 host calls against block execution.
//!
//! A separate binary because `set_default_storage` is process-global and set-once.

use ledger_storage_ledger_8::{
	Storage,
	arena::{ArenaHash, ArenaKey, TypedArenaKey},
	db::ParityDb,
	storage::{default_storage, set_default_storage, try_get_default_storage},
};
use midnight_node_ledger::{
	ephemeral::EphemeralDb,
	host_api::ledger_8::ledger_8_bridge as host,
	ledger_8::{BlockContext, api::ledger::Ledger},
};
use midnight_primitives_ledger::{
	LedgerEphemeralExt, LedgerMetrics, LedgerMetricsExt, LedgerOnchainExt,
};
use parity_scale_codec::Encode;
use sp_externalities::{Externalities, ExternalitiesExt};
use sp_state_machine::BasicExternalities;
use std::sync::{Arc, Mutex};

/// The one storage instantiation host calls dispatch to: the ledger columns after Substrate's.
type Shared = ParityDb<
	storage_sha2::Sha256,
	ledger_storage_ledger_8::db::paritydb::OwnedDb,
	{ midnight_primitives_ledger::LedgerStorageExt::COLUMN_OFFSET },
>;

const RUNTIME_VERSION: u32 = 1;
const NONCE: [u8; 32] = [9; 32];
const TBLOCK: u64 = 1_800_000_000;

/// The stored nodes a state root refers to: the root itself, or the children of an inline
/// (`Direct`) root.
fn nodes_of(state_root: &[u8]) -> Vec<ArenaHash<storage_sha2::Sha256>> {
	let key: TypedArenaKey<Ledger<Shared>, storage_sha2::Sha256> =
		midnight_serialize::tagged_deserialize(&mut &state_root[..]).unwrap();
	ArenaKey::from(key).refs().into_iter().cloned().collect()
}

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

#[derive(Debug, PartialEq)]
struct Outputs {
	applied_system: Vec<u8>,
	post_block: Vec<u8>,
	apply_post_block: Vec<u8>,
	ledger_state_root: Vec<u8>,
	parameters: Vec<u8>,
	dust: Vec<u8>,
	committed_parameters: Vec<u8>,
	state_roots: Vec<Vec<u8>>,
}

fn run_block(genesis: &[u8], system_tx: &[u8], ctx: &BlockContext) -> Outputs {
	let applied_system =
		host::apply_cnight_system_transaction(genesis, system_tx, ctx.clone(), RUNTIME_VERSION)
			.unwrap();
	let post_block = host::post_block_update(&applied_system.state_root, ctx.clone()).unwrap();
	// During block execution the post-block update releases the block's intermediate states,
	// so the second post-block variant runs on a sibling block from the same parent.
	let sibling_system =
		host::apply_cnight_system_transaction(genesis, system_tx, ctx.clone(), RUNTIME_VERSION)
			.unwrap();
	assert_eq!(sibling_system.state_root, applied_system.state_root);
	let apply_post_block =
		host::apply_post_block_update(&sibling_system.state_root, ctx.clone()).unwrap();
	host::flush_storage();

	Outputs {
		ledger_state_root: host::get_ledger_state_root(&apply_post_block).encode(),
		parameters: host::get_ledger_parameters(&apply_post_block).encode(),
		dust: host::dust_generation_values(&apply_post_block, vec![NONCE]).encode(),
		committed_parameters: host::get_ledger_parameters(genesis).encode(),
		state_roots: vec![
			applied_system.state_root.clone(),
			post_block.clone(),
			apply_post_block.clone(),
		],
		applied_system: applied_system.encode(),
		post_block,
		apply_post_block,
	}
}

/// The ledger-8 bridge behaves like the ledger-9 one: off-chain execution returns exactly what
/// block execution returns, keeps its states out of the shared storage, and releases them with
/// its extensions.
#[test]
fn ledger_8_offchain_calls_match_block_execution_without_touching_shared_storage() {
	let dir = tempfile::tempdir().unwrap();
	let _ = set_default_storage(|| {
		use midnight_primitives_ledger::LedgerStorageExt;
		use midnight_storage_core::db::paritydb::{NUM_COLUMNS, OwnedDb, set_init_options};
		let mut options = parity_db::Options::with_columns(
			dir.path(),
			LedgerStorageExt::COLUMN_OFFSET + NUM_COLUMNS,
		);
		set_init_options(&mut options, LedgerStorageExt::COLUMN_OFFSET, true);
		let db = parity_db::Db::open_or_create(&options).expect("open parity-db");
		Storage::new(1024, Shared::from_existing_db(OwnedDb(Arc::new(db))))
	});

	let genesis = {
		let ledger = Ledger::<Shared>::new(mn_ledger_8::structure::LedgerState::new("undeployed"));
		let mut sp = default_storage::<Shared>().arena.alloc(ledger);
		let mut bytes = Vec::new();
		midnight_serialize::tagged_serialize(&sp.as_typed_key(), &mut bytes).unwrap();
		sp.persist();
		default_storage::<Shared>().with_backend(|backend| backend.flush_all_changes_to_db());
		bytes
	};
	let ctx = BlockContext { tblock: TBLOCK, last_block_time: TBLOCK - 6, ..Default::default() };
	let system_tx = {
		use base_crypto::{hash::HashOutput, time::Timestamp};
		use mn_ledger_8::{
			dust::{DustPublicKey, InitialNonce},
			structure::{
				CNightGeneratesDustActionType, CNightGeneratesDustEvent, SystemTransaction,
			},
		};
		use transient_crypto::curve::Fr;
		let event = CNightGeneratesDustEvent {
			value: 1_000,
			owner: DustPublicKey(Fr::from(7u64)),
			time: Timestamp::from_secs(TBLOCK),
			action: CNightGeneratesDustActionType::Create,
			nonce: InitialNonce(HashOutput(NONCE)),
		};
		let mut bytes = Vec::new();
		midnight_serialize::tagged_serialize(
			&SystemTransaction::CNightGeneratesDustUpdate { events: vec![event] },
			&mut bytes,
		)
		.unwrap();
		bytes
	};
	let roots_before = default_storage::<Shared>().with_backend(|backend| backend.get_roots());

	let (mut offchain, offchain_metrics) = externalities(false);
	let offchain_outputs = sp_externalities::set_and_run_with_externalities(&mut offchain, || {
		let outputs = run_block(&genesis, &system_tx, &ctx);
		// Flushing the shared storage now must not include the held states.
		default_storage::<Shared>().with_backend(|backend| backend.flush_all_changes_to_db());
		outputs
	});
	let produced: Vec<_> = offchain_outputs.state_roots.iter().flat_map(|r| nodes_of(r)).collect();
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
	// The sibling block's intermediate is held a second time.
	assert_eq!(held(&mut offchain), Some(offchain_outputs.state_roots.len() + 1));

	drop(offchain);
	for key in &produced {
		assert!(!in_ephemeral(key), "off-chain state outlived its execution");
	}

	let (mut onchain, onchain_metrics) = externalities(true);
	let onchain_outputs = sp_externalities::set_and_run_with_externalities(&mut onchain, || {
		run_block(&genesis, &system_tx, &ctx)
	});
	assert_eq!(offchain_outputs, onchain_outputs);
	// Block execution persists only the post-block tips; the intra-block state (the first
	// root) is held until the post-block update and then released, never stored for itself.
	let durable: Vec<_> =
		onchain_outputs.state_roots[1..].iter().flat_map(|r| nodes_of(r)).collect();
	for key in &durable {
		assert!(in_shared(key).0, "block execution did not store its state");
	}
	let roots_after = default_storage::<Shared>().with_backend(|backend| backend.get_roots());
	assert!(roots_after.len() > roots_before.len(), "block execution did not persist its states");
	assert_eq!(flushes(&onchain_metrics), 1);
	assert_eq!(held(&mut onchain), None);
}
