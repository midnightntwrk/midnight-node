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

//! Verification of Permissioned Candidates Authorization Scripts
//!
//! This module verifies that the permissioned candidates contract is linked to the expected
//! authorization script. The verification performs three checks:
//!
//! 1. The compiled_code hash matches the policy_id (Plutus V3 script hash = blake2b_224(03 || code))
//! 2. The two_stage_policy_id is embedded in the compiled_code
//! 3. The authorization script observed on Cardano matches the expected authorization_policy_id

use midnight_primitives_mainchain_follower::db::{DbDatum, GovernanceBodyUtxoRow};
use serde::{Deserialize, Deserializer};
use sidechain_domain::{McBlockHash, PolicyId};
use sqlx::PgPool;
use std::path::Path;

/// Custom deserializer for PolicyId from hex-encoded string
fn hex_to_policy_id<'de, D>(deserializer: D) -> Result<PolicyId, D::Error>
where
	D: Deserializer<'de>,
{
	let s: String = String::deserialize(deserializer)?;
	PolicyId::decode_hex(&s).map_err(serde::de::Error::custom)
}

/// Permissioned candidates addresses including compiled code and two-stage policy ID
#[derive(Debug, Clone, Deserialize)]
pub struct PermissionedCandidatesAddressesWithCode {
	#[serde(deserialize_with = "hex_to_policy_id")]
	pub permissioned_candidates_policy_id: PolicyId,
	pub permissioned_candidates_compiled_code: String,
	#[serde(deserialize_with = "hex_to_policy_id")]
	pub permissioned_candidates_two_stage_policy_id: PolicyId,
}

/// Custom deserializer for optional PolicyId (empty string = None)
fn hex_to_optional_policy_id<'de, D>(deserializer: D) -> Result<Option<PolicyId>, D::Error>
where
	D: Deserializer<'de>,
{
	let s: String = String::deserialize(deserializer)?;
	if s.is_empty() {
		return Ok(None);
	}
	PolicyId::decode_hex(&s).map(Some).map_err(serde::de::Error::custom)
}

/// Authorization addresses configuration
#[derive(Debug, Clone, Deserialize)]
pub struct AuthorizationAddresses {
	#[serde(deserialize_with = "hex_to_optional_policy_id")]
	pub authorization_policy_id: Option<PolicyId>,
}

#[derive(Debug, thiserror::Error)]
pub enum VerifyPermissionedCandidatesAuthScriptError {
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

/// Result of all verification checks for Permissioned Candidates
#[derive(Debug)]
pub struct PermissionedCandidatesVerificationResult {
	pub policy_hash_check: CheckResult,
	pub two_stage_embedded_check: CheckResult,
	pub authorization_script_check: Option<CheckResult>,
}

impl PermissionedCandidatesVerificationResult {
	pub fn all_passed(&self) -> bool {
		self.policy_hash_check.passed
			&& self.two_stage_embedded_check.passed
			&& self.authorization_script_check.as_ref().is_none_or(|c| c.passed)
	}

	pub fn print_summary(&self) {
		println!("\n=== Permissioned Candidates Auth Script Verification ===\n");

		Self::print_check("Permissioned Candidates Policy Hash", &self.policy_hash_check);
		Self::print_check(
			"Permissioned Candidates Two-Stage Embedded",
			&self.two_stage_embedded_check,
		);

		if let Some(ref check) = self.authorization_script_check {
			Self::print_check("Authorization Script Match", check);
		}

		println!();
		if self.all_passed() {
			println!("RESULT: ALL CHECKS PASSED");
		} else {
			println!("RESULT: SOME CHECKS FAILED");
		}
	}

