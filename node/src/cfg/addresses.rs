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

use super::error::CfgError;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Addresses {
	pub addresses: Address,
	pub policy_ids: MintingPolicies,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Address {
	pub committee_candidate_validator: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MintingPolicies {
	pub d_parameter: String,
	pub permissioned_candidates: String,
}

impl Addresses {
	pub fn load(path: &str) -> Result<Self, CfgError> {
		let data = std::fs::read_to_string(path)?;
		let addresses = serde_json::from_str(&data)?;
		Ok(addresses)
	}
}
