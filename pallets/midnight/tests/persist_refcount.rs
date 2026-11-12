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
//
// In `tests/` rather than `src/tests.rs` so it gets its own test binary, isolating the
// global ledger storage backend from other pallet tests.

#![allow(deprecated)]

use frame_support::{assert_ok, storage::with_storage_layer, traits::OnFinalize};
use midnight_node_ledger::{
	has_ledger_state, latest::storage::get_state_root_count, types::active_version::BlockContext,
};
use midnight_node_res::{
	networks::{MidnightNetwork, UndeployedNetwork},
	undeployed::transactions::{CHECK_TX, DEPLOY_TX, MAINTENANCE_TX, STORE_TX},
};
use pallet_midnight::{
	Call as MidnightCall,
	mock::{self, RuntimeOrigin, Test},
};
use sp_runtime::{
	DispatchError,
	traits::ValidateUnsigned,
	transaction_validity::{InvalidTransaction, TransactionSource, TransactionValidityError},
};

/// `LedgerApiError::NoLedgerState`'s wire code — what an unresolvable state key looks
/// like from the mempool.
const NO_LEDGER_STATE: u8 = 151;

fn init_ledger_state(block_context: BlockContext) {
	let path_buf = tempfile::tempdir().unwrap().keep();
	let state_key = midnight_node_ledger::latest::storage::init_storage_paritydb_separate(
		&path_buf,
		UndeployedNetwork.genesis_state(),
		1024 * 1024,
	);

	sp_tracing::try_init_simple();
	mock::Midnight::initialize_state(UndeployedNetwork.id(), &state_key);
	mock::System::set_block_number(1);
	mock::Timestamp::set_timestamp(block_context.tblock * 1000);
}

fn process_block(block_number: u64, block_context: BlockContext) {
	mock::Midnight::on_finalize(block_number);
	mock::System::set_block_number(block_number + 1);
	mock::Timestamp::set_timestamp(block_context.tblock * 1000);
}

fn current_state_key() -> Vec<u8> {
	pallet_midnight::StateKey::<Test>::get()
}

