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
use crate::mock::{MaxCardanoAddrLen, MaxRegistrationsPerCardanoAddress, RuntimeCall, System};
use crate::*;
use frame_support::sp_runtime::traits::Dispatchable;
use frame_support::testing_prelude::bounded_vec;
use frame_support::{BoundedVec, assert_ok};

fn default_inherent_data(
	new: Vec<(BoundedVec<u8, MaxCardanoAddrLen>, BoundedVec<u8, ConstU32<32>>)>,
	remove: Vec<(BoundedVec<u8, MaxCardanoAddrLen>, BoundedVec<u8, ConstU32<32>>)>,
	utxos: Vec<MidnightRuntimeUtxoRepresentation>,
	latest_block: i64,
) -> InherentData {
	let mut inherent_data = InherentData::new();
	let movement = MidnightObservationTokenMovement {
		new_registrations: new.into_iter().map(|i| (i.0.into_inner(), i.1.into_inner())).collect(),
		registrations_to_remove: remove
			.into_iter()
			.map(|i| (i.0.into_inner(), i.1.into_inner()))
			.collect(),
		utxos,
		latest_block,
	};
	inherent_data
		.put_data(INHERENT_IDENTIFIER, &movement)
		.expect("inherent data insertion should not fail");
	inherent_data
}

fn testbvec<S: Get<u32>>(input: &[u8]) -> BoundedVec<u8, S> {
	BoundedVec::try_from(input.to_vec()).unwrap()
}

// Onchain dust address
fn dust_address(input: &[u8]) -> BoundedVec<u8, ConstU32<32>> {
	testbvec::<ConstU32<32>>(input)
}

// Onchain cardano address
fn cardano_address(input: &[u8]) -> BoundedVec<u8, MaxCardanoAddrLen> {
	testbvec::<MaxCardanoAddrLen>(input)
}

fn test_wallet_pairing() -> (BoundedVec<u8, MaxCardanoAddrLen>, BoundedVec<u8, ConstU32<32>>) {
	(cardano_address(b"cardano1"), dust_address(b"dust1"))
}

#[test]
fn process_tokens_inherent_should_update_storage_correctly() {
	new_test_ext().execute_with(|| {
		let (cardano_addr, dust_addr) = test_wallet_pairing();
		let latest_block = 5000;

		// Set initial block
		LastCardanoBlock::<Test>::set(4000);

		let inherent_data = default_inherent_data(
			vec![(cardano_addr.clone(), dust_addr.clone())],
			vec![],
			vec![],
			latest_block,
		);

		let call = NativeTokenObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");

		let call = RuntimeCall::NativeTokenObservation(call);

		// Dispatch the call
		assert_ok!(call.dispatch(frame_system::RawOrigin::None.into()));

		// Should have stored the new registration
		let stored = NativeTokenObservation::get_registrations_for(cardano_addr.clone());
		assert_eq!(stored, vec![dust_addr.clone()]);

		// LastCardanoBlock should have advanced by window size
		let expected_new_block = 4000 + INITIAL_CARDANO_BLOCK_WINDOW_SIZE;
		assert_eq!(LastCardanoBlock::<Test>::get(), expected_new_block);
	});
}

#[test]
fn cardano_block_window_size_should_shrink_when_approaching_tip() {
	new_test_ext().execute_with(|| {
		// Initial values
		LastCardanoBlock::<Test>::set(9000);
		CardanoBlockWindowSize::<Test>::set(1000);

		let latest_block = 9500;

		let inherent_data = default_inherent_data(vec![], vec![], vec![], latest_block);

		let call = NativeTokenObservation::create_inherent(&inherent_data)
			.expect("Expected to create inherent call");

		let runtime_call = RuntimeCall::NativeTokenObservation(call);

		assert_ok!(runtime_call.dispatch(frame_system::RawOrigin::None.into()));

		// Window size should shrink to 500
		assert_eq!(CardanoBlockWindowSize::<Test>::get(), 500);

		// LastCardanoBlock should move forward by 500
		assert_eq!(LastCardanoBlock::<Test>::get(), 9000 + 500);
	});
}

