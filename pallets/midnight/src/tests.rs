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
use frame_support::{assert_err, assert_ok, dispatch::GetDispatchInfo, pallet_prelude::Weight};
use frame_system::RawOrigin;
use midnight_node_ledger::types::active_version::{
	DeserializationError, InvalidError, LedgerApiError, TransactionError,
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

fn init_ledger_state() {
	let path_buf = tempfile::tempdir().unwrap().keep();
	midnight_node_ledger::init_storage_paritydb(
		&path_buf,
		&UndeployedNetwork.genesis_tx()[1..],
		1024 * 1024,
	);

	sp_tracing::try_init_simple();
	let state_key = midnight_node_ledger::get_root(
		UndeployedNetwork.genesis_state(),
		UndeployedNetwork.network_id(),
	);
	mock::Midnight::initialize_state(UndeployedNetwork.network_id() as u8, &state_key);
	mock::System::set_block_number(1);
}

#[test]
#[ignore = "TODO UNSHIELDED"]
fn test_send_mn_transaction() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();

		assert_ok!(mock::Midnight::send_mn_transaction(
			RuntimeOrigin::none(),
			hex::encode(DEPLOY_TX).as_bytes().to_vec()
		));

		// Check emitted events
		let events = mock::midnight_events();
		assert_matches!(events[0], Event::ContractDeploy(_));
		assert_matches!(events[1], Event::TxApplied(_));
	})
}

#[test]
#[ignore = "TODO UNSHIELDED"]
fn test_send_mn_transaction_malformed_tx() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();

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
#[ignore = "TODO UNSHIELDED"]
fn test_send_mn_transaction_invalid_tx() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();

		let error: sp_runtime::DispatchError =
			Error::<Test>::Transaction(TransactionError::Invalid(InvalidError::Zswap)).into();
		assert_err!(
			mock::Midnight::send_mn_transaction(
				RuntimeOrigin::none(),
				hex::encode(STORE_TX).as_bytes().to_vec()
			),
			error
		);

		// Check emitted events
		assert!(mock::midnight_events().is_empty());
	})
}

#[test]
#[ignore = "TODO UNSHIELDED"]
fn test_get_contract_state_json() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();

		assert_ok!(mock::Midnight::send_mn_transaction(
			RuntimeOrigin::none(),
			hex::encode(DEPLOY_TX).as_bytes().to_vec()
		));
		assert_ok!(mock::Midnight::send_mn_transaction(
			RuntimeOrigin::none(),
			hex::encode(STORE_TX).as_bytes().to_vec()
		));
		assert_ok!(mock::Midnight::send_mn_transaction(
			RuntimeOrigin::none(),
			hex::encode(CHECK_TX).as_bytes().to_vec()
		));
		assert_ok!(mock::Midnight::send_mn_transaction(
			RuntimeOrigin::none(),
			hex::encode(MAINTENANCE_TX).as_bytes().to_vec()
		));

		let addr = hex::decode(CONTRACT_ADDR).expect("Address should be a valid hex code");

		let result = mock::Midnight::get_contract_state_json(&addr);
		assert!(result.is_ok())
	})
}

#[test]
#[ignore = "TODO UNSHIELDED"]
fn test_get_contract_state() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();

		assert_ok!(mock::Midnight::send_mn_transaction(
			RuntimeOrigin::none(),
			hex::encode(DEPLOY_TX).as_bytes().to_vec()
		));
		assert_ok!(mock::Midnight::send_mn_transaction(
			RuntimeOrigin::none(),
			hex::encode(STORE_TX).as_bytes().to_vec()
		));
		assert_ok!(mock::Midnight::send_mn_transaction(
			RuntimeOrigin::none(),
			hex::encode(CHECK_TX).as_bytes().to_vec()
		));
		assert_ok!(mock::Midnight::send_mn_transaction(
			RuntimeOrigin::none(),
			hex::encode(MAINTENANCE_TX).as_bytes().to_vec()
		));

		let addr = hex::decode(CONTRACT_ADDR).expect("Address should be a valid hex code");

		let result = mock::Midnight::get_contract_state(&addr);
		assert!(result.is_ok())
	})
}

#[test]
#[ignore = "TODO UNSHIELDED"]
fn test_validation_works() {
	let call = MidnightCall::send_mn_transaction {
		midnight_tx: hex::encode(DEPLOY_TX).as_bytes().to_vec(),
	};
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();

		assert_ok!(<mock::Midnight as ValidateUnsigned>::validate_unsigned(
			TransactionSource::External,
			&call
		));
	})
}

