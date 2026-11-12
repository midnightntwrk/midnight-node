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

use ledger_storage_ledger_8::{db::ParityDb, storage::default_storage};
use midnight_node_ledger::{host_api::ledger_9::ledger_9_bridge as host, ledger_9::BlockContext};
use midnight_node_res::{
	networks::{MidnightNetwork, UndeployedNetwork},
	undeployed::transactions::DEPLOY_TX,
};
use midnight_primitives_ledger::LedgerOnchainExt;
use sp_state_machine::BasicExternalities;

/// The one storage instantiation host calls dispatch to: the ledger columns after Substrate's.
type Shared = ParityDb<
	storage_sha2::Sha256,
	ledger_storage_ledger_8::db::paritydb::OwnedDb,
	{ midnight_primitives_ledger::LedgerStorageExt::COLUMN_OFFSET },
>;

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
	let genesis = midnight_node_ledger::init_ledger_storage_standalone(
		dir.path(),
		UndeployedNetwork.genesis_state(),
		1024,
	);
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

	let mut onchain = BasicExternalities::default();
	onchain.register_extension(LedgerOnchainExt::default());
	sp_externalities::set_and_run_with_externalities(&mut onchain, || {
		host::validate_guaranteed_execution(&genesis, &tx, ctx.clone(), RUNTIME_VERSION).unwrap();
	});
	assert!(pending_writes() > 0, "block execution no longer validates in the shared storage");
}
