use frame_support::inherent::InherentData;
use midnight_primitives_cnight_observation::INHERENT_IDENTIFIER;
use midnight_primitives_cnight_observation::MidnightObservationTokenMovement;
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
use frame_support::pallet_prelude::*;
use frame_support::sp_runtime::traits::Dispatchable;
use frame_support::{BoundedVec, assert_ok};
use midnight_node_ledger::types::BlockContext;
use midnight_node_ledger_helpers::{
	CNightGeneratesDustActionType, CNightGeneratesDustEvent, DefaultDB, DustPublicKey,
	DustSecretKey, ProofMarker, Signature, SystemTransaction, TransactionWithContext, deserialize,
};
use midnight_node_res::networks::{MidnightNetwork, UndeployedNetwork};
use midnight_primitives_cnight_observation::{CardanoPosition, TimestampUnixMillis};
use midnight_primitives_mainchain_follower::{
	CreateData, DeregistrationData, ObservedUtxo, ObservedUtxoData, ObservedUtxoHeader,
	RedemptionCreateData, RedemptionSpendData, RegistrationData, SpendData, UtxoIndexInTx,
};
use pallet_cnight_observation::*;
use pallet_cnight_observation_mock::mock::{
	self, CNightObservation, RuntimeCall, RuntimeEvent, System, Test, new_test_ext,
};
use rand::prelude::*;
use sp_core::ConstU32;
use sp_core::Get;
use test_log::test;

fn create_inherent(
	utxos: Vec<ObservedUtxo>,
	next_cardano_position: CardanoPosition,
) -> InherentData {
	let mut inherent_data = InherentData::new();
	inherent_data
		.put_data(
			INHERENT_IDENTIFIER,
			&MidnightObservationTokenMovement { utxos, next_cardano_position },
		)
		.expect("inherent data insertion should not fail");
	inherent_data
}

fn tx_hash(block_number: u32, tx_index_in_block: u32) -> [u8; 32] {
	let mut seed = [0u8; 32];
	seed[0..4].copy_from_slice(&block_number.to_be_bytes());
	seed[4..8].copy_from_slice(&tx_index_in_block.to_be_bytes());
	let mut rng = rand::rngs::StdRng::from_seed(seed);
	rng.r#gen()
}

fn block_hash(block_number: u32) -> [u8; 32] {
	let mut seed = [0u8; 32];
	seed[0..4].copy_from_slice(&block_number.to_be_bytes());
	let mut rng = rand::rngs::StdRng::from_seed(seed);
	rng.r#gen()
}

fn test_position(block_number: u32, tx_index_in_block: u32) -> CardanoPosition {
	CardanoPosition {
		block_hash: block_hash(block_number),
		block_number,
		block_timestamp: TimestampUnixMillis(block_number as i64 * 20 * 1000),
		tx_index_in_block,
	}
}

fn test_header(
	block_number: u32,
	tx_index_in_block: u32,
	utxo_index: u16,
	utxo_tx_hash: Option<[u8; 32]>,
) -> ObservedUtxoHeader {
	ObservedUtxoHeader {
		tx_position: test_position(block_number, tx_index_in_block),
		tx_hash: sidechain_domain::McTxHash(tx_hash(block_number, tx_index_in_block)),
		utxo_tx_hash: sidechain_domain::McTxHash(
			utxo_tx_hash.unwrap_or_else(|| tx_hash(block_number, tx_index_in_block)),
		),
		utxo_index: UtxoIndexInTx(utxo_index),
	}
}

fn testbvec<S: Get<u32>>(input: &[u8], pad: Option<usize>) -> BoundedVec<u8, S> {
	let mut input_vec = input.to_vec();
	if let Some(pad) = pad {
		input_vec.resize(pad, 0);
	}
	input_vec.try_into().unwrap()
}

// Onchain dust address
fn dust_address() -> [u8; 32] {
	let mut rng = rand::rngs::StdRng::from_entropy();
	let dust_secret_key = DustSecretKey::sample(&mut rng);
	let dust_public_key = DustPublicKey::from(dust_secret_key);
	dust_public_key.0.as_le_bytes().try_into().unwrap()
}

// Onchain cardano address
fn cardano_address(input: &[u8]) -> BoundedVec<u8, ConstU32<CARDANO_BECH32_ADDRESS_MAX_LENGTH>> {
	testbvec::<ConstU32<CARDANO_BECH32_ADDRESS_MAX_LENGTH>>(input, None)
}

fn test_wallet_pairing() -> (BoundedVec<u8, ConstU32<CARDANO_BECH32_ADDRESS_MAX_LENGTH>>, [u8; 32])
{
	(cardano_address(b"cardano1"), dust_address())
}

fn extract_events(midnight_system_tx: &[u8]) -> Vec<CNightGeneratesDustEvent> {
	let midnight_system_tx: SystemTransaction =
		deserialize(midnight_system_tx).expect("failed to deserialize midnight system tx");
	let SystemTransaction::CNightGeneratesDustUpdate { events } = midnight_system_tx else {
		panic!("midnight system tx != CNightGeneratesDustUpdate");
	};
	events
}

fn init_ledger_state() {
	let block_context = get_block_context(UndeployedNetwork.genesis_block());
	let path_buf = tempfile::tempdir().unwrap().keep();
	let state_key = midnight_node_ledger::init_storage_paritydb(
		&path_buf,
		UndeployedNetwork.genesis_state(),
		1024 * 1024,
	);

	mock::Midnight::initialize_state(UndeployedNetwork.id(), &state_key);
	mock::System::set_block_number(1);
	mock::Timestamp::set_timestamp(block_context.tblock * 1000);
}

pub fn get_block_context(genesis_block: &[u8]) -> BlockContext {
	let tx_with_context: Vec<TransactionWithContext<Signature, ProofMarker, DefaultDB>> =
		deserialize(genesis_block)
			.unwrap_or_else(|err| panic!("Can't deserialize genesis block: {err}"));
	tx_with_context[0].block_context.clone().into()
}