#[ignore = "Re-enable when we add back tx validation after testing"]
#[test]
fn test_validation_fails() {
	let call = MidnightCall::send_mn_transaction { midnight_tx: vec![1, 2, 3] };

	mock::new_test_ext().execute_with(|| {
		init_ledger_state();

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
fn sets_manual_test_weight() {
	mock::new_test_ext().execute_with(|| {
		let midnight_call = MidnightCall::<Test>::send_mn_transaction { midnight_tx: vec![] };
		let call_info = midnight_call.get_dispatch_info();

		// Starting weight
		assert_eq!(call_info.weight, FIXED_MN_TRANSACTION_WEIGHT);

		mock::Midnight::set_mn_tx_weight(RawOrigin::Root.into(), Weight::from_parts(42, 0))
			.unwrap();

		let midnight_call_2 = MidnightCall::<Test>::send_mn_transaction { midnight_tx: vec![] };
		let call_info_2 = midnight_call_2.get_dispatch_info();
		assert_eq!(call_info_2.weight, Weight::from_parts(42, 0));
	});
}

#[test]
fn sets_extra_contract_call_weight() {
	mock::new_test_ext().execute_with(|| {
		let before_weight = mock::Midnight::configurable_contract_call_weight();

		assert_eq!(before_weight, crate::EXTRA_WEIGHT_PER_CONTRACT_CALL);

		let new_weight = Weight::from_parts(42, 0);

		mock::Midnight::set_contract_call_weight(RawOrigin::Root.into(), new_weight).unwrap();

		let after_weight = mock::Midnight::configurable_contract_call_weight();

		assert_eq!(after_weight, new_weight);
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
#[ignore = "TODO UNSHIELDED"]
fn disable_safe_mode() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();

		let tx = hex::encode(ZSWAP_TX).into_bytes();
		let midnight_call = MidnightCall::<Test>::send_mn_transaction { midnight_tx: tx.clone() };
		let call_info = midnight_call.get_dispatch_info();

		// Starting weight
		assert_eq!(call_info.weight, FIXED_MN_TRANSACTION_WEIGHT);

		// Disable safe mode
		mock::Midnight::set_safe_mode(RawOrigin::Root.into(), false).unwrap();

		let midnight_call_2 = MidnightCall::<Test>::send_mn_transaction { midnight_tx: tx };
		let call_info_2 = midnight_call_2.get_dispatch_info();

		assert!(call_info_2.weight != call_info.weight);
		assert!(call_info_2.weight.ref_time() > call_info.weight.ref_time());
	});
}

#[test]
#[ignore = "TODO UNSHIELDED"]
fn test_get_mn_transaction_fee() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();

		let tx = hex::encode(DEPLOY_TX).as_bytes().to_vec();
		let (storage_cost, _gas_cost) = mock::Midnight::get_transaction_cost(&tx).unwrap();

		// Assert the transaction has some associated cost
		assert!(storage_cost > 0);
	});
}

#[test]
fn test_get_ledger_parameters() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();

		let parameters = mock::Midnight::get_ledger_parameters();

		assert_ok!(parameters);
	});
}

#[test]
#[ignore = "TODO UNSHIELDED"]
fn test_send_zswap_tx() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();

		assert_ok!(mock::Midnight::send_mn_transaction(
			RuntimeOrigin::none(),
			hex::encode(ZSWAP_TX).into_bytes()
		));
	});
}

#[test]
#[ignore = "TODO UNSHIELDED"]
fn test_get_zswap_state_root() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();

		let root = mock::Midnight::get_zswap_state_root().unwrap();

		assert_ok!(mock::Midnight::send_mn_transaction(
			RuntimeOrigin::none(),
			hex::encode(ZSWAP_TX).into_bytes()
		));

		mock::System::set_block_number(2);

		let new_root = mock::Midnight::get_zswap_state_root().unwrap();

		assert_ne!(new_root, root);
	});
}

#[cfg(feature = "experimental")]
#[test]
fn test_send_claim_mint() {
	use crate::mock::BeneficiaryId;
	use frame_support::{
		pallet_prelude::ProvideInherent,
		traits::{OnFinalize, UnfilteredDispatchable},
	};
	use midnight_node_res::undeployed::transactions::CLAIM_MINT_TX;
	use sp_inherents::InherentData;

	mock::new_test_ext().execute_with(|| {
		init_ledger_state();

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
}
// grcov-excl-stop
