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

//! What the node and off-chain tools can read about block rewards.

use crate::{
	ActivationRecord, CardanoEpoch, Destination, HaltReason, OpenEpochState, PayoutAmount,
	RewardParameters, SettlementCursor, StakeCredential,
};
use midnight_primitives_block_production::OperatorId;
use parity_scale_codec::{Codec, Decode, Encode};
use scale_info::TypeInfo;

/// The pallet's counters and flags in one read.
#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, TypeInfo)]
pub struct RewardState<BlockNumber> {
	pub activation: Option<ActivationRecord<BlockNumber>>,
	pub parameters_version: Option<u32>,
	/// `B_o`: reserve not drawn yet, in STAR.
	pub reserve_outstanding: u128,
	/// `D`: drawn by blocks so far.
	pub drawn: u128,
	/// `S`: paid to the ledger or held in escrow so far.
	pub settled: u128,
	/// `L`: released by the Cardano Reserve so far.
	pub released: u128,
	/// The head of the settlement queue.
	pub settlement: Option<SettlementCursor>,
	/// Held in escrow, including releases not yet paid.
	pub escrow_total: u128,
	pub halted: Option<HaltReason>,
	pub open_epoch: Option<OpenEpochState>,
}

sp_api::decl_runtime_apis! {
	pub trait BlockRewardsApi<BlockNumber> where BlockNumber: Codec {
		fn state() -> RewardState<BlockNumber>;
		/// Any installed version, so a record's `parameters_version` can be resolved.
		fn parameters(version: u32) -> Option<RewardParameters>;
		/// The producer share an operator has accrued in a Cardano epoch not yet split.
		fn bucket(cardano_epoch: CardanoEpoch, operator: OperatorId) -> u128;
		/// What a destination will receive from a Cardano epoch whose split has credited it, until
		/// it is paid.
		fn payout(cardano_epoch: CardanoEpoch, destination: Destination) -> PayoutAmount;
		/// Escrow held for a stake credential, until its locks name one reward address.
		fn escrow(stake_credential: StakeCredential) -> u128;
	}
}
