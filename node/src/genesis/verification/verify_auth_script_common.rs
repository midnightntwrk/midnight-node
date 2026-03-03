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

//! Common utilities for authorization script verification
//!
//! This module provides shared functionality used by all authorization script
//! verification commands (federated authority, ICS, permissioned candidates).

use midnight_primitives_mainchain_follower::db::{DbDatum, GovernanceBodyUtxoRow};
use serde::{Deserialize, Deserializer};
use sidechain_domain::{McBlockHash, PolicyId};
use sqlx::PgPool;
use std::path::Path;

/// Common error type for auth script verification
#[derive(Debug, thiserror::Error)]
pub enum VerifyAuthScriptError {
	#[error("I/O error: {0}")]
	IoError(#[from] std::io::Error),

	#[error("JSON parse error: {0}")]
	JsonError(#[from] serde_json::Error),

	#[error("Hex decode error: {0}")]
	HexError(#[from] hex::FromHexError),

	#[error("Database error: {0}")]
	DatabaseError(#[from] sqlx::Error),

	#[error("Verification failed: {0}")]
	VerificationFailed(String),

	#[error("Block not found: {0}")]
	BlockNotFound(String),

	#[error("UTxO not found: {0}")]
	UtxoNotFound(String),

	#[error("Datum decode error: {0}")]
	DatumDecodeError(String),
}

/// Result of a single verification check
#[derive(Debug, Clone)]
pub struct CheckResult {
	pub passed: bool,
	pub message: String,
}

/// Authorization addresses configuration
#[derive(Debug, Clone, Deserialize)]
pub struct AuthorizationAddresses {
	#[serde(deserialize_with = "hex_to_optional_policy_id")]
	pub authorization_policy_id: Option<PolicyId>,
}

/// Custom deserializer for PolicyId from hex-encoded string
pub fn hex_to_policy_id<'de, D>(deserializer: D) -> Result<PolicyId, D::Error>
where
	D: Deserializer<'de>,
{
	let s: String = String::deserialize(deserializer)?;
	PolicyId::decode_hex(&s).map_err(serde::de::Error::custom)
}

/// Custom deserializer for optional PolicyId (empty string = None)
pub fn hex_to_optional_policy_id<'de, D>(deserializer: D) -> Result<Option<PolicyId>, D::Error>
where
	D: Deserializer<'de>,
{
	let s: String = String::deserialize(deserializer)?;
	if s.is_empty() {
		return Ok(None);
	}
	PolicyId::decode_hex(&s).map(Some).map_err(serde::de::Error::custom)
}

/// Compute blake2b_224 hash of the input data
pub fn blake2b_224(data: &[u8]) -> [u8; 28] {
	blake2b_simd::Params::new()
		.hash_length(28)
		.hash(data)
		.as_bytes()
		.try_into()
		.expect("blake2b_224 should produce 28 bytes")
}

/// Compute Plutus V3 script hash: blake2b_224(0x03 || script_bytes)
pub fn plutus_v3_script_hash(script_bytes: &[u8]) -> PolicyId {
	let mut data = vec![0x03]; // Plutus V3 prefix
	data.extend_from_slice(script_bytes);
	PolicyId(blake2b_224(&data))
}

/// Check if a policy_id is embedded in the compiled code
pub fn is_policy_id_embedded(compiled_code: &[u8], policy_id: &PolicyId) -> bool {
	compiled_code.windows(28).any(|window| window == policy_id.0)
}

/// Load authorization addresses from JSON file
pub fn load_authorization_addresses(
	path: &Path,
) -> Result<AuthorizationAddresses, VerifyAuthScriptError> {
	let content = std::fs::read_to_string(path)?;
	let addresses: AuthorizationAddresses = serde_json::from_str(&content)?;
	Ok(addresses)
}

/// Query the two-stage UTxO and extract the authorization script from the datum
pub async fn get_authorization_script_from_datum(
	pool: &PgPool,
	two_stage_policy_id: &PolicyId,
	block_number: u32,
) -> Result<PolicyId, VerifyAuthScriptError> {
	let row = sqlx::query_as::<_, GovernanceBodyUtxoRow>(
		r#"
		SELECT
			datum.value::jsonb AS full_datum,
			block.block_no as block_number,
			block.hash as block_hash,
			tx.block_index as tx_index_in_block,
			tx.hash AS tx_hash,
			tx_out.index AS utxo_index
		FROM tx_out
			JOIN datum ON tx_out.data_hash = datum.hash
			JOIN tx ON tx.id = tx_out.tx_id
			JOIN block ON block.id = tx.block_id
			JOIN ma_tx_out ON ma_tx_out.tx_out_id = tx_out.id
			JOIN multi_asset ma ON ma.id = ma_tx_out.ident
		WHERE ma.policy = $1
			AND ma.name = $2
			AND block.block_no <= $3
		ORDER BY block.block_no DESC, tx.block_index DESC
		LIMIT 1
		"#,
	)
	.bind(two_stage_policy_id.0.as_slice())
	.bind(b"main".as_slice())
	.bind(block_number as i32)
	.fetch_optional(pool)
	.await?
	.ok_or_else(|| {
		VerifyAuthScriptError::UtxoNotFound(format!(
			"No UTxO found with two_stage_policy_id {} and asset_name MAIN",
			hex::encode(two_stage_policy_id.0)
		))
	})?;

	decode_authorization_from_datum(&row.full_datum)
}

/// Decode the authorization script hash from a two-stage datum
///
/// The datum format is a list where the 3rd position (index 2) contains the authorization script hash (policy_id)
pub fn decode_authorization_from_datum(datum: &DbDatum) -> Result<PolicyId, VerifyAuthScriptError> {
	let list = datum.0.as_list().ok_or_else(|| {
		VerifyAuthScriptError::DatumDecodeError("Expected datum to be a list".to_string())
	})?;

	if list.len() < 3 {
		return Err(VerifyAuthScriptError::DatumDecodeError(format!(
			"Expected at least 3 elements in datum list, got {}",
			list.len()
		)));
	}

	let auth_script_data = list.get(2);

	let auth_bytes = auth_script_data.as_bytes().ok_or_else(|| {
		VerifyAuthScriptError::DatumDecodeError(
			"Authorization script element is not bytes".to_string(),
		)
	})?;

	if auth_bytes.len() != 28 {
		return Err(VerifyAuthScriptError::DatumDecodeError(format!(
			"Expected 28 bytes for authorization script hash, got {}",
			auth_bytes.len()
		)));
	}

	let mut result = [0u8; 28];
	result.copy_from_slice(&auth_bytes);
	Ok(PolicyId(result))
}

/// Get block number from block hash
pub async fn get_block_number(
	pool: &PgPool,
	block_hash: &McBlockHash,
) -> Result<u32, VerifyAuthScriptError> {
	let row: Option<(i32,)> = sqlx::query_as(
		r#"
		SELECT block_no
		FROM block
		WHERE hash = $1
		"#,
	)
	.bind(block_hash.0)
	.fetch_optional(pool)
	.await?;

	let (block_number,) = row.ok_or_else(|| {
		VerifyAuthScriptError::BlockNotFound(format!("Block {} not found", block_hash))
	})?;

	Ok(block_number as u32)
}

/// Verify a policy hash matches the compiled code
pub fn verify_policy_hash(
	contract_name: &str,
	compiled_code_hex: &str,
	expected_policy_id: &PolicyId,
) -> CheckResult {
	match hex::decode(compiled_code_hex) {
		Ok(code_bytes) => {
			let computed_hash = plutus_v3_script_hash(&code_bytes);
			if computed_hash == *expected_policy_id {
				CheckResult {
					passed: true,
					message: format!(
						"{} policy_id {} matches blake2b_224(03 || compiled_code)",
						contract_name,
						hex::encode(expected_policy_id.0)
					),
				}
			} else {
				CheckResult {
					passed: false,
					message: format!(
						"{} policy hash mismatch: expected {}, computed {}",
						contract_name,
						hex::encode(expected_policy_id.0),
						hex::encode(computed_hash.0)
					),
				}
			}
		},
		Err(e) => CheckResult {
			passed: false,
			message: format!("{} compiled_code hex decode error: {}", contract_name, e),
		},
	}
}

/// Verify that the two_stage_policy_id is embedded in the compiled code
pub fn verify_two_stage_embedded(
	contract_name: &str,
	compiled_code_hex: &str,
	two_stage_policy_id: &PolicyId,
) -> CheckResult {
	match hex::decode(compiled_code_hex) {
		Ok(code_bytes) => {
			if is_policy_id_embedded(&code_bytes, two_stage_policy_id) {
				CheckResult {
					passed: true,
					message: format!(
						"{} two_stage_policy_id {} is embedded in compiled_code",
						contract_name,
						hex::encode(two_stage_policy_id.0)
					),
				}
			} else {
				CheckResult {
					passed: false,
					message: format!(
						"{} two_stage_policy_id {} NOT found in compiled_code",
						contract_name,
						hex::encode(two_stage_policy_id.0)
					),
				}
			}
		},
		Err(e) => CheckResult {
			passed: false,
			message: format!("{} compiled_code hex decode error: {}", contract_name, e),
		},
	}
}

/// Verify authorization script from Cardano matches expected value
pub async fn verify_authorization_script(
	pool: &PgPool,
	two_stage_policy_id: &PolicyId,
	expected_auth_policy_id: &PolicyId,
	block_number: u32,
) -> CheckResult {
	match get_authorization_script_from_datum(pool, two_stage_policy_id, block_number).await {
		Ok(observed_auth_policy_id) => {
			if observed_auth_policy_id == *expected_auth_policy_id {
				CheckResult {
					passed: true,
					message: format!(
						"Authorization script {} matches expected value",
						hex::encode(observed_auth_policy_id.0)
					),
				}
			} else {
				CheckResult {
					passed: false,
					message: format!(
						"Authorization script mismatch: expected {}, observed {}",
						hex::encode(expected_auth_policy_id.0),
						hex::encode(observed_auth_policy_id.0)
					),
				}
			}
		},
		Err(e) => CheckResult {
			passed: false,
			message: format!("Failed to query authorization script: {}", e),
		},
	}
}

/// Print a check result
pub fn print_check(name: &str, check: &CheckResult) {
	let status = if check.passed { "PASS" } else { "FAIL" };
	println!("[{}] {}: {}", status, name, check.message);
}
