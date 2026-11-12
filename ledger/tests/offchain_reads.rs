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

//! Off-chain ledger reads against flushes of the shared ledger storage.
//!
//! A separate binary because `set_default_storage` is process-global and set-once, and because
//! the assertions inspect the shared write cache, which other tests would also fill.

use ledger_storage_ledger_8::{
	arena::TypedArenaKey,
	db::ParityDb,
	storage::{default_storage, try_get_default_storage},
};
use midnight_node_ledger::{
	ephemeral::EphemeralDb,
	host_api::{ledger_8::ledger_8_bridge as host8, ledger_9::ledger_9_bridge as host},
	ledger_8::{BlockContext as BlockContext8, api::ledger::Ledger as Ledger8},
	ledger_9::storage::init_storage_paritydb_separate,
};
use midnight_node_res::networks::{MidnightNetwork, UndeployedNetwork};
use midnight_primitives_ledger::LedgerOnchainExt;
use sp_state_machine::BasicExternalities;
use std::{
	sync::atomic::{AtomicBool, Ordering},
	thread,
};

/// Host calls without a `LedgerStorageExt` use the separate-db storage.
type Shared = ParityDb;

const READS: usize = 200;

/// Raw 32 bytes, as both a contract address and a beneficiary.
const ADDRESS: [u8; 32] = [7; 32];

const NONCE: [u8; 32] = [9; 32];
const TBLOCK: u64 = 1_800_000_000;

fn pending_writes() -> usize {
	default_storage::<Shared>().with_backend(|backend| backend.get_write_cache_len())
}

/// How many objects the ephemeral storage has looked up so far.
fn ephemeral_lookups() -> usize {
	try_get_default_storage::<EphemeralDb<Shared>>().map_or(0, |storage| {
		storage.with_backend(|backend| {
			let stats = backend.get_stats();
			stats.get_cache_hits + stats.get_cache_misses
		})
	})
}

type Read<'a> = (&'static str, Box<dyn Fn() + 'a>);

/// Every ledger read host function, at a ledger-9 state and at a ledger-8 one.
fn reads<'a>(v9: &'a [u8], v8: &'a [u8]) -> Vec<Read<'a>> {
	vec![
		("v9 get_contract_state", Box::new(|| drop(host::get_contract_state(v9, &ADDRESS)))),
		("v9 get_zswap_chain_state", Box::new(|| drop(host::get_zswap_chain_state(v9, &ADDRESS)))),
		("v9 get_unclaimed_amount", Box::new(|| drop(host::get_unclaimed_amount(v9, &ADDRESS)))),
		(
			"v9 get_bridge_receiving_amount",
			Box::new(|| drop(host::get_bridge_receiving_amount(v9, &ADDRESS))),
		),
		("v9 get_ledger_parameters", Box::new(|| drop(host::get_ledger_parameters(v9)))),
		(
			"v9 get_c_to_m_bridge_min_amount",
			Box::new(|| drop(host::get_c_to_m_bridge_min_amount(v9))),
		),
		("v9 get_zswap_state_root", Box::new(|| drop(host::get_zswap_state_root(v9)))),
		("v9 get_ledger_state_root", Box::new(|| drop(host::get_ledger_state_root(v9)))),
		("v8 key get_contract_state", Box::new(|| drop(host::get_contract_state(v8, &ADDRESS)))),
		("v8 key get_ledger_parameters", Box::new(|| drop(host::get_ledger_parameters(v8)))),
		("v8 key get_zswap_state_root", Box::new(|| drop(host::get_zswap_state_root(v8)))),
		("v8 key get_ledger_state_root", Box::new(|| drop(host::get_ledger_state_root(v8)))),
		("v8 get_contract_state", Box::new(|| drop(host8::get_contract_state(v8, &ADDRESS)))),
		("v8 get_zswap_chain_state", Box::new(|| drop(host8::get_zswap_chain_state(v8, &ADDRESS)))),
		("v8 get_unclaimed_amount", Box::new(|| drop(host8::get_unclaimed_amount(v8, &ADDRESS)))),
		(
			"v8 get_bridge_receiving_amount",
			Box::new(|| drop(host8::get_bridge_receiving_amount(v8, &ADDRESS))),
		),
		("v8 get_ledger_parameters", Box::new(|| drop(host8::get_ledger_parameters(v8)))),
		(
			"v8 get_c_to_m_bridge_min_amount",
			Box::new(|| drop(host8::get_c_to_m_bridge_min_amount(v8))),
		),
		("v8 get_zswap_state_root", Box::new(|| drop(host8::get_zswap_state_root(v8)))),
		("v8 get_ledger_state_root", Box::new(|| drop(host8::get_ledger_state_root(v8)))),
		(
			"v8 dust_generation_values",
			Box::new(|| drop(host8::dust_generation_values(v8, vec![NONCE]))),
		),
	]
}

