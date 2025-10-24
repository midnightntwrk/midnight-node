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

use crate::db::{
	get_deregistrations, get_redemption_creates, get_redemption_spends, get_registrations,
};
use crate::{
	CreateData, DeregistrationData, MidnightCNightObservationDataSource, ObservedUtxo,
	ObservedUtxoData, ObservedUtxoHeader, RedemptionCreateData, RedemptionSpendData,
	RegistrationData, SpendData, UtxoIndexInTx,
};
use derive_new::new;
use midnight_primitives_cnight_observation::{CNightAddresses, CardanoPosition, ObservedUtxos};
use partner_chains_db_sync_data_sources::McFollowerMetrics;
use sidechain_domain::{McBlockHash, McBlockNumber, McTxHash, McTxIndexInBlock, TX_HASH_SIZE};
pub use sqlx::PgPool;
use std::fmt::Debug;

#[derive(
	Debug,
	Copy,
	Clone,
	PartialEq,
	PartialOrd,
	parity_scale_codec::Encode,
	parity_scale_codec::Decode,
	scale_info::TypeInfo,
)]
pub struct TxHash(pub [u8; TX_HASH_SIZE]);

#[derive(
	Debug,
	Clone,
	PartialEq,
	parity_scale_codec::Encode,
	parity_scale_codec::Decode,
	scale_info::TypeInfo,
)]
pub struct TxPosition {
	pub block_hash: McBlockHash,
	pub block_number: McBlockNumber,
	pub block_index: McTxIndexInBlock,
}

