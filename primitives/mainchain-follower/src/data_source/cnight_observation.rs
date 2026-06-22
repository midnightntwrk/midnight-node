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

use crate::data_source::candidates_data_source::observed_async_trait;
use crate::data_source::cnight_observation_bulk::{bulk_pull, truncate_to_tx_capacity};
use crate::data_source::metrics::{MidnightDataSourceMetrics, start_sub_query_timer};
use crate::db::{PagedQuery, get_deregistrations, get_registrations};
use crate::{
	CreateData, DeregistrationData, MidnightCNightObservationDataSource, ObservedUtxo,
	ObservedUtxoData, ObservedUtxoHeader, RegistrationData, SpendData, UtxoIndexInTx,
};
use cardano_serialization_lib::{
	BaseAddress, ConstrPlutusData, Credential, Ed25519KeyHash, PlutusData, RewardAddress,
	ScriptHash,
};
use midnight_primitives_cnight_observation::{
	CNightAddresses, CardanoPosition, CardanoRewardAddressBytes, DustPublicKeyBytes, ObservedUtxos,
};
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
	#[error("Error querying database: {0}")]
	DBQueryError(#[from] sqlx::error::Error),
	#[error("Error extracting network id from Cardano address")]
	CardanoNetworkError(String),
	#[error("Invalid value for mapping validator address")]
	MappingValidatorInvalidAddress(String),
}

/// db-sync stores native-asset quantities as a signed `BIGINT`. cNIGHT outputs are
/// always non-negative; a negative value means db-sync is corrupt or forged. Halt
/// loudly — a bare `as u128` would sign-extend (`-1` → ~1.8e19) straight into DUST
/// minting, so failing the node here is strictly safer than producing wrong consensus.
fn checked_asset_quantity(quantity: i64) -> u128 {
	u128::try_from(quantity).unwrap_or_else(|_| {
		panic!(
			"negative cNIGHT asset quantity {quantity} from db-sync — data source corrupt or forged"
		)
	})
}

#[derive(thiserror::Error, Debug)]
pub enum RegistrationDatumDecodeError {
	#[error("Cardano credential not bytes")]
	CardanoCredentialNotBytes,
	#[error("Cardano credential invalid tag")]
	CardanoCredentialInvalidTag(u64),
	#[error("Cardano credential invalid key hash")]
	CardanoCredentialInvalidKeyHash,
	#[error("Cardano credential invalid script hash")]
	CardanoCredentialInvalidScriptHash,
	#[error("Dust address not bytes")]
	DustAddressNotBytes,
	#[error("Dust address invalid length")]
	DustAddressInvalidLength(usize),
}

pub struct MidnightCNightObservationDataSourceImpl {
	pub pool: PgPool,
	pub metrics_opt: Option<MidnightDataSourceMetrics>,
}

impl MidnightCNightObservationDataSourceImpl {
	pub fn new(
		pool: PgPool,
		metrics_opt: Option<MidnightDataSourceMetrics>,
		_cache_size: u16,
	) -> Self {
		Self { pool, metrics_opt }
	}
}

observed_async_trait!(
impl MidnightCNightObservationDataSource for MidnightCNightObservationDataSourceImpl {
	async fn get_utxos_up_to_capacity(
		&self,
		config: &CNightAddresses,
		start_position: &CardanoPosition,
		current_tip: McBlockHash,
		tx_capacity: usize,
		max_utxos: usize,
	) -> Result<ObservedUtxos, Box<dyn std::error::Error + Send + Sync>> {
		// Resolve current_tip -> CardanoPosition. This must preserve the historic
		// replay semantics: query through the block's Cardano tip and only then
		// truncate by tx capacity. Clipping the SQL range earlier changes the
		// inherent payload and breaks imports of already-authored blocks.
		let _block_timer = start_sub_query_timer(&self.metrics_opt, "cnight_get_block_by_hash");
		let end: CardanoPosition = crate::db::get_block_by_hash(&self.pool, current_tip.clone())
			.await?
			.ok_or(MidnightCNightObservationDataSourceError::MissingBlockReference(current_tip))?
			.into();
		drop(_block_timer);
		let end = end.increment();

		// This single query serves exactly one inherent, so fetch only one
		// envelope's worth plus a sentinel row. Anything past `max_utxos` is
		// discarded by `truncate_to_tx_capacity` anyway; a subquery returning the
		// full `max_utxos + 1` rows flags the range row-limited
		// (`complete = false`), holding the cursor short of the tip. Each category
		// query is `ORDER BY (block_no, block_index, ...) LIMIT n`, so this prefix
		// holds the same first-`max_utxos` rows the old whole-range pull did:
		// byte-identical inherent, far less resident. The cache path still pulls
		// whole windows with `LARGE_LIMIT`; both truncate to the same envelope.
		let (utxos, complete) =
			bulk_pull(&self.pool, config, start_position, &end, max_utxos.saturating_add(1)).await?;
		Ok(truncate_to_tx_capacity(utxos, tx_capacity, max_utxos, complete, start_position, end))
	}
}
);