#[test]
fn process_tokens_should_emit_valid_utxo_event_if_registered() {
	new_test_ext().execute_with(|| {
		let (cardano_addr, dust_addr) = test_wallet_pairing();
		let latest_block = 10000;
		let storage_values: BoundedVec<
			BoundedVec<u8, ConstU32<32>>,
			MaxRegistrationsPerCardanoAddress,
		> = bounded_vec![dust_addr.clone()];

		// Pre-register the holder
		Registrations::<Test>::insert(cardano_addr.clone(), storage_values);
		let utxo = MidnightRuntimeUtxoRepresentation::new(100, cardano_addr.clone().into_inner());

		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![],
			vec![],
			vec![utxo],
			latest_block,
		));

		// Should have emitted a ValidUtxo event
		let events = frame_system::Pallet::<Test>::events();

		let mut found = false;
		for record in events {
			if let mock::RuntimeEvent::NativeTokenObservation(crate::Event::ValidUtxo(e)) =
				record.event
			{
				assert_eq!(e.holder_address, cardano_addr.clone().into_inner());
				found = true;
			}
		}
		assert!(found, "Expected ValidUtxo event not found");
	});
}

#[test]
fn new_registration_allows_immediate_utxo() {
	new_test_ext().execute_with(|| {
		let (cardano_addr, dust_addr) = test_wallet_pairing();
		let latest_block = 10000;

		let utxo = MidnightRuntimeUtxoRepresentation::new(100, cardano_addr.clone().into_inner());

		// Register in the same block
		let new_registrations =
			vec![(cardano_addr.clone().into_inner(), dust_addr.clone().into_inner())];

		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			new_registrations,
			vec![],
			vec![utxo.clone()],
			latest_block
		));

		let events = frame_system::Pallet::<Test>::events();
		let found = events.iter().any(|record| {
			matches!(
				&record.event,
				mock::RuntimeEvent::NativeTokenObservation(
					crate::Event::ValidUtxo(e)
				) if e.holder_address == cardano_addr.clone().into_inner()
			)
		});
		assert!(found, "Expected ValidUtxo event for newly registered address");

		let registered_found = events.iter().any(|record| {
			matches!(
				&record.event,
				mock::RuntimeEvent::NativeTokenObservation(
					crate::Event::Registered(e)
				) if e.0 == cardano_addr
			)
		});
		assert!(registered_found, "Expected Registered event for new valid registration")
	});
}

#[test]
fn invalid_registration_is_honored_within_same_block() {
	new_test_ext().execute_with(|| {
		let (cardano_addr, dust_addr) = test_wallet_pairing();
		let latest_block = 10000;

		let utxo = MidnightRuntimeUtxoRepresentation::new(100, cardano_addr.clone().into_inner());

		// Register in the same block
		let new_registrations = vec![
			(cardano_addr.clone().into_inner(), dust_addr.clone().into_inner()),
			// Second registration.
			(cardano_addr.clone().into_inner(), dust_addr.clone().into_inner()),
		];

		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			new_registrations,
			vec![],
			vec![utxo.clone()],
			latest_block
		));

		let events = frame_system::Pallet::<Test>::events();

		let found = events.iter().any(|record| {
			matches!(
				&record.event,
				mock::RuntimeEvent::NativeTokenObservation(
					crate::Event::DuplicatedRegistration(e)
				) if e.0 == cardano_addr
			)
		});
		assert!(found, "Expected DuplicatedRegistration event for newly registered address");

		let system_tx_found = events.iter().any(|record| {
			matches!(
				&record.event,
				mock::RuntimeEvent::NativeTokenObservation(crate::Event::ValidUtxo(_))
			)
		});
		assert!(
			!system_tx_found,
			"Unexpected ValidUtxo event emitted despite duplicate registration"
		);
	});
}

