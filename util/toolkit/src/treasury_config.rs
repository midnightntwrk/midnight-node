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

//! Treasury configuration for genesis initialization.
//!
//! This module defines the configuration structure for initializing the Midnight
//! treasury from observed cNight deposits in the ICS (Illiquid Circulation Supply)
//! contract on Cardano.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Configuration for treasury initialization from ICS contract observations.
///
/// The treasury is funded by observing cNight deposits locked in the ICS contract
/// on Cardano. Each UTxO in the list represents a deposit that contributes to the
/// total treasury amount.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CnightTreasuryConfig {
	/// The ICS contract address on Cardano where cNight is locked.
	pub ics_contract_address: String,

	/// List of UTxOs at the ICS contract containing cNight.
	/// Each UTxO contributes to the total treasury amount.
	pub utxos: Vec<TreasuryUtxo>,

	/// Total Night amount to initialize the treasury with.
	/// This must equal the sum of all UTxO amounts.
	pub total_night_amount: u128,
}

/// A UTxO at the ICS contract containing cNight tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreasuryUtxo {
	/// Transaction hash where the UTxO was created.
	pub tx_hash: String,

	/// Output index within the transaction.
	pub output_index: u32,

	/// Expected cNight amount in this UTxO.
	pub expected_amount: u128,
}

/// Errors that can occur during treasury configuration validation.
#[derive(Debug, Error)]
pub enum TreasuryConfigError {
	/// The configured total does not match the sum of UTxO amounts.
	#[error("Total mismatch: configured {configured}, computed {computed}")]
	TotalMismatch { configured: u128, computed: u128 },

	/// Overflow occurred when computing the sum of UTxO amounts.
	#[error("Overflow computing total from UTxO amounts")]
	Overflow,
}

impl CnightTreasuryConfig {
	/// Validate the treasury configuration.
	///
	/// Checks that the configured `total_night_amount` equals the sum of all
	/// UTxO `expected_amount` values.
	///
	/// # Errors
	///
	/// Returns `TreasuryConfigError::TotalMismatch` if the total doesn't match.
	/// Returns `TreasuryConfigError::Overflow` if the sum would overflow u128.
	pub fn validate(&self) -> Result<(), TreasuryConfigError> {
		let computed = self
			.utxos
			.iter()
			.try_fold(0u128, |acc, utxo| acc.checked_add(utxo.expected_amount))
			.ok_or(TreasuryConfigError::Overflow)?;

		if computed != self.total_night_amount {
			return Err(TreasuryConfigError::TotalMismatch {
				configured: self.total_night_amount,
				computed,
			});
		}

		Ok(())
	}

	/// Get the total treasury amount.
	pub fn treasury_amount(&self) -> u128 {
		self.total_night_amount
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_parse_valid_config() {
		let json = r#"{
            "ics_contract_address": "addr_test1qz...",
            "utxos": [
                {"tx_hash": "abc123", "output_index": 0, "expected_amount": 1000}
            ],
            "total_night_amount": 1000
        }"#;

		let config: CnightTreasuryConfig = serde_json::from_str(json).unwrap();
		assert_eq!(config.total_night_amount, 1000);
		assert_eq!(config.utxos.len(), 1);
		assert!(config.validate().is_ok());
	}

	#[test]
	fn test_validate_total_mismatch_fails() {
		let config = CnightTreasuryConfig {
			ics_contract_address: "addr_test1...".to_string(),
			utxos: vec![TreasuryUtxo {
				tx_hash: "a".to_string(),
				output_index: 0,
				expected_amount: 500,
			}],
			total_night_amount: 1000, // Mismatch!
		};

		let result = config.validate();
		assert!(matches!(result, Err(TreasuryConfigError::TotalMismatch { .. })));
	}

	#[test]
	fn test_validate_overflow_handling() {
		let config = CnightTreasuryConfig {
			ics_contract_address: "addr_test1...".to_string(),
			utxos: vec![
				TreasuryUtxo {
					tx_hash: "a".to_string(),
					output_index: 0,
					expected_amount: u128::MAX,
				},
				TreasuryUtxo { tx_hash: "b".to_string(), output_index: 0, expected_amount: 1 },
			],
			total_night_amount: u128::MAX,
		};

		let result = config.validate();
		assert!(matches!(result, Err(TreasuryConfigError::Overflow)));
	}
}
