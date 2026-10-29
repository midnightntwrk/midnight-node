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

//! Uses the real Compact proof fixture produced by tests/snowbridge/test.sh.
//! Separate process: ledger default storages are process-global and keyed by database type.
use base_crypto::hash::HashOutput;
use coin_structure_ledger_9::{coin::TokenType, contract::ContractAddress};
use ledger_storage_ledger_8::{
	Storage,
	db::{DB, ParityDb, paritydb::OwnedDb},
	storage::{default_storage, set_default_storage},
};
use midnight_node_ledger::{
	ethereum_bridge::{EscrowConfig, EscrowError},
	ledger_9::{Bridge, api::ledger::Ledger},
};
use mn_ledger_9::structure::{LedgerState, Signature};
use onchain_state_ledger_9::state::{ChargedState, ContractState, StateValue};
use parity_scale_codec::Encode;
use sha2::digest::{OutputSizeUser, consts::U32};
use std::{path::PathBuf, sync::Arc};

fn fixtures() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../artifacts/snowbridge/compact-ledger")
}
fn hash(value: &serde_json::Value) -> [u8; 32] {
	hex::decode(value.as_str().unwrap()).unwrap().try_into().unwrap()
}

fn exercise<D: DB>(externalities: &mut frame_support::__private::TestExternalities)
where
	D::Hasher: OutputSizeUser<OutputSize = U32>,
{
	let manifest: serde_json::Value = serde_json::from_slice(
		&std::fs::read(fixtures().join("manifest.json"))
			.expect("Run tests/snowbridge/test.sh first"),
	)
	.unwrap();
	let contract: ContractState<D> = midnight_serialize::tagged_deserialize(
		&std::fs::read(fixtures().join("escrow-state.bin")).unwrap()[..],
	)
	.unwrap();
	let config = EscrowConfig {
		bridge_domain: [2; 32],
		token_domain: [3; 32],
		escrow: hash(&manifest["escrow"]),
		color: hash(&manifest["color"]),
		retire_operation: contract
			.operations
			.get(&b"retire".to_vec().into())
			.unwrap()
			.hash()
			.0
			.into(),
	};
	let mut genesis = LedgerState::<D>::new("bridge-host-test");
	let address = ContractAddress(HashOutput(config.escrow));
	genesis.contract = genesis.contract.insert(address, contract.clone());
	let root = default_storage::<D>().arena.alloc(Ledger::new(genesis));
	let mut state_key = vec![];
	midnight_serialize::tagged_serialize(&root.as_typed_key(), &mut state_key).unwrap();
	let before = root.hash();
	let record =
		Bridge::<Signature, D>::get_bridge_redemption(&state_key, config.clone(), 0).unwrap();
	externalities.execute_with(|| {
		assert_eq!(
			midnight_node_ledger::host_api::ledger_9::ledger_9_bridge::get_bridge_redemption(
				&state_key,
				config.clone(),
				0
			)
			.unwrap(),
			record
		);
	});
	assert_eq!(record.amount, 40);
	assert_eq!(record.recipient, [5; 20]);
	assert_eq!(record.total_retired, 40);
	assert_eq!(record.ledger_balance, 40);
	assert_eq!(record.count, 1);
	assert_eq!(record.encode().len(), 179);
	assert_eq!(root.hash(), before);
	assert_eq!(
		Bridge::<Signature, D>::get_bridge_redemption(&state_key, config.clone(), 1),
		Err(EscrowError::Range)
	);
	assert_eq!(
		Bridge::<Signature, D>::get_bridge_redemption(b"bad-key", config.clone(), 0),
		Err(EscrowError::LedgerUnavailable)
	);
	let mut wrong = config.clone();
	wrong.escrow = [99; 32];
	assert_eq!(
		Bridge::<Signature, D>::get_bridge_redemption(&state_key, wrong, 0),
		Err(EscrowError::ContractMissing)
	);
	let mut wrong = config.clone();
	wrong.retire_operation[0] ^= 1;
	assert_eq!(
		midnight_node_ledger::ledger_9::ethereum_bridge::read_redemption(&contract, &wrong, 0),
		Err(EscrowError::Operation)
	);
	let mut wrong = config.clone();
	wrong.color[0] ^= 1;
	assert_eq!(
		midnight_node_ledger::ledger_9::ethereum_bridge::read_redemption(&contract, &wrong, 0),
		Err(EscrowError::Configuration)
	);
	let mut changed = contract.clone();
	changed.maintenance_authority.threshold = 0;
	assert_eq!(
		midnight_node_ledger::ledger_9::ethereum_bridge::read_redemption(&changed, &config, 0),
		Err(EscrowError::Unsealed)
	);
	let color = address.custom_unshielded_token_type(HashOutput(config.token_domain));
	let mut changed = contract.clone();
	changed.balance = changed.balance.insert(TokenType::Unshielded(color), 39);
	assert_eq!(
		midnight_node_ledger::ledger_9::ethereum_bridge::read_redemption(&changed, &config, 0),
		Err(EscrowError::Balance)
	);
	let mut changed = contract.clone();
	changed.data = ChargedState::new(StateValue::Null);
	assert_eq!(
		midnight_node_ledger::ledger_9::ethereum_bridge::read_redemption(&changed, &config, 0),
		Err(EscrowError::Schema)
	);

	// Activation accepts only the empty form of the exact compiled escrow layout.
	use base_crypto::fab::AlignedValue;
	use ledger_storage_ledger_8::{arena::Sp, storage::HashMap};
	use midnight_node_ledger::ledger_9::ethereum_bridge::{read_redemption, validate_escrow};
	assert_eq!(validate_escrow(&contract, &config), Err(EscrowError::Range));
	let StateValue::Array(fields) = contract.data.get_ref() else { panic!("escrow fields") };
	let empty_fields = fields
		.insert(3, StateValue::Cell(Sp::new(AlignedValue::from(0u64))))
		.unwrap()
		.insert(4, StateValue::Cell(Sp::new(AlignedValue::from(0u128))))
		.unwrap()
		.insert(5, StateValue::Map(HashMap::new()))
		.unwrap();
	let mut empty = contract.clone();
	empty.data = ChargedState::new(StateValue::Array(empty_fields));
	empty.balance = HashMap::new();
	assert_eq!(validate_escrow(&empty, &config), Ok(()));
	let StateValue::Map(records) = fields.get(5).unwrap() else { panic!("records") };
	let first_record = records.get(&AlignedValue::from(0u64)).unwrap();
	let mut full_records = records.clone();
	for sequence in 1..64u64 {
		full_records = full_records.insert(AlignedValue::from(sequence), (*first_record).clone());
	}
	let full_fields = fields
		.insert(3, StateValue::Cell(Sp::new(AlignedValue::from(64u64))))
		.unwrap()
		.insert(4, StateValue::Cell(Sp::new(AlignedValue::from(2560u128))))
		.unwrap()
		.insert(5, StateValue::Map(full_records))
		.unwrap();
	let mut full = contract.clone();
	full.data = ChargedState::new(StateValue::Array(full_fields));
	full.balance = full.balance.insert(TokenType::Unshielded(color), 2560);
	assert_eq!(read_redemption(&full, &config, 63).unwrap().amount, 40);
	assert_eq!(read_redemption(&full, &config, 64), Err(EscrowError::Range));
	let mut pending = root.clone();
	pending.persist();
	default_storage::<D>().with_backend(|b| b.flush_all_changes_to_db());
	assert_eq!(
		Bridge::<Signature, D>::get_bridge_redemption(&state_key, config.clone(), 0).unwrap(),
		record
	);
	#[cfg(feature = "ethereum-bridge-poc")]
	externalities.execute_with(|| issuance_and_rollback(&state_key, &config));
}

