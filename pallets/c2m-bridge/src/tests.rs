use crate::mock::*;
use crate::*;
use midnight_primitives::BridgeRecipient;
use pallet_partner_chains_bridge::TransferHandler;
use sidechain_domain::McTxHash;
use sp_partner_chains_bridge::*;

fn recipient() -> BridgeRecipient {
	BridgeRecipient::try_from(Vec::from([2u8; 32])).unwrap()
}

fn addressed_transfer() -> BridgeTransferV1<BridgeRecipient> {
	BridgeTransferV1 {
		amount: 100,
		recipient: TransferRecipient::Address { recipient: recipient() },
		mc_tx_hash: McTxHash([1; 32]),
	}
}

fn reserve_transfer() -> BridgeTransferV1<BridgeRecipient> {
	BridgeTransferV1 {
		amount: 200,
		mc_tx_hash: McTxHash([2; 32]),
		recipient: TransferRecipient::Reserve,
	}
}

fn invalid_transfer() -> BridgeTransferV1<BridgeRecipient> {
	BridgeTransferV1 {
		amount: 300,
		mc_tx_hash: McTxHash([3; 32]),
		recipient: TransferRecipient::Invalid,
	}
}

// It is valid from partner-chains-bridge-pallet perspective, but amount is below threshold of 99.
fn subminimal_transfer() -> BridgeTransferV1<BridgeRecipient> {
	BridgeTransferV1 {
		amount: 90,
		mc_tx_hash: McTxHash([4; 32]),
		recipient: TransferRecipient::Address { recipient: recipient() },
	}
}

#[test]
fn emits_events() {
	new_test_ext().execute_with(|| {
		// Frame system drops events from block 0.
		frame_system::Pallet::<Test>::set_block_number(1);
		C2MBridge::handle_incoming_transfer(addressed_transfer());
		C2MBridge::handle_incoming_transfer(reserve_transfer());
		C2MBridge::handle_incoming_transfer(invalid_transfer());

		let events: Vec<_> =
			frame_system::Pallet::<Test>::events().into_iter().map(|e| e.event).collect();

		let expected: Vec<<mock::Test as frame_system::Config>::RuntimeEvent> = vec![
			mock::RuntimeEvent::C2MBridge(Event::UserTransfer {
				mc_tx_hash: McTxHash([1; 32]),
				amount: 100,
				recipient: recipient(),
				midnight_tx_hash: [0u8; 32],
			}),
			mock::RuntimeEvent::C2MBridge(Event::ReserveTransfer {
				mc_tx_hash: McTxHash([2; 32]),
				amount: 200,
				midnight_tx_hash: [1u8; 32],
			}),
			mock::RuntimeEvent::C2MBridge(Event::InvalidTransfer {
				mc_tx_hash: McTxHash([3; 32]),
				amount: 300,
				midnight_tx_hash: [2u8; 32],
			}),
		];

		assert_eq!(events, expected);
	})
}

#[test]
fn nonce_influences_addressed_transfers() {
	new_test_ext().execute_with(|| {
		C2MBridge::handle_incoming_transfer(addressed_transfer());
		C2MBridge::handle_incoming_transfer(addressed_transfer());
		let transfers = mock_pallet::Transfers::<Test>::get();
		let [first, second] = transfers.as_slice() else {
			panic!("expected exactly two transfers");
		};
		assert_ne!(first, second);
	})
}

#[test]
fn subminimal_transfer_handling() {
	new_test_ext().execute_with(|| {
		pallet::SubminimalTransfersConfiguration::<Test>::set(SubminimalTransfersConfig {
			subminimal_transfers_flush_threshold: 250,
		});
		//90
		C2MBridge::handle_incoming_transfer(subminimal_transfer());
		assert_eq!(
			pallet::SubminimalTransfers::<Test>::get(),
			SubminimalTransfersState { count: 1, sum: 90 }
		);
		assert!(mock_pallet::Transfers::<Test>::get().is_empty());
		assert!(frame_system::Pallet::<Test>::events().is_empty());
		//180
		C2MBridge::handle_incoming_transfer(subminimal_transfer());
		assert_eq!(
			pallet::SubminimalTransfers::<Test>::get(),
			SubminimalTransfersState { count: 2, sum: 180 }
		);
		assert!(mock_pallet::Transfers::<Test>::get().is_empty());
		//270 > 250. Should flush everything in one transfer.
		C2MBridge::handle_incoming_transfer(subminimal_transfer());
		assert_eq!(
			pallet::SubminimalTransfers::<Test>::get(),
			SubminimalTransfersState { count: 0, sum: 0 }
		);
		assert_eq!(mock_pallet::Transfers::<Test>::get().len(), 1);

		let events: Vec<_> =
			frame_system::Pallet::<Test>::events().into_iter().map(|e| e.event).collect();
		let expected: Vec<<mock::Test as frame_system::Config>::RuntimeEvent> =
			vec![mock::RuntimeEvent::C2MBridge(Event::SubminimalFlushTransfer {
				amount: 270,
				count: 3,
				midnight_tx_hash: [0u8; 32],
			})];

		assert_eq!(events, expected);
	})
}