#[derive(thiserror::Error, Debug)]
pub enum MidnightCNightObservationDataSourceError {
	#[error("missing reference for block hash `{0}` in db-sync")]
	MissingBlockReference(McBlockHash),
	#[error("Error querying database")]
	DBQueryError(#[from] sqlx::error::Error),
}

#[derive(new)]
pub struct MidnightCNightObservationDataSourceImpl {
	pub pool: PgPool,
	pub metrics_opt: Option<McFollowerMetrics>,
	#[allow(dead_code)]
	cache_size: u16,
}

// If we need better logging here, we could use use db_sync_follower::observed_async_trait
// But perhaps there are better options for tracing
#[async_trait::async_trait]
impl MidnightCNightObservationDataSource for MidnightCNightObservationDataSourceImpl {
	async fn get_utxos_up_to_capacity(
		&self,
		config: &CNightAddresses,
		start_position: CardanoPosition,
		current_tip: McBlockHash,
		tx_capacity: usize,
	) -> Result<ObservedUtxos, Box<dyn std::error::Error + Send + Sync>> {
		let cnight_asset_name = config.cnight_asset_name.as_bytes();

		// Get end position from cardano block hash
		let end: CardanoPosition = crate::db::get_block_by_hash(&self.pool, current_tip.clone())
			.await?
			.ok_or(MidnightCNightObservationDataSourceError::MissingBlockReference(current_tip))?
			.into();
		// Increment the end position to tx_index + 1 of the current mainchain position
		let end = end.increment();

		// The "capacity" argument is capacity in terms of TRANSACTIONS,
		// but the various sql queries below want a capacity in terms of UTXOs.
		// Use a generous overestimate of how many UTXOs each TX _may_ have.
		let utxo_capacity = tx_capacity * 64;

		// Call db methods to get UTXOs (offset + limit) until we reach our capacity
		// TODO: (possibly) Replace this with grabbing from a queue that's filled async by an offchain thread
		// ^ We may not have to do the above if the queries are fast enough
		let mut utxos = [
			self.get_registration_utxos(
				&config.mapping_validator_address,
				start_position,
				end,
				utxo_capacity,
				0,
			)
			.await?,
			self.get_deregistration_utxos(
				&config.mapping_validator_address,
				start_position,
				end,
				utxo_capacity,
				0,
			)
			.await?,
			self.get_asset_create_utxos(
				config.cnight_policy_id,
				cnight_asset_name,
				start_position,
				end,
				utxo_capacity,
				0,
			)
			.await?,
			self.get_asset_spend_utxos(
				config.cnight_policy_id,
				cnight_asset_name,
				start_position,
				end,
				utxo_capacity,
				0,
			)
			.await?,
			self.get_redemption_create_utxos(
				&config.redemption_validator_address,
				config.cnight_policy_id,
				cnight_asset_name,
				start_position,
				end,
				utxo_capacity,
				0,
			)
			.await?,
			self.get_redemption_spend_utxos(
				&config.redemption_validator_address,
				config.cnight_policy_id,
				cnight_asset_name,
				start_position,
				end,
				utxo_capacity,
				0,
			)
			.await?,
		]
		.concat();

		utxos.sort();

		// Truncate UTXOs but include full transactions
		let mut truncated_utxos = Vec::with_capacity(utxo_capacity);
		let mut num_txs = 0;
		let mut cur_tx: Option<CardanoPosition> = None;
		for utxo in utxos {
			if cur_tx.is_none_or(|tx| tx < utxo.header.tx_position) {
				num_txs += 1;
				cur_tx = Some(utxo.header.tx_position);
			}
			if num_txs == tx_capacity {
				break;
			}
			truncated_utxos.push(utxo);
		}

		if num_txs < tx_capacity {
			// We couldn't find enough UTXOs in the range, which means we're up-to-date with the
			// current_tip
			Ok(ObservedUtxos { start: start_position, end, utxos: truncated_utxos })
		} else {
			Ok(ObservedUtxos {
				start: start_position,
				end: truncated_utxos
					.last()
					.map_or(start_position, |u| u.header.tx_position)
					.increment(),
				utxos: truncated_utxos,
			})
		}
	}
}

impl MidnightCNightObservationDataSourceImpl {
	#[allow(clippy::too_many_arguments)]
	async fn get_redemption_create_utxos(
		&self,
		address: &str,
		policy_id: [u8; 28],
		asset_name: &[u8],
		start: CardanoPosition,
		end: CardanoPosition,
		limit: usize,
		offset: usize,
	) -> Result<Vec<ObservedUtxo>, Box<dyn std::error::Error + Send + Sync>> {
		let rows = get_redemption_creates(
			&self.pool, address, policy_id, asset_name, start, end, limit, offset,
		)
		.await
		.map_err(|e| format!("Failed to fetch data: {e}"))?;

		let mut utxos = Vec::new();

		for row in rows {
			let header = ObservedUtxoHeader {
				tx_position: CardanoPosition {
					block_hash: row.block_hash.0,
					block_number: row.block_number.0,
					block_timestamp: row.block_timestamp.and_utc().into(),
					tx_index_in_block: row.tx_index_in_block.0,
				},
				tx_hash: McTxHash(row.tx_hash.0),
				utxo_tx_hash: McTxHash(row.tx_hash.0),
				utxo_index: UtxoIndexInTx(row.utxo_index.0),
			};

			let Some(constr) = row.full_datum.0.as_constr_plutus_data() else {
				log::error!(
					"INTERNAL ERROR: Plutus data for mapping validator not Constr ({header:?})"
				);
				continue;
			};
			let list = constr.data();

			let Some(owner_bytes) = list.get(0).as_bytes() else {
				log::error!("Owner Cardano address not bytes ({header:?})");
				continue;
			};

			let Some(owner) = cardano_serialization_lib::Address::from_bytes(owner_bytes.clone())
				.map(|addr| addr.to_bytes())
				.ok()
			else {
				log::error!(
					"Cardano address {owner_bytes:?} not valid cardano address ({header:?})"
				);
				continue;
			};

			let utxo = ObservedUtxo {
				header,
				data: ObservedUtxoData::RedemptionCreate(RedemptionCreateData {
					owner,
					value: row.quantity as u128,
					utxo_tx_hash: row.tx_hash.0,
					utxo_tx_index: row.utxo_index.0,
				}),
			};

			utxos.push(utxo);
		}

		Ok(utxos)
	}

