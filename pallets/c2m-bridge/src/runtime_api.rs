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

//! Runtime API definitions for the c2m-bridge pallet.

use alloc::vec::Vec;
use sidechain_domain::McTxHash;

sp_api::decl_runtime_apis! {
	/// Runtime API for querying c2m-bridge approval state.
	pub trait C2MBridgeApi {
		/// Returns the full set of mainchain transaction hashes that have been
		/// pre-approved by governance for crediting mNIGHT to the recipient.
		fn get_approved_mc_tx_hashes() -> Vec<McTxHash>;
	}
}
