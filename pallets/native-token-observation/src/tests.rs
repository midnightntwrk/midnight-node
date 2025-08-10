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

use self::mock::NativeTokenObservation;
use self::mock::Test;
use crate::mock::new_test_ext;
use crate::mock::{RuntimeCall, System};
// use crate::mock::MaxRegistrationsPerCardanoAddress;
use crate::*;
use frame_support::sp_runtime::traits::Dispatchable;
// use frame_support::testing_prelude::bounded_vec;
use frame_support::{BoundedVec, assert_ok};
use midnight_primitives_mainchain_follower::CreateData;
use midnight_primitives_mainchain_follower::DeregistrationData;
use midnight_primitives_mainchain_follower::ObservedUtxo;
use midnight_primitives_mainchain_follower::ObservedUtxoData;
use midnight_primitives_mainchain_follower::ObservedUtxoHeader;
use midnight_primitives_mainchain_follower::RegistrationData;
use midnight_primitives_mainchain_follower::UtxoIndexInTx;
// use midnight_primitives_mainchain_follower::data_source::ObservedUtxos;
use rand::prelude::*;

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
	rng.random()
}

fn block_hash(block_number: u32) -> [u8; 32] {
	let mut seed = [0u8; 32];
	seed[0..4].copy_from_slice(&block_number.to_be_bytes());
	let mut rng = rand::rngs::StdRng::from_seed(seed);
	rng.random()
}