	fn print_check(name: &str, check: &CheckResult) {
		let status = if check.passed { "PASS" } else { "FAIL" };
		println!("[{}] {}: {}", status, name, check.message);
	}
}

/// Compute blake2b_224 hash of the input data
fn blake2b_224(data: &[u8]) -> [u8; 28] {
	blake2b_simd::Params::new()
		.hash_length(28)
		.hash(data)
		.as_bytes()
		.try_into()
		.expect("blake2b_224 should produce 28 bytes")
}

/// Compute Plutus V3 script hash: blake2b_224(0x03 || script_bytes)
fn plutus_v3_script_hash(script_bytes: &[u8]) -> PolicyId {
	let mut data = vec![0x03]; // Plutus V3 prefix
	data.extend_from_slice(script_bytes);
	PolicyId(blake2b_224(&data))
}

/// Check if a policy_id is embedded in the compiled code
fn is_policy_id_embedded(compiled_code: &[u8], policy_id: &PolicyId) -> bool {
	compiled_code.windows(28).any(|window| window == policy_id.0)
}

/// Load permissioned candidates addresses with compiled code from JSON file
fn load_permissioned_candidates_addresses(
	path: &Path,
) -> Result<PermissionedCandidatesAddressesWithCode, VerifyPermissionedCandidatesAuthScriptError> {
	let content = std::fs::read_to_string(path)?;
	let addresses: PermissionedCandidatesAddressesWithCode = serde_json::from_str(&content)?;
	Ok(addresses)
}

/// Load authorization addresses from JSON file
fn load_authorization_addresses(
	path: &Path,
) -> Result<AuthorizationAddresses, VerifyPermissionedCandidatesAuthScriptError> {
	let content = std::fs::read_to_string(path)?;
	let addresses: AuthorizationAddresses = serde_json::from_str(&content)?;
	Ok(addresses)
}

/// Query the two-stage UTxO and extract the authorization script from the datum
async fn get_authorization_script_from_datum(
	pool: &PgPool,
	two_stage_policy_id: &PolicyId,
	block_number: u32,
) -> Result<PolicyId, VerifyPermissionedCandidatesAuthScriptError> {
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
		VerifyPermissionedCandidatesAuthScriptError::UtxoNotFound(format!(
			"No UTxO found with two_stage_policy_id {} and asset_name MAIN",
			hex::encode(two_stage_policy_id.0)
		))
	})?;

	decode_authorization_from_datum(&row.full_datum)
}

/// Decode the authorization script hash from a two-stage datum
fn decode_authorization_from_datum(
	datum: &DbDatum,
) -> Result<PolicyId, VerifyPermissionedCandidatesAuthScriptError> {
	let list = datum.0.as_list().ok_or_else(|| {
		VerifyPermissionedCandidatesAuthScriptError::DatumDecodeError(
			"Expected datum to be a list".to_string(),
		)
	})?;

	if list.len() < 3 {
		return Err(VerifyPermissionedCandidatesAuthScriptError::DatumDecodeError(format!(
			"Expected at least 3 elements in datum list, got {}",
			list.len()
		)));
	}

	let auth_script_data = list.get(2);

	let auth_bytes = auth_script_data.as_bytes().ok_or_else(|| {
		VerifyPermissionedCandidatesAuthScriptError::DatumDecodeError(
			"Authorization script element is not bytes".to_string(),
		)
	})?;

	if auth_bytes.len() != 28 {
		return Err(VerifyPermissionedCandidatesAuthScriptError::DatumDecodeError(format!(
			"Expected 28 bytes for authorization script hash, got {}",
			auth_bytes.len()
		)));
	}

	let mut result = [0u8; 28];
	result.copy_from_slice(&auth_bytes);
	Ok(PolicyId(result))
}

/// Get block number from block hash
async fn get_block_number(
	pool: &PgPool,
	block_hash: &McBlockHash,
) -> Result<u32, VerifyPermissionedCandidatesAuthScriptError> {
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
		VerifyPermissionedCandidatesAuthScriptError::BlockNotFound(format!(
			"Block {} not found",
			block_hash
		))
	})?;

	Ok(block_number as u32)
}

/// Verify policy hash
fn verify_policy_hash(compiled_code_hex: &str, expected_policy_id: &PolicyId) -> CheckResult {
	match hex::decode(compiled_code_hex) {
		Ok(code_bytes) => {
			let computed_hash = plutus_v3_script_hash(&code_bytes);
			if computed_hash == *expected_policy_id {
				CheckResult {
					passed: true,
					message: format!(
						"Permissioned Candidates policy_id {} matches blake2b_224(03 || compiled_code)",
						hex::encode(expected_policy_id.0)
					),
				}
			} else {
				CheckResult {
					passed: false,
					message: format!(
						"Permissioned Candidates policy hash mismatch: expected {}, computed {}",
						hex::encode(expected_policy_id.0),
						hex::encode(computed_hash.0)
					),
				}
			}
		},
		Err(e) => CheckResult {
			passed: false,
			message: format!("Permissioned Candidates compiled_code hex decode error: {}", e),
		},
	}
}