#[test]
fn lowering_to_one_registration() {
	new_test_ext().execute_with(|| {
		let (cardano_addr, dust_addr) = test_wallet_pairing();
		let latest_block = 10000;

		let utxo = MidnightRuntimeUtxoRepresentation::new(100, cardano_addr.clone().into_inner());

		// Register in the same block
		let new_registrations = vec![
			(cardano_addr.clone().into_inner(), dust_addr.clone().into_inner()),
			(cardano_addr.clone().into_inner(), dust_addr.clone().into_inner()),
		];

		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			new_registrations,
			vec![],
			vec![utxo.clone()],
			latest_block
		));

		// Advance block and clear events for a clean slate
		System::set_block_number(System::block_number() + 1);
		frame_system::Pallet::<Test>::reset_events();

		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![],
			// Remove the extra registration which invalidated the user's registratin
			vec![(cardano_addr.clone().into_inner(), dust_addr.clone().into_inner())],
			vec![],
			latest_block
		));

		let events_after_addition = frame_system::Pallet::<Test>::events();

		let registered_event = events_after_addition.iter().any(|record| {
			matches!(
				&record.event,
				mock::RuntimeEvent::NativeTokenObservation(crate::Event::Registered(_))
			)
		});
		assert!(registered_event, "Event indicating re-registration of account was not found");
	});
}

#[test]
fn no_registered_event_when_still_invalid_after_removal() {
	new_test_ext().execute_with(|| {
		let cardano_addr = cardano_address(b"cardano_still_invalid");
		let dust1 = dust_address(b"dust1");
		let dust2 = dust_address(b"dust2");
		let dust3 = dust_address(b"dust3");
		let latest_block = 7000;

		// Register 3 dust addresses (clearly invalid)
		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![
				(cardano_addr.clone().into_inner(), dust1.clone().into_inner()),
				(cardano_addr.clone().into_inner(), dust2.clone().into_inner()),
				(cardano_addr.clone().into_inner(), dust3.clone().into_inner())
			],
			vec![],
			vec![],
			latest_block
		));

		// Advance block to isolate events and extrinsics
		System::set_block_number(System::block_number() + 1);
		frame_system::Pallet::<Test>::reset_events();

		let events_first = frame_system::Pallet::<Test>::events();
		assert_eq!(events_first, vec![]);

		// Remove 1 dust address, bringing it from 3 -> 2 (still invalid)
		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![],
			vec![(cardano_addr.clone().into_inner(), dust2.clone().into_inner())],
			vec![],
			latest_block
		));

		let events = frame_system::Pallet::<Test>::events();

		let re_registered_found = events.iter().any(|record| {
			matches!(
				&record.event,
				mock::RuntimeEvent::NativeTokenObservation(crate::Event::Registered(e))
				if e.0 == cardano_addr
			)
		});

		assert!(
			!re_registered_found,
			"Should NOT emit Registered event when still invalid after removal"
		);
	});
}

#[test]
fn specific_registration_is_removed_correctly() {
	new_test_ext().execute_with(|| {
		let cardano_addr = cardano_address(b"cardanoX");
		let dust_addresses: BoundedVec<
			BoundedVec<u8, ConstU32<32>>,
			MaxRegistrationsPerCardanoAddress,
		> = bounded_vec![
			dust_address(b"dust0"),
			dust_address(b"dust1"),
			dust_address(b"dust2"),
			dust_address(b"dust3"),
			dust_address(b"dust4")
		];
		let latest_block = 12345;

		// Insert all five as initial registrations
		Registrations::<Test>::insert(cardano_addr.clone(), dust_addresses.clone());

		// Remove dust2
		let to_remove = (cardano_addr.clone().into_inner(), dust_address(b"dust2").into_inner());
		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![],
			vec![to_remove],
			vec![],
			latest_block
		));

		let updated = NativeTokenObservation::get_registrations_for(cardano_addr.clone());

		// Assert it no longer includes dust2
		assert!(!updated.contains(&dust_address(b"dust2")), "dust2 should be removed");

		// Assert it still includes the others
		assert!(updated.contains(&dust_address(b"dust0")));
		assert!(updated.contains(&dust_address(b"dust1")));
		assert!(updated.contains(&dust_address(b"dust3")));
		assert!(updated.contains(&dust_address(b"dust4")));

		// Assert correct length (should now be 4)
		assert_eq!(updated.len(), 4);
	});
}

