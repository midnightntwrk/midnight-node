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

//! Verification of ICS (Illiquid Circulation Supply) Authorization Scripts
//!
//! This module verifies that the ICS validator contract is linked to the expected
//! authorization script. The verification performs three checks:
//!
//! 1. The compiled_code hash matches the policy_id (Plutus V3 script hash = blake2b_224(03 || code))
//! 2. The two_stage_policy_id is embedded in the compiled_code
//! 3. The authorization script observed on Cardano matches the expected authorization_policy_id

use super::verify_auth_script_common::{
	CheckResult, VerifyAuthScriptError, get_block_number, hex_to_policy_id,
	load_authorization_addresses, print_check, verify_authorization_script, verify_policy_hash,
	verify_two_stage_embedded,
};
use serde::Deserialize;
use sidechain_domain::{McBlockHash, PolicyId};
use sqlx::PgPool;
use std::path::Path;

/// ICS addresses including compiled code and two-stage policy ID
#[derive(Debug, Clone, Deserialize)]
pub struct IcsAddressesWithCode {
	pub illiquid_circulation_supply_validator_address: String,
	#[serde(deserialize_with = "hex_to_policy_id")]
	pub illiquid_circulation_supply_validator_policy_id: PolicyId,
	pub illiquid_circulation_supply_validator_compiled_code: String,
	#[serde(deserialize_with = "hex_to_policy_id")]
	pub illiquid_circulation_supply_validator_two_stage_policy_id: PolicyId,
}

/// Result of all verification checks for ICS
#[derive(Debug)]
pub struct IcsVerificationResult {
	pub policy_hash_check: CheckResult,
	pub two_stage_embedded_check: CheckResult,
	pub authorization_script_check: Option<CheckResult>,
}

impl IcsVerificationResult {
	pub fn all_passed(&self) -> bool {
		self.policy_hash_check.passed
			&& self.two_stage_embedded_check.passed
			&& self.authorization_script_check.as_ref().is_none_or(|c| c.passed)
	}

	pub fn print_summary(&self) {
		println!("\n=== ICS Auth Script Verification ===\n");

		print_check("ICS Policy Hash", &self.policy_hash_check);
		print_check("ICS Two-Stage Embedded", &self.two_stage_embedded_check);

		if let Some(ref check) = self.authorization_script_check {
			print_check("Authorization Script Match", check);
		}

		println!();
		if self.all_passed() {
			println!("RESULT: ALL CHECKS PASSED");
		} else {
			println!("RESULT: SOME CHECKS FAILED");
		}
	}
}

/// Load ICS addresses with compiled code from JSON file
fn load_ics_addresses(path: &Path) -> Result<IcsAddressesWithCode, VerifyAuthScriptError> {
	let content = std::fs::read_to_string(path)?;
	let addresses: IcsAddressesWithCode = serde_json::from_str(&content)?;
	Ok(addresses)
}

/// Main verification function for ICS
pub async fn verify_ics_auth_script(
	ics_addresses_path: &Path,
	authorization_addresses_path: Option<&Path>,
	pool: &PgPool,
	cardano_tip: &McBlockHash,
) -> Result<IcsVerificationResult, VerifyAuthScriptError> {
	log::info!("Loading ICS addresses from {}", ics_addresses_path.display());
	let addresses = load_ics_addresses(ics_addresses_path)?;

	// Check 1: ICS policy hash
	let policy_hash_check = verify_policy_hash(
		"ICS",
		&addresses.illiquid_circulation_supply_validator_compiled_code,
		&addresses.illiquid_circulation_supply_validator_policy_id,
	);

	// Check 2: ICS two_stage_policy_id embedded
	let two_stage_embedded_check = verify_two_stage_embedded(
		"ICS",
		&addresses.illiquid_circulation_supply_validator_compiled_code,
		&addresses.illiquid_circulation_supply_validator_two_stage_policy_id,
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

				Some(
					verify_authorization_script(
						pool,
						&addresses.illiquid_circulation_supply_validator_two_stage_policy_id,
						&expected_auth_policy_id,
						block_number,
					)
					.await,
				)
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

	Ok(IcsVerificationResult {
		policy_hash_check,
		two_stage_embedded_check,
		authorization_script_check,
	})
}