fn any_event<F: Fn(&RuntimeEvent) -> bool>(f: F) -> bool {
	System::events().iter().any(|r| f(&r.event))
}

#[test]
fn asset_create_should_emit_valid_event_if_registered() {
	new_test_ext().execute_with(|| {
		init_ledger_state();
		let (cardano_addr, dust_addr) = test_wallet_pairing();

		let utxos = vec![
			ObservedUtxo {
				header: test_header(1, 2, 0, None),
				data: ObservedUtxoData::Registration(RegistrationData {
					cardano_address: cardano_addr.clone().into(),
					dust_address: dust_addr.into(),
				}),
			},
			ObservedUtxo {
				header: test_header(2, 0, 0, None),
				data: ObservedUtxoData::AssetCreate(CreateData {
					value: 100,
					owner: cardano_addr.clone().into_inner(),
					utxo_tx_hash: tx_hash(1, 3),
					utxo_tx_index: 0,
				}),
			},
		];

		let inherent_data = create_inherent(utxos, test_position(3, 0));
		let call = CNightObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = RuntimeCall::CNightObservation(call);
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		// Confirm the expected SystemTxCreateUtxo event was emitted
		let found = frame_system::Pallet::<Test>::events().iter().any(|record| {
			println!("found event: {record:?}");
			if let mock::RuntimeEvent::MidnightSystem(
				pallet_midnight_system::Event::SystemTransactionApplied(e),
			) = &record.event
			{
				println!("system tx detected: {e:?}");
				println!("looking for owner: {:?}", dust_addr.as_slice());
				let events = extract_events(&e.serialized_system_transaction);
				for event in events.iter() {
					if event.action == CNightGeneratesDustActionType::Create {
						let owner_bytes = event.owner.0.as_le_bytes();
						if owner_bytes.as_slice() == dust_addr.as_slice() {
							return true;
						}
					}
				}
			}
			false
		});

		assert!(found, "Could not find SystemTx event with correct owner");
	});
}

#[test]
fn asset_destroy_should_emit_valid_event_if_registered() {
	new_test_ext().execute_with(|| {
		init_ledger_state();
		let (cardano_addr, dust_addr) = test_wallet_pairing();

		let utxos = vec![
			ObservedUtxo {
				header: test_header(1, 2, 0, None),
				data: ObservedUtxoData::Registration(RegistrationData {
					cardano_address: cardano_addr.clone().into(),
					dust_address: dust_addr.into(),
				}),
			},
			ObservedUtxo {
				header: test_header(2, 0, 0, None),
				data: ObservedUtxoData::AssetCreate(CreateData {
					value: 100,
					owner: cardano_addr.clone().into_inner(),
					utxo_tx_hash: tx_hash(2, 0),
					utxo_tx_index: 0,
				}),
			},
			ObservedUtxo {
				header: test_header(2, 1, 0, None),
				data: ObservedUtxoData::AssetSpend(SpendData {
					value: 100,
					owner: cardano_addr.clone().into_inner(),
					utxo_tx_hash: tx_hash(2, 0),
					utxo_tx_index: 0,
					spending_tx_hash: tx_hash(2, 1),
				}),
			},
		];

		let inherent_data = create_inherent(utxos, test_position(3, 0));
		let call = CNightObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = RuntimeCall::CNightObservation(call);
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		// Confirm the expected SystemTxCreateUtxo event was emitted
		let found = frame_system::Pallet::<Test>::events().iter().any(|record| {
			println!("found event: {record:?}");
			if let mock::RuntimeEvent::MidnightSystem(
				pallet_midnight_system::Event::SystemTransactionApplied(e),
			) = &record.event
			{
				println!("system tx detected: {e:?}");
				println!("looking for owner: {:?}", dust_addr.as_slice());
				let events = extract_events(&e.serialized_system_transaction);
				for event in events.iter() {
					if event.action == CNightGeneratesDustActionType::Destroy
						&& event.owner.0.as_le_bytes() == dust_addr.as_slice()
					{
						return true;
					}
				}
			}
			false
		});

		assert!(found, "Could not find SystemTx event with correct owner");
	});
}

#[test]
fn redemption_create_should_emit_valid_event_if_registered() {
	new_test_ext().execute_with(|| {
		init_ledger_state();
		let (cardano_addr, dust_addr) = test_wallet_pairing();

		let utxos = vec![
			ObservedUtxo {
				header: test_header(1, 2, 0, None),
				data: ObservedUtxoData::Registration(RegistrationData {
					cardano_address: cardano_addr.clone().into(),
					dust_address: dust_addr.into(),
				}),
			},
			ObservedUtxo {
				header: test_header(2, 0, 0, None),
				data: ObservedUtxoData::RedemptionCreate(RedemptionCreateData {
					value: 100,
					owner: cardano_addr.clone().into_inner(),
					utxo_tx_hash: tx_hash(1, 3),
					utxo_tx_index: 0,
				}),
			},
		];

		let inherent_data = create_inherent(utxos, test_position(3, 0));
		let call = CNightObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = RuntimeCall::CNightObservation(call);
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		// Confirm the expected SystemTxCreateUtxo event was emitted
		let found = frame_system::Pallet::<Test>::events().iter().any(|record| {
			println!("found event: {record:?}");
			if let mock::RuntimeEvent::MidnightSystem(
				pallet_midnight_system::Event::SystemTransactionApplied(e),
			) = &record.event
			{
				println!("system tx detected: {e:?}");
				println!("looking for owner: {:?}", dust_addr.as_slice());
				let events = extract_events(&e.serialized_system_transaction);
				for event in events.iter() {
					if event.action == CNightGeneratesDustActionType::Create
						&& event.owner.0.as_le_bytes() == dust_addr.as_slice()
					{
						return true;
					}
				}
			}
			false
		});

		assert!(found, "Could not find SystemTx event with correct owner");
	});
}

