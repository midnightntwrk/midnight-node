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

// grcov-excl-start
#![allow(deprecated)]

use super::*;
use crate::{
	Call as MidnightCall, mock,
	mock::{RuntimeOrigin, Test},
};
use assert_matches::assert_matches;
use frame_support::{
	assert_err, assert_ok,
	pallet_prelude::Weight,
	traits::{OnFinalize, OnInitialize},
};
use frame_system::RawOrigin;
use midnight_node_ledger::types::{
	LedgerEvent, LedgerEventSource,
	active_version::{
		BlockContext, DeserializationError, LedgerApiError, MalformedError, TransactionError,
	},
};
use midnight_node_res::{
	networks::{MidnightNetwork, UndeployedNetwork},
	undeployed::transactions::{
		CHECK_TX, CONTRACT_ADDR, DEPLOY_TX, MAINTENANCE_TX, STORE_TX, ZSWAP_TX,
	},
};
use parity_scale_codec::Encode;
use sp_runtime::{
	traits::ValidateUnsigned,
	transaction_validity::{InvalidTransaction, TransactionSource, TransactionValidityError},
};
use test_log::test;

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

/// Drives the start of a block the way a real one is built: `on_initialize` records the parent
/// block's timestamp (which becomes the ledger's `last_block_time`) while `pallet_timestamp`
/// still holds it, and only then does the timestamp inherent set the block's own.
fn begin_block(block_number: u64, parent_ts: u64, block_ts: u64) {
	mock::System::set_block_number(block_number);
	mock::Timestamp::set_timestamp(parent_ts * 1000);
	<mock::Midnight as OnInitialize<u64>>::on_initialize(block_number);
	mock::Timestamp::set_timestamp(block_ts * 1000);
}

/// The correction's fixed offset (`slot_duration_secs * (1 + MaxSkippedSlots)`), hardcoded in
/// `midnight-node-ledger`. The tests below subtract it from the parent timestamp they want the
/// correction to *produce*; the arithmetic itself is covered by the `well_formed_tblock` unit
/// tests in that crate.
const TBLOCK_CORRECTION_OFFSET_SECS: u64 = 12;

fn send_mn_transaction(tx: Vec<u8>) -> sp_runtime::DispatchResult {
	mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), tx)
}

/// A block timestamp shortly after `block_context`'s, distinct on every call.
///
/// The strict transaction-validation cache is a process-global static keyed by
/// `(ledger state hash, tx hash, block timestamp)`, and every `tblock_correction` test below
/// validates the same fixture against the same genesis state. Handing out a distinct block
/// timestamp per assertion keeps them from serving each other's cached `well_formed` results —
/// which would make an assertion pass without ever running the code it is testing. The
/// timestamps stay well inside the fixture's validity window.
fn uncached_block_ts(block_context: &BlockContext) -> u64 {
	static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(101);
	block_context.tblock + NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[test]
fn test_send_mn_transaction() {
	mock::new_test_ext().execute_with(|| {
		let (tx, block_context) =
			midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(DEPLOY_TX);
		init_ledger_state(block_context.into());

		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), tx));

		// Check emitted events: ContractDeploy first, LedgerEvent(s) in between, TxApplied last.
		let events = mock::midnight_events();
		assert_matches!(events[0], Event::ContractDeploy(_));
		assert_matches!(events.last().unwrap(), Event::TxApplied(_));
		assert!(events.iter().any(|e| matches!(e, Event::LedgerEvent(_))));
	})
}

/// PR1849-TC-01: one `LedgerEvent` deposited per ledger event, all carrying the tx hash.
#[test]
fn test_send_mn_transaction_deposits_ledger_events() {
	mock::new_test_ext().execute_with(|| {
		let (tx, block_context) =
			midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(DEPLOY_TX);
		init_ledger_state(block_context.into());

		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), tx));

		let events = mock::midnight_events();
		let ledger_events: Vec<_> = events
			.iter()
			.filter_map(|e| if let Event::LedgerEvent(le) = e { Some(le) } else { None })
			.collect();
		assert!(!ledger_events.is_empty(), "deploy should emit at least one ledger event");

		let tx_hash = events
			.iter()
			.find_map(|e| if let Event::TxApplied(d) = e { Some(d.tx_hash) } else { None })
			.expect("TxApplied present");
		for le in &ledger_events {
			assert_eq!(le.source.transaction_hash, tx_hash);
		}
	})
}

