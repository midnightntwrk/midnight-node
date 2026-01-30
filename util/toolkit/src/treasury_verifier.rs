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

//! Treasury configuration verification against Cardano db-sync.
//!
//! This module provides verification of treasury UTxOs against the Cardano
//! mainchain state via db-sync. Per ADR-0023, genesis configuration must be
//! fully verifiable against on-chain state.

use crate::treasury_config::{CNIGHT_ASSET_NAME, CnightTreasuryConfig, TreasuryUtxo};
use sidechain_domain::McBlockHash;
use sqlx::{FromRow, Pool, Postgres, error::Error as SqlxError};
use thiserror::Error;

/// Length of a Cardano policy ID in bytes.
const POLICY_ID_LEN: usize = 28;

/// Length of a Cardano block hash in bytes.
const BLOCK_HASH_LEN: usize = 32;

/// Errors that can occur during treasury verification.
#[derive(Debug, Error)]
pub enum TreasuryVerificationError {
	/// Failed to parse the reference block hash from hex.
	#[error("Invalid reference block hash: {0}")]
	InvalidBlockHash(String),

	/// Failed to parse the cNight policy ID from hex.
	#[error("Invalid cNight policy ID: {0}")]
	InvalidPolicyId(String),

	/// Failed to parse a transaction hash from hex.
	#[error("Invalid transaction hash '{tx_hash}': {reason}")]
	InvalidTxHash { tx_hash: String, reason: String },

	/// Reference block not found in db-sync.
	#[error("Reference block not found: {0}")]
	BlockNotFound(String),

	/// A configured UTxO was not found at the reference block.
	#[error("UTxO not found: {tx_hash}#{output_index}")]
	UtxoNotFound { tx_hash: String, output_index: u32 },

	/// The UTxO exists but is not at the expected address.
	#[error("UTxO {tx_hash}#{output_index} is at wrong address: expected {expected}, got {actual}")]
	WrongAddress { tx_hash: String, output_index: u32, expected: String, actual: String },

	/// The UTxO exists but doesn't contain the expected cNight asset.
	#[error("UTxO {tx_hash}#{output_index} does not contain cNight asset")]
	CnightAssetMissing { tx_hash: String, output_index: u32 },

	/// The UTxO exists but has a different amount than configured.
	#[error("UTxO {tx_hash}#{output_index} amount mismatch: expected {expected}, got {actual}")]
	AmountMismatch { tx_hash: String, output_index: u32, expected: u128, actual: u128 },

	/// Database query failed.
	#[error("Database error: {0}")]
	DatabaseError(#[from] SqlxError),
}

/// Result of verifying a single UTxO.
#[derive(Debug)]
pub struct UtxoVerificationResult {
	/// The transaction hash.
	pub tx_hash: String,
	/// The output index.
	pub output_index: u32,
	/// The verified amount (if successful).
	pub amount: u128,
}

/// Verifies treasury configuration against Cardano db-sync.
pub struct TreasuryVerifier {
	pool: Pool<Postgres>,
}

impl TreasuryVerifier {
	/// Create a new treasury verifier with the given database connection pool.
	pub fn new(pool: Pool<Postgres>) -> Self {
		Self { pool }
	}

	/// Verify all UTxOs in the treasury configuration against db-sync.
	///
	/// Returns the list of verified UTxOs with their amounts if all verifications pass.
	///
	/// # Errors
	///
	/// Returns an error if:
	/// - The reference block is not found
	/// - Any UTxO is not found
	/// - Any UTxO is not at the expected address
	/// - Any UTxO doesn't contain cNight
	/// - Any UTxO has a different amount than configured
	pub async fn verify(
		&self,
		config: &CnightTreasuryConfig,
	) -> Result<Vec<UtxoVerificationResult>, TreasuryVerificationError> {
		// Parse and validate configuration values
		let block_hash = parse_block_hash(&config.reference_block_hash)?;
		let policy_id = parse_policy_id(&config.cnight_policy_id)?;

		// Verify the reference block exists
		let block = self.get_block_by_hash(&block_hash).await?;
		if block.is_none() {
			return Err(TreasuryVerificationError::BlockNotFound(
				config.reference_block_hash.clone(),
			));
		}
		let block = block.unwrap();

		// Verify each UTxO
		let mut results = Vec::with_capacity(config.utxos.len());
		for utxo in &config.utxos {
			let result = self
				.verify_utxo(
					utxo,
					&config.illiquid_circulation_supply_validator_address,
					&policy_id,
					block.block_number,
				)
				.await?;
			results.push(result);
		}

		Ok(results)
	}

