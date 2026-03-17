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

pub mod ledger_8;
pub use ledger_8::*;

pub mod ledger_7;

// Conversion impls for encoded zswap types to ledger types.
// These live here (not in common/) because common/ is compiled twice
// (once for ledger_7, once for ledger_8) and both versions now share
// the same coin_structure types, which would cause E0119 conflicts.
use crate::toolkit_js::encoded_zswap_local_state::{
	EncodedOutput, EncodedQualifiedShieldedCoinInfo, EncodedRecipient,
};
use midnight_node_ledger_helpers::ledger_8::{
	CoinInfo, CoinPublicKey, ContractAddress, Deserializable, HashOutput, Nonce, QualifiedInfo,
	Recipient, Serializable, ShieldedTokenType,
};

impl From<&EncodedRecipient> for Recipient {
	fn from(value: &EncodedRecipient) -> Self {
		if value.is_left {
			let bytes = value.left.0.0.0;
			Recipient::User(CoinPublicKey(HashOutput(bytes)))
		} else {
			let mut serialized = Vec::new();
			Serializable::serialize(&value.right.0, &mut serialized)
				.expect("failed to serialize contract address");
			let contract_address =
				<ContractAddress as Deserializable>::deserialize(&mut &serialized[..], 0)
					.expect("failed to deserialize contract address");
			Recipient::Contract(contract_address)
		}
	}
}

impl From<&EncodedOutput> for CoinInfo {
	fn from(value: &EncodedOutput) -> Self {
		CoinInfo {
			nonce: Nonce(HashOutput(value.coin_info.nonce)),
			type_: ShieldedTokenType(HashOutput(value.coin_info.color)),
			value: value.coin_info.value,
		}
	}
}

impl From<&EncodedQualifiedShieldedCoinInfo> for CoinInfo {
	fn from(value: &EncodedQualifiedShieldedCoinInfo) -> Self {
		CoinInfo {
			nonce: Nonce(HashOutput(value.nonce)),
			type_: ShieldedTokenType(HashOutput(value.color)),
			value: value.value,
		}
	}
}

impl From<&EncodedQualifiedShieldedCoinInfo> for QualifiedInfo {
	fn from(value: &EncodedQualifiedShieldedCoinInfo) -> Self {
		CoinInfo::from(value).qualify(value.mt_index)
	}
}