#[test]
fn redemption_destroy_should_emit_valid_event_if_registered() {
	new_test_ext().execute_with(|| {
		init_ledger_state();
		let (cardano_addr, dust_addr) = test_wallet_pairing();

		let utxos = vec![
			ObservedUtxo {
				header: test_header(1, 2, 0, None),
				data: ObservedUtxoData::Registration(RegistrationData {
					cardano_address: cardano_addr.clone().into(),
					dust_address: dust_addr.into(),
				}),
			},
			ObservedUtxo {
				header: test_header(2, 0, 0, None),
				data: ObservedUtxoData::RedemptionCreate(RedemptionCreateData {
					value: 100,
					owner: cardano_addr.clone().into_inner(),
					utxo_tx_hash: tx_hash(2, 0),
					utxo_tx_index: 0,
				}),
			},
			ObservedUtxo {
				header: test_header(2, 1, 0, None),
				data: ObservedUtxoData::RedemptionSpend(RedemptionSpendData {
					value: 100,
					owner: cardano_addr.clone().into_inner(),
					utxo_tx_hash: tx_hash(2, 0),
					utxo_tx_index: 0,
					spending_tx_hash: tx_hash(2, 1),
				}),
			},
		];

		let inherent_data = create_inherent(utxos, test_position(3, 0));
		let call = CNightObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = RuntimeCall::CNightObservation(call);
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		// Confirm the expected SystemTxCreateUtxo event was emitted
		let found = frame_system::Pallet::<Test>::events().iter().any(|record| {
			println!("found event: {record:?}");
			if let mock::RuntimeEvent::MidnightSystem(
				pallet_midnight_system::Event::SystemTransactionApplied(e),
			) = &record.event
			{
				println!("system tx detected: {e:?}");
				println!("looking for owner: {:?}", dust_addr.as_slice());
				let events = extract_events(&e.serialized_system_transaction);
				for event in events.iter() {
					if event.action == CNightGeneratesDustActionType::Destroy
						&& event.owner.0.as_le_bytes() == dust_addr.as_slice()
					{
						return true;
					}
				}
			}
			false
		});

		assert!(found, "Could not find SystemTx event with correct owner");
	});
}

#[test]
fn process_tokens_should_not_emit_valid_utxo_event_if_not_registered() {
	new_test_ext().execute_with(|| {
		init_ledger_state();
		let (cardano_addr, _dust_addr) = test_wallet_pairing();

		let utxos = vec![ObservedUtxo {
			header: test_header(2, 0, 0, None),
			data: ObservedUtxoData::AssetCreate(CreateData {
				value: 100,
				owner: cardano_addr.clone().into_inner(),
				utxo_tx_hash: tx_hash(1, 3),
				utxo_tx_index: 0,
			}),
		}];

		let inherent_data = create_inherent(utxos, test_position(3, 0));
		let call = CNightObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = RuntimeCall::CNightObservation(call);
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		let found = frame_system::Pallet::<Test>::events().iter().any(|record| {
			println!("event: {record:?}");
			matches!(
				record.event,
				mock::RuntimeEvent::MidnightSystem(
					pallet_midnight_system::Event::SystemTransactionApplied(_)
				)
			)
		});

		assert!(!found, "Found a SystemTx event");
	});
}

#[test]
fn process_tokens_inherent_should_update_storage_correctly() {
	new_test_ext().execute_with(|| {
		init_ledger_state();
		let (cardano_addr, dust_addr) = test_wallet_pairing();

		let utxos = vec![
			ObservedUtxo {
				header: test_header(1, 2, 0, None),
				data: ObservedUtxoData::Registration(RegistrationData {
					cardano_address: cardano_addr.clone().into(),
					dust_address: dust_addr.into(),
				}),
			},
			ObservedUtxo {
				header: test_header(2, 0, 0, None),
				data: ObservedUtxoData::AssetCreate(CreateData {
					value: 100,
					owner: cardano_addr.clone().into_inner(),
					utxo_tx_hash: tx_hash(1, 3),
					utxo_tx_index: 0,
				}),
			},
		];

		let inherent_data = create_inherent(utxos, test_position(3, 0));
		let call = CNightObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = RuntimeCall::CNightObservation(call);
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		let stored: Vec<DustAddress> = Mappings::<Test>::get(cardano_addr.clone())
			.into_iter()
			.map(|r| r.dust_address)
			.collect();
		let expected: [u8; 32] =
			dust_addr.as_slice().try_into().expect("dust addr must be 32 bytes");

		assert_eq!(stored, vec![expected]);

		let last_processed_block = NextCardanoPosition::<Test>::get();
		assert_eq!(
			test_position(3, 0),
			last_processed_block,
			"Last processed block not updated correctly"
		);
	});
}

