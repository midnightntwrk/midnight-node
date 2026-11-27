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

//! Who produced a block.
//!
//! Enum variants are pinned with `#[codec(index)]`: never reorder or remove them.

#![cfg_attr(not(feature = "std"), no_std)]

use authority_selection_inherents::CommitteeMember;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// Cardano stake pool id: the blake2b-224 hash of the pool's public key.
pub type PoolId = sidechain_domain::MainchainKeyHash;

/// Epoch label stored with the committee that produced a block. It lags behind the wall-clock
/// epoch while the committee is not rotated, e.g. when no next committee was selected; a distinct
/// type keeps the two apart.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	Default,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct EpochLabel(pub u64);

impl From<sidechain_domain::ScEpochNumber> for EpochLabel {
	fn from(epoch: sidechain_domain::ScEpochNumber) -> Self {
		Self(epoch.0)
	}
}

/// Operator of a committee seat.
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
pub enum OperatorId {
	/// A Cardano stake pool operator.
	#[codec(index = 0)]
	AdaSpo(PoolId),
}

/// Who produced a block.
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
pub enum Producer {
	/// A permissioned committee member; it has no operator.
	#[codec(index = 0)]
	Permissioned,
	#[codec(index = 1)]
	Operator(OperatorId),
}

/// The producer of the current block and the committee label of its epoch.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Attribution {
	pub epoch: EpochLabel,
	/// `None` only if the author index could not be resolved, which a correct chain never does.
	pub producer: Option<Producer>,
}

/// The producer of the block being executed.
///
/// Read it only from code that runs after `pallet_session` (a higher pallet index, extrinsics,
/// `on_finalize`): in a block that opens an epoch, an earlier read sees the outgoing committee.
pub trait BlockProducer {
	fn current() -> Attribution;
}

/// The producer a committee seat stands for: the member's stake pool if it registered on Cardano,
/// otherwise [`Producer::Permissioned`].
pub fn producer_of<AuthorityId, AuthorityKeys>(
	member: &CommitteeMember<AuthorityId, AuthorityKeys>,
) -> Producer {
	match member {
		CommitteeMember::Permissioned { .. } => Producer::Permissioned,
		CommitteeMember::Registered { stake_pool_pub_key, .. } => {
			Producer::Operator(OperatorId::AdaSpo(stake_pool_pub_key.hash()))
		},
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use hex_literal::hex;
	use sidechain_domain::StakePoolPublicKey;

	const POOL: PoolId = sidechain_domain::MainchainKeyHash([0xAB; 28]);

	#[test]
	fn encodings_are_pinned() {
		assert_eq!(Producer::Permissioned.encode(), hex!("00"));
		let mut operator = hex!("0100").to_vec();
		operator.extend_from_slice(&[0xAB; 28]);
		assert_eq!(Producer::Operator(OperatorId::AdaSpo(POOL)).encode(), operator);
		assert_eq!(Producer::max_encoded_len(), operator.len());
		assert_eq!(EpochLabel(0x0102).encode(), hex!("0201000000000000"));
	}

	#[test]
	fn a_registered_member_stands_for_its_stake_pool() {
		let key = StakePoolPublicKey([9; 32]);
		let member =
			CommitteeMember::Registered { id: 1u8, keys: 2u8, stake_pool_pub_key: key.clone() };

		assert_eq!(producer_of(&member), Producer::Operator(OperatorId::AdaSpo(key.hash())));
	}

	#[test]
	fn a_permissioned_member_stands_for_no_operator() {
		assert_eq!(producer_of(&CommitteeMember::permissioned(1u8, 2u8)), Producer::Permissioned);
	}
}
