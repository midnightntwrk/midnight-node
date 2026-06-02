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

//! Reserve Contract Genesis Generation
//!
//! This module generates the reserve genesis configuration by querying Cardano db-sync
//! for cNIGHT tokens locked at the reserve contract address.

// Re-export ReserveAddresses for use in command.rs
pub use midnight_primitives_reserve_observation::ReserveAddresses;
use midnight_primitives_reserve_observation::{ReserveConfig, ReserveUtxo};
use sidechain_domain::McBlockHash;
use sqlx::PgPool;
use std::path::Path;
use tokio::{fs::File, io::AsyncWriteExt};

#[derive(Debug, thiserror::Error)]
pub enum ReserveGenesisError {
	#[error("Database query error: {0}")]
	DatabaseError(#[from] sqlx::Error),

	#[error("Failed to serialize reserve config to JSON: {0}")]
	SerdeError(#[from] serde_json::Error),

	#[error("I/O error: {0}")]
	IoError(#[from] std::io::Error),

	#[error("Block not found: {0}")]
	BlockNotFound(String),

	#[error("Empty reserve address - cannot query empty address")]
	EmptyAddress,
}

/// Query all unspent UTxOs containing cNIGHT tokens at the reserve address
/// at a specific block hash.
async fn query_reserve_utxos(
	pool: &PgPool,
	reserve_address: &str,
	policy_id: &str,
	asset_name: &str,
	at_block: &McBlockHash,
) -> Result<Vec<ReserveUtxo>, ReserveGenesisError> {
	let block_hash_hex = hex::encode(at_block.0);

	// First verify the block exists
	let block_exists: Option<i64> = sqlx::query_scalar(
		r#"
		SELECT id FROM block WHERE hash = decode($1, 'hex')
		"#,
	)
	.bind(&block_hash_hex)
	.fetch_optional(pool)
	.await?;

	if block_exists.is_none() {
		return Err(ReserveGenesisError::BlockNotFound(block_hash_hex));
	}

	// Query all unspent UTxOs at the reserve address containing the cNIGHT asset
	// that were created at or before the reference block.
	//
	// This query finds UTxOs locked at the reserve contract by:
	// 1. Starting from tx_out (transaction outputs) at the reserve validator address
	// 2. Joining with ma_tx_out/multi_asset to filter only outputs containing
	//    the specific cNIGHT token (identified by policy_id and asset_name)
	// 3. Filtering to outputs created at or before the reference block
	// 4. Excluding spent outputs using NOT EXISTS - a UTxO is spent if there's
	//    a tx_in referencing it (by tx_id and output index) in a block at or
	//    before the reference block
	// 5. Ordering deterministically by block number, tx index, and output index
	let utxos: Vec<(String, i16, i64)> = sqlx::query_as::<_, (String, i16, i64)>(
		r#"
		SELECT
			encode(tx.hash, 'hex') as tx_hash,
			txo.index as output_index,
			ma.quantity::BIGINT as amount
		FROM tx_out txo
		JOIN tx ON tx.id = txo.tx_id
		JOIN block b ON b.id = tx.block_id
		JOIN block ref_block ON ref_block.hash = decode($1, 'hex')
		JOIN ma_tx_out ma ON ma.tx_out_id = txo.id
		JOIN multi_asset asset ON asset.id = ma.ident
		WHERE txo.address = $2
		  AND encode(asset.policy, 'hex') = $3
		  AND encode(asset.name, 'hex') = $4
		  AND b.block_no <= ref_block.block_no
		  AND NOT EXISTS (
			SELECT 1 FROM tx_in ti
			JOIN tx spend_tx ON spend_tx.id = ti.tx_in_id
			JOIN block spend_block ON spend_block.id = spend_tx.block_id
			WHERE ti.tx_out_id = txo.tx_id
			  AND ti.tx_out_index = txo.index
			  AND spend_block.block_no <= ref_block.block_no
		  )
		ORDER BY b.block_no, tx.block_index, txo.index
		"#,
	)
	.bind(&block_hash_hex)
	.bind(reserve_address)
	.bind(policy_id)
	.bind(asset_name)
	.fetch_all(pool)
	.await?;

	Ok(utxos
		.into_iter()
		.map(|(tx_hash, output_index, amount)| ReserveUtxo {
			tx_hash,
			output_index: output_index as u16,
			amount: amount as u64,
		})
		.collect())
}

/// Generate reserve genesis configuration by querying Cardano db-sync
pub async fn generate_reserve_genesis(
	addresses: ReserveAddresses,
	pool: &PgPool,
	cardano_tip: McBlockHash,
	output_path: impl AsRef<Path>,
) -> Result<(), ReserveGenesisError> {
	let output_path = output_path.as_ref();

	if addresses.reserve_validator_address.is_empty() {
		return Err(ReserveGenesisError::EmptyAddress);
	}

	log::info!(
		"Querying reserve UTxOs at address {} for block {}",
		&addresses.reserve_validator_address,
		hex::encode(cardano_tip.0)
	);

	let policy_id_hex = hex::encode(addresses.asset.policy_id.0);
	let asset_name_hex = hex::encode(&addresses.asset.asset_name);
	let utxos = query_reserve_utxos(
		pool,
		&addresses.reserve_validator_address,
		&policy_id_hex,
		&asset_name_hex,
		&cardano_tip,
	)
	.await?;

	let total_amount: u128 = utxos.iter().map(|u| u.amount as u128).sum();

	log::info!("Found {} UTxOs with total {} cNIGHT at reserve address", utxos.len(), total_amount);

	let config = ReserveConfig {
		reserve_validator_address: addresses.reserve_validator_address,
		asset: addresses.asset,
		utxos,
		total_amount,
	};

	let json = serde_json::to_string_pretty(&config)?;
	let mut file = File::create(output_path).await?;
	file.write_all(json.as_bytes()).await?;
	log::info!("Wrote reserve genesis config to {}", output_path.display());
	Ok(())
}
