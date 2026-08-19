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
// Integration test for the LedgerStateKey Anchored/Transient persist contract.
// Lives in `tests/` rather than `src/tests.rs` so it runs in its own test
// binary, isolating the global ledger storage backend from other pallet tests.

use frame_support::{assert_ok, traits::OnFinalize};
use midnight_node_ledger::types::{LedgerStateKey, active_version::BlockContext};
use midnight_node_res::{
	networks::{MidnightNetwork, UndeployedNetwork},
	undeployed::transactions::{CHECK_TX, DEPLOY_TX, STORE_TX},
};
use pallet_midnight::{
	Call as MidnightCall,
	mock::{self, RuntimeOrigin, Test},
};
use sp_runtime::{traits::ValidateUnsigned, transaction_validity::TransactionSource};

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

fn root_count(state_key: &LedgerStateKey) -> Option<u32> {
	midnight_node_ledger::latest::storage::get_state_root_count(state_key.bytes())
}

fn tagged_pin_count(state_key: &LedgerStateKey) -> Option<u32> {
	midnight_node_ledger::latest::storage::get_tagged_pin_count(state_key.bytes())
}

fn current_state_key() -> LedgerStateKey {
	pallet_midnight::StateKey::<Test>::get()
}

/// Walks an entire block lifecycle and asserts the LedgerStateKey persist
/// contract at every transition:
///   - `Anchored` states (genesis, post-block tips) are persisted at rc=1 as a
///     raw ledger-hash GC root and are NEVER unpersisted by subsequent Bridge
///     calls — so they survive sibling forks and remain queryable for history.
///     The native import path later swaps that raw pin for a wrapper tagged
///     with the block hash (exercised at the end of this test).
///   - `Transient` states (intra-block intermediates) are persisted at rc=1
///     and unpersisted to rc=0 by their successor.
///
/// Also guards that `validate_unsigned` and `pre_dispatch` are read-only —
/// they must not change the refcount of the current tip, since the next
/// dispatch needs to load that same state.
///
/// Single test fn: this integration test binary's tests share the global
/// default ledger storage; absolute refcount assertions would be polluted by
/// another test in the same binary alloc'ing the same genesis. Sequencing
/// every scenario inside one test keeps the assertions precise.
#[test]
fn persist_refcount_invariants() {
	let (deploy_tx, deploy_ctx) =
		midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(DEPLOY_TX);
	let (store_tx, store_ctx) =
		midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(STORE_TX);
	let (check_tx, check_ctx) =
		midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(CHECK_TX);

	let deploy_call = MidnightCall::<Test>::send_mn_transaction { midnight_tx: deploy_tx.clone() };

	mock::new_test_ext().execute_with(|| {
		init_ledger_state(deploy_ctx.clone().into());

		let genesis_key = current_state_key();
		assert!(matches!(genesis_key, LedgerStateKey::Anchored(_)), "genesis stored as Anchored");
		assert_eq!(root_count(&genesis_key), Some(1), "genesis persisted at rc=1");
		assert_eq!(
			tagged_pin_count(&genesis_key),
			None,
			"genesis is a raw persist until native hash-tagging"
		);

		// Read-only guard 1: validate_unsigned + pre_dispatch against the genesis
		// Anchored tip. DEPLOY is valid here.
		assert_ok!(<mock::Midnight as ValidateUnsigned>::validate_unsigned(
			TransactionSource::External,
			&deploy_call
		));
		assert_eq!(current_state_key(), genesis_key);
		assert_eq!(root_count(&genesis_key), Some(1), "validate_unsigned must not change tip rc");
		assert_ok!(<mock::Midnight as ValidateUnsigned>::pre_dispatch(&deploy_call));
		assert_eq!(current_state_key(), genesis_key);
		assert_eq!(root_count(&genesis_key), Some(1), "pre_dispatch must not change tip rc");

		// Block 1: apply DEPLOY. Genesis is Anchored — Bridge must NOT unpersist
		// it. (This is the property that makes sibling forks safe: a second fork
		// applying its own first-tx on the same Anchored parent leaves rc=1.)
		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), deploy_tx));
		let post_deploy_key = current_state_key();
		assert!(
			matches!(post_deploy_key, LedgerStateKey::Transient(_)),
			"apply_transaction returns Transient"
		);
		assert_ne!(post_deploy_key, genesis_key);
		assert_eq!(root_count(&post_deploy_key), Some(1), "new Transient state at rc=1");
		assert_eq!(
			root_count(&genesis_key),
			Some(1),
			"Anchored genesis must NOT be unpersisted by apply_transaction"
		);

		// Finalize block 1. post_block_update unpersists the Transient predecessor
		// (post_deploy → rc=0) and returns Anchored at rc=1 (raw persist).
		process_block(2, store_ctx.clone().into());
		let post_block_1_key = current_state_key();
		assert!(
			matches!(post_block_1_key, LedgerStateKey::Anchored(_)),
			"post_block_update returns Anchored"
		);
		assert_ne!(post_block_1_key, post_deploy_key);
		assert_eq!(
			root_count(&post_deploy_key),
			None,
			"Transient last-apply output unrooted by post_block_update"
		);
		assert_eq!(
			root_count(&post_block_1_key),
			Some(1),
			"Anchored post-block state persisted at rc=1, retained for history"
		);
		assert_eq!(
			tagged_pin_count(&post_block_1_key),
			None,
			"post-block tip is a raw persist until native hash-tagging"
		);

		// Read-only guard 2: validate against the Anchored post-block tip. DEPLOY
		// fails here (contract already deployed) but rc must be untouched on
		// either path.
		let _ = <mock::Midnight as ValidateUnsigned>::validate_unsigned(
			TransactionSource::External,
			&deploy_call,
		);
		assert_eq!(
			root_count(&post_block_1_key),
			Some(1),
			"validate_unsigned against post-block tip must not change rc"
		);
		let _ = <mock::Midnight as ValidateUnsigned>::pre_dispatch(&deploy_call);
		assert_eq!(
			root_count(&post_block_1_key),
			Some(1),
			"pre_dispatch against post-block tip must not change rc"
		);

		// Block 2: apply STORE. post_block_1 is Anchored — must NOT be unpersisted.
		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), store_tx));
		let post_store_key = current_state_key();
		assert_eq!(
			root_count(&post_block_1_key),
			Some(1),
			"Anchored post-block-1 must NOT be unpersisted by next block's first apply"
		);
		assert_eq!(root_count(&post_store_key), Some(1), "new Transient state at rc=1");

		// Finalize block 2.
		process_block(3, check_ctx.clone().into());
		let post_block_2_key = current_state_key();
		assert_eq!(
			root_count(&post_store_key),
			None,
			"Transient last-apply output of block 2 unrooted by post_block_update"
		);
		assert_eq!(
			root_count(&post_block_2_key),
			Some(1),
			"Anchored post-block-2 persisted at rc=1"
		);
		assert_eq!(
			root_count(&post_block_1_key),
			Some(1),
			"prior Anchored post-block-1 unaffected, still at rc=1"
		);

		// Block 3: apply CHECK and confirm Anchored states remain untouched.
		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), check_tx));
		assert_eq!(
			root_count(&post_block_2_key),
			Some(1),
			"Anchored post-block-2 unaffected by block 3's first apply"
		);
		assert_eq!(
			root_count(&post_block_1_key),
			Some(1),
			"Anchored post-block-1 still at rc=1"
		);
		assert_eq!(root_count(&genesis_key), Some(1), "Anchored genesis still at rc=1");

		// Native import path: swap each Anchored raw pin for a hash-tagged
		// wrapper. Distinct hashes are independent roots; the same hash is
		// idempotent.
		assert_eq!(
			midnight_node_ledger::tag_anchored_tip(genesis_key.bytes(), &[0x00; 32]),
			Some(true)
		);
		assert_eq!(root_count(&genesis_key), None);
		assert_eq!(tagged_pin_count(&genesis_key), Some(1));
		assert_eq!(
			midnight_node_ledger::tag_anchored_tip(genesis_key.bytes(), &[0x00; 32]),
			Some(false),
			"hash tagging is idempotent"
		);

		assert_eq!(
			midnight_node_ledger::tag_anchored_tip(post_block_1_key.bytes(), &[0x01; 32]),
			Some(true)
		);
		assert_eq!(
			midnight_node_ledger::tag_anchored_tip(post_block_2_key.bytes(), &[0x02; 32]),
			Some(true)
		);
		assert_eq!(root_count(&post_block_1_key), None);
		assert_eq!(root_count(&post_block_2_key), None);
		assert_eq!(tagged_pin_count(&post_block_1_key), Some(1));
		assert_eq!(tagged_pin_count(&post_block_2_key), Some(1));
		assert_eq!(tagged_pin_count(&genesis_key), Some(1));
	});
}
