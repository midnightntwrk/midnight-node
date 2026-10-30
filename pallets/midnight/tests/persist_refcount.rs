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

use frame_support::{assert_ok, traits::OnFinalize};
use midnight_node_ledger::{
	latest::storage::{get_state_root_count, tagged_pin_count, transient_state_is_retained},
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
	let state_key = midnight_node_ledger::init_ledger_storage_standalone(
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

/// Walks several blocks and a sibling fork, asserting the keep-alive contract at each
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

	let replay_deploy_tx = deploy_tx.clone();
	let deploy_call = MidnightCall::<Test>::send_mn_transaction { midnight_tx: deploy_tx.clone() };
	let store_call = MidnightCall::<Test>::send_mn_transaction { midnight_tx: store_tx.clone() };

	mock::new_test_ext().execute_with(|| {
		init_ledger_state(deploy_ctx.clone().into());

		let genesis_key = current_state_key();
		assert_eq!(
			get_state_root_count(&genesis_key),
			None,
			"must not be a raw GC root, it is pinned by a tagged root"
		);
		assert_eq!(
			tagged_pin_count(&genesis_key),
			Some(1),
			"genesis is pinned by a tagged root by alloc_with_initial_state"
		);
		assert!(
			!transient_state_is_retained(&genesis_key),
			"genesis is a persisted tip, not a transient intermediate"
		);

		assert_ok!(<mock::Midnight as ValidateUnsigned>::validate_unsigned(
			TransactionSource::External,
			&deploy_call
		));
		assert_ok!(<mock::Midnight as ValidateUnsigned>::pre_dispatch(&deploy_call));
		assert_eq!(current_state_key(), genesis_key);
		assert_eq!(
			get_state_root_count(&genesis_key),
			None,
			"must not be a raw GC root, it is pinned by a tagged root"
		);
		assert_eq!(
			tagged_pin_count(&genesis_key),
			Some(1),
			"validation must not change the tip's pin count"
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
			transient_state_is_retained(&post_deploy_key),
			"the intermediate must be retained between its apply and its successor"
		);
		assert_eq!(
			get_state_root_count(&genesis_key),
			None,
			"must not be a raw GC root, it is pinned by a tagged root"
		);
		assert_eq!(
			tagged_pin_count(&genesis_key),
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
		// the keep-alive held it, and releases it.
		process_block(1, store_ctx.clone().into());
		let post_block_1_key = current_state_key();
		assert_ne!(post_block_1_key, post_deploy_key);
		assert!(
			!transient_state_is_retained(&post_deploy_key),
			"the block's last intermediate must be released by apply_post_block_update"
		);
		assert!(
			!transient_state_is_retained(&post_block_1_key),
			"the post-block tip is persisted, so it is never in the transient cache"
		);
		assert_eq!(
			get_state_root_count(&post_block_1_key),
			None,
			"must not be a raw GC root, it is pinned by a tagged root"
		);
		assert_eq!(
			tagged_pin_count(&post_block_1_key),
			Some(1),
			"the post-block tip is the one state per block that is persisted"
		);
		// `on_finalize` flushed storage. A released intermediate must be gone from the arena,
		// not merely unrooted, or that flush wrote it to disk as garbage.
		assert!(
			!midnight_node_ledger::has_ledger_state(&post_deploy_key),
			"a released intermediate must not survive the block's storage flush"
		);
		assert!(
			midnight_node_ledger::has_ledger_state(&post_block_1_key),
			"the persisted post-block tip must survive it"
		);

		// Block 2: apply STORE against the persisted tip.
		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), store_tx));
		let post_store_key = current_state_key();
		assert_eq!(get_state_root_count(&post_store_key), None, "intermediate, so not rooted");
		assert!(transient_state_is_retained(&post_store_key));
		assert_eq!(
			get_state_root_count(&post_block_1_key),
			None,
			"must not be a raw GC root, it is pinned by a tagged root"
		);
		assert_eq!(
			tagged_pin_count(&post_block_1_key),
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
		assert_eq!(get_state_root_count(&genesis_key), None);
		assert_eq!(tagged_pin_count(&genesis_key), Some(1), "genesis still pinned");
		assert_eq!(
			get_state_root_count(&post_block_1_key),
			None,
			"must not be a raw GC root, it is pinned by a tagged root"
		);
		assert_eq!(
			tagged_pin_count(&post_block_1_key),
			Some(1),
			"older post-block tips stay pinned for history"
		);
		process_block(3, maintenance_ctx.clone().into());
		assert!(!transient_state_is_retained(&post_check_key));

		// --- The release path on a real post-block tip ---
		//
		// Replay block 1 on a second branch: rewind to genesis and finalize the same
		// block again. Same parent context, hence the same tag, hence the same wrapper:
		// the count goes 1 -> 2 and no second wrapper appears. The returned state key
		// stays the tip's own (inner) key.
		//
		// This runs before the maintenance fork below on purpose: see the note there.
		let tip3 = current_state_key();
		assert_eq!(tagged_pin_count(&post_block_1_key), Some(1), "one branch so far");
		pallet_midnight::StateKey::<Test>::put(genesis_key.clone());
		mock::System::set_block_number(1);
		mock::Timestamp::set_timestamp(BlockContext::from(deploy_ctx.clone()).tblock * 1000);
		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), replay_deploy_tx));
		mock::Midnight::on_finalize(1);
		assert_eq!(
			current_state_key(),
			post_block_1_key,
			"the same block on both branches yields the same tip key"
		);
		assert_eq!(get_state_root_count(&post_block_1_key), None, "never a raw root");
		assert_eq!(
			tagged_pin_count(&post_block_1_key),
			Some(2),
			"same-parent forks share one wrapper, persisted once per branch"
		);
		// Back on the main chain.
		pallet_midnight::StateKey::<Test>::put(tip3.clone());
		mock::System::set_block_number(4);
		mock::Timestamp::set_timestamp(BlockContext::from(maintenance_ctx.clone()).tblock * 1000);

		// The only wrapper persisted twice is block 1's; find it through the DB-only scan.
		let (roots, _hold) = midnight_node_ledger::gc::tagged_roots();
		let mut twice: Vec<_> = roots.into_iter().filter(|r| r.count == 2).collect();
		assert_eq!(twice.len(), 1, "exactly one wrapper is persisted twice: {twice:?}");
		let wrapper = twice.remove(0);
		assert_eq!(wrapper.tag.len(), 32, "the tag is the 32-byte parent block hash");

		let (released, _hold) =
			midnight_node_ledger::gc::release_to_zero(&[(wrapper.wrapper.clone(), wrapper.count)]);
		assert_eq!(released, 1);
		// Staged only; durable at the next flush, exactly as on a block boundary.
		midnight_node_ledger::latest::storage::flush_default_storage_for_test();
		assert_eq!(
			tagged_pin_count(&post_block_1_key),
			None,
			"released to zero: no wrapper pins the tip any more"
		);
		assert!(
			!midnight_node_ledger::gc::tagged_roots()
				.0
				.iter()
				.any(|r| r.wrapper == wrapper.wrapper),
			"the released wrapper is gone (a replay of the list has nothing to do)"
		);
		assert_eq!(tagged_pin_count(&genesis_key), Some(1), "other wrappers are untouched");
		assert_eq!(tagged_pin_count(&tip3), Some(1));

		// Drop the keep-alive caches so only the arena decides,
		// then sweep. A slice is a no-op on a dirty write cache, so flush and retry.
		midnight_node_ledger::latest::storage::clear_state_caches_for_test();
		let mut reclaimed = false;
		for _ in 0..200 {
			midnight_node_ledger::latest::storage::flush_default_storage_for_test();
			midnight_node_ledger::gc::collect_garbage(std::time::Duration::from_millis(250))
				.expect("storage initialised");
			if !midnight_node_ledger::has_ledger_state(&post_block_1_key) {
				reclaimed = true;
				break;
			}
		}
		assert!(reclaimed, "the released tip must be swept from the arena");
		assert!(
			midnight_node_ledger::has_ledger_state(&tip3),
			"a still-pinned tip survives the sweep"
		);
		assert!(midnight_node_ledger::has_ledger_state(&genesis_key));

		// --- The refcount check ---
		//
		// Sibling executions applying the same tx to the same parent share one content
		// hash; each release must drop only its own claim.
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
		let fork_tip = current_state_key();
		assert_eq!(get_state_root_count(&fork_tip), None, "tip is never a raw root");
		assert_eq!(tagged_pin_count(&fork_tip), Some(1), "one branch finalized: one pin");
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
			None,
			"must not be a raw GC root, it is pinned by a tagged root"
		);
		assert_eq!(
			tagged_pin_count(&fork_parent),
			Some(1),
			"the shared parent is persisted and must survive both forks"
		);
		assert_eq!(current_state_key(), fork_tip, "both branches produce the same tip key");
		assert_eq!(
			tagged_pin_count(&fork_tip),
			Some(2),
			"the same block on the second branch shares the wrapper: count 2"
		);
	});
}
