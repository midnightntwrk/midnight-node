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
use crate::{
	Error,
	mock::{self, RuntimeOrigin, Test},
};
use frame_support::{assert_err, assert_ok};
use midnight_node_ledger::types::active_ledger_bridge as LedgerApi;
use midnight_node_ledger_helpers::{DustPublicKey, Fr, serialize_untagged};
use midnight_node_res::networks::{MidnightNetwork, UndeployedNetwork};
use midnight_primitives::{
	MidnightSystemTransactionBridgeExecutor, MidnightSystemTransactionCNightExecutor,
};

fn init_ledger_state() {
	let path_buf = tempfile::tempdir().unwrap().keep();
	let state_key = midnight_node_ledger::latest::storage::init_storage_paritydb_separate(
		&path_buf,
		UndeployedNetwork.genesis_state(),
		1024 * 1024,
	);
	mock::Midnight::initialize_state(UndeployedNetwork.id(), &state_key);
	mock::System::set_block_number(1);
}

fn cnight_tx() -> Vec<u8> {
	let owner = serialize_untagged(&DustPublicKey(Fr::from(7u64))).unwrap();
	let event =
		LedgerApi::construct_cnight_generates_dust_event(1_000, &owner, 0, 0, [0u8; 32]).unwrap();
	LedgerApi::construct_cnight_generates_dust_system_tx(vec![event]).unwrap()
}

fn unlock_to_treasury_tx() -> Vec<u8> {
	LedgerApi::construct_unlock_to_treasury_system_tx(0).unwrap()
}

fn distribute_reserve_tx() -> Vec<u8> {
	LedgerApi::construct_distribute_reserve_system_tx(0).unwrap()
}

fn distribute_night_cardano_bridge_tx() -> Vec<u8> {
	LedgerApi::construct_distribute_night_cardano_bridge_system_tx(0, &[0u8; 32], [0u8; 32])
		.unwrap()
}

#[test]
fn governance_rejects_non_overwrite_parameters() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();
		assert_err!(
			mock::MidnightSystem::send_mn_system_transaction(RuntimeOrigin::root(), cnight_tx()),
			Error::<Test>::SystemTransactionNotAllowedForGovernance,
		);
	});
}

#[test]
fn cnight_executor_rejects_bridge_only_tx() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();
		assert_err!(
			<mock::MidnightSystem as MidnightSystemTransactionCNightExecutor>::execute_system_transaction(
				unlock_to_treasury_tx()
			),
			Error::<Test>::SystemTransactionNotAllowedForCNight,
		);
		assert_err!(
			<mock::MidnightSystem as MidnightSystemTransactionCNightExecutor>::execute_system_transaction(
				distribute_reserve_tx()
			),
			Error::<Test>::SystemTransactionNotAllowedForCNight,
		);
	});
}

#[test]
fn cnight_executor_accepts_cnight_generates_dust_update() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();
		assert_ok!(
			<mock::MidnightSystem as MidnightSystemTransactionCNightExecutor>::execute_system_transaction(
				cnight_tx()
			)
		);
	});
}

#[test]
fn bridge_executor_rejects_cnight_generates_dust_update() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();
		assert_err!(
			<mock::MidnightSystem as MidnightSystemTransactionBridgeExecutor>::execute_system_transaction(
				cnight_tx()
			),
			Error::<Test>::SystemTransactionNotAllowedForBridge,
		);
	});
}

#[test]
fn bridge_executor_accepts_its_three_allowed_variants() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();
		assert_ok!(
			<mock::MidnightSystem as MidnightSystemTransactionBridgeExecutor>::execute_system_transaction(
				unlock_to_treasury_tx()
			)
		);
		assert_ok!(
			<mock::MidnightSystem as MidnightSystemTransactionBridgeExecutor>::execute_system_transaction(
				distribute_reserve_tx()
			)
		);
		assert_ok!(
			<mock::MidnightSystem as MidnightSystemTransactionBridgeExecutor>::execute_system_transaction(
				distribute_night_cardano_bridge_tx()
			)
		);
	});
}

use midnight_node_ledger_helpers::{
	ClaimKind, HashOutput, INITIAL_PARAMETERS, ShieldedTokenType, SystemTransaction,
	UnshieldedTokenType, serialize,
};

fn ser(tx: &SystemTransaction) -> Vec<u8> {
	serialize(tx).expect("system transaction serializes")
}

fn overwrite_parameters_tx() -> Vec<u8> {
	ser(&SystemTransaction::OverwriteParameters(INITIAL_PARAMETERS))
}

fn distribute_night_reward_tx() -> Vec<u8> {
	ser(&SystemTransaction::DistributeNight(ClaimKind::Reward, vec![]))
}

fn pay_block_rewards_to_treasury_tx() -> Vec<u8> {
	ser(&SystemTransaction::PayBlockRewardsToTreasury { amount: 0 })
}