	#[allow(clippy::too_many_arguments)]
	async fn get_redemption_spend_utxos(
		&self,
		address: &str,
		policy_id: [u8; 28],
		asset_name: &[u8],
		start: CardanoPosition,
		end: CardanoPosition,
		limit: usize,
		offset: usize,
	) -> Result<Vec<ObservedUtxo>, Box<dyn std::error::Error + Send + Sync>> {
		let rows = get_redemption_spends(
			&self.pool, address, policy_id, asset_name, start, end, limit, offset,
		)
		.await
		.map_err(|e| format!("Failed to fetch data: {e}"))?;

		let mut utxos = Vec::new();

		for row in rows {
			let header = ObservedUtxoHeader {
				tx_position: CardanoPosition {
					block_hash: row.block_hash.0,
					block_number: row.block_number.0,
					block_timestamp: row.block_timestamp.and_utc().into(),
					tx_index_in_block: row.tx_index_in_block.0,
				},
				tx_hash: McTxHash(row.tx_hash.0),
				utxo_tx_hash: McTxHash(row.utxo_tx_hash.0),
				utxo_index: UtxoIndexInTx(row.utxo_index.0),
			};

			let Some(constr) = row.full_datum.0.as_constr_plutus_data() else {
				log::error!(
					"INTERNAL ERROR: Plutus data for mapping validator not Constr ({header:?})"
				);
				continue;
			};
			let list = constr.data();

			let Some(owner_bytes) = list.get(0).as_bytes() else {
				log::error!("Owner Cardano address not bytes ({header:?})");
				continue;
			};

			let Some(owner) = cardano_serialization_lib::Address::from_bytes(owner_bytes.clone())
				.map(|addr| addr.to_bytes())
				.ok()
			else {
				log::error!(
					"Cardano address {owner_bytes:?} not valid cardano address ({header:?})"
				);
				continue;
			};

			let utxo = ObservedUtxo {
				header,
				data: ObservedUtxoData::RedemptionSpend(RedemptionSpendData {
					value: row.quantity as u128,
					owner,
					utxo_tx_hash: row.utxo_tx_hash.0,
					utxo_tx_index: row.utxo_index.0,
					spending_tx_hash: row.tx_hash.0,
				}),
			};

			utxos.push(utxo);
		}

		Ok(utxos)
	}

	async fn get_registration_utxos(
		&self,
		address: &str,
		start: CardanoPosition,
		end: CardanoPosition,
		limit: usize,
		offset: usize,
	) -> Result<Vec<ObservedUtxo>, MidnightCNightObservationDataSourceError> {
		let rows = get_registrations(&self.pool, address, start, end, limit, offset).await?;

		let mut utxos = Vec::new();

		for row in rows {
			let header = ObservedUtxoHeader {
				tx_position: CardanoPosition {
					block_hash: row.block_hash.0,
					block_number: row.block_number.0,
					block_timestamp: row.block_timestamp.and_utc().into(),
					tx_index_in_block: row.tx_index_in_block.0,
				},
				tx_hash: McTxHash(row.tx_hash.0),
				utxo_tx_hash: McTxHash(row.tx_hash.0),
				utxo_index: UtxoIndexInTx(row.utxo_index.0),
			};

			let Some(constr) = row.full_datum.0.as_constr_plutus_data() else {
				log::error!(
					"INTERNAL ERROR: Plutus data for mapping validator not Constr ({header:?})"
				);
				continue;
			};
			let list = constr.data();

			let Some(cardano_address_bytes) = list.get(0).as_bytes() else {
				log::error!("Cardano address not bytes ({header:?})");
				continue;
			};

			let Some(dust_address) = list.get(1).as_bytes() else {
				log::error!("Midnight address not bytes ({header:?})");
				continue;
			};

			let Some(cardano_address) =
				cardano_serialization_lib::Address::from_bytes(cardano_address_bytes.clone())
					.map(|addr| addr.to_bytes())
					.ok()
			else {
				log::error!(
					"Cardano address {cardano_address_bytes:?} not valid cardano address ({header:?})"
				);
				continue;
			};

			let utxo = ObservedUtxo {
				header,
				data: ObservedUtxoData::Registration(RegistrationData {
					cardano_address,
					dust_address,
				}),
			};

			utxos.push(utxo);
		}

		Ok(utxos)
	}

	async fn get_deregistration_utxos(
		&self,
		address: &str,
		start: CardanoPosition,
		end: CardanoPosition,
		limit: usize,
		offset: usize,
	) -> Result<Vec<ObservedUtxo>, MidnightCNightObservationDataSourceError> {
		let rows = get_deregistrations(&self.pool, address, start, end, limit, offset).await?;

		let mut utxos = Vec::new();

		for row in rows {
			let header = ObservedUtxoHeader {
				tx_position: CardanoPosition {
					block_hash: row.block_hash.0,
					block_number: row.block_number.0,
					block_timestamp: row.block_timestamp.and_utc().into(),
					tx_index_in_block: row.tx_index_in_block.0,
				},
				tx_hash: McTxHash(row.tx_hash.0),
				utxo_tx_hash: McTxHash(row.utxo_tx_hash.0),
				utxo_index: UtxoIndexInTx(row.utxo_index.0),
			};

			let Some(constr) = row.full_datum.0.as_constr_plutus_data() else {
				log::error!(
					"INTERNAL ERROR: Plutus data for mapping validator not Constr ({header:?})"
				);
				continue;
			};
			let list = constr.data();

			let Some(cardano_address_bytes) = list.get(0).as_bytes() else {
				log::error!("Cardano address not bytes ({header:?})");
				continue;
			};
			let Some(dust_address) = list.get(1).as_bytes() else {
				log::error!("Midnight address not bytes ({header:?})");
				continue;
			};

			let Some(cardano_address) =
				cardano_serialization_lib::Address::from_bytes(cardano_address_bytes.clone())
					.map(|addr| addr.to_bytes())
					.ok()
			else {
				log::error!(
					"Cardano address {cardano_address_bytes:?} not valid cardano address ({header:?})"
				);
				continue;
			};

			let utxo = ObservedUtxo {
				header,
				data: ObservedUtxoData::Deregistration(DeregistrationData {
					cardano_address,
					dust_address,
				}),
			};

			utxos.push(utxo);
		}

		Ok(utxos)
	}