#[test]
fn next_block_range_should_compute_bounds_correctly() {
	new_test_ext().execute_with(|| {
		LastCardanoBlock::<Test>::set(2000);
		CardanoBlockWindowSize::<Test>::set(100);
		let (min, max) = NativeTokenObservation::get_next_block_range();
		assert_eq!(min, 2000);
		assert_eq!(max, 2100);
	});
}

#[test]
fn is_registered_should_return_true_for_registered_wallet() {
	new_test_ext().execute_with(|| {
		let addr = BoundedVec::try_from(b"cardano3".to_vec()).unwrap();
		// let storage_values_before: BoundedVec::<u8, MaxRegistrationsPerCardanoAddress> = bounded_vec![dust_address(b"dustA")];
		// let storage_values_after: BoundedVec::<u8, MaxRegistrationsPerCardanoAddress> = bounded_vec![dust_address(b"dustA"), dust_address(b"dustB")];

		let storage_values_before: BoundedVec<
			BoundedVec<u8, ConstU32<32>>,
			MaxRegistrationsPerCardanoAddress,
		> = bounded_vec![dust_address(b"dustA")];
		let storage_values_after: BoundedVec<
			BoundedVec<u8, ConstU32<32>>,
			MaxRegistrationsPerCardanoAddress,
		> = bounded_vec![dust_address(b"dustA"), dust_address(b"dustB")];

		Registrations::<Test>::insert(addr.clone(), storage_values_before);
		assert!(NativeTokenObservation::is_registered(&addr));
		// Registrations are unique by cardano wallet address. This is considered invalid
		Registrations::<Test>::insert(addr.clone(), storage_values_after);
		assert!(!NativeTokenObservation::is_registered(&addr));
	});
}

#[test]
fn oldest_registration_should_be_evicted_when_capacity_reached() {
	new_test_ext().execute_with(|| {
		let cardano_addr = cardano_address(b"cardano_eviction");
		let latest_block = 9999;

		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![(cardano_addr.clone().into_inner(), dust_address(b"dust-0").into_inner())],
			vec![],
			vec![],
			latest_block
		));

		let updated = NativeTokenObservation::get_registrations_for(cardano_addr.clone());
		assert!(updated.contains(&dust_address(b"dust-0")));
		let new_dust = dust_address(b"dust-1");

		for _ in 0..MaxRegistrationsPerCardanoAddress::get() {
			assert_ok!(Pallet::<Test>::process_tokens(
				frame_system::RawOrigin::None.into(),
				vec![(cardano_addr.clone().into_inner(), new_dust.clone().into_inner())],
				vec![],
				vec![],
				latest_block
			));
		}

		let updated = NativeTokenObservation::get_registrations_for(cardano_addr.clone());

		// Expect dust0 to be evicted
		assert!(!updated.contains(&dust_address(b"dust0")), "dust0 should have been evicted");

		// Expect other original and new one to remain
		assert!(updated.contains(&dust_address(b"dust-1")));
		// Still at capacity
		assert_eq!(updated.len(), MaxRegistrationsPerCardanoAddress::get() as usize);
	});
}

