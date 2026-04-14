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

//! Genesis message verification module.
//!
//! Verifies that the expected genesis remark message from `message-config.json`
//! is present in the `genesis_extrinsics` of a chain-spec-raw.json file.

use midnight_node_res::networks::MessageConfig;
use midnight_node_runtime::{RuntimeCall, SystemCall, UncheckedExtrinsic};
use parity_scale_codec::Decode;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VerifyGenesisMessageError {
	#[error("Failed to read file: {0}")]
	IoError(#[from] std::io::Error),

	#[error("Failed to parse JSON: {0}")]
	JsonError(#[from] serde_json::Error),

	#[error("Missing genesis_extrinsics in chain spec properties")]
	MissingGenesisExtrinsics,

	#[error("Invalid hex encoding in extrinsic: {0}")]
	InvalidHex(#[from] hex::FromHexError),
}

#[derive(Debug, Clone)]
pub struct VerificationResult {
	pub message_found: bool,
	pub message_found_detail: String,
	pub message_matches: bool,
	pub message_matches_detail: String,
}

impl VerificationResult {
	pub fn all_passed(&self) -> bool {
		self.message_found && self.message_matches
	}

	pub fn print_summary(&self) {
		// Print machine-parseable markers for the shell script
		if self.message_found {
			println!("GENESIS_MESSAGE_FOUND");
		}
		if self.message_matches {
			println!("GENESIS_MESSAGE_MATCH");
		}

		// Print human-readable details
		println!("\n=== Genesis Message Verification Results ===\n");

		println!(
			"Message Found: {} - {}",
			if self.message_found { "PASS" } else { "FAIL" },
			self.message_found_detail
		);

		println!(
			"Message Match: {} - {}",
			if self.message_matches { "PASS" } else { "FAIL" },
			self.message_matches_detail
		);
	}
}

/// Extract genesis_extrinsics from chain-spec-raw.json
fn load_genesis_extrinsics(
	chain_spec_path: &Path,
) -> Result<Vec<String>, VerifyGenesisMessageError> {
	let content = std::fs::read_to_string(chain_spec_path)?;
	let spec: serde_json::Value = serde_json::from_str(&content)?;

	let extrinsics = spec
		.get("properties")
		.and_then(|p| p.get("genesis_extrinsics"))
		.and_then(|e| e.as_array())
		.ok_or(VerifyGenesisMessageError::MissingGenesisExtrinsics)?;

	let hex_strings: Vec<String> =
		extrinsics.iter().filter_map(|v| v.as_str().map(String::from)).collect();

	Ok(hex_strings)
}

/// Load message from message-config.json
fn load_message_config(
	message_config_path: &Path,
) -> Result<MessageConfig, VerifyGenesisMessageError> {
	let content = std::fs::read_to_string(message_config_path)?;
	let config: MessageConfig = serde_json::from_str(&content)?;
	Ok(config)
}

/// Decode an extrinsic hex string and extract the remark payload if it's a System::remark
fn extract_remark_from_extrinsic(hex_str: &str) -> Option<Vec<u8>> {
	let bytes = hex::decode(hex_str).ok()?;
	let extrinsic = UncheckedExtrinsic::decode(&mut bytes.as_slice()).ok()?;

	match extrinsic.function {
		RuntimeCall::System(SystemCall::remark { remark }) => Some(remark),
		_ => None,
	}
}

/// Verify that the expected genesis message is present in the chain spec's genesis extrinsics.
pub fn verify_genesis_message(
	chain_spec_path: &Path,
	message_config_path: &Path,
) -> Result<VerificationResult, VerifyGenesisMessageError> {
	log::info!("Loading genesis extrinsics from {}", chain_spec_path.display());
	let extrinsic_hexes = load_genesis_extrinsics(chain_spec_path)?;

	log::info!("Loading expected message from {}", message_config_path.display());
	let message_config = load_message_config(message_config_path)?;
	let expected_message = message_config.message.as_bytes();

	log::info!("Scanning {} extrinsics for System::remark", extrinsic_hexes.len());

	// Find all remark extrinsics
	let remarks: Vec<Vec<u8>> = extrinsic_hexes
		.iter()
		.filter_map(|hex_str| extract_remark_from_extrinsic(hex_str))
		.collect();

	let message_found = !remarks.is_empty();
	let message_found_detail = if message_found {
		format!("Found {} System::remark extrinsic(s) in genesis_extrinsics", remarks.len())
	} else {
		"No System::remark extrinsic found in genesis_extrinsics".to_string()
	};

	let message_matches = remarks.iter().any(|remark| remark == expected_message);
	let message_matches_detail = if message_matches {
		let msg_preview = String::from_utf8_lossy(expected_message);
		let preview = if msg_preview.len() > 80 {
			format!("{}...", &msg_preview[..80])
		} else {
			msg_preview.to_string()
		};
		format!("Genesis remark matches expected message: \"{}\"", preview)
	} else if message_found {
		let found_msgs: Vec<String> = remarks
			.iter()
			.map(|r| {
				let s = String::from_utf8_lossy(r);
				if s.len() > 80 { format!("\"{}...\"", &s[..80]) } else { format!("\"{}\"", s) }
			})
			.collect();
		format!("Genesis remark does not match expected message. Found: {}", found_msgs.join(", "))
	} else {
		"No System::remark extrinsic found to compare".to_string()
	};

	Ok(VerificationResult {
		message_found,
		message_found_detail,
		message_matches,
		message_matches_detail,
	})
}