	async fn get_asset_create_utxos(
		&self,
		policy_id: [u8; 28],
		asset_name: &[u8],
		start: CardanoPosition,
		end: CardanoPosition,
		limit: usize,
		offset: usize,
	) -> Result<Vec<ObservedUtxo>, MidnightCNightObservationDataSourceError> {
		let rows = crate::db::get_asset_creates(
			&self.pool, policy_id, asset_name, start, end, limit, offset,
		)
		.await?;

		let mut utxos = Vec::new();

		for row in rows {
			let header = ObservedUtxoHeader {
				tx_position: CardanoPosition {
					block_hash: row.block_hash.0,
					block_number: row.block_number.0,
					block_timestamp: row.block_timestamp.and_utc().into(),
					tx_index_in_block: row.tx_index_in_block.0,
				},
				tx_hash: McTxHash(row.tx_hash.0),
				utxo_tx_hash: McTxHash(row.tx_hash.0),
				utxo_index: UtxoIndexInTx(row.utxo_index.0),
			};

			let Some(cardano_address) =
				cardano_serialization_lib::Address::from_bech32(&row.holder_address)
					.map(|addr| addr.to_bytes())
					.ok()
			else {
				log::error!(
					"Cardano address {:?} not valid bech32 cardano address",
					row.holder_address
				);
				continue;
			};
			let utxo = ObservedUtxo {
				header,
				data: ObservedUtxoData::AssetCreate(CreateData {
					value: row.quantity as u128,
					owner: cardano_address,
					utxo_tx_hash: row.tx_hash.0,
					utxo_tx_index: row.utxo_index.0,
				}),
			};

			utxos.push(utxo);
		}

		Ok(utxos)
	}

	async fn get_asset_spend_utxos(
		&self,
		policy_id: [u8; 28],
		asset_name: &[u8],
		start: CardanoPosition,
		end: CardanoPosition,
		limit: usize,
		offset: usize,
	) -> Result<Vec<ObservedUtxo>, MidnightCNightObservationDataSourceError> {
		let rows = crate::db::get_asset_spends(
			&self.pool, policy_id, asset_name, start, end, limit, offset,
		)
		.await?;

		let mut utxos = Vec::new();

		for row in rows {
			let header = ObservedUtxoHeader {
				tx_position: CardanoPosition {
					block_hash: row.block_hash.0,
					block_number: row.block_number.0,
					block_timestamp: row.block_timestamp.and_utc().into(),
					tx_index_in_block: row.tx_index_in_block.0,
				},
				tx_hash: McTxHash(row.spending_tx_hash.0),
				utxo_tx_hash: McTxHash(row.utxo_tx_hash.0),
				utxo_index: UtxoIndexInTx(row.utxo_index.0),
			};

			let Some(cardano_address) =
				cardano_serialization_lib::Address::from_bech32(&row.holder_address)
					.map(|addr| addr.to_bytes())
					.ok()
			else {
				log::error!(
					"Cardano address {:?} not valid bech32 cardano address",
					row.holder_address
				);
				continue;
			};

			let utxo = ObservedUtxo {
				header,
				data: ObservedUtxoData::AssetSpend(SpendData {
					value: row.quantity as u128,
					owner: cardano_address,
					utxo_tx_hash: row.utxo_tx_hash.0,
					utxo_tx_index: row.utxo_index.0,
					spending_tx_hash: row.spending_tx_hash.0,
				}),
			};

			utxos.push(utxo);
		}

		Ok(utxos)
	}
}