/// A committed ledger-8 state large enough to be stored as nodes rather than inline in its key.
fn ledger_8_state() -> Vec<u8> {
	use base_crypto::{hash::HashOutput, time::Timestamp};
	use mn_ledger_8::{
		dust::{DustPublicKey, InitialNonce},
		structure::{CNightGeneratesDustActionType, CNightGeneratesDustEvent, SystemTransaction},
	};
	use transient_crypto::curve::Fr;

	let empty = {
		let ledger = Ledger8::new(mn_ledger_8::structure::LedgerState::new("undeployed"));
		let mut sp = default_storage::<Shared>().arena.alloc(ledger);
		let mut bytes = Vec::new();
		midnight_serialize::tagged_serialize(&sp.as_typed_key(), &mut bytes).unwrap();
		sp.persist();
		bytes
	};
	let event = CNightGeneratesDustEvent {
		value: 1_000,
		owner: DustPublicKey(Fr::from(7u64)),
		time: Timestamp::from_secs(TBLOCK),
		action: CNightGeneratesDustActionType::Create,
		nonce: InitialNonce(HashOutput(NONCE)),
	};
	let mut system_tx = Vec::new();
	midnight_serialize::tagged_serialize(
		&SystemTransaction::CNightGeneratesDustUpdate { events: vec![event] },
		&mut system_tx,
	)
	.unwrap();
	let ctx = BlockContext8 { tblock: TBLOCK, last_block_time: TBLOCK - 6, ..Default::default() };

	let mut onchain = BasicExternalities::default();
	onchain.register_extension(LedgerOnchainExt::default());
	// Block execution holds intra-block states only until the block ends, so commit this one
	// here, as a block's tip would be.
	sp_externalities::set_and_run_with_externalities(&mut onchain, || {
		let applied = host8::apply_cnight_system_transaction(&empty, &system_tx, ctx, 1)
			.unwrap()
			.state_root;
		let key: TypedArenaKey<Ledger8<Shared>, storage_sha2::Sha256> =
			midnight_serialize::tagged_deserialize(&mut &applied[..]).unwrap();
		default_storage::<Shared>().arena.get_lazy(&key).unwrap().persist();
		applied
	})
}

/// Reads at a committed state allocate. Outside block execution each read must be served by the
/// ephemeral storage, and none of its allocations may reach the shared write cache, where a
/// flush, including one running during the reads, would write them to disk.
#[test]
fn offchain_reads_leave_nothing_for_a_flush() {
	let dir = tempfile::tempdir().unwrap();
	let v9 = init_storage_paritydb_separate(dir.path(), UndeployedNetwork.genesis_state(), 1024);
	let v8 = ledger_8_state();
	default_storage::<Shared>().with_backend(|backend| backend.flush_all_changes_to_db());
	let reads = reads(&v9, &v8);

	let mut offchain = BasicExternalities::default();
	sp_externalities::set_and_run_with_externalities(&mut offchain, || {
		for (name, read) in &reads {
			let before = ephemeral_lookups();
			read();
			assert!(ephemeral_lookups() > before, "{name} was not served by the ephemeral storage");
			assert_eq!(pending_writes(), 0, "{name} left writes in the shared storage");
		}
	});

	let done = AtomicBool::new(false);
	let flushed = thread::scope(|scope| {
		let flusher = scope.spawn(|| {
			let mut flushed = 0;
			while !done.load(Ordering::Relaxed) {
				flushed += default_storage::<Shared>().with_backend(|backend| {
					let pending = backend.get_write_cache_len();
					backend.flush_all_changes_to_db();
					pending
				});
			}
			flushed
		});

		let mut offchain = BasicExternalities::default();
		sp_externalities::set_and_run_with_externalities(&mut offchain, || {
			for _ in 0..READS {
				for (_, read) in &reads {
					read();
				}
			}
		});
		done.store(true, Ordering::Relaxed);
		flusher.join().unwrap()
	});

	assert_eq!(flushed + pending_writes(), 0, "off-chain reads left writes for a flush");
}
