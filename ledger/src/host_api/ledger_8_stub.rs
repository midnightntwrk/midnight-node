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

//! Ledger-8 host API surface for builds without the `legacy-ledgers` feature.
//!
//! The runtime (pallet-cnight-observation's v2 migration) links
//! `ledger_8_bridge::dust_generation_values` by name, so the host function must exist
//! in every node build. Here it reports `NoLedgerState`, which the migration treats as
//! "nothing to restore" and cancels cleanly. The SCALE types are declared identically to
//! the real `ledger_8` module so the runtime-side encoding is unchanged.

use crate::latest::types::LedgerApiError;
use alloc::vec::Vec;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode};
use sp_runtime_interface::pass_by::{
	AllocateAndReturnByCodec, PassFatPointerAndDecode, PassFatPointerAndRead,
};
use sp_runtime_interface::runtime_interface;

/// One live dust generation entry in the pre-fork (ledger-8) state.
#[derive(Encode, Decode, DecodeWithMemTracking, Debug, Clone, PartialEq)]
pub struct DustGenerationEntry {
	/// The entry's night value.
	pub value: u128,
	/// The (untagged) serialized `DustPublicKey`.
	pub owner: Vec<u8>,
}

/// The result of one batched [`Ledger8Bridge::dust_generation_values`] read.
#[derive(Encode, Decode, DecodeWithMemTracking, Debug, Clone, PartialEq)]
pub struct DustGenerationValues {
	/// The dust parameters' `time_to_cap`, in seconds.
	pub time_to_cap: u64,
	/// One per requested nonce and positionally aligned with them.
	pub entries: Vec<Option<DustGenerationEntry>>,
}

#[runtime_interface]
pub trait Ledger8Bridge {
	fn dust_generation_values(
		&mut self,
		_state_key: PassFatPointerAndRead<&[u8]>,
		_nonces: PassFatPointerAndDecode<Vec<[u8; 32]>>,
	) -> AllocateAndReturnByCodec<Result<DustGenerationValues, LedgerApiError>> {
		log::error!(
			target: "midnight::ledger_v2",
			"ledger 8 is not compiled into this node (`legacy-ledgers` off); \
			 refusing to serve pre-fork dust generation values"
		);
		Err(LedgerApiError::NoLedgerState)
	}
}