#[test]
#[ignore = "requires generated real-proof Compact escrow: tests/snowbridge/test.sh"]
fn committed_escrow_reader_in_both_database_modes() {
	let directory =
		std::env::temp_dir().join(format!("midnight-bridge-host-{}", std::process::id()));
	std::fs::create_dir_all(&directory).unwrap();
	set_default_storage(|| {
		Storage::<ParityDb>::new(0, ParityDb::open(&directory.join("separate")))
	})
	.unwrap();
	let mut separate_ext = frame_support::__private::TestExternalities::default();
	separate_ext.register_extension(midnight_primitives_ledger::LedgerStorageExt::new(
		midnight_primitives_ledger::LedgerStorage::new_separate(directory.join("separate"), 0),
	));
	exercise::<ParityDb>(&mut separate_ext);
	const OFFSET: u8 = midnight_primitives_ledger::LedgerStorageExt::COLUMN_OFFSET;
	type Unified = ParityDb<sha2::Sha256, OwnedDb, OFFSET>;
	let mut options = parity_db::Options::with_columns(&directory.join("unified"), OFFSET + 3);
	midnight_node_ledger::ledger_9::storage::set_init_options_paritydb(&mut options, OFFSET, false);
	let database = Arc::new(parity_db::Db::open_or_create(&options).unwrap());
	set_default_storage(|| Storage::new(0, Unified::from_existing_db(OwnedDb(database.clone()))))
		.unwrap();
	let mut unified_ext = frame_support::__private::TestExternalities::default();
	unified_ext.register_extension(midnight_primitives_ledger::LedgerStorageExt::new(
		midnight_primitives_ledger::LedgerStorage::new_existing(database, 0),
	));
	exercise::<Unified>(&mut unified_ext);
	midnight_node_ledger::drop_all_default_storage();
}

