use crate::mock::*;
use crate::*;
use frame_support::{assert_noop, assert_ok, traits::ConstU32};
use midnight_primitives::BridgeRecipient;
use pallet_partner_chains_bridge::TransferHandler;
use sidechain_domain::McTxHash;
use sp_partner_chains_bridge::*;
use sp_runtime::{BoundedVec, DispatchError};

fn sort_hashes(mut v: Vec<McTxHash>) -> Vec<McTxHash> {
	v.sort_by_key(|h| h.0);
	v
}

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

fn approve(hash: McTxHash) {
	pallet::ApprovedMcTxHashes::<Test>::insert(hash, ());
}

#[test]
fn emits_events() {
	new_test_ext().execute_with(|| {
		// Frame system drops events from block 0.
		frame_system::Pallet::<Test>::set_block_number(1);
		approve(McTxHash([1; 32]));
		C2MBridge::handle_incoming_transfer(addressed_transfer());
		C2MBridge::handle_incoming_transfer(reserve_transfer());
		C2MBridge::handle_incoming_transfer(invalid_transfer());

		let events = frame_system::Pallet::<Test>::read_events_for_pallet::<Event<Test>>();

		let expected = vec![
			Event::UserTransfer {
				mc_tx_hash: McTxHash([1; 32]),
				amount: 100,
				recipient: recipient(),
				midnight_tx_hash: [0u8; 32],
			},
			Event::ReserveTransfer {
				mc_tx_hash: McTxHash([2; 32]),
				amount: 200,
				midnight_tx_hash: [1u8; 32],
			},
			Event::InvalidTransfer {
				mc_tx_hash: McTxHash([3; 32]),
				amount: 300,
				midnight_tx_hash: [2u8; 32],
			},
		];

		assert_eq!(events, expected);
	})
}

#[test]
fn nonce_influences_addressed_transfers() {
	new_test_ext().execute_with(|| {
		let first = BridgeTransferV1 { mc_tx_hash: McTxHash([10; 32]), ..addressed_transfer() };
		let second = BridgeTransferV1 { mc_tx_hash: McTxHash([11; 32]), ..addressed_transfer() };
		approve(first.mc_tx_hash);
		approve(second.mc_tx_hash);
		C2MBridge::handle_incoming_transfer(first);
		C2MBridge::handle_incoming_transfer(second);
		let transfers = mock_pallet::Transfers::<Test>::get();
		let [first, second] = transfers.as_slice() else {
			panic!("expected exactly two transfers");
		};
		assert_ne!(first, second);
	})
}

#[test]
fn unapproved_user_transfer_routes_to_treasury() {
	new_test_ext().execute_with(|| {
		frame_system::Pallet::<Test>::set_block_number(1);
		// No approval inserted for `addressed_transfer().mc_tx_hash`.
		C2MBridge::handle_incoming_transfer(addressed_transfer());

		let events = frame_system::Pallet::<Test>::read_events_for_pallet::<Event<Test>>();
		assert_eq!(
			events,
			vec![Event::UnapprovedTransfer {
				mc_tx_hash: McTxHash([1; 32]),
				amount: 100,
				recipient: recipient(),
				midnight_tx_hash: [0u8; 32],
			}]
		);
	})
}

