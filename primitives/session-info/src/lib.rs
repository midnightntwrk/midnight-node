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

//! Runtime APIs about substrate sessions: the session index and session key generation.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use sp_core::{crypto::KeyTypeId, ecdsa};

sp_api::decl_runtime_apis! {
	pub trait SessionInfoApi {
		fn current_session_index() -> u32;
	}

	pub trait SessionKeysGenerationApi {
		/// Generates a fresh set of the runtime's session keys into the keystore and returns,
		/// in the runtime's key order, each key's type, raw public key and proof of possession
		/// for the candidate identified by `cross_chain`. The runtime defines what the proof
		/// signs.
		/// The runtime's session key types, in key order.
		fn key_ids() -> Vec<KeyTypeId>;

		fn generate_session_keys_with_proofs(
			cross_chain: ecdsa::Public,
		) -> Vec<(KeyTypeId, Vec<u8>, Vec<u8>)>;
	}
}
