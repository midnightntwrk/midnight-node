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

//! Conversions from crate-level (ledger_8) types to version-local types.
//!
//! When compiled through `ledger_8.rs`, these are identity operations (same types).
//! When compiled through `ledger_7.rs`, these convert through raw bytes/strings.

use super::ledger_helpers_local::{
	CoinPublicKey, ContractAddress, HashOutput, ShieldedTokenType, UnshieldedTokenType, WalletSeed,
};
use std::str::FromStr;

pub fn convert_shielded_token_type(
	stt: midnight_node_ledger_helpers::ShieldedTokenType,
) -> ShieldedTokenType {
	ShieldedTokenType(HashOutput(stt.0.0))
}

pub fn convert_unshielded_token_type(
	utt: midnight_node_ledger_helpers::UnshieldedTokenType,
) -> UnshieldedTokenType {
	UnshieldedTokenType(HashOutput(utt.0.0))
}

pub fn convert_contract_address(
	ca: midnight_node_ledger_helpers::ContractAddress,
) -> ContractAddress {
	ContractAddress(HashOutput(ca.0.0))
}

pub fn convert_wallet_seed(ws: midnight_node_ledger_helpers::WalletSeed) -> WalletSeed {
	WalletSeed::try_from(ws.as_bytes()).expect("wallet seed conversion between versions")
}

pub fn convert_coin_public_key(cpk: midnight_node_ledger_helpers::CoinPublicKey) -> CoinPublicKey {
	CoinPublicKey(HashOutput(cpk.0.0))
}

pub fn convert_wallet_address(
	wa: &midnight_node_ledger_helpers::WalletAddress,
) -> super::ledger_helpers_local::WalletAddress {
	super::ledger_helpers_local::WalletAddress::from_str(&wa.to_bech32())
		.expect("wallet address conversion between versions")
}
