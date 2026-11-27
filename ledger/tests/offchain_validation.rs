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

//! Off-chain transaction validation against the shared ledger storage.
//!
//! A separate binary because `set_default_storage` is process-global and set-once, and because
//! the assertions inspect the shared write cache, which other tests would also fill.

use ledger_storage_ledger_8::{
	db::ParityDb,
	storage::{default_storage, try_get_default_storage},
};
use midnight_node_ledger::{
	ephemeral::EphemeralDb,
	host_api::ledger_9::ledger_9_bridge as host,
	ledger_9::{BlockContext, storage::init_storage_paritydb_separate},
};
use midnight_node_res::{
	networks::{MidnightNetwork, UndeployedNetwork},
	undeployed::transactions::DEPLOY_TX,
};
use midnight_primitives_ledger::LedgerOnchainExt;
use sp_state_machine::BasicExternalities;

/// Host calls without a `LedgerStorageExt` use the separate-db storage.
type Shared = ParityDb;

const RUNTIME_VERSION: u32 = 1;

fn pending_writes() -> usize {
	default_storage::<Shared>().with_backend(|backend| backend.get_write_cache_len())
}

/// Validating a transaction outside block execution leaves nothing in the shared write cache,
/// so a later flush writes nothing for it, even while its validation result stays cached.
/// Validation during block execution keeps using the shared storage.
#[test]
fn offchain_validation_leaves_no_pending_shared_writes() {
	let dir = tempfile::tempdir().unwrap();
	let genesis =
		init_storage_paritydb_separate(dir.path(), UndeployedNetwork.genesis_state(), 1024);
	let (tx, ctx) = midnight_ledger_unsafe_helpers::ledger_9::extract_tx_with_context(DEPLOY_TX);
	let ctx: BlockContext = ctx.into();
	assert_eq!(pending_writes(), 0);

	let mut offchain = BasicExternalities::default();
	sp_externalities::set_and_run_with_externalities(&mut offchain, || {
		host::validate_transaction(&genesis, &tx, ctx.clone(), RUNTIME_VERSION, u64::MAX).unwrap();
		host::validate_guaranteed_execution(&genesis, &tx, ctx.clone(), RUNTIME_VERSION).unwrap();
	});
	drop(offchain);
	assert_eq!(pending_writes(), 0, "off-chain validation left writes for the next flush");

	let (shared_before, ephemeral_before) = (lookups::<Shared>(), lookups::<EphemeralDb<Shared>>());
	let mut onchain = BasicExternalities::default();
	onchain.register_extension(LedgerOnchainExt::default());
	sp_externalities::set_and_run_with_externalities(&mut onchain, || {
		host::validate_guaranteed_execution(&genesis, &tx, ctx.clone(), RUNTIME_VERSION).unwrap();
	});
	assert!(lookups::<Shared>() > shared_before, "block execution no longer reads shared storage");
	assert_eq!(
		lookups::<EphemeralDb<Shared>>(),
		ephemeral_before,
		"block execution read the ephemeral storage"
	);
	assert_eq!(pending_writes(), 0, "the validation cache kept allocations for the next flush");
}

fn lookups<D: ledger_storage_ledger_8::db::DB>() -> usize {
	try_get_default_storage::<D>().map_or(0, |storage| {
		storage.with_backend(|backend| {
			let stats = backend.get_stats();
			stats.get_cache_hits + stats.get_cache_misses
		})
	})
}