/// PR1849-TC-15: a failed transaction deposits no `LedgerEvent`.
#[test]
fn test_failed_transaction_deposits_no_ledger_event() {
	mock::new_test_ext().execute_with(|| {
		// STORE_TX with no prior deploy fails on the guaranteed part.
		let (tx, block_context) =
			midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(STORE_TX);
		init_ledger_state(block_context.into());

		let error: sp_runtime::DispatchError = Error::<Test>::Transaction(
			TransactionError::Malformed(MalformedError::ContractNotPresent),
		)
		.into();
		assert_err!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), tx), error);

		assert!(mock::midnight_events().iter().all(|e| !matches!(e, Event::LedgerEvent(_))));
	})
}

/// PR1849-TC-17: dry-run validation (`pre_dispatch`) deposits no `LedgerEvent`.
#[test]
fn test_pre_dispatch_deposits_no_ledger_event() {
	let (tx, block_context) =
		midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(DEPLOY_TX);
	let call = MidnightCall::send_mn_transaction { midnight_tx: tx };
	mock::new_test_ext().execute_with(|| {
		init_ledger_state(block_context.into());

		assert_ok!(<mock::Midnight as ValidateUnsigned>::pre_dispatch(&call));

		assert!(mock::midnight_events().iter().all(|e| !matches!(e, Event::LedgerEvent(_))));
	})
}

/// PR1849-TC-11: `LedgerEvent` is appended as the last variant (index 8);
/// existing variant indices are unchanged.
#[test]
fn ledger_event_is_last_variant() {
	let ledger_event = Event::LedgerEvent(LedgerEvent {
		source: LedgerEventSource {
			transaction_hash: [0u8; 32],
			logical_segment: 0,
			physical_segment: 0,
		},
		content_tagged_bytes: alloc::vec::Vec::new(),
	});
	assert_eq!(ledger_event.encode()[0], 8);
}

#[test]
fn test_send_mn_transaction_malformed_tx() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state(BlockContext::default());

		let bytes = vec![1, 2, 3];
		let error: sp_runtime::DispatchError =
			Error::<Test>::Deserialization(DeserializationError::Transaction).into();
		assert_err!(
			mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), bytes.clone()),
			error
		);

		// Check emitted events
		assert!(mock::midnight_events().is_empty());
	})
}

#[test]
fn test_send_mn_transaction_invalid_tx() {
	mock::new_test_ext().execute_with(|| {
		let (tx, block_context) =
			midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(STORE_TX);
		init_ledger_state(block_context.into());

		let error: sp_runtime::DispatchError = Error::<Test>::Transaction(
			TransactionError::Malformed(MalformedError::ContractNotPresent),
		)
		.into();
		assert_err!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), tx), error);

		// Check emitted events
		assert!(mock::midnight_events().is_empty());
	})
}

#[test]
fn test_get_contract_state() {
	mock::new_test_ext().execute_with(|| {
		let (tx_deploy, block_context_deploy) =
			midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(DEPLOY_TX);
		let (tx_store, block_context_store) =
			midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(STORE_TX);
		let (tx_check, block_context_check) =
			midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(CHECK_TX);
		let (tx_maintenance, block_context_maintenance) =
			midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(MAINTENANCE_TX);

		init_ledger_state(block_context_deploy.into());

		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), tx_deploy));
		process_block(2, block_context_store.into());

		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), tx_store));
		process_block(3, block_context_check.into());

		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), tx_check));
		process_block(4, block_context_maintenance.into());

		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), tx_maintenance));

		let addr = hex::decode(CONTRACT_ADDR).expect("Address should be a valid hex code");

		let result = mock::Midnight::get_contract_state(&addr);
		assert!(result.is_ok(), "Failed calling `get_contract_state`");
	})
}

#[test]
fn test_get_contract_state_not_present() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state(BlockContext::default());

		// A random address that hasn't been deployed
		let addr = hex::decode(CONTRACT_ADDR).expect("Address should be a valid hex code");

		let result = mock::Midnight::get_contract_state(&addr);
		assert_eq!(result, Err(LedgerApiError::ContractNotPresent));
	})
}

#[test]
fn test_get_unclaimed_amount_beneficiary_not_found() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state(BlockContext::default());

		// A 32-byte address that has never had any unclaimed rewards in the genesis state
		let addr = [0u8; 32];
		let result = mock::Midnight::get_unclaimed_amount(&addr);
		assert_eq!(result, Err(LedgerApiError::BeneficiaryNotFound));
	})
}