	/// Verify a single UTxO exists with the expected properties.
	async fn verify_utxo(
		&self,
		utxo: &TreasuryUtxo,
		expected_address: &str,
		policy_id: &[u8; POLICY_ID_LEN],
		at_block: i32,
	) -> Result<UtxoVerificationResult, TreasuryVerificationError> {
		let tx_hash = parse_tx_hash(&utxo.tx_hash)?;

		// Query the UTxO from db-sync
		let row = self
			.get_utxo_with_asset(
				&tx_hash,
				utxo.output_index,
				policy_id,
				CNIGHT_ASSET_NAME.as_bytes(),
				at_block,
			)
			.await?;

		let row = row.ok_or_else(|| TreasuryVerificationError::UtxoNotFound {
			tx_hash: utxo.tx_hash.clone(),
			output_index: utxo.output_index,
		})?;

		// Verify address
		if row.address != expected_address {
			return Err(TreasuryVerificationError::WrongAddress {
				tx_hash: utxo.tx_hash.clone(),
				output_index: utxo.output_index,
				expected: expected_address.to_string(),
				actual: row.address,
			});
		}

		// Verify amount
		let actual_amount = row.quantity as u128;
		if actual_amount != utxo.expected_amount {
			return Err(TreasuryVerificationError::AmountMismatch {
				tx_hash: utxo.tx_hash.clone(),
				output_index: utxo.output_index,
				expected: utxo.expected_amount,
				actual: actual_amount,
			});
		}

		Ok(UtxoVerificationResult {
			tx_hash: utxo.tx_hash.clone(),
			output_index: utxo.output_index,
			amount: actual_amount,
		})
	}

	/// Query db-sync for a block by its hash.
	async fn get_block_by_hash(
		&self,
		hash: &McBlockHash,
	) -> Result<Option<BlockRow>, TreasuryVerificationError> {
		let row: Option<BlockRow> = sqlx::query_as(
			r#"
SELECT 
    block_no as block_number
FROM block
WHERE hash = $1
"#,
		)
		.bind(hash.0.as_slice())
		.fetch_optional(&self.pool)
		.await?;

		Ok(row)
	}