fn pay_from_treasury_shielded_tx() -> Vec<u8> {
	ser(&SystemTransaction::PayFromTreasuryShielded {
		outputs: vec![],
		nonce: HashOutput([0u8; 32]),
		token_type: ShieldedTokenType(HashOutput([0u8; 32])),
	})
}

fn pay_from_treasury_unshielded_tx() -> Vec<u8> {
	ser(&SystemTransaction::PayFromTreasuryUnshielded {
		outputs: vec![],
		token_type: UnshieldedTokenType(HashOutput([0u8; 32])),
	})
}

/// Every variant `get_system_tx_type` recognizes, so the matrix below stays exhaustive
/// as the ledger gains new ones.
fn all_variants() -> Vec<(&'static str, Vec<u8>)> {
	vec![
		("overwrite_parameters", overwrite_parameters_tx()),
		("distribute_night_reward", distribute_night_reward_tx()),
		("distribute_night_cardano_bridge", distribute_night_cardano_bridge_tx()),
		("pay_block_rewards_to_treasury", pay_block_rewards_to_treasury_tx()),
		("pay_from_treasury_shielded", pay_from_treasury_shielded_tx()),
		("pay_from_treasury_unshielded", pay_from_treasury_unshielded_tx()),
		("distribute_reserve", distribute_reserve_tx()),
		("unlock_to_treasury", unlock_to_treasury_tx()),
		("cnight_generates_dust_update", cnight_tx()),
	]
}

#[test]
fn governance_rejects_every_variant_but_overwrite_parameters() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();
		for (name, tx) in all_variants() {
			if name == "overwrite_parameters" {
				continue;
			}
			assert_eq!(
				mock::MidnightSystem::send_mn_system_transaction(RuntimeOrigin::root(), tx),
				Err(Error::<Test>::SystemTransactionNotAllowedForGovernance.into()),
				"governance must reject {name}",
			);
		}
	});
}

#[test]
fn cnight_executor_rejects_every_variant_but_cnight_generates_dust_update() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();
		for (name, tx) in all_variants() {
			if name == "cnight_generates_dust_update" {
				continue;
			}
			assert_eq!(
				<mock::MidnightSystem as MidnightSystemTransactionCNightExecutor>::execute_system_transaction(tx),
				Err(Error::<Test>::SystemTransactionNotAllowedForCNight.into()),
				"cnight executor must reject {name}",
			);
		}
	});
}

#[test]
fn bridge_executor_rejects_every_variant_outside_its_three() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();
		const ALLOWED: [&str; 3] =
			["distribute_night_cardano_bridge", "distribute_reserve", "unlock_to_treasury"];
		for (name, tx) in all_variants() {
			if ALLOWED.contains(&name) {
				continue;
			}
			assert_eq!(
				<mock::MidnightSystem as MidnightSystemTransactionBridgeExecutor>::execute_system_transaction(tx),
				Err(Error::<Test>::SystemTransactionNotAllowedForBridge.into()),
				"bridge executor must reject {name}",
			);
		}
	});
}

/// The bridge allow-list discriminates on `ClaimKind`, not just the variant: `Reward`
/// mints NIGHT as block rewards and is not the bridge's to issue.
#[test]
fn bridge_executor_discriminates_distribute_night_claim_kind() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();
		assert_err!(
			<mock::MidnightSystem as MidnightSystemTransactionBridgeExecutor>::execute_system_transaction(
				distribute_night_reward_tx()
			),
			Error::<Test>::SystemTransactionNotAllowedForBridge,
		);
		assert_ok!(
			<mock::MidnightSystem as MidnightSystemTransactionBridgeExecutor>::execute_system_transaction(
				distribute_night_cardano_bridge_tx()
			)
		);
	});
}

/// The allow-list guard runs ahead of `apply_system_tx`, so a rejection must be
/// invisible both on chain and to the indexer.
#[test]
fn rejected_system_tx_mutates_no_state_and_emits_no_event() {
	mock::new_test_ext().execute_with(|| {
		init_ledger_state();
		let before = pallet_midnight::StateKey::<Test>::get();

		assert!(
			mock::MidnightSystem::send_mn_system_transaction(
				RuntimeOrigin::root(),
				pay_from_treasury_unshielded_tx()
			)
			.is_err()
		);
		assert!(
			<mock::MidnightSystem as MidnightSystemTransactionCNightExecutor>::execute_system_transaction(
				distribute_night_reward_tx()
			)
			.is_err()
		);
		assert!(
			<mock::MidnightSystem as MidnightSystemTransactionBridgeExecutor>::execute_system_transaction(
				overwrite_parameters_tx()
			)
			.is_err()
		);

		assert_eq!(
			before,
			pallet_midnight::StateKey::<Test>::get(),
			"a rejected system transaction must not advance the ledger state key",
		);
		assert!(
			!mock::System::events().iter().any(|r| matches!(
				r.event,
				mock::RuntimeEvent::MidnightSystem(crate::Event::SystemTransactionApplied(_))
			)),
			"a rejected system transaction must emit no SystemTransactionApplied",
		);
	});
}
