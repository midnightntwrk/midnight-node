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

//! What the pallet reads from, and offers to, the rest of the runtime.

use crate::{
	BlockUsage, CardanoEpoch, LockOwner, RewardAddress, RewardPayee, SnapshotPayee, SnapshotPool,
	StakeCredential,
};
use alloc::vec::Vec;
use frame_support::weights::Weight;
use midnight_primitives_block_production::OperatorId;
use parity_scale_codec::{Decode, Encode};

/// The stake snapshot record of each Cardano epoch, kept until the epoch is settled.
pub trait StakeSnapshot {
	/// The hash of the epoch's snapshot record; `None` until the record is complete.
	fn snapshot_id(cardano_epoch: CardanoEpoch) -> Option<[u8; 32]>;
	/// `None` if the operator is not a pool of the epoch's snapshot.
	fn pool(cardano_epoch: CardanoEpoch, operator: &OperatorId) -> Option<SnapshotPool>;
	/// The pool's delegators and locks after `after`, in [`RewardPayee`] order, at most `limit`.
	/// The operator is not among them. The order is `Ord`, not the encoding: a lock's
	/// `UtxoId::index` encodes little-endian, so iterating raw storage keys would differ.
	/// Every call for an epoch must return the same rows, since a pool's shares are summed and
	/// then credited from separate calls.
	fn payees(
		cardano_epoch: CardanoEpoch,
		operator: &OperatorId,
		after: Option<RewardPayee>,
		limit: u32,
	) -> Vec<SnapshotPayee>;
	/// Owners of the snapshot's locks after `after`, in [`StakeCredential`] `Ord` order, at most
	/// `limit`. The snapshot lists only locks that name a pool of the snapshot.
	fn lock_owners(
		cardano_epoch: CardanoEpoch,
		after: Option<StakeCredential>,
		limit: u32,
	) -> Vec<LockOwner>;
}

/// What the pallet reads from the ledger.
pub trait RewardLedger {
	/// The ledger's `block_reward_pool`, in STAR; `None` if it cannot be read.
	fn block_reward_pool() -> Option<u128>;
	/// The ledger's `reserve_pool`, in STAR; `None` if it cannot be read.
	fn reserve_pool() -> Option<u128>;
	/// Moves `amount` from the ledger's `block_reward_pool` to its Treasury.
	fn pay_block_rewards_to_treasury(amount: u128) -> Result<(), PayoutFailure>;
	/// Upper bound of [`Self::pay_block_rewards_to_treasury`]'s weight.
	fn treasury_payment_weight() -> Weight;
	/// Credits each output's amount from `block_reward_pool` to the address's unclaimed rewards,
	/// in one `DistributeNight` transaction.
	fn distribute_night(outputs: Vec<RewardPayout>) -> Result<(), PayoutFailure>;
	/// Upper bound of [`Self::distribute_night`]'s weight for `outputs` outputs.
	fn distribution_weight(outputs: u32) -> Weight;
	/// What the block has used of the ledger's block limit so far; `None` if it cannot be read.
	fn block_usage() -> Option<BlockUsage>;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PayoutFailure {
	/// The block has no room left; try again in a later block.
	BlockFull,
	Rejected,
}

/// One output of a `DistributeNight` reward transaction.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Encode, Decode)]
pub struct RewardPayout {
	pub address: RewardAddress,
	/// STAR
	pub amount: u128,
	/// Unique per epoch and address, so the ledger's replay protection accepts it once.
	pub nonce: [u8; 32],
}

/// Conditions outside the pallet that activation requires.
pub trait ActivationPrerequisites {
	/// The Unix time in milliseconds from which the Reserve releases NIGHT for rewards, once the
	/// reserve release observation reports the released total; `None` until then.
	fn reserve_anchor_ms() -> Option<u64>;
	/// [`crate::Config::CardanoEpoch`] names the epoch of every block's committee.
	fn cardano_epoch_source_live() -> bool;
}