	/// Query db-sync for a UTxO with a specific asset at a given block.
	async fn get_utxo_with_asset(
		&self,
		tx_hash: &[u8; 32],
		output_index: u32,
		policy_id: &[u8; POLICY_ID_LEN],
		asset_name: &[u8],
		at_block: i32,
	) -> Result<Option<UtxoRow>, TreasuryVerificationError> {
		// Query UTxO that:
		// 1. Was created in a transaction with the given hash
		// 2. Has the given output index
		// 3. Contains the specified asset (policy_id + asset_name)
		// 4. Was created at or before the reference block
		// 5. Was not spent before the reference block
		let row: Option<UtxoRow> = sqlx::query_as(
			r#"
SELECT
    tx_out.address as address,
    ma_tx_out.quantity::BIGINT as quantity
FROM tx
    JOIN tx_out ON tx_out.tx_id = tx.id
    JOIN block ON tx.block_id = block.id
    JOIN ma_tx_out ON ma_tx_out.tx_out_id = tx_out.id
    JOIN multi_asset ma ON ma.id = ma_tx_out.ident
WHERE tx.hash = $1
    AND tx_out.index = $2
    AND ma.policy = $3
    AND ma.name = $4
    AND block.block_no <= $5
    AND NOT EXISTS (
        SELECT 1 FROM tx_in
        JOIN tx AS spending_tx ON tx_in.tx_in_id = spending_tx.id
        JOIN block AS spending_block ON spending_tx.block_id = spending_block.id
        WHERE tx_in.tx_out_id = tx_out.tx_id
          AND tx_in.tx_out_index = tx_out.index
          AND spending_block.block_no <= $5
    )
"#,
		)
		.bind(tx_hash.as_slice())
		.bind(output_index as i16)
		.bind(policy_id.as_slice())
		.bind(asset_name)
		.bind(at_block)
		.fetch_optional(&self.pool)
		.await?;

		Ok(row)
	}
}

/// Row type for block queries.
#[derive(Debug, FromRow)]
struct BlockRow {
	block_number: i32,
}

/// Row type for UTxO queries.
#[derive(Debug, FromRow)]
struct UtxoRow {
	address: String,
	quantity: i64,
}

/// Parse a hex-encoded block hash string into McBlockHash.
fn parse_block_hash(hex_str: &str) -> Result<McBlockHash, TreasuryVerificationError> {
	let bytes = hex::decode(hex_str)
		.map_err(|e| TreasuryVerificationError::InvalidBlockHash(format!("{}: {}", hex_str, e)))?;

	if bytes.len() != BLOCK_HASH_LEN {
		return Err(TreasuryVerificationError::InvalidBlockHash(format!(
			"{}: expected {} bytes, got {}",
			hex_str,
			BLOCK_HASH_LEN,
			bytes.len()
		)));
	}

	let mut arr = [0u8; BLOCK_HASH_LEN];
	arr.copy_from_slice(&bytes);
	Ok(McBlockHash(arr))
}

/// Parse a hex-encoded policy ID string into a byte array.
fn parse_policy_id(hex_str: &str) -> Result<[u8; POLICY_ID_LEN], TreasuryVerificationError> {
	let bytes = hex::decode(hex_str)
		.map_err(|e| TreasuryVerificationError::InvalidPolicyId(format!("{}: {}", hex_str, e)))?;

	if bytes.len() != POLICY_ID_LEN {
		return Err(TreasuryVerificationError::InvalidPolicyId(format!(
			"{}: expected {} bytes, got {}",
			hex_str,
			POLICY_ID_LEN,
			bytes.len()
		)));
	}

	let mut arr = [0u8; POLICY_ID_LEN];
	arr.copy_from_slice(&bytes);
	Ok(arr)
}

/// Parse a hex-encoded transaction hash string into a byte array.
fn parse_tx_hash(hex_str: &str) -> Result<[u8; 32], TreasuryVerificationError> {
	let bytes = hex::decode(hex_str).map_err(|e| TreasuryVerificationError::InvalidTxHash {
		tx_hash: hex_str.to_string(),
		reason: e.to_string(),
	})?;

	if bytes.len() != 32 {
		return Err(TreasuryVerificationError::InvalidTxHash {
			tx_hash: hex_str.to_string(),
			reason: format!("expected 32 bytes, got {}", bytes.len()),
		});
	}

	let mut arr = [0u8; 32];
	arr.copy_from_slice(&bytes);
	Ok(arr)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_parse_block_hash_valid() {
		let hash = "0".repeat(64);
		let result = parse_block_hash(&hash);
		assert!(result.is_ok());
		assert_eq!(result.unwrap().0, [0u8; 32]);
	}

	#[test]
	fn test_parse_block_hash_invalid_length() {
		let hash = "0".repeat(62); // Too short
		let result = parse_block_hash(&hash);
		assert!(matches!(result, Err(TreasuryVerificationError::InvalidBlockHash(_))));
	}

	#[test]
	fn test_parse_block_hash_invalid_hex() {
		let hash = "g".repeat(64); // Invalid hex
		let result = parse_block_hash(&hash);
		assert!(matches!(result, Err(TreasuryVerificationError::InvalidBlockHash(_))));
	}

	#[test]
	fn test_parse_policy_id_valid() {
		let policy = "d2dbff622e509dda256fedbd31ef6e9fd98ed49ad91d5c0e07f68af1";
		let result = parse_policy_id(policy);
		assert!(result.is_ok());
	}

	#[test]
	fn test_parse_policy_id_invalid_length() {
		let policy = "d2dbff622e509dda256fedbd31ef6e9fd98ed49ad91d5c0e07f68a"; // Too short
		let result = parse_policy_id(policy);
		assert!(matches!(result, Err(TreasuryVerificationError::InvalidPolicyId(_))));
	}

	#[test]
	fn test_parse_tx_hash_valid() {
		let hash = "a".repeat(64);
		let result = parse_tx_hash(&hash);
		assert!(result.is_ok());
	}

	#[test]
	fn test_parse_tx_hash_invalid() {
		let hash = "abc"; // Too short
		let result = parse_tx_hash(&hash);
		assert!(matches!(result, Err(TreasuryVerificationError::InvalidTxHash { .. })));
	}
}