impl MidnightCNightObservationDataSourceImpl {
	fn decode_registration_datum(
		datum: ConstrPlutusData,
	) -> Result<(Credential, DustPublicKeyBytes), RegistrationDatumDecodeError> {
		// We use a Vec here because the `get` method on `PlutusList` can panic
		let list: Vec<PlutusData> = datum.data().into_iter().cloned().collect();

		let Some(cardano_credential) = list.first().and_then(|d| d.as_constr_plutus_data()) else {
			return Err(RegistrationDatumDecodeError::CardanoCredentialNotBytes);
		};

		let credential = match u64::from(cardano_credential.alternative()) {
			0 => cardano_credential
				.data()
				.into_iter()
				.next()
				.and_then(|d| d.as_bytes())
				.and_then(|hash_bytes| Ed25519KeyHash::from_bytes(hash_bytes).ok())
				.map(|hash| Credential::from_keyhash(&hash))
				.ok_or(RegistrationDatumDecodeError::CardanoCredentialInvalidKeyHash)?,
			1 => cardano_credential
				.data()
				.into_iter()
				.next()
				.and_then(|d| d.as_bytes())
				.and_then(|hash_bytes| ScriptHash::from_bytes(hash_bytes).ok())
				.map(|hash| Credential::from_scripthash(&hash))
				.ok_or(RegistrationDatumDecodeError::CardanoCredentialInvalidScriptHash)?,
			tag => {
				return Err(RegistrationDatumDecodeError::CardanoCredentialInvalidTag(tag));
			},
		};

		let Some(dust_address) = list.get(1).and_then(|d| d.as_bytes()) else {
			return Err(RegistrationDatumDecodeError::DustAddressNotBytes);
		};

		let dust_addr_length = dust_address.len();
		let Ok(dust_address) = <DustPublicKeyBytes>::try_from(dust_address) else {
			return Err(RegistrationDatumDecodeError::DustAddressInvalidLength(dust_addr_length));
		};

		Ok((credential, dust_address))
	}

	pub async fn get_registration_utxos(
		&self,
		cardano_network: u8,
		auth_token_ident: i64,
		address: &str,
		query: &PagedQuery<'_>,
	) -> Result<Vec<ObservedUtxo>, MidnightCNightObservationDataSourceError> {
		let rows = get_registrations(&self.pool, address, auth_token_ident, query).await?;

		let mut utxos = Vec::new();

		for row in rows {
			let header = ObservedUtxoHeader {
				tx_position: CardanoPosition {
					block_hash: McBlockHash(row.block_hash.0),
					block_number: row.block_number.0,
					block_timestamp: row.block_timestamp.and_utc().into(),
					tx_index_in_block: row.tx_index_in_block.0,
				},
				tx_hash: McTxHash(row.tx_hash.0),
				utxo_tx_hash: McTxHash(row.tx_hash.0),
				utxo_index: UtxoIndexInTx(row.utxo_index.0),
			};

			let Some(constr) = row.full_datum.0.as_constr_plutus_data() else {
				log::error!("Plutus data for mapping validator not Constr ({header:?})");
				continue;
			};
			let (credential, dust_public_key) = match Self::decode_registration_datum(constr) {
				Ok(pair) => pair,
				Err(e) => {
					log::error!("Failed to decode registration datum: {e:?} ({header:?})");
					continue;
				},
			};

			let reward_address = RewardAddress::new(cardano_network, &credential);
			// Unwrap here is OK - we know the reward_address is always 29 bytes
			let cardano_address = reward_address.to_address().to_bytes().try_into().unwrap();

			let utxo = ObservedUtxo {
				header,
				data: ObservedUtxoData::Registration(RegistrationData {
					cardano_reward_address: CardanoRewardAddressBytes(cardano_address),
					dust_public_key,
				}),
			};

			utxos.push(utxo);
		}

		Ok(utxos)
	}

	pub async fn get_deregistration_utxos(
		&self,
		cardano_network: u8,
		address: &str,
		query: &PagedQuery<'_>,
	) -> Result<Vec<ObservedUtxo>, MidnightCNightObservationDataSourceError> {
		let rows = get_deregistrations(&self.pool, address, query).await?;

		let mut utxos = Vec::new();

		for row in rows {
			let header = ObservedUtxoHeader {
				tx_position: CardanoPosition {
					block_hash: McBlockHash(row.block_hash.0),
					block_number: row.block_number.0,
					block_timestamp: row.block_timestamp.and_utc().into(),
					tx_index_in_block: row.tx_index_in_block.0,
				},
				tx_hash: McTxHash(row.tx_hash.0),
				utxo_tx_hash: McTxHash(row.utxo_tx_hash.0),
				utxo_index: UtxoIndexInTx(row.utxo_index.0),
			};

			let Some(constr) = row.full_datum.0.as_constr_plutus_data() else {
				log::error!("Plutus data for mapping validator not Constr ({header:?})");
				continue;
			};
			let (credential, dust_public_key) = match Self::decode_registration_datum(constr) {
				Ok(pair) => pair,
				Err(e) => {
					log::error!("Failed to decode registration datum: {e:?} ({header:?})");
					continue;
				},
			};

			let reward_address = RewardAddress::new(cardano_network, &credential);
			// Unwrap here is OK - we know the reward_address is always 29 bytes
			let cardano_address = reward_address.to_address().to_bytes().try_into().unwrap();

			let utxo = ObservedUtxo {
				header,
				data: ObservedUtxoData::Deregistration(DeregistrationData {
					cardano_reward_address: CardanoRewardAddressBytes(cardano_address),
					dust_public_key,
				}),
			};

			utxos.push(utxo);
		}

		Ok(utxos)
	}