#[test]
fn test_validation_works() {
	let (tx, block_context) =
		midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(DEPLOY_TX);

	let call = MidnightCall::send_mn_transaction { midnight_tx: tx };
	mock::new_test_ext().execute_with(|| {
		init_ledger_state(block_context.into());

		assert_ok!(<mock::Midnight as ValidateUnsigned>::validate_unsigned(
			TransactionSource::External,
			&call
		));
	})
}

#[test]
fn test_validation_fails() {
	let call = MidnightCall::send_mn_transaction { midnight_tx: vec![1, 2, 3] };

	mock::new_test_ext().execute_with(|| {
		init_ledger_state(BlockContext::default());

		assert_err!(
			<mock::Midnight as ValidateUnsigned>::validate_unsigned(
				TransactionSource::External,
				&call
			),
			//todo here
			TransactionValidityError::Invalid(InvalidTransaction::Custom(
				LedgerApiError::Deserialization(DeserializationError::Transaction).into()
			))
		);
	});
}

#[test]
fn test_pre_dispatch_accepts_valid_transaction() {
	let (tx, block_context) =
		midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(DEPLOY_TX);

	let call = MidnightCall::send_mn_transaction { midnight_tx: tx };
	mock::new_test_ext().execute_with(|| {
		init_ledger_state(block_context.into());

		// pre_dispatch should succeed for a valid transaction
		assert_ok!(<mock::Midnight as ValidateUnsigned>::pre_dispatch(&call));
	})
}

#[test]
fn test_pre_dispatch_rejects_contract_not_present() {
	// STORE_TX requires a deployed contract, so without DEPLOY_TX it will fail
	// This tests the DDoS mitigation: transactions that would fail the guaranteed
	// part are rejected at pre_dispatch time, before consuming blockspace.
	let (tx, block_context) =
		midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(STORE_TX);

	let call = MidnightCall::send_mn_transaction { midnight_tx: tx };
	mock::new_test_ext().execute_with(|| {
		init_ledger_state(block_context.into());
		// Note: DEPLOY_TX not applied - contract doesn't exist

		// pre_dispatch should fail because the contract doesn't exist
		let result = <mock::Midnight as ValidateUnsigned>::pre_dispatch(&call);
		assert!(
			result.is_err(),
			"pre_dispatch should reject transaction with missing contract dependency"
		);
	});
}

#[test]
fn test_pre_dispatch_rejects_malformed_transaction() {
	let call = MidnightCall::send_mn_transaction { midnight_tx: vec![1, 2, 3] };

	mock::new_test_ext().execute_with(|| {
		init_ledger_state(BlockContext::default());

		// pre_dispatch should fail for malformed transaction
		assert_err!(
			<mock::Midnight as ValidateUnsigned>::pre_dispatch(&call),
			TransactionValidityError::Invalid(InvalidTransaction::Custom(
				LedgerApiError::Deserialization(DeserializationError::Transaction).into()
			))
		);
	});
}

/// PR367-TC-0003-02: ReplayProtection Rejection
/// Verify that a replayed transaction is rejected at `pre_dispatch`.
#[test]
fn test_pre_dispatch_rejects_replay_attack() {
	mock::new_test_ext().execute_with(|| {
		// Set up ledger state and deploy contract
		let (deploy_tx, block_context_deploy) =
			midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(DEPLOY_TX);
		let (store_tx, block_context_store) =
			midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(STORE_TX);

		init_ledger_state(block_context_deploy.into());

		// Step 1: Deploy the contract
		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), deploy_tx));

		// Process block and advance to store transaction context
		process_block(2, block_context_store.into());

		// Step 2: Apply STORE_TX successfully via the pallet (not pre_dispatch)
		let store_tx_clone = store_tx.clone();
		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), store_tx));

		// Step 3: Try to replay the same STORE_TX via pre_dispatch
		// This should fail because the replay protection counter has been consumed
		let call = MidnightCall::send_mn_transaction { midnight_tx: store_tx_clone };
		let result = <mock::Midnight as ValidateUnsigned>::pre_dispatch(&call);

		// pre_dispatch should reject the replay attempt
		assert!(result.is_err(), "pre_dispatch should reject replayed transaction");
	});
}

