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

//! Reserve contract observation primitives.
//!
//! This crate defines the shared types for reserve genesis configuration,
//! used by both the node (to generate the config) and the toolkit (to consume it).

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};
// Re-export PolicyId so consumers don't need to depend on sidechain-domain directly
pub use sidechain_domain::PolicyId;

/// Asset identifier for cNIGHT tokens locked in the reserve contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReserveAsset {
	/// The policy ID of the cNIGHT token
	pub policy_id: PolicyId,
	/// The asset name of the cNIGHT token (hex string without 0x prefix)
	pub asset_name: String,
}

/// A single UTxO at the reserve contract address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReserveUtxo {
	/// Transaction hash (hex encoded)
	pub tx_hash: String,
	/// Output index within the transaction
	pub output_index: u16,
	/// Amount of cNIGHT tokens in this UTxO
	pub amount: u64,
}

/// Reserve genesis configuration.
///
/// This is the output of the node's `generate-reserve-genesis` command and
/// the input to the toolkit's `generate-genesis` command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReserveConfig {
	/// The reserve validator address (bech32 format)
	pub reserve_validator_address: String,
	/// The cNIGHT asset identifier
	pub asset: ReserveAsset,
	/// All UTxOs at the reserve address (for verification purposes)
	pub utxos: Vec<ReserveUtxo>,
	/// Total amount of cNIGHT locked at the reserve address
	pub total_amount: u128,
}

/// Errors that can occur during reserve configuration validation.
#[cfg(feature = "std")]
#[derive(Debug, thiserror::Error)]
pub enum ReserveConfigError {
	/// The configured total does not match the sum of UTxO amounts.
	#[error("Total mismatch: configured {configured}, computed {computed}")]
	TotalMismatch { configured: u128, computed: u128 },

	/// Overflow occurred when computing the sum of UTxO amounts.
	#[error("Overflow computing total from UTxO amounts")]
	Overflow,
}

impl ReserveConfig {
	/// Validate the reserve configuration.
	///
	/// Checks that the configured `total_amount` equals the sum of all
	/// UTxO `amount` values.
	#[cfg(feature = "std")]
	pub fn validate(&self) -> Result<(), ReserveConfigError> {
		let computed = self
			.utxos
			.iter()
			.try_fold(0u128, |acc, utxo| acc.checked_add(utxo.amount as u128))
			.ok_or(ReserveConfigError::Overflow)?;

		if computed != self.total_amount {
			return Err(ReserveConfigError::TotalMismatch {
				configured: self.total_amount,
				computed,
			});
		}

		Ok(())
	}

	/// Get the total reserve amount.
	pub fn reserve_amount(&self) -> u128 {
		self.total_amount
	}
}

/// Reserve addresses configuration - input to genesis generation.
///
/// This is used by the node to know which Cardano address to query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReserveAddresses {
	/// The Cardano address of the reserve validator (bech32 format)
	pub reserve_validator_address: String,
	/// The cNIGHT asset identifier
	pub asset: ReserveAsset,
}

#[cfg(test)]
mod tests {
	use super::*;

	fn make_config(utxos: Vec<ReserveUtxo>, total_amount: u128) -> ReserveConfig {
		ReserveConfig {
			reserve_validator_address: "addr_test1qz...".into(),
			asset: ReserveAsset { policy_id: PolicyId([0u8; 28]), asset_name: "NIGHT".into() },
			utxos,
			total_amount,
		}
	}

	fn make_utxo(amount: u64) -> ReserveUtxo {
		ReserveUtxo { tx_hash: "abc123".into(), output_index: 0, amount }
	}

	#[test]
	fn validate_succeeds_with_matching_total() {
		let config = make_config(vec![make_utxo(100), make_utxo(200), make_utxo(300)], 600);
		assert!(config.validate().is_ok());
	}

	#[test]
	fn validate_succeeds_with_empty_utxos_and_zero_total() {
		let config = make_config(vec![], 0);
		assert!(config.validate().is_ok());
	}

	#[test]
	fn validate_fails_when_total_is_greater_than_utxo_sum() {
		let config = make_config(vec![make_utxo(100), make_utxo(200)], 500);
		let err = config.validate().unwrap_err();
		assert!(matches!(
			err,
			ReserveConfigError::TotalMismatch { configured: 500, computed: 300 }
		));
	}

	#[test]
	fn validate_fails_when_total_is_less_than_utxo_sum() {
		let config = make_config(vec![make_utxo(100), make_utxo(200)], 100);
		let err = config.validate().unwrap_err();
		assert!(matches!(
			err,
			ReserveConfigError::TotalMismatch { configured: 100, computed: 300 }
		));
	}

	#[test]
	fn validate_overflow_error_exists() {
		let err = ReserveConfigError::Overflow;
		assert!(matches!(err, ReserveConfigError::Overflow));
		assert_eq!(err.to_string(), "Overflow computing total from UTxO amounts");
	}

	#[test]
	fn validate_total_mismatch_error_message() {
		let err = ReserveConfigError::TotalMismatch { configured: 500, computed: 300 };
		assert_eq!(err.to_string(), "Total mismatch: configured 500, computed 300");
	}
}