#[test]
fn duplicated_registration_event_only_once_on_first_duplicate() {
	new_test_ext().execute_with(|| {
		let cardano_addr = cardano_address(b"cardano_dupe");
		let dust_addr = dust_address(b"dust_dupe");
		let latest_block = 4242;

		// Add the initial (unique) registration
		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![(cardano_addr.clone().into_inner(), dust_addr.clone().into_inner())],
			vec![],
			vec![],
			latest_block
		));

		// Add the *same* registration again (should go from 1 -> 2, triggering DuplicatedRegistration)
		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![(cardano_addr.clone().into_inner(), dust_addr.clone().into_inner())],
			vec![],
			vec![],
			latest_block
		));

		// Add the same one *again* (should be 3rd or more — no DuplicatedRegistration event)
		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![(cardano_addr.clone().into_inner(), dust_addr.clone().into_inner())],
			vec![],
			vec![],
			latest_block
		));

		let events = frame_system::Pallet::<Test>::events();

		// Count the number of DuplicatedRegistration events
		let dupe_event_count = events
			.iter()
			.filter(|record| {
				matches!(
					&record.event,
					mock::RuntimeEvent::NativeTokenObservation(
						crate::Event::DuplicatedRegistration(e)
					) if e.0 == cardano_addr
				)
			})
			.count();

		assert_eq!(
			dupe_event_count, 1,
			"DuplicatedRegistration should be emitted only once when count transitions from 1 → 2"
		);
	});
}

#[test]
fn registered_event_emitted_only_once_per_cardano_address() {
	new_test_ext().execute_with(|| {
		let cardano_addr = cardano_address(b"cardano_once");
		let dust1 = dust_address(b"dust1");
		let dust2 = dust_address(b"dust2");
		let dust3 = dust_address(b"dust3");
		let latest_block = 7777;

		// Add the first (valid) registration
		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![(cardano_addr.clone().into_inner(), dust1.clone().into_inner())],
			vec![],
			vec![],
			latest_block
		));

		// Add more dust addresses to same Cardano address (now invalid as per `is_registered`)
		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![(cardano_addr.clone().into_inner(), dust2.clone().into_inner())],
			vec![],
			vec![],
			latest_block
		));
		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![(cardano_addr.clone().into_inner(), dust3.clone().into_inner())],
			vec![],
			vec![],
			latest_block
		));

		let events = frame_system::Pallet::<Test>::events();

		// Count number of Registered events emitted for this cardano address
		let registered_event_count = events
			.iter()
			.filter(|record| {
				matches!(
					&record.event,
					mock::RuntimeEvent::NativeTokenObservation(crate::Event::Registered(e))
					if e.0 == cardano_addr
				)
			})
			.count();

		assert_eq!(
			registered_event_count, 1,
			"Registered event should only be emitted once for a valid Cardano address"
		);
	});
}

#[test]
fn removed_old_event_emitted_when_eviction_occurs() {
	new_test_ext().execute_with(|| {
		let cardano_addr = cardano_address(b"cardano_removed_old");
		let latest_block = 1234;

		for i in 0..MaxRegistrationsPerCardanoAddress::get() {
			let dust = dust_address(&[i]);
			assert_ok!(Pallet::<Test>::process_tokens(
				frame_system::RawOrigin::None.into(),
				vec![(cardano_addr.clone().into_inner(), dust.clone().into_inner())],
				vec![],
				vec![],
				latest_block
			));
		}

		System::set_block_number(System::block_number() + 1);
		frame_system::Pallet::<Test>::reset_events();

		let new_dust = dust_address(b"newer");
		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![(cardano_addr.clone().into_inner(), new_dust.clone().into_inner())],
			vec![],
			vec![],
			latest_block
		));

		let events = frame_system::Pallet::<Test>::events();
		let found = events.iter().any(|record| {
			matches!(
				&record.event,
				mock::RuntimeEvent::NativeTokenObservation(
					crate::Event::RemovedOld((addr, _))
				) if addr == &cardano_addr
			)
		});

		assert!(found, "Expected RemovedOld event not found");
	});
}