	pub async fn get_asset_create_utxos(
		&self,
		cardano_network: u8,
		ident: i64,
		query: &PagedQuery<'_>,
	) -> Result<Vec<ObservedUtxo>, MidnightCNightObservationDataSourceError> {
		let rows = crate::db::get_asset_creates(&self.pool, ident, query).await?;

		let mut utxos = Vec::new();

		for row in rows {
			let header = ObservedUtxoHeader {
				tx_position: CardanoPosition {
					block_hash: McBlockHash(row.block_hash.0),
					block_number: row.block_number.0,
					block_timestamp: row.block_timestamp.and_utc().into(),
					tx_index_in_block: row.tx_index_in_block.0,
				},
				tx_hash: McTxHash(row.tx_hash.0),
				utxo_tx_hash: McTxHash(row.tx_hash.0),
				utxo_index: UtxoIndexInTx(row.utxo_index.0),
			};

			let Some(cardano_address) =
				cardano_serialization_lib::Address::from_bech32(&row.holder_address).ok()
			else {
				log::debug!(
					"Cardano address {:?} not valid bech32 cardano address",
					&row.holder_address
				);
				continue;
			};

			let Some(base_address) = BaseAddress::from_address(&cardano_address) else {
				// Non-base addresses (enterprise, pointer, reward) carry no stake
				// credential so they can't be mapped to a reward address — skip silently.
				continue;
			};
			let reward_address = RewardAddress::new(cardano_network, &base_address.stake_cred());
			let owner = reward_address.to_address().to_bytes().try_into().unwrap();

			let utxo = ObservedUtxo {
				header,
				data: ObservedUtxoData::AssetCreate(CreateData {
					value: checked_asset_quantity(row.quantity),
					owner,
					utxo_tx_hash: McTxHash(row.tx_hash.0),
					utxo_tx_index: row.utxo_index.0,
				}),
			};

			utxos.push(utxo);
		}

		Ok(utxos)
	}

	pub async fn get_asset_spend_utxos(
		&self,
		cardano_network: u8,
		ident: i64,
		query: &PagedQuery<'_>,
	) -> Result<Vec<ObservedUtxo>, MidnightCNightObservationDataSourceError> {
		let rows = crate::db::get_asset_spends(&self.pool, ident, query).await?;

		let mut utxos = Vec::new();

		for row in rows {
			let header = ObservedUtxoHeader {
				tx_position: CardanoPosition {
					block_hash: McBlockHash(row.block_hash.0),
					block_number: row.block_number.0,
					block_timestamp: row.block_timestamp.and_utc().into(),
					tx_index_in_block: row.tx_index_in_block.0,
				},
				tx_hash: McTxHash(row.spending_tx_hash.0),
				utxo_tx_hash: McTxHash(row.utxo_tx_hash.0),
				utxo_index: UtxoIndexInTx(row.utxo_index.0),
			};

			let Some(cardano_address) =
				cardano_serialization_lib::Address::from_bech32(&row.holder_address).ok()
			else {
				log::debug!(
					"Cardano address {:?} not valid bech32 cardano address",
					row.holder_address
				);
				continue;
			};

			let Some(base_address) = BaseAddress::from_address(&cardano_address) else {
				// Non-base addresses (enterprise, pointer, reward) carry no stake
				// credential so they can't be mapped to a reward address — skip silently.
				continue;
			};
			let reward_address = RewardAddress::new(cardano_network, &base_address.stake_cred());
			let owner = reward_address.to_address().to_bytes().try_into().unwrap();

			let utxo = ObservedUtxo {
				header,
				data: ObservedUtxoData::AssetSpend(SpendData {
					value: checked_asset_quantity(row.quantity),
					owner,
					utxo_tx_hash: McTxHash(row.utxo_tx_hash.0),
					utxo_tx_index: row.utxo_index.0,
					spending_tx_hash: McTxHash(row.spending_tx_hash.0),
				}),
			};

			utxos.push(utxo);
		}

		Ok(utxos)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn checked_asset_quantity_accepts_non_negative() {
		assert_eq!(checked_asset_quantity(0), 0);
		assert_eq!(checked_asset_quantity(1), 1);
		assert_eq!(checked_asset_quantity(i64::MAX), i64::MAX as u128);
	}

	#[test]
	#[should_panic(expected = "negative cNIGHT asset quantity")]
	fn checked_asset_quantity_panics_on_negative() {
		// A bare `as u128` would sign-extend -1 to ~1.8e19 and mint it as DUST.
		let _ = checked_asset_quantity(-1);
	}
}