/// Walks several blocks and a storage rollback, asserting the keep-alive contract at each
/// transition.
///
/// One test fn on purpose: tests in a binary share the global ledger storage, so another
/// test alloc'ing the same genesis would skew the absolute refcount assertions.
#[test]
fn keep_alive_invariants() {
	let (deploy_tx, deploy_ctx) =
		midnight_ledger_unsafe_helpers::ledger_9::extract_tx_with_context(DEPLOY_TX);
	let (store_tx, store_ctx) =
		midnight_ledger_unsafe_helpers::ledger_9::extract_tx_with_context(STORE_TX);
	let (check_tx, check_ctx) =
		midnight_ledger_unsafe_helpers::ledger_9::extract_tx_with_context(CHECK_TX);
	let (maintenance_tx, maintenance_ctx) =
		midnight_ledger_unsafe_helpers::ledger_9::extract_tx_with_context(MAINTENANCE_TX);

	let deploy_call = MidnightCall::<Test>::send_mn_transaction { midnight_tx: deploy_tx.clone() };
	let store_call = MidnightCall::<Test>::send_mn_transaction { midnight_tx: store_tx.clone() };

	mock::new_test_ext().execute_with(|| {
		init_ledger_state(deploy_ctx.clone().into());

		let genesis_key = current_state_key();
		assert_eq!(
			get_state_root_count(&genesis_key),
			Some(1),
			"genesis is persisted at rc=1 by alloc_with_initial_state"
		);

		assert_ok!(<mock::Midnight as ValidateUnsigned>::validate_unsigned(
			TransactionSource::External,
			&deploy_call
		));
		assert_ok!(<mock::Midnight as ValidateUnsigned>::pre_dispatch(&deploy_call));
		assert_eq!(current_state_key(), genesis_key);
		assert_eq!(
			get_state_root_count(&genesis_key),
			Some(1),
			"validation must not change the tip's root count"
		);

		// Block 1: apply DEPLOY.
		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), deploy_tx));
		let post_deploy_key = current_state_key();
		assert_ne!(post_deploy_key, genesis_key);
		assert_eq!(
			get_state_root_count(&post_deploy_key),
			None,
			"an intra-block intermediate must never be a GC root"
		);
		assert!(
			has_ledger_state(false, &post_deploy_key),
			"the intermediate must stay addressable until the post-block update"
		);
		assert_eq!(
			get_state_root_count(&genesis_key),
			Some(1),
			"the persisted genesis tip must be untouched by apply_transaction"
		);

		// These catch a broken keep-alive: each reads through `get_ledger` with the raw
		// intermediate key, mid-block.
		assert_ok!(mock::Midnight::get_ledger_state_root());
		assert_ok!(mock::Midnight::get_transaction_cost(&store_tx));
		// These fixtures' dust proofs don't survive `validate_unsigned`'s block-context
		// skew, so only assert the state resolved: rejection is fine, `NoLedgerState` isn't.
		for outcome in [
			<mock::Midnight as ValidateUnsigned>::validate_unsigned(
				TransactionSource::External,
				&store_call,
			)
			.map(|_| ()),
			<mock::Midnight as ValidateUnsigned>::pre_dispatch(&store_call),
		] {
			assert_ne!(
				outcome,
				Err(TransactionValidityError::Invalid(InvalidTransaction::Custom(NO_LEDGER_STATE))),
				"validation must resolve the intra-block intermediate tip"
			);
		}

		// Finalize block 1: `apply_post_block_update` only resolves the intermediate because
		// `LedgerStatesExt` held it, and then drops it.
		process_block(1, store_ctx.clone().into());
		let post_block_1_key = current_state_key();
		assert_ne!(post_block_1_key, post_deploy_key);
		assert_eq!(
			get_state_root_count(&post_block_1_key),
			Some(1),
			"the post-block tip is the one state per block that is persisted"
		);
		// `on_finalize` flushed storage. A released intermediate must be gone from the arena,
		// not merely unrooted, or that flush wrote it to disk as garbage.
		assert!(
			!midnight_node_ledger::has_ledger_state(false, &post_deploy_key),
			"a released intermediate must not survive the block's storage flush"
		);
		assert!(
			midnight_node_ledger::has_ledger_state(false, &post_block_1_key),
			"the persisted post-block tip must survive it"
		);

		// Block 2: apply STORE against the persisted tip.
		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), store_tx));
		let post_store_key = current_state_key();
		assert_eq!(get_state_root_count(&post_store_key), None, "intermediate, so not rooted");
		assert!(has_ledger_state(false, &post_store_key));
		assert_eq!(
			get_state_root_count(&post_block_1_key),
			Some(1),
			"the previous block's persisted tip must not be unpersisted by the next apply"
		);
		process_block(2, check_ctx.clone().into());
		assert!(!has_ledger_state(false, &post_store_key));

		// Block 3: apply CHECK, then finalize into the parent of the fork below.
		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), check_tx));
		let post_check_key = current_state_key();
		assert_eq!(get_state_root_count(&post_check_key), None);
		assert!(has_ledger_state(false, &post_check_key));
		assert_eq!(get_state_root_count(&genesis_key), Some(1), "genesis still rooted");
		assert_eq!(
			get_state_root_count(&post_block_1_key),
			Some(1),
			"older post-block tips stay rooted for history"
		);
		process_block(3, maintenance_ctx.clone().into());
		assert!(!has_ledger_state(false, &post_check_key));

		// --- Storage rollback ---
		//
		// A layer that fails after a successful apply reverts `StateKey`, but not the ledger.
		let fork_parent = current_state_key();
		let rolled_back = with_storage_layer(|| {
			mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), maintenance_tx.clone())?;
			Err::<(), _>(DispatchError::Other("roll back after a successful apply"))
		});
		assert!(rolled_back.is_err());
		assert_eq!(current_state_key(), fork_parent, "the rollback reverts StateKey");

		// Re-applying on the reverted key reproduces the rolled-back state, now held twice.
		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), maintenance_tx));
		let replayed_key = current_state_key();
		assert_ne!(replayed_key, fork_parent);
		assert_eq!(get_state_root_count(&replayed_key), None, "still never rooted");

		mock::Midnight::on_finalize(4);
		assert!(
			!has_ledger_state(false, &replayed_key),
			"both holds must be dropped before the block's storage flush"
		);
		assert_eq!(
			get_state_root_count(&fork_parent),
			Some(1),
			"the persisted parent must survive the rollback"
		);
	});
}