#[test]
fn attempted_remove_nonexistent_emits_event() {
	new_test_ext().execute_with(|| {
		let cardano_addr = cardano_address(b"cardano_nonexistent_removal");
		let dust_present = dust_address(b"present");
		let dust_missing = dust_address(b"missing");
		let latest_block = 2222;

		let dust_address: BoundedVec<
			BoundedVec<u8, ConstU32<32>>,
			MaxRegistrationsPerCardanoAddress,
		> = bounded_vec![dust_present];

		Registrations::<Test>::insert(cardano_addr.clone(), dust_address);

		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![],
			vec![(cardano_addr.clone().into_inner(), dust_missing.clone().into_inner())],
			vec![],
			latest_block
		));

		let events = frame_system::Pallet::<Test>::events();
		let found = events.iter().any(|record| {
			matches!(
				&record.event,
				mock::RuntimeEvent::NativeTokenObservation(
					crate::Event::AttemptedRemoveNonexistantElement
				)
			)
		});

		assert!(found, "Expected AttemptedRemoveNonexistantElement event not found");
	});
}

#[test]
fn invalid_cardano_and_dust_address_should_emit_respective_events() {
	new_test_ext().execute_with(|| {
		let latest_block = 4444;

		// First: test invalid Cardano address (Dust is valid)
		let too_long_cardano = vec![0u8; MaxCardanoAddrLen::get() as usize + 1];
		let valid_dust = vec![1u8; 32];

		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![(too_long_cardano.clone(), valid_dust.clone())],
			vec![],
			vec![],
			latest_block
		));

		let events = frame_system::Pallet::<Test>::events();
		assert!(
			events.iter().any(|record| matches!(
				&record.event,
				mock::RuntimeEvent::NativeTokenObservation(crate::Event::InvalidCardanoAddress)
			)),
			"Expected InvalidCardanoAddress event"
		);

		frame_system::Pallet::<Test>::reset_events();

		// Then: test invalid Dust address (Cardano is valid)
		let valid_cardano = vec![1u8; MaxCardanoAddrLen::get() as usize];
		let too_long_dust = vec![9u8; 33];

		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![(valid_cardano.clone(), too_long_dust.clone())],
			vec![],
			vec![],
			latest_block
		));

		let events = frame_system::Pallet::<Test>::events();
		assert!(
			events.iter().any(|record| matches!(
				&record.event,
				mock::RuntimeEvent::NativeTokenObservation(crate::Event::InvalidDustAddress)
			)),
			"Expected InvalidDustAddress event"
		);
	});
}