#[test]
fn removing_duplicate_registration_results_in_valid_registration() {
	new_test_ext().execute_with(|| {
		init_ledger_state();
		let (cardano_addr, dust_addr) = test_wallet_pairing();

		let utxos = vec![ObservedUtxo {
			header: test_header(1, 2, 0, None),
			data: ObservedUtxoData::Registration(RegistrationData {
				cardano_address: cardano_addr.clone().into(),
				dust_address: dust_addr.into(),
			}),
		}];

		let inherent_data = create_inherent(utxos, test_position(3, 0));
		let call = CNightObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = RuntimeCall::CNightObservation(call);
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		// Advance block and clear events
		System::set_block_number(System::block_number() + 1);
		frame_system::Pallet::<Test>::reset_events();

		let reg_header = test_header(4, 2, 0, None);

		let utxos = vec![ObservedUtxo {
			header: reg_header.clone(),
			data: ObservedUtxoData::Registration(RegistrationData {
				cardano_address: cardano_addr.clone().into(),
				dust_address: dust_addr.into(),
			}),
		}];

		let inherent_data = create_inherent(utxos, test_position(5, 0));
		let call = CNightObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = RuntimeCall::CNightObservation(call);
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		let registration_found = frame_system::Pallet::<Test>::events().iter().any(|record| {
			if let mock::RuntimeEvent::CNightObservation(crate::Event::Registration(reg)) =
				&record.event
			{
				let expected = Registration::new(cardano_addr.clone(), dust_addr.clone().to_vec());
				*reg == expected
			} else {
				false
			}
		});
		// Registration is not emitted when a duplicate is received
		assert!(!registration_found);

		// Advance block and clear events
		System::set_block_number(System::block_number() + 1);
		frame_system::Pallet::<Test>::reset_events();

		let dereg_header = test_header(5, 0, 0, Some(tx_hash(4, 2)));

		let utxos = vec![
			ObservedUtxo {
				header: dereg_header,
				data: ObservedUtxoData::Deregistration(DeregistrationData {
					cardano_address: cardano_addr.clone().into(),
					dust_address: dust_addr.into(),
				}),
			},
			ObservedUtxo {
				header: test_header(5, 1, 0, None),
				data: ObservedUtxoData::AssetCreate(CreateData {
					value: 100,
					owner: cardano_addr.clone().into_inner(),
					utxo_tx_hash: tx_hash(1, 3),
					utxo_tx_index: 0,
				}),
			},
		];

		let inherent_data = create_inherent(utxos, test_position(5, 3));
		let call = CNightObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = RuntimeCall::CNightObservation(call);
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		let registration_found = frame_system::Pallet::<Test>::events().iter().any(|record| {
			if let mock::RuntimeEvent::CNightObservation(crate::Event::Registration(reg)) =
				&record.event
			{
				let expected = Registration::new(cardano_addr.clone(), dust_addr.clone().to_vec());
				*reg == expected
			} else {
				false
			}
		});
		assert!(registration_found);

		// Confirm the expected SystemTxCreateUtxo event was emitted
		let found = frame_system::Pallet::<Test>::events().iter().any(|record| {
			println!("found event: {record:?}");
			if let mock::RuntimeEvent::MidnightSystem(
				pallet_midnight_system::Event::SystemTransactionApplied(e),
			) = &record.event
			{
				println!("system tx detected: {e:?}");
				println!("looking for owner: {:?}", dust_addr.as_slice());
				let events = extract_events(&e.serialized_system_transaction);
				for event in events.iter() {
					if event.owner.0.as_le_bytes() == dust_addr.as_slice() {
						return true;
					}
				}
			}
			false
		});

		assert!(found, "Could not find SystemTx event with correct owner");
	});
}

// TODO: come back and enable
#[ignore]
#[test]
fn two_registrations_in_same_block_emit_no_registered_event() {
	new_test_ext().execute_with(|| {
		// Arrange
		let (cardano_addr, dust_addr) = test_wallet_pairing();

		System::set_block_number(1);

		let utxos = vec![
			ObservedUtxo {
				header: test_header(1, 2, 0, None),
				data: ObservedUtxoData::Registration(RegistrationData {
					cardano_address: cardano_addr.clone().into(),
					dust_address: dust_addr.into(),
				}),
			},
			// Duplicate!
			ObservedUtxo {
				header: test_header(1, 2, 0, None),
				data: ObservedUtxoData::Registration(RegistrationData {
					cardano_address: cardano_addr.clone().into(),
					dust_address: dust_addr.into(),
				}),
			},
		];

		let inherent_data = create_inherent(utxos, test_position(5, 3));
		let call = CNightObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = RuntimeCall::CNightObservation(call);
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		CNightObservation::on_initialize(1);
		CNightObservation::on_finalize(1);

		let saw_registered = any_event(|e| {
			matches!(e, RuntimeEvent::CNightObservation(crate::Event::Registration { .. }))
		});
		assert!(
			!saw_registered,
			"expected NO `Registered` event when two registrations land in the same block"
		);

		let mapping_added_events_count = frame_system::Pallet::<Test>::events()
			.iter()
			.filter(|r| {
				matches!(
					r.event,
					RuntimeEvent::CNightObservation(crate::Event::MappingAdded { .. })
				)
			})
			.count();

		assert_eq!(mapping_added_events_count, 2, "expected exactly two MappingAdded events");
	});
}

#[test]
fn emits_registration_and_mapping_added_on_first_valid_registration() {
	new_test_ext().execute_with(|| {
		let (cardano_addr, dust_addr) = test_wallet_pairing();
		let reg_header = test_header(10, 0, 0, None);

		let utxos = vec![ObservedUtxo {
			header: reg_header.clone(),
			data: ObservedUtxoData::Registration(RegistrationData {
				cardano_address: cardano_addr.clone().into(),
				dust_address: dust_addr.into(),
			}),
		}];

		let inherent_data = create_inherent(utxos, test_position(10, 1));
		let call = CNightObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = RuntimeCall::CNightObservation(call);
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		let expected_dust_hex = hex::encode(dust_addr.as_slice());
		let expected_utxo_hex = hex::encode(reg_header.utxo_tx_hash.0);

		// Assert: Registration + MappingAdded both emitted with correct payloads
		let mut saw_registration = false;
		let mut saw_mapping_added = false;

		for record in frame_system::Pallet::<Test>::events() {
			if let mock::RuntimeEvent::CNightObservation(crate::Event::Registration(reg)) =
				&record.event
			{
				let expected = Registration::new(cardano_addr.clone(), dust_addr.to_vec());
				assert_eq!(reg, &expected);
				saw_registration = true;
			}

			if let mock::RuntimeEvent::CNightObservation(crate::Event::MappingAdded(m)) =
				&record.event
			{
				let expected = Mapping::new(
					cardano_addr.clone(),
					expected_dust_hex.clone(),
					expected_utxo_hex.clone(),
				);
				assert_eq!(m, &expected);
				saw_mapping_added = true;
			}
		}

		assert!(saw_registration, "Registration event not found");
		assert!(saw_mapping_added, "MappingAdded event not found");
	});
}