#[test]
fn approval_is_consumed_on_user_transfer() {
	new_test_ext().execute_with(|| {
		frame_system::Pallet::<Test>::set_block_number(1);
		let tx = addressed_transfer();
		approve(tx.mc_tx_hash);
		assert!(pallet::ApprovedMcTxHashes::<Test>::contains_key(tx.mc_tx_hash));

		// First delivery is approved -> consumes the approval.
		C2MBridge::handle_incoming_transfer(tx.clone());
		assert!(!pallet::ApprovedMcTxHashes::<Test>::contains_key(tx.mc_tx_hash));

		// Second delivery of the same Cardano tx hash is unapproved.
		C2MBridge::handle_incoming_transfer(tx);

		let events = frame_system::Pallet::<Test>::read_events_for_pallet::<Event<Test>>();
		assert_eq!(
			events,
			vec![
				Event::UserTransfer {
					mc_tx_hash: McTxHash([1; 32]),
					amount: 100,
					recipient: recipient(),
					midnight_tx_hash: [0u8; 32],
				},
				Event::UnapprovedTransfer {
					mc_tx_hash: McTxHash([1; 32]),
					amount: 100,
					recipient: recipient(),
					midnight_tx_hash: [1u8; 32],
				},
			]
		);
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

fn set_flush_threshold(threshold: u64) {
	pallet::SubminimalTransfersConfiguration::<Test>::set(SubminimalTransfersConfig {
		subminimal_transfers_flush_threshold: threshold,
	});
}

fn subminimal_events() -> Vec<Event<Test>> {
	frame_system::Pallet::<Test>::read_events_for_pallet::<Event<Test>>()
}

#[test]
fn subminimal_no_flush_just_below_threshold() {
	new_test_ext().execute_with(|| {
		// sum = 180, threshold = 181 → 180 > 181 is false, no flush.
		set_flush_threshold(181);
		C2MBridge::handle_incoming_transfer(subminimal_transfer());
		C2MBridge::handle_incoming_transfer(subminimal_transfer());
		assert_eq!(
			pallet::SubminimalTransfers::<Test>::get(),
			SubminimalTransfersState { count: 2, sum: 180 }
		);
		assert!(subminimal_events().is_empty());
		assert!(mock_pallet::Transfers::<Test>::get().is_empty());
	})
}

#[test]
fn subminimal_no_flush_at_exact_threshold() {
	new_test_ext().execute_with(|| {
		// sum = 180, threshold = 180 → strict `sum > threshold` is false, no flush.
		set_flush_threshold(180);
		C2MBridge::handle_incoming_transfer(subminimal_transfer());
		C2MBridge::handle_incoming_transfer(subminimal_transfer());
		assert_eq!(
			pallet::SubminimalTransfers::<Test>::get(),
			SubminimalTransfersState { count: 2, sum: 180 }
		);
		assert!(subminimal_events().is_empty());
		assert!(mock_pallet::Transfers::<Test>::get().is_empty());
	})
}

#[test]
fn subminimal_flushes_just_above_threshold() {
	new_test_ext().execute_with(|| {
		// sum = 180, threshold = 179 → 180 > 179 is true, flush.
		set_flush_threshold(179);
		C2MBridge::handle_incoming_transfer(subminimal_transfer());
		C2MBridge::handle_incoming_transfer(subminimal_transfer());
		assert_eq!(
			pallet::SubminimalTransfers::<Test>::get(),
			SubminimalTransfersState { count: 0, sum: 0 }
		);
		assert_eq!(mock_pallet::Transfers::<Test>::get().len(), 1);
		assert_eq!(
			subminimal_events(),
			vec![Event::SubminimalFlushTransfer {
				amount: 180,
				count: 2,
				midnight_tx_hash: [0u8; 32],
			}],
		);
	})
}

#[test]
fn subminimal_state_resets_after_flush_and_accumulates_again() {
	new_test_ext().execute_with(|| {
		set_flush_threshold(180);
		// Accumulate to (count=2, sum=180) — no flush at strict equality.
		C2MBridge::handle_incoming_transfer(subminimal_transfer());
		C2MBridge::handle_incoming_transfer(subminimal_transfer());
		assert_eq!(
			pallet::SubminimalTransfers::<Test>::get(),
			SubminimalTransfersState { count: 2, sum: 180 }
		);
		// 3rd transfer pushes sum to 270 > 180 → flush, storage reset.
		C2MBridge::handle_incoming_transfer(subminimal_transfer());
		assert_eq!(
			pallet::SubminimalTransfers::<Test>::get(),
			SubminimalTransfersState { count: 0, sum: 0 }
		);
		assert_eq!(mock_pallet::Transfers::<Test>::get().len(), 1);

		// New subminimal after a flush must restart the accumulator from zero,
		// not inherit any residue from the prior cycle.
		C2MBridge::handle_incoming_transfer(subminimal_transfer());
		assert_eq!(
			pallet::SubminimalTransfers::<Test>::get(),
			SubminimalTransfersState { count: 1, sum: 90 }
		);
		// Still only the flush from the first cycle — the 4th transfer must
		// not have produced a second system tx.
		assert_eq!(mock_pallet::Transfers::<Test>::get().len(), 1);
		assert_eq!(
			subminimal_events(),
			vec![Event::SubminimalFlushTransfer {
				amount: 270,
				count: 3,
				midnight_tx_hash: [0u8; 32],
			}],
		);
	})
}

#[test]
fn subminimal_invalid_recipient_accumulates_not_unlocks() {
	new_test_ext().execute_with(|| {
		// `handle_incoming_transfer` routes by amount before recipient kind,
		// so a subminimal `Invalid` transfer accumulates rather than emitting
		// `InvalidTransfer` / `UnlockToTreasury`.
		set_flush_threshold(1_000);
		let transfer = BridgeTransferV1 {
			amount: 90,
			mc_tx_hash: McTxHash([42; 32]),
			recipient: TransferRecipient::Invalid,
		};
		C2MBridge::handle_incoming_transfer(transfer);

		assert_eq!(
			pallet::SubminimalTransfers::<Test>::get(),
			SubminimalTransfersState { count: 1, sum: 90 }
		);
		assert!(subminimal_events().is_empty());
		assert!(mock_pallet::Transfers::<Test>::get().is_empty());
	})
}

#[test]
fn subminimal_unapproved_user_accumulates_not_unlocks() {
	new_test_ext().execute_with(|| {
		// Same routing argument as the invalid-recipient case: an addressed
		// subminimal with no governance approval must accumulate, not emit
		// `UnapprovedTransfer`.
		set_flush_threshold(1_000);
		let transfer = BridgeTransferV1 {
			amount: 90,
			mc_tx_hash: McTxHash([7; 32]),
			recipient: TransferRecipient::Address { recipient: recipient() },
		};
		assert!(!pallet::ApprovedMcTxHashes::<Test>::contains_key(transfer.mc_tx_hash));
		C2MBridge::handle_incoming_transfer(transfer);

		assert_eq!(
			pallet::SubminimalTransfers::<Test>::get(),
			SubminimalTransfersState { count: 1, sum: 90 }
		);
		assert!(subminimal_events().is_empty());
		assert!(mock_pallet::Transfers::<Test>::get().is_empty());
	})
}

#[test]
fn regular_transfer_does_not_disturb_subminimal_accumulator() {
	new_test_ext().execute_with(|| {
		set_flush_threshold(1_000);
		// Seed the accumulator.
		C2MBridge::handle_incoming_transfer(subminimal_transfer());
		assert_eq!(
			pallet::SubminimalTransfers::<Test>::get(),
			SubminimalTransfersState { count: 1, sum: 90 }
		);

		// A regular approved User transfer interleaves; it must take the
		// regular path (system tx + UserTransfer event) and leave the
		// subminimal accumulator untouched.
		let tx = addressed_transfer();
		approve(tx.mc_tx_hash);
		C2MBridge::handle_incoming_transfer(tx);
		assert_eq!(mock_pallet::Transfers::<Test>::get().len(), 1);
		assert_eq!(
			pallet::SubminimalTransfers::<Test>::get(),
			SubminimalTransfersState { count: 1, sum: 90 }
		);

		// Next subminimal continues accumulating from where it left off.
		C2MBridge::handle_incoming_transfer(subminimal_transfer());
		assert_eq!(
			pallet::SubminimalTransfers::<Test>::get(),
			SubminimalTransfersState { count: 2, sum: 180 }
		);
		// Still just the one regular system tx — no flush.
		assert_eq!(mock_pallet::Transfers::<Test>::get().len(), 1);
		assert_eq!(
			subminimal_events(),
			vec![Event::UserTransfer {
				mc_tx_hash: McTxHash([1; 32]),
				amount: 100,
				recipient: recipient(),
				midnight_tx_hash: [0u8; 32],
			}],
		);
	})
}

fn batch(hashes: Vec<McTxHash>) -> BoundedVec<McTxHash, ConstU32<MAX_APPROVALS_PER_BATCH>> {
	BoundedVec::try_from(hashes).expect("batch exceeds bound")
}

#[test]
fn add_approved_mc_tx_hashes_requires_governance_origin() {
	new_test_ext().execute_with(|| {
		let hashes = batch(vec![McTxHash([1; 32])]);
		assert_noop!(
			C2MBridge::add_approved_mc_tx_hashes(frame_system::RawOrigin::None.into(), hashes),
			DispatchError::BadOrigin,
		);
	})
}

#[test]
fn add_approved_mc_tx_hashes_inserts_unique_entries() {
	new_test_ext().execute_with(|| {
		let h1 = McTxHash([1; 32]);
		let h2 = McTxHash([2; 32]);
		let h3 = McTxHash([3; 32]);

		assert_ok!(C2MBridge::add_approved_mc_tx_hashes(
			frame_system::RawOrigin::Root.into(),
			batch(vec![h1, h2]),
		));
		// Re-submitting `h1` along with a new `h3` should only newly insert `h3`.
		assert_ok!(C2MBridge::add_approved_mc_tx_hashes(
			frame_system::RawOrigin::Root.into(),
			batch(vec![h1, h3]),
		));

		assert_eq!(
			sort_hashes(C2MBridge::get_approved_mc_tx_hashes()),
			sort_hashes(vec![h1, h2, h3])
		);
	})
}