#[cfg(feature = "ethereum-bridge-poc")]
fn issuance_and_rollback(state_key: &[u8], config: &EscrowConfig) {
	use frame_support::__private::sp_io;
	use midnight_node_ledger::{
		ethereum_bridge::EthereumCredit,
		host_api::ledger_9::ledger_9_bridge as host,
		ledger_9::{
			BlockContext,
			types::{LedgerApiError, SystemTransactionError, TransactionError},
		},
	};
	let credit = EthereumCredit {
		deposit_id: [11; 32],
		output_id: [12; 32],
		color: config.color,
		amount: 100,
		recipient: [7; 32],
	};
	let tx = host::create_ethereum_credit(credit.clone()).unwrap();
	let context = BlockContext { tblock: 1, ..Default::default() };
	for apply in [
		host::apply_governance_system_transaction,
		host::apply_cnight_system_transaction,
		host::apply_bridge_system_transaction,
	] {
		assert!(matches!(
			apply(state_key, &tx, context.clone(), 0),
			Err(LedgerApiError::Transaction(TransactionError::SystemTransaction(
				SystemTransactionError::NotAllowedForCaller
			)))
		));
	}
	let unrelated = host::construct_distribute_reserve_system_tx(0).unwrap();
	assert!(matches!(
		host::apply_ethereum_system_transaction(state_key, &unrelated, context.clone(), 0),
		Err(LedgerApiError::Transaction(TransactionError::SystemTransaction(
			SystemTransactionError::NotAllowedForCaller
		)))
	));
	sp_io::storage::set(b"canonical-bridge-root", state_key);
	sp_io::storage::start_transaction();
	let issued =
		host::apply_ethereum_system_transaction(state_key, &tx, context.clone(), 0).unwrap();
	sp_io::storage::set(b"canonical-bridge-root", &issued.state_root);
	sp_io::storage::rollback_transaction();
	assert_eq!(sp_io::storage::get(b"canonical-bridge-root").unwrap().as_ref(), state_key);
	let retry =
		host::apply_ethereum_system_transaction(state_key, &tx, context.clone(), 0).unwrap();
	assert_eq!(
		issued.state_root, retry.state_root,
		"orphaned arena writes cannot consume a canonical deposit"
	);
	let replay = host::apply_system_transaction(state_key, &tx, context.clone(), 0).unwrap();
	assert_eq!(
		issued.state_root, replay.state_root,
		"event-byte replay must include the same block cost"
	);
	assert!(host::apply_ethereum_system_transaction(&issued.state_root, &tx, context, 0).is_err());
	assert_eq!(issued.tx_type, "credit_ethereum_asset");
}
