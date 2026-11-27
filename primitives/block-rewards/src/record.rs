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

//! The record every block leaves, rewarded or not.

use crate::{BlockDraw, Utilization};
use midnight_primitives_block_production::{EpochLabel, OperatorId};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// Cardano epoch whose stake snapshot selected the committee that produced a block. Rewards are
/// accounted per Cardano epoch: many Midnight epochs (committee sessions) share one.
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
pub struct CardanoEpoch(pub u32);

/// Enough to recompute the block's draw from the parent's outstanding reserve and the parameter
/// version. A block that earns nothing has a zero draw.
#[derive(
	Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct BlockRewardRecord {
	pub midnight_epoch: EpochLabel,
	pub cardano_epoch: CardanoEpoch,
	/// `None` for a permissioned producer.
	pub operator: Option<OperatorId>,
	pub parameters_version: u32,
	pub utilization: Utilization,
	pub draw: BlockDraw,
}

#[cfg(test)]
mod tests {
	use super::*;
	use hex_literal::hex;
	use midnight_primitives_block_production::PoolId;

	#[test]
	fn encoding_is_pinned() {
		let record = BlockRewardRecord {
			midnight_epoch: EpochLabel(0x0102),
			cardano_epoch: CardanoEpoch(0x0304),
			operator: Some(OperatorId::AdaSpo(PoolId::decode(&mut &[0xaa; 28][..]).unwrap())),
			parameters_version: 1,
			utilization: Utilization { used: 1, max: 2 },
			draw: BlockDraw { base: 3, producer: 2, treasury: 1 },
		};
		let expected = [
			&hex!("0201000000000000")[..],
			&hex!("04030000")[..],
			&hex!("0100")[..],
			&[0xaa; 28][..],
			&hex!("01000000")[..],
			&1u128.to_le_bytes()[..],
			&2u128.to_le_bytes()[..],
			&3u128.to_le_bytes()[..],
			&2u128.to_le_bytes()[..],
			&1u128.to_le_bytes()[..],
		]
		.concat();
		assert_eq!(record.encode(), expected);
		assert_eq!(record.encode().len(), BlockRewardRecord::max_encoded_len());
	}

	#[test]
	fn a_permissioned_block_has_no_operator() {
		let record = BlockRewardRecord {
			midnight_epoch: EpochLabel(0),
			cardano_epoch: CardanoEpoch(0),
			operator: None,
			parameters_version: 1,
			utilization: Utilization { used: 0, max: 1 },
			draw: BlockDraw::default(),
		};
		assert_eq!(record.encode()[12], 0);
	}
}