#[test]
fn emits_deregistration_and_mapping_removed_on_last_mapping_removed() {
	new_test_ext().execute_with(|| {
		let (cardano_addr, dust_addr) = test_wallet_pairing();

		let reg_header = test_header(20, 0, 0, None);
		let utxos = vec![ObservedUtxo {
			header: reg_header.clone(),
			data: ObservedUtxoData::Registration(RegistrationData {
				cardano_address: cardano_addr.clone().into(),
				dust_address: dust_addr.into(),
			}),
		}];
		let inherent_data = create_inherent(utxos, test_position(20, 1));
		let call = CNightObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = RuntimeCall::CNightObservation(call);
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		System::set_block_number(System::block_number() + 1);
		frame_system::Pallet::<Test>::reset_events();

		// make the removal UTXO reference the registration UTXO so the MappingEntry matches
		let dereg_header = test_header(21, 0, 0, Some(reg_header.utxo_tx_hash.0));

		let utxos = vec![ObservedUtxo {
			header: dereg_header.clone(),
			data: ObservedUtxoData::Deregistration(DeregistrationData {
				cardano_address: cardano_addr.clone().into(),
				dust_address: dust_addr.into(),
			}),
		}];
		let inherent_data = create_inherent(utxos, test_position(21, 1));
		let call = CNightObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = RuntimeCall::CNightObservation(call);
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		let expected_dust_hex = hex::encode(dust_addr.as_slice());
		let expected_removed_utxo_hex = hex::encode(dereg_header.utxo_tx_hash.0);

		let mut saw_deregistration = false;
		let mut saw_mapping_removed = false;

		for record in frame_system::Pallet::<Test>::events() {
			if let mock::RuntimeEvent::CNightObservation(crate::Event::Deregistration(d)) =
				&record.event
			{
				let expected = Deregistration::new(cardano_addr.clone(), dust_addr.to_vec());
				assert_eq!(d, &expected);
				saw_deregistration = true;
			}

			if let mock::RuntimeEvent::CNightObservation(crate::Event::MappingRemoved(m)) =
				&record.event
			{
				let expected = Mapping::new(
					cardano_addr.clone(),
					expected_dust_hex.clone(),
					expected_removed_utxo_hex.clone(),
				);
				assert_eq!(m, &expected);
				saw_mapping_removed = true;
			}
		}

		assert!(saw_deregistration, "Deregistration event not found");
		assert!(saw_mapping_removed, "MappingRemoved event not found");
	});
}

// #[test]
// fn no_registered_event_when_still_invalid_after_removal() {
// 	new_test_ext().execute_with(|| {
// 		let cardano_addr = cardano_address(b"cardano_still_invalid");
// 		let dust1 = dust_address(b"dust1");
// 		let dust2 = dust_address(b"dust2");
// 		let dust3 = dust_address(b"dust3");
// 		let latest_block = 7000;

// 		let cmst_header = default_cmst_header(latest_block);

// 		// Register 3 dust addresses (invalid - too many)
// 		assert_ok!(Pallet::<Test>::process_tokens(
// 			frame_system::RawOrigin::None.into(),
// 			vec![
// 				(cardano_addr.clone().into_inner(), dust1.clone().into_inner()),
// 				(cardano_addr.clone().into_inner(), dust2.clone().into_inner()),
// 				(cardano_addr.clone().into_inner(), dust3.clone().into_inner()),
// 			],
// 			vec![],
// 			vec![],
// 			vec![],
// 			cmst_header.clone(),
// 		));

// 		// Advance block to isolate next action
// 		System::set_block_number(System::block_number() + 1);
// 		frame_system::Pallet::<Test>::reset_events();

// 		let events_first = frame_system::Pallet::<Test>::events();
// 		assert_eq!(events_first.len(), 0, "Expected no events after invalid registration");

// 		// Remove 1 dust address: 3 → 2 (still invalid)
// 		assert_ok!(Pallet::<Test>::process_tokens(
// 			frame_system::RawOrigin::None.into(),
// 			vec![],
// 			vec![(cardano_addr.clone().into_inner(), dust2.clone().into_inner())],
// 			vec![],
// 			vec![],
// 			cmst_header,
// 		));

// 		let events = frame_system::Pallet::<Test>::events();

// 		// Should NOT emit Registered event since 2 registrations still exceeds limit
// 		let re_registered_found = events.iter().any(|record| {
// 			matches!(
// 				&record.event,
// 				mock::RuntimeEvent::CNightObservation(crate::Event::Registered(e))
// 					if e.0 == cardano_addr
// 			)
// 		});

// 		assert!(
// 			!re_registered_found,
// 			"Should NOT emit Registered event when still invalid after removal"
// 		);
// 	});
// }

