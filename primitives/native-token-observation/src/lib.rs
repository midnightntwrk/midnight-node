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

use sp_api::decl_runtime_apis;
extern crate alloc;

use alloc::vec::Vec;

decl_runtime_apis! {
	pub trait NativeTokenObservationApi {
		/// Get the contract address on Cardano which emits registration mappings in utxo datums
		fn get_mapping_validator_address() -> Vec<u8>;
		/// Get the range of Cardano blocks to query data for next
		fn get_next_block_range() -> (i64, i64);
		/// Get the Cardano native token identifier for the chosen asset
		fn get_native_token_identifier() -> (Vec<u8>, Vec<u8>);
	}
}
