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

//! What the epoch split reads from a Cardano epoch's stake snapshot record.

use crate::PoolTerms;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// A Midnight address that receives rewards.
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
pub struct RewardAddress(pub [u8; 32]);

/// A Cardano stake credential: kind 0 for a key hash, 1 for a script hash.
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
pub struct StakeCredential {
	pub kind: u8,
	pub hash: [u8; 28],
}

/// A Cardano output: transaction id and output index.
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
pub struct UtxoId {
	pub transaction: [u8; 32],
	pub index: u16,
}

/// One payee of a pool. The derived order is the order a pool's payees settle in: the operator,
/// then delegators, then locks, each by its fields.
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
pub enum RewardPayee {
	#[codec(index = 0)]
	Operator,
	/// An ADA delegator that is not one of the pool's owners.
	#[codec(index = 1)]
	Delegator { stake_credential: StakeCredential },
	#[codec(index = 2)]
	Lock { utxo: UtxoId },
}

/// A pool as the snapshot of an epoch records it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Encode, Decode, TypeInfo)]
pub struct SnapshotPool {
	pub terms: PoolTerms,
	pub reward_address: RewardAddress,
	/// `s_operator`: the lovelace of the pool's owners, counting only rows whose stake credential
	/// is an owner's key (kind 0); a script credential with the same hash is a delegator.
	pub operator_stake: u128,
	/// `s`: the pool's virtual stake, `s_operator` plus every payee's stake.
	pub total_stake: u128,
}

/// A delegator or lock of a pool, with what the split needs.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Encode, Decode, TypeInfo)]
pub struct SnapshotPayee {
	pub payee: RewardPayee,
	/// `s_i`: lovelace for a delegator; for a lock, `floor(cnight × w_n / w_d)` with `w` the
	/// record's `cnight_weight`, rounded for each lock on its own.
	pub stake: u128,
	/// A lock's own address. A delegator's is the one address its locks for the pool name;
	/// `None` when it has no such lock or they name different addresses, and its share goes to
	/// escrow.
	pub reward_address: Option<RewardAddress>,
}

/// A credential that owns locks in the snapshot, for releasing its escrow.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Encode, Decode, TypeInfo)]
pub struct LockOwner {
	pub credential: StakeCredential,
	/// The one address all its locks name; `None` when they name different addresses.
	pub reward_address: Option<RewardAddress>,
}

#[cfg(test)]
mod tests {
	use super::*;

	fn delegator(byte: u8) -> RewardPayee {
		RewardPayee::Delegator { stake_credential: StakeCredential { kind: 0, hash: [byte; 28] } }
	}

	fn lock(byte: u8, index: u16) -> RewardPayee {
		RewardPayee::Lock { utxo: UtxoId { transaction: [byte; 32], index } }
	}

	#[test]
	fn payees_order_operator_then_delegators_then_locks() {
		let mut payees =
			[lock(0, 1), delegator(9), lock(0, 0), RewardPayee::Operator, delegator(1)];
		payees.sort();
		assert_eq!(
			payees,
			[RewardPayee::Operator, delegator(1), delegator(9), lock(0, 0), lock(0, 1)]
		);
	}

	#[test]
	fn payee_encoding_is_pinned() {
		assert_eq!(RewardPayee::Operator.encode(), [0]);
		let encoded = delegator(7).encode();
		assert_eq!((encoded[0], encoded[1], encoded.len()), (1, 0, 30));
		let encoded = lock(3, 0x0102).encode();
		assert_eq!((encoded[0], &encoded[33..], encoded.len()), (2, &[0x02, 0x01][..], 35));
	}
}
