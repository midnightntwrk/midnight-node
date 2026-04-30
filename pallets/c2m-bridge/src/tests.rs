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
			mock::RuntimeEvent::C2MBridge(Event::Transfer {
				mc_tx_hash: McTxHash([1; 32]),
				amount: 100,
				result: [0u8; 32],
				recipient: TransferRecipient::Address { recipient: recipient() },
			}),
			mock::RuntimeEvent::C2MBridge(Event::Transfer {
				mc_tx_hash: McTxHash([2; 32]),
				amount: 200,
				result: [1u8; 32],
				recipient: TransferRecipient::Reserve,
			}),
			mock::RuntimeEvent::C2MBridge(Event::Transfer {
				mc_tx_hash: McTxHash([3; 32]),
				amount: 300,
				result: [2u8; 32],
				recipient: TransferRecipient::Invalid,
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
