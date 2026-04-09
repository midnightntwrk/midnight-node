use crate::mock::*;
use crate::pallet::Call;
use crate::*;
use TransferRecipient::*;
use core::str::FromStr;
use frame_support::{
	assert_err, assert_ok,
	inherent::{InherentData, ProvideInherent},
};
use sidechain_domain::{AssetName, MainchainAddress, McTxHash, PolicyId};
use sp_core::bounded_vec;
use sp_partner_chains_bridge::*;
use sp_runtime::{AccountId32, BoundedVec};

fn transfers() -> BoundedVec<BridgeTransferV1<RecipientAddress>, MaxTransfersPerBlock> {
	bounded_vec![
		BridgeTransferV1 {
			amount: 100,
			recipient: Address { recipient: AccountId32::new([2; 32]) },
			mc_tx_hash: McTxHash([1; 32])
		},
		BridgeTransferV1 { amount: 200, mc_tx_hash: McTxHash([2; 32]), recipient: Reserve },
		BridgeTransferV1 { amount: 300, mc_tx_hash: McTxHash([3; 32]), recipient: Invalid }
	]
}

fn main_chain_scripts() -> MainChainScripts {
	MainChainScripts {
		token_policy_id: PolicyId([1; 28]),
		token_asset_name: AssetName(bounded_vec![2;8]),
		illiquid_circulation_supply_validator_address: MainchainAddress::from_str(
			"validator address",
		)
		.unwrap(),
	}
}

fn data_checkpoint() -> BridgeDataCheckpoint {
	BridgeDataCheckpoint::Tx(McTxHash([1; 32]))
}

mod set_main_chain_scripts {
	use super::*;

	#[test]
	fn updates_scripts_and_data_checkpoint_in_storage() {
		new_test_ext().execute_with(|| {
			assert_ok!(Bridge::set_main_chain_scripts(
				RuntimeOrigin::root(),
				main_chain_scripts(),
				data_checkpoint()
			));

			assert_eq!(Bridge::get_main_chain_scripts(), Some(main_chain_scripts()));
			assert_eq!(Bridge::get_data_checkpoint(), Some(data_checkpoint()));
		})
	}
}

mod handle_transfers {
	use super::*;

	#[test]
	fn calls_the_handler() {
		new_test_ext().execute_with(|| {
			assert_ok!(Bridge::handle_transfers(
				RuntimeOrigin::none(),
				transfers(),
				data_checkpoint()
			));

			assert_eq!(mock_pallet::Transfers::<Test>::get(), Some(transfers().to_vec()));
		})
	}

	#[test]
	fn emits_events() {
		new_test_ext().execute_with(|| {
			// Frame system drops events from block 0.
			frame_system::Pallet::<Test>::set_block_number(1);
			assert_ok!(Bridge::handle_transfers(
				RuntimeOrigin::none(),
				transfers(),
				data_checkpoint()
			));

			let events: Vec<_> =
				frame_system::Pallet::<Test>::events().into_iter().map(|e| e.event).collect();
			let expected: Vec<<mock::Test as frame_system::Config>::RuntimeEvent> = transfers()
				.into_iter()
				.map(|t| {
					mock::RuntimeEvent::Bridge(Event::Transfer {
						mc_tx_hash: t.mc_tx_hash,
						amount: t.amount,
						result: t.amount / 10,
						recipient: t.recipient,
					})
				})
				.collect();
			assert_eq!(events, expected);
		})
	}

	#[test]
	fn updates_the_data_checkpoint() {
		new_test_ext().execute_with(|| {
			assert_ok!(Bridge::handle_transfers(
				RuntimeOrigin::none(),
				transfers(),
				data_checkpoint()
			));

			assert_eq!(DataCheckpoint::<Test>::get(), Some(data_checkpoint()));
		})
	}

	#[test]
	fn rejects_non_extrinsic_calls() {
		new_test_ext().execute_with(|| {
			assert_err!(
				Bridge::handle_transfers(RuntimeOrigin::root(), transfers(), data_checkpoint()),
				sp_runtime::DispatchError::BadOrigin
			);

			assert_err!(
				Bridge::handle_transfers(
					RuntimeOrigin::signed(AccountId32::new(Default::default())),
					transfers(),
					data_checkpoint()
				),
				sp_runtime::DispatchError::BadOrigin
			);
		})
	}