fn test_position(block_number: u32, tx_index_in_block: u32) -> CardanoPosition {
	CardanoPosition { block_hash: block_hash(block_number), block_number, tx_index_in_block }
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

fn testbvec<S: Get<u32>>(input: &[u8]) -> BoundedVec<u8, S> {
	BoundedVec::try_from(input.to_vec()).unwrap()
}

// Onchain dust address
fn dust_address(input: &[u8]) -> BoundedVec<u8, ConstU32<32>> {
	testbvec::<ConstU32<32>>(input)
}

// Onchain cardano address
fn cardano_address(input: &[u8]) -> BoundedVec<u8, ConstU32<MAX_CARDANO_ADDR_LEN>> {
	testbvec::<ConstU32<MAX_CARDANO_ADDR_LEN>>(input)
}

fn test_wallet_pairing()
-> (BoundedVec<u8, ConstU32<MAX_CARDANO_ADDR_LEN>>, BoundedVec<u8, ConstU32<32>>) {
	(cardano_address(b"cardano1"), dust_address(b"dust1"))
}

#[test]
fn asset_create_should_emit_valid_event_if_registered() {
	new_test_ext().execute_with(|| {
		let (cardano_addr, dust_addr) = test_wallet_pairing();

		let utxos = vec![
			ObservedUtxo {
				header: test_header(1, 2, 0, None),
				data: ObservedUtxoData::Registration(RegistrationData {
					cardano_address: cardano_addr.clone().into(),
					dust_address: dust_addr.clone().into(),
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
		let call = NativeTokenObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = RuntimeCall::NativeTokenObservation(call);
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		// Confirm the expected SystemTxCreateUtxo event was emitted
		let found = frame_system::Pallet::<Test>::events().iter().any(|record| {
			println!("found event: {record:?}");
			if let mock::RuntimeEvent::NativeTokenObservation(crate::Event::SystemTx(e)) =
				&record.event
			{
				println!("system tx detected: {e:?}");
				println!("looking for owner: {:?}", dust_addr.as_slice());
				for event in e.body.events.iter() {
					if event.owner.as_slice() == dust_addr.as_slice() {
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
		let call = NativeTokenObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = RuntimeCall::NativeTokenObservation(call);
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		let found = frame_system::Pallet::<Test>::events().iter().any(|record| {
			println!("event: {record:?}");
			matches!(
				record.event,
				mock::RuntimeEvent::NativeTokenObservation(crate::Event::SystemTx(_))
			)
		});

		assert!(!found, "Found a SystemTx event");
	});
}

#[test]
fn process_tokens_inherent_should_update_storage_correctly() {
	new_test_ext().execute_with(|| {
		let (cardano_addr, dust_addr) = test_wallet_pairing();

		let utxos = vec![
			ObservedUtxo {
				header: test_header(1, 2, 0, None),
				data: ObservedUtxoData::Registration(RegistrationData {
					cardano_address: cardano_addr.clone().into(),
					dust_address: dust_addr.clone().into(),
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
		let call = NativeTokenObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = RuntimeCall::NativeTokenObservation(call);
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		let stored = NativeTokenObservation::get_registrations_for(cardano_addr.clone());
		assert_eq!(stored, vec![dust_addr.clone()]);

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
		let (cardano_addr, dust_addr) = test_wallet_pairing();

		let utxos = vec![ObservedUtxo {
			header: test_header(1, 2, 0, None),
			data: ObservedUtxoData::Registration(RegistrationData {
				cardano_address: cardano_addr.clone().into(),
				dust_address: dust_addr.clone().into(),
			}),
		}];

		let inherent_data = create_inherent(utxos, test_position(3, 0));
		let call = NativeTokenObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = RuntimeCall::NativeTokenObservation(call);
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		// Advance block and clear events
		System::set_block_number(System::block_number() + 1);
		frame_system::Pallet::<Test>::reset_events();

		let reg_header = test_header(4, 2, 0, None);

		let utxos = vec![ObservedUtxo {
			header: reg_header.clone(),
			data: ObservedUtxoData::Registration(RegistrationData {
				cardano_address: cardano_addr.clone().into(),
				dust_address: dust_addr.clone().into(),
			}),
		}];

		let inherent_data = create_inherent(utxos, test_position(5, 0));
		let call = NativeTokenObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = RuntimeCall::NativeTokenObservation(call);
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		// Advance block and clear events
		System::set_block_number(System::block_number() + 1);
		frame_system::Pallet::<Test>::reset_events();

		let dereg_header = test_header(5, 0, 0, Some(tx_hash(4, 2)));

		let utxos = vec![
			ObservedUtxo {
				header: dereg_header,
				data: ObservedUtxoData::Deregistration(DeregistrationData {
					cardano_address: cardano_addr.clone().into(),
					dust_address: dust_addr.clone().into(),
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
		let call = NativeTokenObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");
		let call = RuntimeCall::NativeTokenObservation(call);
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		// Confirm the expected SystemTxCreateUtxo event was emitted
		let found = frame_system::Pallet::<Test>::events().iter().any(|record| {
			println!("found event: {record:?}");
			if let mock::RuntimeEvent::NativeTokenObservation(crate::Event::SystemTx(e)) =
				&record.event
			{
				println!("system tx detected: {e:?}");
				println!("looking for owner: {:?}", dust_addr.as_slice());
				for event in e.body.events.iter() {
					if event.owner.as_slice() == dust_addr.as_slice() {
						return true;
					}
				}
			}
			false
		});

		assert!(found, "Could not find SystemTx event with correct owner");
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
//
// 		let cmst_header = default_cmst_header(latest_block);
//
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
//
// 		// Advance block to isolate next action
// 		System::set_block_number(System::block_number() + 1);
// 		frame_system::Pallet::<Test>::reset_events();
//
// 		let events_first = frame_system::Pallet::<Test>::events();
// 		assert_eq!(events_first.len(), 0, "Expected no events after invalid registration");
//
// 		// Remove 1 dust address: 3 → 2 (still invalid)
// 		assert_ok!(Pallet::<Test>::process_tokens(
// 			frame_system::RawOrigin::None.into(),
// 			vec![],
// 			vec![(cardano_addr.clone().into_inner(), dust2.clone().into_inner())],
// 			vec![],
// 			vec![],
// 			cmst_header,
// 		));
//
// 		let events = frame_system::Pallet::<Test>::events();
//
// 		// Should NOT emit Registered event since 2 registrations still exceeds limit
// 		let re_registered_found = events.iter().any(|record| {
// 			matches!(
// 				&record.event,
// 				mock::RuntimeEvent::NativeTokenObservation(crate::Event::Registered(e))
// 					if e.0 == cardano_addr
// 			)
// 		});
//
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
// 		let updated = NativeTokenObservation::get_registrations_for(cardano_addr.clone());
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
// 		assert!(NativeTokenObservation::is_registered(&addr));
// 		// Registrations are unique by cardano wallet address. This is considered invalid
// 		Registrations::<Test>::insert(addr.clone(), storage_values_after);
// 		assert!(!NativeTokenObservation::is_registered(&addr));
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
// 		let updated = NativeTokenObservation::get_registrations_for(cardano_addr.clone());
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
// 		let updated = NativeTokenObservation::get_registrations_for(cardano_addr.clone());
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
// 					mock::RuntimeEvent::NativeTokenObservation(crate::Event::Registered(e))
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
// 				mock::RuntimeEvent::NativeTokenObservation(
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
// 				mock::RuntimeEvent::NativeTokenObservation(
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
// 				mock::RuntimeEvent::NativeTokenObservation(crate::Event::InvalidCardanoAddress)
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
// 				mock::RuntimeEvent::NativeTokenObservation(crate::Event::InvalidDustAddress)
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
// 					mock::RuntimeEvent::NativeTokenObservation(crate::Event::Added((addr, _)))
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
// 				mock::RuntimeEvent::NativeTokenObservation(crate::Event::Added((addr, _)))
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
// 					mock::RuntimeEvent::NativeTokenObservation(crate::Event::Removed((addr, _)))
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
// 				mock::RuntimeEvent::NativeTokenObservation(crate::Event::Removed((addr, _)))
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
