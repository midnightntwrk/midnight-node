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

use super::*;

#[test]
fn keeps_the_first_reason_until_governance_clears_it() {
	new_test_ext().execute_with(|| {
		BlockRewards::halt(HaltReason::TreasuryPaymentFailed);
		BlockRewards::halt(HaltReason::LedgerPoolMismatch);
		assert_eq!(SettlementHalted::<Test>::get(), Some(HaltReason::TreasuryPaymentFailed));
		assert_eq!(System::events().len(), 1);

		assert_noop!(
			BlockRewards::clear_halt(RuntimeOrigin::signed(1), [7; 32]),
			DispatchError::BadOrigin
		);
		assert_ok!(BlockRewards::clear_halt(RuntimeOrigin::root(), [7; 32]));
		assert_eq!(SettlementHalted::<Test>::get(), None);
		assert_eq!(
			last_event(),
			Event::HaltCleared {
				reason: HaltReason::TreasuryPaymentFailed,
				reconciliation: [7; 32]
			}
		);
	});
}

#[test]
fn clearing_needs_a_halt() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			BlockRewards::clear_halt(RuntimeOrigin::root(), [0; 32]),
			Error::<Test>::NotHalted
		);
	});
}

#[test]
fn reasons_are_pinned() {
	use parity_scale_codec::Encode;
	let reasons = [
		HaltReason::SettledAboveReleased,
		HaltReason::LedgerPoolMismatch,
		HaltReason::TreasuryPaymentFailed,
		HaltReason::SnapshotInconsistent,
		HaltReason::DistributionFailed,
	];
	for (index, reason) in reasons.into_iter().enumerate() {
		assert_eq!(reason.encode(), [index as u8]);
	}
}

#[test]
fn queue_encodings_are_pinned() {
	use parity_scale_codec::Encode;
	let cursor =
		|entry| SettlementCursor { cardano_epoch: CardanoEpoch(0x0102_0304), entry }.encode();
	let at = |tail: &[&[u8]]| [&[4u8, 3, 2, 1][..], &tail.concat()].concat();
	assert_eq!(cursor(SettlementEntry::Treasury), at(&[&[0]]));
	assert_eq!(cursor(SettlementEntry::Payout), at(&[&[2]]));
	assert_eq!(cursor(SettlementEntry::Release { after: None }), at(&[&[3, 0]]));
	let encoded_operator = operator(1).encode();
	let summing = SplitStage::Summing { after: None, shares: 5, stake: 6 };
	assert_eq!(
		cursor(SettlementEntry::Split { operator: operator(1), stage: summing }),
		at(&[&[1], &encoded_operator, &[0, 0], &5u128.to_le_bytes(), &6u128.to_le_bytes()])
	);
	let crediting =
		SplitStage::Crediting { after: Some(RewardPayee::Operator), payee_shares: 5, credited: 6 };
	assert_eq!(
		cursor(SettlementEntry::Split { operator: operator(1), stage: crediting }),
		at(&[&[1], &encoded_operator, &[1, 1, 0], &5u128.to_le_bytes(), &6u128.to_le_bytes()])
	);
	assert_eq!(Destination::Address(address(7)).encode(), [&[0u8][..], &[7; 32]].concat());
	assert_eq!(Destination::Escrow(credential(9)).encode(), [&[1u8, 0][..], &[9; 28]].concat());
}
