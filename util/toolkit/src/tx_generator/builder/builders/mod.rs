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

pub mod ledger_9;
pub use ledger_9::*;

pub mod ledger_7;
pub mod ledger_8;

// Conversion impls for encoded zswap types to ledger types.
// These live here (not in common/) because common/ is compiled once per ledger
// version, which would cause E0119 conflicts wherever versions share types.
// Ledger 9 uses coin-structure 3.x; ledgers 7 and 8 share coin-structure 2.x,
// so there are exactly two impl sets.
use crate::toolkit_js::encoded_zswap_local_state::{
	EncodedOutput, EncodedQualifiedShieldedCoinInfo, EncodedRecipient,
};
use midnight_node_ledger_helpers::ledger_7::{
	CoinInfo as CoinInfoV2, CoinPublicKey as CoinPublicKeyV2, ContractAddress as ContractAddressV2,
	Nonce as NonceV2, QualifiedInfo as QualifiedInfoV2, Recipient as RecipientV2,
	ShieldedTokenType as ShieldedTokenTypeV2,
};
use midnight_node_ledger_helpers::ledger_9::{
	CoinInfo, CoinPublicKey, ContractAddress, HashOutput, Nonce, QualifiedInfo, Recipient,
	ShieldedTokenType,
};

// All these types are HashOutput newtypes with identical raw bytes in
// coin-structure 2.x and 3.x (the same invariant type_convert.rs relies on),
// so conversion is direct byte reconstruction.
macro_rules! impl_encoded_zswap_conversions {
	($Recipient:ident, $CoinPublicKey:ident, $ContractAddress:ident,
	 $CoinInfo:ident, $Nonce:ident, $ShieldedTokenType:ident, $QualifiedInfo:ident) => {
		impl From<&EncodedRecipient> for $Recipient {
			fn from(value: &EncodedRecipient) -> Self {
				if value.is_left {
					$Recipient::User($CoinPublicKey(HashOutput(value.left.0.0.0)))
				} else {
					$Recipient::Contract($ContractAddress(HashOutput(value.right.0.0.0)))
				}
			}
		}

		impl From<&EncodedOutput> for $CoinInfo {
			fn from(value: &EncodedOutput) -> Self {
				$CoinInfo {
					nonce: $Nonce(HashOutput(value.coin_info.nonce)),
					type_: $ShieldedTokenType(HashOutput(value.coin_info.color)),
					value: value.coin_info.value,
				}
			}
		}

		impl From<&EncodedQualifiedShieldedCoinInfo> for $CoinInfo {
			fn from(value: &EncodedQualifiedShieldedCoinInfo) -> Self {
				$CoinInfo {
					nonce: $Nonce(HashOutput(value.nonce)),
					type_: $ShieldedTokenType(HashOutput(value.color)),
					value: value.value,
				}
			}
		}

		impl From<&EncodedQualifiedShieldedCoinInfo> for $QualifiedInfo {
			fn from(value: &EncodedQualifiedShieldedCoinInfo) -> Self {
				$CoinInfo::from(value).qualify(value.mt_index)
			}
		}
	};
}

impl_encoded_zswap_conversions!(
	RecipientV2,
	CoinPublicKeyV2,
	ContractAddressV2,
	CoinInfoV2,
	NonceV2,
	ShieldedTokenTypeV2,
	QualifiedInfoV2
);
impl_encoded_zswap_conversions!(
	Recipient,
	CoinPublicKey,
	ContractAddress,
	CoinInfo,
	Nonce,
	ShieldedTokenType,
	QualifiedInfo
);