	// #[test]
	// fn duplicate_inherent_protection_works() {
	// 	new_test_ext().execute_with(|| {
	// 		init_ledger_state();
	// 		let (cardano_reward_address, dust_public_key) = test_wallet_pairing();

	// 		let utxos = vec![ObservedUtxo {
	// 			header: test_header(1, 2, 0, None),
	// 			data: ObservedUtxoData::Registration(RegistrationData {
	// 				cardano_reward_address,
	// 				dust_public_key: dust_public_key.clone(),
	// 			}),
	// 		}];

	// 		// First call succeeds
	// 		let inherent_data = create_inherent(utxos.clone(), test_position(3, 0));
	// 		let call = CNightObservation::create_inherent(&inherent_data).unwrap();
	// 		assert_ok!(RuntimeCall::CNightObservation(call).dispatch(RawOrigin::None.into()));

	// 		// Second call in same block fails
	// 		let call2 = Call::process_tokens {
	// 			utxos: utxos.clone(),
	// 			next_cardano_position: test_position(3, 0),
	// 		};
	// 		assert_noop!(
	// 			RuntimeCall::CNightObservation(call2).dispatch(RawOrigin::None.into()),
	// 			Error::<Test>::InherentAlreadyExecuted
	// 		);

	// 		advance_block_and_reset_events();

	// 		// Third call in new block succeeds
	// 		let utxos2 = vec![ObservedUtxo {
	// 			header: test_header(4, 0, 0, None),
	// 			data: ObservedUtxoData::Registration(RegistrationData {
	// 				cardano_reward_address,
	// 				dust_public_key,
	// 			}),
	// 		}];
	// 		let inherent_data2 = create_inherent(utxos2, test_position(5, 0));
	// 		let call3 = CNightObservation::create_inherent(&inherent_data2).unwrap();
	// 		assert_ok!(RuntimeCall::CNightObservation(call3).dispatch(RawOrigin::None.into()));
	// 	});
	// }
}

mod provide_inherent {
	use super::*;

	fn inherent_data() -> InherentData {
		let mut inherent_data = InherentData::new();
		inherent_data
			.put_data(
				INHERENT_IDENTIFIER,
				&TokenBridgeTransfersV1 {
					transfers: transfers().to_vec(),
					data_checkpoint: data_checkpoint(),
				},
			)
			.expect("Putting data should succeed");
		inherent_data
	}

	#[test]
	fn creates_inherent() {
		let inherent = Bridge::create_inherent(&inherent_data()).expect("Should create inherent");

		assert_eq!(
			inherent,
			Call::handle_transfers { transfers: transfers(), data_checkpoint: data_checkpoint() }
		)
	}

	#[test]
	fn requires_inherent_when_data_present() {
		let result = Bridge::is_inherent_required(&inherent_data())
			.expect("Checking if inherent is required should not fail");

		assert_eq!(result, Some(InherentError::InherentRequired))
	}

	#[test]
	fn allows_no_inherent_when_data_missing() {
		let result = Bridge::is_inherent_required(&InherentData::new())
			.expect("Checking if inherent is required should not fail");

		assert_eq!(result, None)
	}

	#[test]
	fn verifies_inherent() {
		let correct_inherent =
			Bridge::create_inherent(&inherent_data()).expect("Should create inherent");

		assert_ok!(Bridge::check_inherent(&correct_inherent, &inherent_data()));

		let invalid_inherent = Call::handle_transfers {
			transfers: bounded_vec![],
			data_checkpoint: data_checkpoint(),
		};
		assert_err!(
			Bridge::check_inherent(&invalid_inherent, &inherent_data()),
			InherentError::IncorrectInherent
		);
	}

	#[test]
	fn only_handle_transfers_is_inherent() {
		let handle_transfers = Call::handle_transfers {
			transfers: bounded_vec![],
			data_checkpoint: data_checkpoint(),
		};

		let set_main_chain_scripts = Call::set_main_chain_scripts {
			new_scripts: main_chain_scripts(),
			data_checkpoint: data_checkpoint(),
		};

		assert_eq!(Bridge::is_inherent(&handle_transfers), true);
		assert_eq!(Bridge::is_inherent(&set_main_chain_scripts), false);
	}
}
