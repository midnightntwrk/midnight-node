// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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

#![cfg_attr(not(feature = "std"), no_std)]

use hex_literal::hex;
use midnight_node_ledger::types::{Tx, active_version::LedgerApiError};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;

#[derive(Clone, Encode, Decode, DecodeWithMemTracking, Debug, TypeInfo)]
pub enum TransactionType {
	MidnightTx(sp_std::vec::Vec<u8>, Option<Tx>),
	TimestampTx(u64),
	UnknownTx,
}

#[derive(Clone, Encode, Decode, DecodeWithMemTracking, Debug, TypeInfo)]
pub enum TransactionTypeV2 {
	MidnightTx(sp_std::vec::Vec<u8>, Result<Tx, LedgerApiError>),
	TimestampTx(u64),
	UnknownTx,
}

pub mod well_known_keys {
	use super::hex;

	pub const MIDNIGHT_STATE_KEY: &[u8] =
		&hex!["2a760f9a173a6df5cd4373ff49fa999bf39a107f2d8d3854c9aba9b021f43d9c"];

	pub const MIDNIGHT_NETWORK_ID_KEY: &[u8] =
		&hex!["2a760f9a173a6df5cd4373ff49fa999b47872dec514b30607df0c271efce9fc4"];
}
