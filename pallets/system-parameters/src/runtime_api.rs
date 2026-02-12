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

//! Runtime API definitions for System Parameters pallet

use alloc::vec::Vec;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sidechain_domain::DParameter;

/// Terms and Conditions for runtime API (uses Vec<u8> for URL to avoid generic bounds)
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct TermsAndConditionsResponse<Hash> {
	/// SHA-256 hash of the terms and conditions document
	pub hash: Hash,
	/// URL where the terms and conditions can be found (UTF-8 encoded)
	pub url: Vec<u8>,
}

sp_api::decl_runtime_apis! {
	/// Runtime API for querying system parameters
	pub trait SystemParametersApi<Hash> where Hash: Encode + Decode {
		/// Get the current Terms and Conditions
		fn get_terms_and_conditions() -> Option<TermsAndConditionsResponse<Hash>>;

		/// Get the current D-Parameter
		fn get_d_parameter() -> DParameter;
	}
}