/// Verify that the two_stage_policy_id is embedded in the compiled code
fn verify_two_stage_embedded(
	compiled_code_hex: &str,
	two_stage_policy_id: &PolicyId,
) -> CheckResult {
	match hex::decode(compiled_code_hex) {
		Ok(code_bytes) => {
			if is_policy_id_embedded(&code_bytes, two_stage_policy_id) {
				CheckResult {
					passed: true,
					message: format!(
						"Permissioned Candidates two_stage_policy_id {} is embedded in compiled_code",
						hex::encode(two_stage_policy_id.0)
					),
				}
			} else {
				CheckResult {
					passed: false,
					message: format!(
						"Permissioned Candidates two_stage_policy_id {} NOT found in compiled_code",
						hex::encode(two_stage_policy_id.0)
					),
				}
			}
		},
		Err(e) => CheckResult {
			passed: false,
			message: format!("Permissioned Candidates compiled_code hex decode error: {}", e),
		},
	}
}

/// Main verification function for Permissioned Candidates
pub async fn verify_permissioned_candidates_auth_script(
	permissioned_candidates_addresses_path: &Path,
	authorization_addresses_path: Option<&Path>,
	pool: &PgPool,
	cardano_tip: &McBlockHash,
) -> Result<PermissionedCandidatesVerificationResult, VerifyPermissionedCandidatesAuthScriptError> {
	log::info!(
		"Loading permissioned candidates addresses from {}",
		permissioned_candidates_addresses_path.display()
	);
	let addresses = load_permissioned_candidates_addresses(permissioned_candidates_addresses_path)?;

	// Check 1: Permissioned Candidates policy hash
	let policy_hash_check = verify_policy_hash(
		&addresses.permissioned_candidates_compiled_code,
		&addresses.permissioned_candidates_policy_id,
	);

	// Check 2: Permissioned Candidates two_stage_policy_id embedded
	let two_stage_embedded_check = verify_two_stage_embedded(
		&addresses.permissioned_candidates_compiled_code,
		&addresses.permissioned_candidates_two_stage_policy_id,
	);

	// Check 3: Authorization script from Cardano matches expected
	let authorization_script_check = if let Some(auth_path) = authorization_addresses_path {
		log::info!("Loading authorization addresses from {}", auth_path.display());
		let auth_addresses = load_authorization_addresses(auth_path)?;

		match auth_addresses.authorization_policy_id {
			Some(expected_auth_policy_id) => {
				log::info!("Querying Cardano for authorization script...");

				let block_number = get_block_number(pool, cardano_tip).await?;
				log::info!("Resolved cardano tip {} to block number {}", cardano_tip, block_number);

				match get_authorization_script_from_datum(
					pool,
					&addresses.permissioned_candidates_two_stage_policy_id,
					block_number,
				)
				.await
				{
					Ok(observed_auth_policy_id) => {
						if observed_auth_policy_id == expected_auth_policy_id {
							Some(CheckResult {
								passed: true,
								message: format!(
									"Authorization script {} matches expected value",
									hex::encode(observed_auth_policy_id.0)
								),
							})
						} else {
							Some(CheckResult {
								passed: false,
								message: format!(
									"Authorization script mismatch: expected {}, observed {}",
									hex::encode(expected_auth_policy_id.0),
									hex::encode(observed_auth_policy_id.0)
								),
							})
						}
					},
					Err(e) => Some(CheckResult {
						passed: false,
						message: format!("Failed to query authorization script: {}", e),
					}),
				}
			},
			None => {
				log::info!(
					"No expected authorization_policy_id configured, skipping Cardano query"
				);
				None
			},
		}
	} else {
		None
	};

	Ok(PermissionedCandidatesVerificationResult {
		policy_hash_check,
		two_stage_embedded_check,
		authorization_script_check,
	})
}
