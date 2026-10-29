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

//! Fixed-size boundary for the experimental Ethereum bridge. Configuration is runtime-owned.
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct EscrowConfig {
	pub bridge_domain: [u8; 32],
	pub token_domain: [u8; 32],
	pub escrow: [u8; 32],
	pub color: [u8; 32],
	/// Ledger arena hash of the complete compiled `retire` operation, including its verifier key.
	pub retire_operation: [u8; 32],
}

#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct Redemption {
	pub magic: [u8; 4],
	pub version: u8,
	pub bridge_domain: [u8; 32],
	pub escrow: [u8; 32],
	pub sequence: u64,
	pub amount: u128,
	pub recipient: [u8; 20],
	pub color: [u8; 32],
	pub total_retired: u128,
	pub count: u16,
	pub ledger_balance: u128,
}

#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub enum EscrowError {
	LedgerUnavailable,
	ContractMissing,
	Schema,
	Configuration,
	Unsealed,
	Operation,
	Range,
	RecordMissing,
	Balance,
}

#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct EthereumCredit {
	pub deposit_id: [u8; 32],
	pub output_id: [u8; 32],
	pub color: [u8; 32],
	pub amount: u128,
	pub recipient: [u8; 32],
}
