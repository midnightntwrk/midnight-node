use crate::mock::*;
use crate::pallet::Call;
use crate::*;
use TransferRecipient::*;
use core::str::FromStr;
use frame_support::{
	assert_err, assert_ok,
	inherent::{InherentData, ProvideInherent},
};
use sidechain_domain::{AssetName, MainchainAddress, McBlockNumber, McTxHash, PolicyId};
use sp_core::bounded_vec;
use sp_partner_chains_bridge::*;
use sp_runtime::{AccountId32, BoundedVec};

fn transfers() -> BoundedVec<BridgeTransferV1<RecipientAddress>, MaxTransfersPerBlock> {
	bounded_vec![
		BridgeTransferV1 {
			amount: 100,
			recipient: Address { recipient: AccountId32::new([2; 32]) },
			mc_tx_hash: McTxHash([1; 32]),
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
		reserve_validator_address: MainchainAddress::from_str("reserve validator address").unwrap(),
	}
}

fn data_checkpoint() -> BridgeDataCheckpoint {
	BridgeDataCheckpoint::Tx(McTxHash([1; 32]))
}

/// Checkpoint the observability layer reports for a block. It is only reached when every
/// transfer of that block has been handled.
fn final_checkpoint() -> BridgeDataCheckpoint {
	BridgeDataCheckpoint::Block(McBlockNumber(42))
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
	fn updates_the_data_checkpoint() {
		new_test_ext().execute_with(|| {
			assert_ok!(Bridge::handle_transfers(
				RuntimeOrigin::none(),
				transfers(),
				final_checkpoint()
			));

			assert_eq!(DataCheckpoint::<Test>::get(), Some(final_checkpoint()));
		})
	}

	/// Transfers of two Cardano transactions, the second of which transferred both to the
	/// reserve and to a user, mirroring what the observability layer delivers for such a
	/// transaction.
	fn transfers_of_two_txs() -> BoundedVec<BridgeTransferV1<RecipientAddress>, MaxTransfersPerBlock>
	{
		bounded_vec![
			BridgeTransferV1 { amount: 100, mc_tx_hash: McTxHash([1; 32]), recipient: Invalid },
			BridgeTransferV1 { amount: 200, mc_tx_hash: McTxHash([2; 32]), recipient: Reserve },
			BridgeTransferV1 {
				amount: 300,
				mc_tx_hash: McTxHash([2; 32]),
				recipient: Address { recipient: AccountId32::new([2; 32]) },
			}
		]
	}

	fn reject(transfer: &BridgeTransferV1<RecipientAddress>) {
		mock_pallet::FailingTransfers::<Test>::mutate(|ts| ts.push(transfer.clone()));
	}

	fn handled_transfers() -> Vec<BridgeTransferV1<RecipientAddress>> {
		mock_pallet::Transfers::<Test>::get().unwrap_or_default()
	}

	#[test]
	fn stops_handling_transfers_at_the_first_failure() {
		new_test_ext().execute_with(|| {
			let transfers = transfers();
			reject(&transfers[1]);

			assert_ok!(Bridge::handle_transfers(
				RuntimeOrigin::none(),
				transfers.clone(),
				final_checkpoint()
			));

			// The failing transfer and everything observed after it is left for a later block.
			assert_eq!(handled_transfers(), transfers[..1].to_vec());
			// The checkpoint only moves up to the last fully handled Cardano transaction.
			assert_eq!(
				DataCheckpoint::<Test>::get(),
				Some(BridgeDataCheckpoint::Tx(transfers[0].mc_tx_hash))
			);
		})
	}

	#[test]
	fn keeps_the_data_checkpoint_when_no_transfer_could_be_handled() {
		new_test_ext().execute_with(|| {
			DataCheckpoint::<Test>::put(data_checkpoint());
			let transfers = transfers();
			reject(&transfers[0]);

			assert_ok!(Bridge::handle_transfers(
				RuntimeOrigin::none(),
				transfers,
				final_checkpoint()
			));

			assert_eq!(handled_transfers(), vec![]);
			assert_eq!(DataCheckpoint::<Test>::get(), Some(data_checkpoint()));
		})
	}

	#[test]
	fn records_how_many_transfers_of_a_partially_handled_cardano_tx_were_handled() {
		new_test_ext().execute_with(|| {
			// The second transfer of the second Cardano transaction fails, so that transaction is
			// presented again — without the transfer that was handled.
			let transfers = transfers_of_two_txs();
			reject(&transfers[2]);

			assert_ok!(Bridge::handle_transfers(
				RuntimeOrigin::none(),
				transfers.clone(),
				final_checkpoint()
			));

			assert_eq!(handled_transfers(), transfers[..2].to_vec());
			assert_eq!(
				DataCheckpoint::<Test>::get(),
				Some(BridgeDataCheckpoint::PartialTx {
					tx: transfers[1].mc_tx_hash,
					transfers_processed: 1
				})
			);
		})
	}

	#[test]
	fn counts_transfers_handled_for_a_partial_tx_in_an_earlier_block() {
		new_test_ext().execute_with(|| {
			let tx = McTxHash([2; 32]);
			// One transfer of `tx` was handled before, so the observability layer left it out and
			// presents the remaining two.
			DataCheckpoint::<Test>::put(BridgeDataCheckpoint::PartialTx {
				tx,
				transfers_processed: 1,
			});
			let transfers: BoundedVec<_, MaxTransfersPerBlock> = bounded_vec![
				BridgeTransferV1 { amount: 200, mc_tx_hash: tx, recipient: Reserve },
				BridgeTransferV1 { amount: 300, mc_tx_hash: tx, recipient: Invalid },
			];
			reject(&transfers[1]);

			assert_ok!(Bridge::handle_transfers(
				RuntimeOrigin::none(),
				transfers.clone(),
				final_checkpoint()
			));

			assert_eq!(handled_transfers(), transfers[..1].to_vec());
			// One from the earlier block plus one from this one.
			assert_eq!(
				DataCheckpoint::<Test>::get(),
				Some(BridgeDataCheckpoint::PartialTx { tx, transfers_processed: 2 })
			);
		})
	}

	#[test]
	fn keeps_a_partial_tx_checkpoint_when_its_next_transfer_fails_again() {
		new_test_ext().execute_with(|| {
			let tx = McTxHash([2; 32]);
			let checkpoint = BridgeDataCheckpoint::PartialTx { tx, transfers_processed: 1 };
			DataCheckpoint::<Test>::put(checkpoint.clone());
			let transfers: BoundedVec<_, MaxTransfersPerBlock> =
				bounded_vec![BridgeTransferV1 { amount: 300, mc_tx_hash: tx, recipient: Invalid }];
			reject(&transfers[0]);

			assert_ok!(Bridge::handle_transfers(
				RuntimeOrigin::none(),
				transfers,
				final_checkpoint()
			));

			assert_eq!(handled_transfers(), vec![]);
			assert_eq!(DataCheckpoint::<Test>::get(), Some(checkpoint));
		})
	}

	#[test]
	fn moves_the_checkpoint_past_a_partial_tx_once_its_transfers_are_handled() {
		new_test_ext().execute_with(|| {
			let tx = McTxHash([2; 32]);
			DataCheckpoint::<Test>::put(BridgeDataCheckpoint::PartialTx {
				tx,
				transfers_processed: 1,
			});
			let transfers: BoundedVec<_, MaxTransfersPerBlock> =
				bounded_vec![BridgeTransferV1 { amount: 300, mc_tx_hash: tx, recipient: Invalid }];

			assert_ok!(Bridge::handle_transfers(
				RuntimeOrigin::none(),
				transfers.clone(),
				final_checkpoint()
			));

			assert_eq!(handled_transfers(), transfers.to_vec());
			assert_eq!(DataCheckpoint::<Test>::get(), Some(final_checkpoint()));
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

	#[test]
	fn duplicate_inherent_protection_works() {
		new_test_ext().execute_with(|| {
			assert_ok!(Bridge::handle_transfers(
				RuntimeOrigin::none(),
				BoundedVec::new(),
				data_checkpoint()
			));
			frame_support::assert_noop!(
				Bridge::handle_transfers(
					RuntimeOrigin::none(),
					BoundedVec::new(),
					data_checkpoint()
				),
				Error::<Test>::InherentAlreadyExecuted
			);

			Bridge::on_finalize(System::block_number());
			System::set_block_number(System::block_number() + 1);
			Bridge::on_initialize(System::block_number());

			assert_ok!(Bridge::handle_transfers(
				RuntimeOrigin::none(),
				BoundedVec::new(),
				data_checkpoint()
			));
		});
	}
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