/// PR367-TC-0003-05: Validation Does Not Modify State
/// Verify that `validate_guaranteed_execution` (via pre_dispatch) is read-only.
#[test]
fn test_pre_dispatch_validation_does_not_modify_state() {
	mock::new_test_ext().execute_with(|| {
		let (tx, block_context) =
			midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(DEPLOY_TX);

		init_ledger_state(block_context.into());

		// Record state before validation
		let state_root_before =
			mock::Midnight::get_zswap_state_root().expect("Should be able to get state root");

		// Create call and run pre_dispatch
		let call = MidnightCall::send_mn_transaction { midnight_tx: tx };
		let _result = <mock::Midnight as ValidateUnsigned>::pre_dispatch(&call);

		// Record state after validation
		let state_root_after =
			mock::Midnight::get_zswap_state_root().expect("Should be able to get state root");

		// State should be unchanged - validation must be read-only
		assert_eq!(
			state_root_before, state_root_after,
			"State should not be modified by pre_dispatch validation"
		);
	});
}

/// PR367-TC-0003-05 (variant): Verify validation doesn't modify state even for failing validation
#[test]
fn test_pre_dispatch_validation_does_not_modify_state_on_failure() {
	mock::new_test_ext().execute_with(|| {
		// STORE_TX will fail (no contract deployed) but should not modify state
		let (tx, block_context) =
			midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(STORE_TX);

		init_ledger_state(block_context.into());

		// Record state before validation
		let state_root_before =
			mock::Midnight::get_zswap_state_root().expect("Should be able to get state root");

		// Create call and run pre_dispatch (will fail)
		let call = MidnightCall::send_mn_transaction { midnight_tx: tx };
		let result = <mock::Midnight as ValidateUnsigned>::pre_dispatch(&call);
		assert!(result.is_err(), "pre_dispatch should fail for missing contract");

		// Record state after validation
		let state_root_after =
			mock::Midnight::get_zswap_state_root().expect("Should be able to get state root");

		// State should be unchanged - even failed validation must be read-only
		assert_eq!(
			state_root_before, state_root_after,
			"State should not be modified by failed pre_dispatch validation"
		);
	});
}

#[test]
fn sets_extra_transaction_size_weight() {
	mock::new_test_ext().execute_with(|| {
		let before_weight = mock::Midnight::configurable_transaction_size_weight();

		assert_eq!(before_weight, crate::EXTRA_WEIGHT_TX_SIZE);

		let new_weight = Weight::from_parts(42, 0);

		mock::Midnight::set_tx_size_weight(RawOrigin::Root.into(), new_weight).unwrap();

		let after_weight = mock::Midnight::configurable_transaction_size_weight();

		assert_eq!(after_weight, new_weight);
	});
}

#[test]
fn test_get_mn_transaction_fee() {
	mock::new_test_ext().execute_with(|| {
		let (tx, block_context) =
			midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(DEPLOY_TX);

		init_ledger_state(block_context.into());

		let gas_cost = mock::Midnight::get_transaction_cost(&tx).unwrap();

		// Assert the transaction has some associated cost
		assert!(gas_cost > 0);
	});
}

#[test]
fn test_get_ledger_parameters() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state(BlockContext::default());

		let parameters = mock::Midnight::get_ledger_parameters();

		assert_ok!(parameters);
	});
}

#[test]
#[ignore = "Cannot update ZSWAP_TX because we have no test tokens in genesis"]
fn test_send_zswap_tx() {
	mock::new_test_ext().execute_with(|| {
		let (tx, block_context) =
			midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(ZSWAP_TX);

		init_ledger_state(block_context.into());

		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), tx));
	});
}

#[test]
#[ignore = "Cannot update ZSWAP_TX because we have no test tokens in genesis"]
fn test_get_zswap_state_root() {
	mock::new_test_ext().execute_with(|| {
		let (tx, block_context) =
			midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(ZSWAP_TX);

		init_ledger_state(block_context.into());

		let root = mock::Midnight::get_zswap_state_root().unwrap();

		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), tx));

		mock::System::set_block_number(2);

		let new_root = mock::Midnight::get_zswap_state_root().unwrap();

		assert_ne!(new_root, root);
	});
}

#[test]
fn test_get_ledger_state_root() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state(BlockContext::default());

		let root = mock::Midnight::get_ledger_state_root();

		assert_ok!(&root);
		assert!(!root.unwrap().is_empty());
	});
}

#[test]
fn test_get_ledger_state_root_differs_from_zswap_state_root() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state(BlockContext::default());

		let ledger_root = mock::Midnight::get_ledger_state_root().unwrap();
		let zswap_root = mock::Midnight::get_zswap_state_root().unwrap();

		assert_ne!(ledger_root, zswap_root);
	});
}

