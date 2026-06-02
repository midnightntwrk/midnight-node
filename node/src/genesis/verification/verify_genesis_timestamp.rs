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

//! Genesis timestamp verification module.
//!
//! Verifies that the `Timestamp::set` extrinsic in `genesis_extrinsics` of
//! chain-spec-raw.json matches the expected timestamp from `cardano-tip.json`.

use midnight_node_runtime::{RuntimeCall, TimestampCall, UncheckedExtrinsic};
use parity_scale_codec::Decode;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VerifyGenesisTimestampError {
	#[error("Failed to read file: {0}")]
	IoError(#[from] std::io::Error),

	#[error("Failed to parse JSON: {0}")]
	JsonError(#[from] serde_json::Error),

	#[error("Missing genesis_extrinsics in chain spec properties")]
	MissingGenesisExtrinsics,

	#[error("Invalid hex encoding in extrinsic: {0}")]
	InvalidHex(#[from] hex::FromHexError),

	#[error("Missing or invalid timestamp in cardano-tip config: {0}")]
	InvalidTimestamp(String),
}

use crate::genesis::CardanoTipConfig;

#[derive(Debug, Clone)]
pub struct VerificationResult {
	pub timestamp_found: bool,
	pub timestamp_found_detail: String,
	pub timestamp_matches: bool,
	pub timestamp_matches_detail: String,
}

impl VerificationResult {
	pub fn all_passed(&self) -> bool {
		self.timestamp_found && self.timestamp_matches
	}

	pub fn print_summary(&self) {
		// Print machine-parseable markers for the shell script
		if self.timestamp_found {
			println!("GENESIS_TIMESTAMP_FOUND");
		}
		if self.timestamp_matches {
			println!("GENESIS_TIMESTAMP_MATCH");
		}

		// Print human-readable details
		println!("\n=== Genesis Timestamp Verification Results ===\n");

		println!(
			"Timestamp Found: {} - {}",
			if self.timestamp_found { "PASS" } else { "FAIL" },
			self.timestamp_found_detail
		);

		println!(
			"Timestamp Match: {} - {}",
			if self.timestamp_matches { "PASS" } else { "FAIL" },
			self.timestamp_matches_detail
		);
	}
}

/// Extract genesis_extrinsics from chain-spec-raw.json
fn load_genesis_extrinsics(
	chain_spec_path: &Path,
) -> Result<Vec<String>, VerifyGenesisTimestampError> {
	let content = std::fs::read_to_string(chain_spec_path)?;
	let spec: serde_json::Value = serde_json::from_str(&content)?;

	let extrinsics = spec
		.get("properties")
		.and_then(|p| p.get("genesis_extrinsics"))
		.and_then(|e| e.as_array())
		.ok_or(VerifyGenesisTimestampError::MissingGenesisExtrinsics)?;

	let hex_strings: Vec<String> =
		extrinsics.iter().filter_map(|v| v.as_str().map(String::from)).collect();

	Ok(hex_strings)
}

/// Load and parse the expected timestamp from cardano-tip.json (in seconds)
fn load_expected_timestamp_secs(
	cardano_tip_config_path: &Path,
) -> Result<u64, VerifyGenesisTimestampError> {
	let content = std::fs::read_to_string(cardano_tip_config_path)?;
	let config: CardanoTipConfig = serde_json::from_str(&content)?;

	config.timestamp.parse::<u64>().map_err(|e| {
		VerifyGenesisTimestampError::InvalidTimestamp(format!(
			"cannot parse '{}' as u64: {}",
			config.timestamp, e
		))
	})
}

/// Decode an extrinsic hex string and extract the timestamp if it's a Timestamp::set
fn extract_timestamp_from_extrinsic(hex_str: &str) -> Option<u64> {
	let bytes = hex::decode(hex_str).ok()?;
	let extrinsic = UncheckedExtrinsic::decode(&mut bytes.as_slice()).ok()?;

	match extrinsic.function {
		RuntimeCall::Timestamp(TimestampCall::set { now }) => Some(now),
		_ => None,
	}
}

/// Verify that the genesis timestamp in the chain spec matches the expected
/// timestamp from cardano-tip.json.
pub fn verify_genesis_timestamp(
	chain_spec_path: &Path,
	cardano_tip_config_path: &Path,
) -> Result<VerificationResult, VerifyGenesisTimestampError> {
	log::info!("Loading genesis extrinsics from {}", chain_spec_path.display());
	let extrinsic_hexes = load_genesis_extrinsics(chain_spec_path)?;

	log::info!("Loading expected timestamp from {}", cardano_tip_config_path.display());
	let expected_timestamp_secs = load_expected_timestamp_secs(cardano_tip_config_path)?;
	let expected_timestamp_millis = expected_timestamp_secs * 1000;

	log::info!(
		"Expected timestamp: {} secs ({} ms). Scanning {} extrinsics for Timestamp::set",
		expected_timestamp_secs,
		expected_timestamp_millis,
		extrinsic_hexes.len()
	);

	// Find all Timestamp::set extrinsics
	let timestamps: Vec<u64> = extrinsic_hexes
		.iter()
		.filter_map(|hex_str| extract_timestamp_from_extrinsic(hex_str))
		.collect();

	let timestamp_found = !timestamps.is_empty();
	let timestamp_found_detail = if timestamp_found {
		format!("Found {} Timestamp::set extrinsic(s) in genesis_extrinsics", timestamps.len())
	} else {
		"No Timestamp::set extrinsic found in genesis_extrinsics".to_string()
	};

	let timestamp_matches = timestamps.contains(&expected_timestamp_millis);
	let timestamp_matches_detail = if timestamp_matches {
		format!(
			"Genesis timestamp matches: {} ms (cardano-tip: {} secs)",
			expected_timestamp_millis, expected_timestamp_secs
		)
	} else if timestamp_found {
		let found_values: Vec<String> =
			timestamps.iter().map(|ts| format!("{} ms ({} secs)", ts, ts / 1000)).collect();
		format!(
			"Genesis timestamp does not match. Expected {} ms (from cardano-tip {} secs). Found: {}",
			expected_timestamp_millis,
			expected_timestamp_secs,
			found_values.join(", ")
		)
	} else {
		"No Timestamp::set extrinsic found to compare".to_string()
	};

	Ok(VerificationResult {
		timestamp_found,
		timestamp_found_detail,
		timestamp_matches,
		timestamp_matches_detail,
	})
}
