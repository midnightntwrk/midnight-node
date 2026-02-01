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

use std::{path::Path, sync::Arc};

use authority_selection_inherents::AuthoritySelectionDataSource;
use serde::{Deserialize, Serialize};
use sidechain_domain::{McEpochNumber, PolicyId};
use sp_core::crypto::KeyTypeId;
use tokio::{fs::File, io::AsyncWriteExt};

#[derive(Debug, thiserror::Error)]
pub enum PermissionedCandidatesGenesisError {
	#[error("Failed to serialize to JSON: {0}")]
	SerdeError(#[from] serde_json::Error),

	#[error("Failed retrieving from data source: {0}")]
	DatasourceError(String),

	#[error("I/O error: {0}")]
	IoError(#[from] std::io::Error),

	#[error("No permissioned candidates found at the given cardano tip")]
	NoCandidatesFound,
}

/// Input addresses file structure for permissioned candidates genesis generation.
/// This file contains only the policy ID needed to query the mainchain.
#[derive(Debug, Clone, Default, Serialize, Deserialize, serde_valid::Validate)]
pub struct PermissionedCandidatesAddresses {
	/// Policy ID of the permissioned candidates token on Cardano (hex-encoded, no 0x prefix)
	#[serde(with = "hex")]
	pub permissioned_candidates_policy_id: [u8; 28],
}

/// A single permissioned candidate entry for the genesis config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionedCandidateEntry {
	/// AURA public key (32 bytes, hex with 0x prefix)
	pub aura_pub_key: String,
	/// GRANDPA public key (32 bytes, hex with 0x prefix)
	pub grandpa_pub_key: String,
	/// Sidechain/cross-chain public key (33 bytes compressed, hex with 0x prefix)
	pub sidechain_pub_key: String,
	/// BEEFY public key (33 bytes compressed, hex with 0x prefix)
	pub beefy_pub_key: String,
}

/// Output genesis config structure for permissioned candidates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionedCandidatesGenesisConfig {
	/// Policy ID of the permissioned candidates token (hex-encoded, no 0x prefix)
	#[serde(serialize_with = "serialize_policy_id", deserialize_with = "deserialize_policy_id")]
	pub permissioned_candidates_policy_id: PolicyId,
	/// List of initial permissioned candidates
	pub initial_permissioned_candidates: Vec<PermissionedCandidateEntry>,
}

fn serialize_policy_id<S>(policy_id: &PolicyId, serializer: S) -> Result<S::Ok, S::Error>
where
	S: serde::Serializer,
{
	let hex_str = format!("0x{}", hex::encode(policy_id.0));
	serializer.serialize_str(&hex_str)
}

fn deserialize_policy_id<'de, D>(deserializer: D) -> Result<PolicyId, D::Error>
where
	D: serde::Deserializer<'de>,
{
	let s: String = String::deserialize(deserializer)?;
	let s = s.strip_prefix("0x").unwrap_or(&s);
	let bytes = hex::decode(s).map_err(serde::de::Error::custom)?;
	let arr: [u8; 28] = bytes
		.try_into()
		.map_err(|_| serde::de::Error::custom("PolicyId must be 28 bytes"))?;
	Ok(PolicyId(arr))
}

/// Key type identifiers used in CandidateKeys
const AURA: KeyTypeId = KeyTypeId(*b"aura");
const GRANDPA: KeyTypeId = KeyTypeId(*b"gran");
const CROSS_CHAIN: KeyTypeId = KeyTypeId(*b"crch");
// BEEFY uses the same key as cross-chain for now
const BEEFY: KeyTypeId = KeyTypeId(*b"beef");

/// Cardano section of pc-chain-config.json
#[derive(Debug, Clone, Deserialize)]
pub struct PcChainConfigCardano {
	pub security_parameter: u32,
}

/// Partial structure of pc-chain-config.json for reading security_parameter
#[derive(Debug, Clone, Deserialize)]
pub struct PcChainConfig {
	pub cardano: PcChainConfigCardano,
}

/// Saves as json file the Permissioned Candidates Genesis Config
///
/// The `epoch` parameter specifies the Cardano epoch to query for permissioned candidates.
/// Note: The data source applies a 2-epoch offset internally, so data from epoch N-2 is returned
/// when querying for epoch N.
pub async fn generate_permissioned_candidates_genesis(
	addresses: PermissionedCandidatesAddresses,
	authority_selection_data_source: Arc<dyn AuthoritySelectionDataSource + Send + Sync>,
	epoch: McEpochNumber,
	output_path: impl AsRef<Path>,
) -> Result<(), PermissionedCandidatesGenesisError> {
	let policy_id = PolicyId(addresses.permissioned_candidates_policy_id);

	// D-parameter policy is not used (hardcoded in the data source), so we pass the same policy
	let ariadne_params = authority_selection_data_source
		.get_ariadne_parameters(epoch, policy_id.clone(), policy_id.clone())
		.await
		.map_err(|e| PermissionedCandidatesGenesisError::DatasourceError(e.to_string()))?;

	let permissioned_candidates = ariadne_params
		.permissioned_candidates
		.ok_or(PermissionedCandidatesGenesisError::NoCandidatesFound)?;

	let mut entries = Vec::new();

	for candidate in permissioned_candidates {
		// Extract keys from CandidateKeys
		let aura_key = candidate.keys.find(AURA).map(|k| format!("0x{}", hex::encode(k)));
		let grandpa_key = candidate.keys.find(GRANDPA).map(|k| format!("0x{}", hex::encode(k)));
		let sidechain_key = format!("0x{}", hex::encode(&candidate.sidechain_public_key.0));
		// BEEFY key - try to find it, otherwise use sidechain key
		let beefy_key = candidate
			.keys
			.find(BEEFY)
			.map(|k| format!("0x{}", hex::encode(k)))
			.or_else(|| candidate.keys.find(CROSS_CHAIN).map(|k| format!("0x{}", hex::encode(k))))
			.unwrap_or_else(|| sidechain_key.clone());

		if let (Some(aura), Some(grandpa)) = (aura_key, grandpa_key) {
			entries.push(PermissionedCandidateEntry {
				aura_pub_key: aura,
				grandpa_pub_key: grandpa,
				sidechain_pub_key: sidechain_key,
				beefy_pub_key: beefy_key,
			});
		} else {
			log::warn!(
				"Skipping candidate with missing AURA or GRANDPA key: sidechain_public_key={}",
				hex::encode(&candidate.sidechain_public_key.0)
			);
		}
	}

	let config = PermissionedCandidatesGenesisConfig {
		permissioned_candidates_policy_id: policy_id,
		initial_permissioned_candidates: entries,
	};

	let json = serde_json::to_string_pretty(&config)?;
	let mut file = File::create(output_path.as_ref()).await?;
	file.write_all(json.as_bytes()).await?;
	log::info!(
		"Wrote Permissioned Candidates genesis to {} ({} candidates)",
		output_path.as_ref().display(),
		config.initial_permissioned_candidates.len()
	);

	Ok(())
}
