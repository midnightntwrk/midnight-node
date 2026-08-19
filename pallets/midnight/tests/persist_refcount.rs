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
// Integration test for the ledger-state keep-alive contract: intra-block
// intermediates are kept addressable by the global keep-alive cache and are
// never made GC roots; only the post-block tip is persisted.
//
// Lives in `tests/` rather than `src/tests.rs` so it runs in its own test
// binary, isolating the global ledger storage backend from other pallet tests.

use frame_support::{assert_ok, traits::OnFinalize};
use midnight_node_ledger::{
	latest::storage::{get_state_root_count, transient_state_is_retained},
	types::active_version::BlockContext,
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

/// Walks a whole block lifecycle and asserts the keep-alive contract at every
/// transition:
///   - intra-block intermediates are NEVER GC roots (`get_state_root_count ==
///     None`) and stay addressable only because the transient keep-alive cache
///     holds their `Sp`,
///   - the successor call releases its predecessor, so the transient cache is
///     empty once the block is finalized (the unit-test mirror of the
///     `ledger_state_cache_size{cache_type="transient"}` gauge),
///   - post-block tips are persisted at rc=1 and are never unpersisted, so they
///     survive sibling forks and stay queryable for history,
///   - the keep-alive is real: reads and validation against an intermediate tip
///     mid-block succeed,
///   - the transient keep-alive is refcounted, so two executions applying the
///     same transaction to the same parent (sibling forks, or authoring
///     alongside import) don't drop each other's state.
///
/// Single test fn: this integration test binary's tests share the global default
/// ledger storage; absolute refcount assertions would be polluted by another
/// test in the same binary alloc'ing the same genesis. Sequencing every scenario
/// inside one test keeps the assertions precise.
#[test]
fn keep_alive_invariants() {
	let (deploy_tx, deploy_ctx) =
		midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(DEPLOY_TX);
	let (store_tx, store_ctx) =
		midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(STORE_TX);
	let (check_tx, check_ctx) =
		midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(CHECK_TX);
	let (maintenance_tx, maintenance_ctx) =
		midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(MAINTENANCE_TX);

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
		assert!(
			!transient_state_is_retained(&genesis_key),
			"genesis is anchored, not a transient intermediate"
		);

		// Read-only guard: validate_unsigned + pre_dispatch against the genesis tip.
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

		// Block 1: apply DEPLOY. The result is kept alive, not rooted; genesis is
		// persisted so it must survive untouched.
		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), deploy_tx));
		let post_deploy_key = current_state_key();
		assert_ne!(post_deploy_key, genesis_key);
		assert_eq!(
			get_state_root_count(&post_deploy_key),
			None,
			"an intra-block intermediate must never be a GC root"
		);
		assert!(
			transient_state_is_retained(&post_deploy_key),
			"the intermediate must be retained between its apply and its successor"
		);
		assert_eq!(
			get_state_root_count(&genesis_key),
			Some(1),
			"the persisted genesis tip must be untouched by apply_transaction"
		);

		// The assertions that actually catch a broken keep-alive: read *through* the
		// intermediate tip mid-block. Every one of these paths goes through
		// `get_ledger` with the raw intermediate key.
		assert_ok!(mock::Midnight::get_ledger_state_root());
		assert_ok!(mock::Midnight::get_transaction_cost(&store_tx));
		// The mempool paths too. These fixtures' dust spend proofs don't survive the
		// skew `validate_unsigned` applies to the block context, so the assertion is
		// that the *ledger state resolved* — the tx may then be rejected on its
		// merits, but never with `NoLedgerState` (code 151).
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

		// Finalize block 1. `apply_post_block_update` is the successor call that
		// consumes the intermediate: it only resolves because the keep-alive held it,
		// and it releases it, leaving the transient cache empty — the unit-test mirror
		// of the gauge reading 0 at block end.
		process_block(1, store_ctx.clone().into());
		let post_block_1_key = current_state_key();
		assert_ne!(post_block_1_key, post_deploy_key);
		assert!(
			!transient_state_is_retained(&post_deploy_key),
			"the block's last intermediate must be released by apply_post_block_update"
		);
		assert!(
			!transient_state_is_retained(&post_block_1_key),
			"the post-block tip is anchored, so it is never in the transient cache"
		);
		assert_eq!(
			get_state_root_count(&post_block_1_key),
			Some(1),
			"the post-block tip is the one state per block that is persisted"
		);
		// `flush_storage` ran as part of `on_finalize`. A released intermediate must be
		// gone from the arena entirely — not merely unrooted — or its nodes would have
		// been written to disk by that flush as unreferenced garbage.
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
		assert!(transient_state_is_retained(&post_store_key));
		assert_eq!(
			get_state_root_count(&post_block_1_key),
			Some(1),
			"the previous block's persisted tip must not be unpersisted by the next apply"
		);
		process_block(2, check_ctx.clone().into());
		assert!(!transient_state_is_retained(&post_store_key));

		// Block 3: apply CHECK, then finalize into the parent of the fork below.
		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), check_tx));
		let post_check_key = current_state_key();
		assert_eq!(get_state_root_count(&post_check_key), None);
		assert!(transient_state_is_retained(&post_check_key));
		assert_eq!(get_state_root_count(&genesis_key), Some(1), "genesis still rooted");
		assert_eq!(
			get_state_root_count(&post_block_1_key),
			Some(1),
			"older post-block tips stay rooted for history"
		);
		process_block(3, maintenance_ctx.clone().into());
		assert!(!transient_state_is_retained(&post_check_key));

		// --- The refcount check ---
		//
		// Two executions can be in flight at once (authoring alongside import), and
		// two forks off the same parent applying the same transaction produce the
		// *same* content hash. Without a refcount the first release would drop the
		// other execution's keep-alive and it would fail with `NoLedgerState`.
		let fork_parent = current_state_key();

		assert_ok!(mock::Midnight::send_mn_transaction(
			RuntimeOrigin::none(),
			maintenance_tx.clone()
		));
		let forked_key = current_state_key();
		assert!(transient_state_is_retained(&forked_key));

		// Rewind to the shared parent and replay: the sibling fork's execution.
		pallet_midnight::StateKey::<Test>::put(fork_parent.clone());
		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), maintenance_tx));
		assert_eq!(
			current_state_key(),
			forked_key,
			"the same transaction on the same parent must produce the same state key"
		);
		assert_eq!(get_state_root_count(&forked_key), None, "still never rooted");

		// One fork finalizes: that releases the shared intermediate once.
		mock::Midnight::on_finalize(4);
		assert!(
			transient_state_is_retained(&forked_key),
			"the other execution's claim must keep the state alive"
		);

		// ...and now the other.
		pallet_midnight::StateKey::<Test>::put(forked_key.clone());
		mock::Midnight::on_finalize(4);
		assert!(
			!transient_state_is_retained(&forked_key),
			"the last release must drop the keep-alive"
		);
		assert_eq!(
			get_state_root_count(&fork_parent),
			Some(1),
			"the shared parent is persisted and must survive both forks"
		);
	});
}
