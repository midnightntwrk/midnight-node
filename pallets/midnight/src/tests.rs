// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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
use super::*;
use crate::{
	Call as MidnightCall, mock,
	mock::{RuntimeOrigin, Test},
};
use assert_matches::assert_matches;
use frame_support::{assert_err, assert_ok, pallet_prelude::Weight, traits::OnFinalize};
use frame_system::RawOrigin;
use midnight_node_ledger::types::{
	BlockContext,
	active_version::{DeserializationError, LedgerApiError, MalformedError, TransactionError},
};
use midnight_node_res::{
	networks::{MidnightNetwork, UndeployedNetwork},
	undeployed::transactions::{
		CHECK_TX, CONTRACT_ADDR, DEPLOY_TX, MAINTENANCE_TX, STORE_TX, ZSWAP_TX,
	},
};
use sp_runtime::{
	traits::ValidateUnsigned,
	transaction_validity::{InvalidTransaction, TransactionSource, TransactionValidityError},
};
use test_log::test;

fn init_ledger_state(block_context: BlockContext) {
	let path_buf = tempfile::tempdir().unwrap().keep();
	let state_key = midnight_node_ledger::init_storage_paritydb(
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

#[test]
fn test_send_mn_transaction() {
	mock::new_test_ext().execute_with(|| {
		let (tx, block_context) =
			midnight_node_ledger_helpers::extract_info_from_tx_with_context(DEPLOY_TX);
		init_ledger_state(block_context.into());

		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), tx));

		// Check emitted events
		let events = mock::midnight_events();
		assert_matches!(events[0], Event::ContractDeploy(_));
		assert_matches!(events[1], Event::TxApplied(_));
	})
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
			midnight_node_ledger_helpers::extract_info_from_tx_with_context(STORE_TX);
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
			midnight_node_ledger_helpers::extract_info_from_tx_with_context(DEPLOY_TX);
		let (tx_store, block_context_store) =
			midnight_node_ledger_helpers::extract_info_from_tx_with_context(STORE_TX);
		let (tx_check, block_context_check) =
			midnight_node_ledger_helpers::extract_info_from_tx_with_context(CHECK_TX);
		let (tx_maintenance, block_context_maintenance) =
			midnight_node_ledger_helpers::extract_info_from_tx_with_context(MAINTENANCE_TX);

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
fn test_validation_works() {
	let (tx, block_context) =
		midnight_node_ledger_helpers::extract_info_from_tx_with_context(DEPLOY_TX);

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
		midnight_node_ledger_helpers::extract_info_from_tx_with_context(DEPLOY_TX);

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
		midnight_node_ledger_helpers::extract_info_from_tx_with_context(STORE_TX);

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
			midnight_node_ledger_helpers::extract_info_from_tx_with_context(DEPLOY_TX);
		let (store_tx, block_context_store) =
			midnight_node_ledger_helpers::extract_info_from_tx_with_context(STORE_TX);

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
			midnight_node_ledger_helpers::extract_info_from_tx_with_context(DEPLOY_TX);

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
			midnight_node_ledger_helpers::extract_info_from_tx_with_context(STORE_TX);

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
#[ignore = "TODO COST MODEL - fix when new Ledger's cost model is available"]
fn test_get_mn_transaction_fee() {
	mock::new_test_ext().execute_with(|| {
		let (tx, block_context) =
			midnight_node_ledger_helpers::extract_info_from_tx_with_context(DEPLOY_TX);

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
			midnight_node_ledger_helpers::extract_info_from_tx_with_context(ZSWAP_TX);

		init_ledger_state(block_context.into());

		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), tx));
	});
}

#[test]
#[ignore = "Cannot update ZSWAP_TX because we have no test tokens in genesis"]
fn test_get_zswap_state_root() {
	mock::new_test_ext().execute_with(|| {
		let (tx, block_context) =
			midnight_node_ledger_helpers::extract_info_from_tx_with_context(ZSWAP_TX);

		init_ledger_state(block_context.into());

		let root = mock::Midnight::get_zswap_state_root().unwrap();

		assert_ok!(mock::Midnight::send_mn_transaction(RuntimeOrigin::none(), tx));

		mock::System::set_block_number(2);

		let new_root = mock::Midnight::get_zswap_state_root().unwrap();

		assert_ne!(new_root, root);
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