#[test]
fn added_event_emitted_for_each_dust_mapping_created() {
	new_test_ext().execute_with(|| {
		let latest_block = 3000;

		let registrations = vec![
			(cardano_address(b"cardanoA"), dust_address(b"dustA")),
			(cardano_address(b"cardanoB"), dust_address(b"dustB")),
			(cardano_address(b"cardanoC"), dust_address(b"dustC")),
		];

		let new_registrations: Vec<(Vec<u8>, Vec<u8>)> = registrations
			.iter()
			.map(|(c, d)| (c.clone().into_inner(), d.clone().into_inner()))
			.collect();

		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			new_registrations,
			vec![],
			vec![],
			latest_block
		));

		let events = frame_system::Pallet::<Test>::events();

		for (cardano, _) in &registrations {
			let found = events.iter().any(|record| {
				matches!(
					&record.event,
					mock::RuntimeEvent::NativeTokenObservation(crate::Event::Added((addr, _)))
					if addr == cardano
				)
			});
			assert!(found, "Expected Added event for {:?}", cardano);
		}

		// Clear events and advance block
		System::set_block_number(System::block_number() + 1);
		frame_system::Pallet::<Test>::reset_events();

		// Add one more registration
		let extra_cardano = cardano_address(b"cardanoD");
		let extra_dust = dust_address(b"dustD");

		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![(extra_cardano.clone().into_inner(), extra_dust.clone().into_inner())],
			vec![],
			vec![],
			latest_block
		));

		let events_after = frame_system::Pallet::<Test>::events();
		let added_found = events_after.iter().any(|record| {
			matches!(
				&record.event,
				mock::RuntimeEvent::NativeTokenObservation(crate::Event::Added((addr, _)))
				if addr == &extra_cardano
			)
		});

		assert!(added_found, "Expected Added event for {:?}", extra_cardano);
	});
}
#[test]
fn removed_event_emitted_for_each_dust_mapping_removal() {
	new_test_ext().execute_with(|| {
		let latest_block = 3141;

		let cardano_addr = cardano_address(b"cardano_to_remove");
		let dust1 = dust_address(b"remove1");
		let dust2 = dust_address(b"remove2");
		let dust3 = dust_address(b"remove3");
		let dust4 = dust_address(b"remove4"); // Used later

		let prefill: BoundedVec<BoundedVec<u8, ConstU32<32>>, MaxRegistrationsPerCardanoAddress> =
			bounded_vec![dust1.clone(), dust2.clone(), dust3.clone(), dust4.clone()];
		Registrations::<Test>::insert(cardano_addr.clone(), prefill);

		// Remove dust1 and dust2
		let removals = vec![
			(cardano_addr.clone().into_inner(), dust1.clone().into_inner()),
			(cardano_addr.clone().into_inner(), dust2.clone().into_inner()),
		];

		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![],
			removals.clone(),
			vec![],
			latest_block
		));

		let events = frame_system::Pallet::<Test>::events();
		for (cardano, _) in &removals {
			let cardano_bounded: BoundedCardanoAddress<Test> =
				BoundedVec::try_from(cardano.clone()).unwrap();
			let found = events.iter().any(|record| {
				matches!(
					&record.event,
					mock::RuntimeEvent::NativeTokenObservation(crate::Event::Removed((addr, _)))
					if addr == &cardano_bounded
				)
			});
			assert!(found, "Expected Removed event for {:?}", cardano_bounded);
		}

		// Advance block and clear events
		System::set_block_number(System::block_number() + 1);
		frame_system::Pallet::<Test>::reset_events();

		// Remove dust3
		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![],
			vec![(cardano_addr.clone().into_inner(), dust3.clone().into_inner())],
			vec![],
			latest_block
		));

		let events_after = frame_system::Pallet::<Test>::events();
		let removed_found = events_after.iter().any(|record| {
			matches!(
				&record.event,
				mock::RuntimeEvent::NativeTokenObservation(crate::Event::Removed((addr, _)))
				if addr == &cardano_addr
			)
		});

		assert!(removed_found, "Expected Removed event for {:?}", cardano_addr);
	});
}

#[test]
fn decode_len_should_differ_between_empty_vec_and_removed_key() {
	new_test_ext().execute_with(|| {
		let cardano_addr = cardano_address(b"cardano_decode_test");
		let dust_addr = dust_address(b"dustA");
		let latest_block = 6000;

		// Add a registration
		assert_ok!(Pallet::<Test>::process_tokens(
			frame_system::RawOrigin::None.into(),
			vec![(cardano_addr.clone().into_inner(), dust_addr.clone().into_inner())],
			vec![],
			vec![],
			latest_block
		));

		// Manually reduce the registration to an empty vec
		Registrations::<Test>::insert(
			cardano_addr.clone(),
			BoundedVec::<_, MaxRegistrationsPerCardanoAddress>::default(),
		);

		// Ensure decode_len sees a zero-length vec (still occupies storage)
		let len = Registrations::<Test>::decode_len(cardano_addr.clone());
		assert_eq!(len, Some(0), "Empty vec still encoded in storage");

		// Now actually remove the key
		Registrations::<Test>::remove(cardano_addr.clone());

		// decode_len should now return None (key no longer present)
		let len_after_removal = Registrations::<Test>::decode_len(cardano_addr.clone());
		assert_eq!(len_after_removal, None, "Key removed entirely from storage");
	});
}