/// The parent timestamp that makes the correction land a minute before the block context the
/// DEPLOY_TX fixture was built for — a point at which its intent no longer verifies. So the
/// correction, when it applies, turns an accepted block into a rejected one.
fn parent_ts_that_the_correction_rejects(block_context: &BlockContext) -> u64 {
	block_context.tblock - 60 - TBLOCK_CORRECTION_OFFSET_SECS
}

/// The runtime this node ships imports version 2 of the host function, which does not correct:
/// a block whose first transaction the correction would have rejected is verified against its own
/// timestamp and accepted. This is the loophole closing — it is gated on the runtime upgrade,
/// nothing else.
///
/// Version 1's side of the split is only reachable for ledger 8 (the pallet here runs ledger 9,
/// which never skews), so it is covered by the `well_formed_tblock` / `strict_cache_key` unit
/// tests in `midnight-node-ledger` rather than from a pallet test.
///
/// See <https://github.com/midnightntwrk/midnight-node/issues/1924>
#[test]
fn test_tblock_correction_not_applied_by_the_current_runtime() {
	let (tx, block_context) =
		midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(DEPLOY_TX);
	let block_context: BlockContext = block_context.into();
	let parent_ts = parent_ts_that_the_correction_rejects(&block_context);

	mock::new_test_ext().execute_with(|| {
		init_ledger_state(block_context.clone());
		begin_block(1, parent_ts, uncached_block_ts(&block_context));

		assert_ok!(send_mn_transaction(tx.clone()));
	});
}

/// Mempool ingress must not be corrected: `validate_unsigned` already skews the block context it
/// passes to the ledger by `slot_duration * (1 + MaxSkippedSlots)`, so correcting there too would
/// double-count a slot and reject valid transactions.
#[test]
fn test_tblock_correction_does_not_affect_mempool_validation() {
	let (tx, block_context) =
		midnight_node_ledger_helpers::ledger_9::extract_tx_with_context(DEPLOY_TX);
	let block_context: BlockContext = block_context.into();
	let parent_ts = parent_ts_that_the_correction_rejects(&block_context);

	mock::new_test_ext().execute_with(|| {
		init_ledger_state(block_context.clone());
		begin_block(1, parent_ts, uncached_block_ts(&block_context));

		let call = MidnightCall::send_mn_transaction { midnight_tx: tx.clone() };
		assert_ok!(<mock::Midnight as ValidateUnsigned>::validate_unsigned(
			TransactionSource::External,
			&call
		));
	});
}

#[cfg(feature = "experimental")]
#[ignore = "TODO UNSHIELDED - fix when Claim Mint is properly handled for Unshielded"]
#[test]
fn test_send_claim_mint() {
	/*
	test commented out because it references block_rewards which no longer exist
		use crate::mock::BeneficiaryId;
		use frame_support::{
			pallet_prelude::ProvideInherent,
			traits::{OnFinalize, UnfilteredDispatchable},
		};
		use midnight_node_res::undeployed::transactions::CLAIM_MINT_TX;
		use sp_inherents::InherentData;

		mock::new_test_ext().execute_with(|| {
			init_ledger_state(BlockContext::default());

			let mut inherent_data = InherentData::new();

			let block_beneficiary_provider = sp_block_rewards::BlockBeneficiaryInherentProvider::<
				BeneficiaryId,
			>::from_env("SIDECHAIN_BLOCK_BENEFICIARY")
			.expect("SIDECHAIN_BLOCK_BENEFICIARY env variable not provided");

			inherent_data
				.put_data(
					sp_block_rewards::INHERENT_IDENTIFIER,
					&block_beneficiary_provider.beneficiary_id,
				)
				.unwrap();

			let call = <mock::BlockRewards as ProvideInherent>::create_inherent(&inherent_data)
				.expect("Creating test inherent should not fail");

			call.dispatch_bypass_filter(RuntimeOrigin::none())
				.expect("dispatching test call should work");

			mock::Midnight::on_finalize(mock::System::block_number());
			let events = mock::midnight_events();

			assert_matches!(events[0], Event::PayoutMinted(_));

			assert_ok!(mock::Midnight::send_mn_transaction(
				RuntimeOrigin::none(),
				hex::encode(CLAIM_MINT_TX).into_bytes()
			));
		});
	*/
}
// grcov-excl-stop
