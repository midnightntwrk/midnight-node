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

//! The pallet's stored and queued types.

use crate::{CardanoEpoch, ParameterError, RewardAddress, RewardPayee, StakeCredential};
use frame_support::pallet_prelude::*;
use midnight_primitives_block_production::OperatorId;

#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub enum HaltReason {
	/// `S` would exceed the released total `L`, or does.
	#[codec(index = 0)]
	SettledAboveReleased,
	/// The ledger's `block_reward_pool` is below `baseline + escrow + (L − S)`.
	#[codec(index = 1)]
	LedgerPoolBelowBaseline,
	/// The ledger rejected the Treasury payment. One that does not fit waits for a later block.
	#[codec(index = 2)]
	TreasuryPaymentFailed,
	/// The snapshot lacks a pool that earned in the epoch, or its record changed or is gone, or
	/// it holds terms or stakes the split cannot use.
	#[codec(index = 3)]
	SnapshotInconsistent,
	/// The ledger rejected a `DistributeNight`. One that does not fit waits for a later block.
	#[codec(index = 4)]
	DistributionFailed,
}

#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct ActivationRecord<BlockNumber> {
	pub at: BlockNumber,
	/// Unix time in milliseconds. Rewards start with the first block after `at` whose timestamp
	/// is at or after it.
	pub anchor_ms: u64,
	/// `B_o` at activation, in STAR: `B_o + D` stays equal to it.
	pub reserve: u128,
	/// The ledger's `block_reward_pool` at activation less the released total `L` then. The
	/// pool may already hold NIGHT moved there by other system transactions, which rewards must
	/// not count as theirs.
	pub baseline: u128,
}

/// The epoch blocks are currently accruing into.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct OpenEpochState {
	pub cardano_epoch: CardanoEpoch,
	pub parameters_version: u32,
	/// Sum of the blocks' Treasury shares.
	pub treasury: u128,
	pub blocks: u32,
}

#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct ClosedEpoch {
	pub parameters_version: u32,
	pub blocks: u32,
	pub treasury: u128,
	/// The snapshot record the epoch's draws and split rely on, read once when the epoch closes;
	/// `None` if it was never complete, so no block of the epoch drew.
	pub snapshot_id: Option<[u8; 32]>,
}

/// A kind of settlement entry. An epoch settles its Treasury amount, its splits, its escrow
/// releases and then its payouts.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub enum SettlementEntry {
	/// The epoch's Treasury amount.
	#[codec(index = 0)]
	Treasury,
	/// Splitting one pool's total into the epoch's [`crate::Payouts`]. Pools go in operator order.
	#[codec(index = 1)]
	Split { operator: OperatorId, stage: SplitStage },
	/// Paying the epoch's [`crate::Payouts`] in [`Destination`] order.
	#[codec(index = 2)]
	Payout,
	/// Moving escrow to the address of its owner's locks, for each lock owner of the epoch's
	/// snapshot after `after`. Comes after the splits and before the payouts.
	#[codec(index = 3)]
	Release { after: Option<StakeCredential> },
}

/// What one destination receives for an epoch.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
	Default,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct PayoutAmount {
	/// From the epoch's split: counts in `S`, so it waits for `L − S` to cover it.
	pub settle: u128,
	/// Escrow released to the address: already in `S`, and paid with the settled part.
	pub release: u128,
}

#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub enum SplitStage {
	/// Adding up the payees' shares, which the operator's amount depends on, and their stakes,
	/// which must make up the pool's total stake with the operator's.
	#[codec(index = 0)]
	Summing { after: Option<RewardPayee>, shares: u128, stake: u128 },
	/// Crediting the operator, then each payee after `after`. `credited` is this pool's total in
	/// [`crate::Payouts`] so far.
	#[codec(index = 1)]
	Crediting { after: Option<RewardPayee>, payee_shares: u128, credited: u128 },
}

/// Where a payout goes. The derived order is the order an epoch pays in: reward addresses
/// ascending, which the ledger needs to keep `DistributeNight` cheap, then escrows.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub enum Destination {
	#[codec(index = 0)]
	Address(RewardAddress),
	/// Held for a delegator whose locks for the pool name no single reward address.
	#[codec(index = 1)]
	Escrow(StakeCredential),
}

/// The next entry to settle.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct SettlementCursor {
	pub cardano_epoch: CardanoEpoch,
	pub entry: SettlementEntry,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InstallError {
	Invalid(ParameterError),
	/// Versions are installed in order, starting at 1.
	VersionNotNext {
		expected: u32,
	},
	/// The reserve a later version starts from differs from the current version's; it changes
	/// only with a superseding proposal.
	ReserveChanged,
}