//
// #[test]
// fn specific_registration_is_removed_correctly() {
// 	new_test_ext().execute_with(|| {
// 		let cardano_addr = cardano_address(b"cardanoX");
// 		let dust_addresses: BoundedVec<
// 			BoundedVec<u8, ConstU32<32>>,
// 			MaxRegistrationsPerCardanoAddress,
// 		> = bounded_vec![
// 			dust_address(b"dust0"),
// 			dust_address(b"dust1"),
// 			dust_address(b"dust2"),
// 			dust_address(b"dust3"),
// 			dust_address(b"dust4")
// 		];
// 		let latest_block = 12345;
//
// 		// Insert all five as initial registrations manually
// 		Registrations::<Test>::insert(cardano_addr.clone(), dust_addresses.clone());
//
// 		// Create a mock CMST header
// 		let cmst_header = default_cmst_header(latest_block);
//
// 		// Remove dust2
// 		let to_remove = (cardano_addr.clone().into_inner(), dust_address(b"dust2").into_inner());
// 		assert_ok!(Pallet::<Test>::process_tokens(
// 			frame_system::RawOrigin::None.into(),
// 			vec![],          // new registrations
// 			vec![to_remove], // removals
// 			vec![],          // utxos
// 			vec![],          // system txs
// 			cmst_header
// 		));
//
// 		let updated = CNightObservation::get_registrations_for(cardano_addr.clone());
//
// 		// Assert it no longer includes dust2
// 		assert!(!updated.contains(&dust_address(b"dust2")), "dust2 should be removed");
//
// 		// Assert it still includes the others
// 		assert!(updated.contains(&dust_address(b"dust0")));
// 		assert!(updated.contains(&dust_address(b"dust1")));
// 		assert!(updated.contains(&dust_address(b"dust3")));
// 		assert!(updated.contains(&dust_address(b"dust4")));
//
// 		// Assert correct length (should now be 4)
// 		assert_eq!(updated.len(), 4);
// 	});
// }
//
// #[test]
// fn is_registered_should_return_true_for_registered_wallet() {
// 	new_test_ext().execute_with(|| {
// 		let addr = BoundedVec::try_from(b"cardano3".to_vec()).unwrap();
// 		let storage_values_before: BoundedVec<
// 			BoundedVec<u8, ConstU32<32>>,
// 			MaxRegistrationsPerCardanoAddress,
// 		> = bounded_vec![dust_address(b"dustA")];
// 		let storage_values_after: BoundedVec<
// 			BoundedVec<u8, ConstU32<32>>,
// 			MaxRegistrationsPerCardanoAddress,
// 		> = bounded_vec![dust_address(b"dustA"), dust_address(b"dustB")];
//
// 		Registrations::<Test>::insert(addr.clone(), storage_values_before);
// 		assert!(CNightObservation::is_registered(&addr));
// 		// Registrations are unique by cardano wallet address. This is considered invalid
// 		Registrations::<Test>::insert(addr.clone(), storage_values_after);
// 		assert!(!CNightObservation::is_registered(&addr));
// 	});
// }
//
// #[test]
// fn oldest_registration_should_be_evicted_when_capacity_reached() {
// 	new_test_ext().execute_with(|| {
// 		let cardano_addr = cardano_address(b"cardano_eviction");
// 		let latest_block = 9999;
//
// 		let cmst_header = default_cmst_header(latest_block);
//
// 		// Initial registration
// 		assert_ok!(Pallet::<Test>::process_tokens(
// 			frame_system::RawOrigin::None.into(),
// 			vec![(cardano_addr.clone().into_inner(), dust_address(b"dust-0").into_inner())],
// 			vec![],
// 			vec![],
// 			vec![],
// 			cmst_header.clone()
// 		));
//
// 		let updated = CNightObservation::get_registrations_for(cardano_addr.clone());
// 		assert!(updated.contains(&dust_address(b"dust-0")));
//
// 		let new_dust = dust_address(b"dust-1");
//
// 		// Fill to capacity with duplicates of the new dust address (causing evictions)
// 		for _ in 0..MaxRegistrationsPerCardanoAddress::get() {
// 			assert_ok!(Pallet::<Test>::process_tokens(
// 				frame_system::RawOrigin::None.into(),
// 				vec![(cardano_addr.clone().into_inner(), new_dust.clone().into_inner())],
// 				vec![],
// 				vec![],
// 				vec![],
// 				cmst_header.clone()
// 			));
// 		}
//
// 		let updated = CNightObservation::get_registrations_for(cardano_addr.clone());
//
// 		// Expect dust-0 to be evicted
// 		assert!(!updated.contains(&dust_address(b"dust-0")), "dust-0 should have been evicted");
//
// 		// Expect dust-1 to be retained
// 		assert!(updated.contains(&dust_address(b"dust-1")), "dust-1 should still be present");
//
// 		// Ensure we're at max capacity
// 		assert_eq!(updated.len(), MaxRegistrationsPerCardanoAddress::get() as usize);
// 	});
// }
//
// #[test]
// fn registered_event_emitted_only_once_per_cardano_address() {
// 	new_test_ext().execute_with(|| {
// 		let cardano_addr = cardano_address(b"cardano_once");
// 		let dust1 = dust_address(b"dust1");
// 		let dust2 = dust_address(b"dust2");
// 		let dust3 = dust_address(b"dust3");
// 		let latest_block = 7777;
// 		let cmst_header = default_cmst_header(latest_block);
//
// 		// Add the first (valid) registration
// 		assert_ok!(Pallet::<Test>::process_tokens(
// 			frame_system::RawOrigin::None.into(),
// 			vec![(cardano_addr.clone().into_inner(), dust1.clone().into_inner())],
// 			vec![],
// 			vec![],
// 			vec![],
// 			cmst_header.clone()
// 		));
//
// 		// Add more dust addresses to same Cardano address (now invalid as per `is_registered`)
// 		assert_ok!(Pallet::<Test>::process_tokens(
// 			frame_system::RawOrigin::None.into(),
// 			vec![(cardano_addr.clone().into_inner(), dust2.clone().into_inner())],
// 			vec![],
// 			vec![],
// 			vec![],
// 			cmst_header.clone()
// 		));
// 		assert_ok!(Pallet::<Test>::process_tokens(
// 			frame_system::RawOrigin::None.into(),
// 			vec![(cardano_addr.clone().into_inner(), dust3.clone().into_inner())],
// 			vec![],
// 			vec![],
// 			vec![],
// 			cmst_header
// 		));
//
// 		let events = frame_system::Pallet::<Test>::events();
//
// 		// Count number of Registered events emitted for this cardano address
// 		let registered_event_count = events
// 			.iter()
// 			.filter(|record| {
// 				matches!(
// 					&record.event,
// 					mock::RuntimeEvent::CNightObservation(crate::Event::Registered(e))
// 					if e.0 == cardano_addr
// 				)
// 			})
// 			.count();
//
// 		assert_eq!(
// 			registered_event_count, 1,
// 			"Registered event should only be emitted once for a valid Cardano address"
// 		);
// 	});
// }
//
// #[test]
// fn removed_old_event_emitted_when_eviction_occurs() {
// 	new_test_ext().execute_with(|| {
// 		let cardano_addr = cardano_address(b"cardano_removed_old");
// 		let latest_block = 1234;
// 		let cmst_header = default_cmst_header(latest_block);
//
// 		for i in 0..MaxRegistrationsPerCardanoAddress::get() {
// 			let dust = dust_address(&[i]);
// 			assert_ok!(Pallet::<Test>::process_tokens(
// 				frame_system::RawOrigin::None.into(),
// 				vec![(cardano_addr.clone().into_inner(), dust.clone().into_inner())],
// 				vec![],
// 				vec![],
// 				vec![],
// 				cmst_header.clone()
// 			));
// 		}
//
// 		System::set_block_number(System::block_number() + 1);
// 		frame_system::Pallet::<Test>::reset_events();
//
// 		let new_dust = dust_address(b"newer");
// 		assert_ok!(Pallet::<Test>::process_tokens(
// 			frame_system::RawOrigin::None.into(),
// 			vec![(cardano_addr.clone().into_inner(), new_dust.clone().into_inner())],
// 			vec![],
// 			vec![],
// 			vec![],
// 			cmst_header
// 		));
//
// 		let events = frame_system::Pallet::<Test>::events();
// 		let found = events.iter().any(|record| {
// 			matches!(
// 				&record.event,
// 				mock::RuntimeEvent::CNightObservation(
// 					crate::Event::RemovedOld((addr, _))
// 				) if addr == &cardano_addr
// 			)
// 		});
//
// 		assert!(found, "Expected RemovedOld event not found");
// 	});
// }
//
// #[test]
// fn attempted_remove_nonexistent_emits_event() {
// 	new_test_ext().execute_with(|| {
// 		let cardano_addr = cardano_address(b"cardano_nonexistent_removal");
// 		let dust_present = dust_address(b"present");
// 		let dust_missing = dust_address(b"missing");
// 		let latest_block = 2222;
// 		let cmst_header = default_cmst_header(latest_block);
//
// 		let dust_address: BoundedVec<
// 			BoundedVec<u8, ConstU32<32>>,
// 			MaxRegistrationsPerCardanoAddress,
// 		> = bounded_vec![dust_present];
//
// 		Registrations::<Test>::insert(cardano_addr.clone(), dust_address);
//
// 		assert_ok!(Pallet::<Test>::process_tokens(
// 			frame_system::RawOrigin::None.into(),
// 			vec![],
// 			vec![(cardano_addr.clone().into_inner(), dust_missing.clone().into_inner())],
// 			vec![],
// 			vec![],
// 			cmst_header
// 		));
//
// 		let events = frame_system::Pallet::<Test>::events();
// 		let found = events.iter().any(|record| {
// 			matches!(
// 				&record.event,
// 				mock::RuntimeEvent::CNightObservation(
// 					crate::Event::AttemptedRemoveNonexistantElement
// 				)
// 			)
// 		});
//
// 		assert!(found, "Expected AttemptedRemoveNonexistantElement event not found");
// 	});
// }
//
// #[test]
// fn invalid_cardano_and_dust_address_should_emit_respective_events() {
// 	new_test_ext().execute_with(|| {
// 		let latest_block = 4444;
// 		let cmst_header = default_cmst_header(latest_block);
//
// 		// First: test invalid Cardano address (Dust is valid)
// 		let too_long_cardano = vec![0u8; MaxCardanoAddrLen::get() as usize + 1];
// 		let valid_dust = vec![1u8; 32];
//
// 		assert_ok!(Pallet::<Test>::process_tokens(
// 			frame_system::RawOrigin::None.into(),
// 			vec![(too_long_cardano.clone(), valid_dust.clone())],
// 			vec![],
// 			vec![],
// 			vec![],
// 			cmst_header.clone()
// 		));
//
// 		let events = frame_system::Pallet::<Test>::events();
// 		assert!(
// 			events.iter().any(|record| matches!(
// 				&record.event,
// 				mock::RuntimeEvent::CNightObservation(crate::Event::InvalidCardanoAddress)
// 			)),
// 			"Expected InvalidCardanoAddress event"
// 		);
//
// 		frame_system::Pallet::<Test>::reset_events();
//
// 		// Then: test invalid Dust address (Cardano is valid)
// 		let valid_cardano = vec![1u8; MaxCardanoAddrLen::get() as usize];
// 		let too_long_dust = vec![9u8; 33];
//
// 		assert_ok!(Pallet::<Test>::process_tokens(
// 			frame_system::RawOrigin::None.into(),
// 			vec![(valid_cardano.clone(), too_long_dust.clone())],
// 			vec![],
// 			vec![],
// 			vec![],
// 			cmst_header
// 		));
//
// 		let events = frame_system::Pallet::<Test>::events();
// 		assert!(
// 			events.iter().any(|record| matches!(
// 				&record.event,
// 				mock::RuntimeEvent::CNightObservation(crate::Event::InvalidDustAddress)
// 			)),
// 			"Expected InvalidDustAddress event"
// 		);
// 	});
// }
//
// #[test]
// fn added_event_emitted_for_each_dust_mapping_created() {
// 	new_test_ext().execute_with(|| {
// 		let latest_block = 3000;
// 		let cmst_header = default_cmst_header(latest_block);
//
// 		let registrations = vec![
// 			(cardano_address(b"cardanoA"), dust_address(b"dustA")),
// 			(cardano_address(b"cardanoB"), dust_address(b"dustB")),
// 			(cardano_address(b"cardanoC"), dust_address(b"dustC")),
// 		];
//
// 		let new_registrations: Vec<(Vec<u8>, Vec<u8>)> = registrations
// 			.iter()
// 			.map(|(c, d)| (c.clone().into_inner(), d.clone().into_inner()))
// 			.collect();
//
// 		assert_ok!(Pallet::<Test>::process_tokens(
// 			frame_system::RawOrigin::None.into(),
// 			new_registrations,
// 			vec![],
// 			vec![],
// 			vec![],
// 			cmst_header.clone()
// 		));
//
// 		let events = frame_system::Pallet::<Test>::events();
//
// 		for (cardano, _) in &registrations {
// 			let found = events.iter().any(|record| {
// 				matches!(
// 					&record.event,
// 					mock::RuntimeEvent::CNightObservation(crate::Event::Added((addr, _)))
// 					if addr == cardano
// 				)
// 			});
// 			assert!(found, "Expected Added event for {:?}", cardano);
// 		}
//
// 		// Clear events and advance block
// 		System::set_block_number(System::block_number() + 1);
// 		frame_system::Pallet::<Test>::reset_events();
//
// 		// Add one more registration
// 		let extra_cardano = cardano_address(b"cardanoD");
// 		let extra_dust = dust_address(b"dustD");
//
// 		assert_ok!(Pallet::<Test>::process_tokens(
// 			frame_system::RawOrigin::None.into(),
// 			vec![(extra_cardano.clone().into_inner(), extra_dust.clone().into_inner())],
// 			vec![],
// 			vec![],
// 			vec![],
// 			cmst_header
// 		));
//
// 		let events_after = frame_system::Pallet::<Test>::events();
// 		let added_found = events_after.iter().any(|record| {
// 			matches!(
// 				&record.event,
// 				mock::RuntimeEvent::CNightObservation(crate::Event::Added((addr, _)))
// 				if addr == &extra_cardano
// 			)
// 		});
//
// 		assert!(added_found, "Expected Added event for {:?}", extra_cardano);
// 	});
// }
// #[test]
// fn removed_event_emitted_for_each_dust_mapping_removal() {
// 	new_test_ext().execute_with(|| {
// 		let latest_block = 3141;
// 		let cmst_header = default_cmst_header(latest_block);
//
// 		let cardano_addr = cardano_address(b"cardano_to_remove");
// 		let dust1 = dust_address(b"remove1");
// 		let dust2 = dust_address(b"remove2");
// 		let dust3 = dust_address(b"remove3");
// 		let dust4 = dust_address(b"remove4"); // Used later
//
// 		let prefill: BoundedVec<BoundedVec<u8, ConstU32<32>>, MaxRegistrationsPerCardanoAddress> =
// 			bounded_vec![dust1.clone(), dust2.clone(), dust3.clone(), dust4.clone()];
// 		Registrations::<Test>::insert(cardano_addr.clone(), prefill);
//
// 		// Remove dust1 and dust2
// 		let removals = vec![
// 			(cardano_addr.clone().into_inner(), dust1.clone().into_inner()),
// 			(cardano_addr.clone().into_inner(), dust2.clone().into_inner()),
// 		];
//
// 		assert_ok!(Pallet::<Test>::process_tokens(
// 			frame_system::RawOrigin::None.into(),
// 			vec![],
// 			removals.clone(),
// 			vec![],
// 			vec![],
// 			cmst_header.clone()
// 		));
//
// 		let events = frame_system::Pallet::<Test>::events();
// 		for (cardano, _) in &removals {
// 			let cardano_bounded: BoundedCardanoAddress<Test> =
// 				BoundedVec::try_from(cardano.clone()).unwrap();
// 			let found = events.iter().any(|record| {
// 				matches!(
// 					&record.event,
// 					mock::RuntimeEvent::CNightObservation(crate::Event::Removed((addr, _)))
// 					if addr == &cardano_bounded
// 				)
// 			});
// 			assert!(found, "Expected Removed event for {:?}", cardano_bounded);
// 		}
//
// 		// Advance block and clear events
// 		System::set_block_number(System::block_number() + 1);
// 		frame_system::Pallet::<Test>::reset_events();
//
// 		// Remove dust3
// 		assert_ok!(Pallet::<Test>::process_tokens(
// 			frame_system::RawOrigin::None.into(),
// 			vec![],
// 			vec![(cardano_addr.clone().into_inner(), dust3.clone().into_inner())],
// 			vec![],
// 			vec![],
// 			cmst_header
// 		));
//
// 		let events_after = frame_system::Pallet::<Test>::events();
// 		let removed_found = events_after.iter().any(|record| {
// 			matches!(
// 				&record.event,
// 				mock::RuntimeEvent::CNightObservation(crate::Event::Removed((addr, _)))
// 				if addr == &cardano_addr
// 			)
// 		});
//
// 		assert!(removed_found, "Expected Removed event for {:?}", cardano_addr);
// 	});
// }
//
// #[test]
// fn decode_len_should_differ_between_empty_vec_and_removed_key() {
// 	new_test_ext().execute_with(|| {
// 		let cardano_addr = cardano_address(b"cardano_decode_test");
// 		let dust_addr = dust_address(b"dustA");
// 		let latest_block = 6000;
// 		let cmst_header = default_cmst_header(latest_block);
//
// 		// Add a registration
// 		assert_ok!(Pallet::<Test>::process_tokens(
// 			frame_system::RawOrigin::None.into(),
// 			vec![(cardano_addr.clone().into_inner(), dust_addr.clone().into_inner())],
// 			vec![],
// 			vec![],
// 			vec![],
// 			cmst_header
// 		));
//
// 		// Manually reduce the registration to an empty vec
// 		Registrations::<Test>::insert(
// 			cardano_addr.clone(),
// 			BoundedVec::<_, MaxRegistrationsPerCardanoAddress>::default(),
// 		);
//
// 		// Ensure decode_len sees a zero-length vec (still occupies storage)
// 		let len = Registrations::<Test>::decode_len(cardano_addr.clone());
// 		assert_eq!(len, Some(0), "Empty vec still encoded in storage");
//
// 		// Now actually remove the key
// 		Registrations::<Test>::remove(cardano_addr.clone());
//
// 		// decode_len should now return None (key no longer present)
// 		let len_after_removal = Registrations::<Test>::decode_len(cardano_addr.clone());
// 		assert_eq!(len_after_removal, None, "Key removed entirely from storage");
// 	});
// }
