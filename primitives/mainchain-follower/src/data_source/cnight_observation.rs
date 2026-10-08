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
use crate::data_source::metrics::{MidnightDataSourceMetrics, start_sub_query_timer};
use crate::db::{MultiAssetCache, PagedQuery, get_deregistrations, get_registrations};
use crate::{
	CreateData, DeregistrationData, MidnightCNightObservationDataSource, ObservationBound,
	ObservedUtxo, ObservedUtxoData, ObservedUtxoHeader, RegistrationData, SpendData, UtxoIndexInTx,
};
use cardano_serialization_lib::{
	Address, BaseAddress, ConstrPlutusData, Credential, Ed25519KeyHash, EnterpriseAddress,
	PlutusData, RewardAddress, ScriptHash,
};
use midnight_primitives_cnight_observation::{
	CNightAddresses, CardanoPosition, CardanoRewardAddressBytes, DustPublicKeyBytes, ObservedUtxos,
	sort_observed_utxos,
};
use sidechain_domain::{McBlockHash, McTxHash};
pub use sqlx::PgPool;
use std::fmt::Debug;

/// Row cap per UTXO kind for an [`ObservationBound::Count`] fetch (API version 1
/// runtimes and genesis only), in UTXOs per Cardano transaction. Fixed: nodes without
/// the cost model check our inherent against their own, fetched with this cap.
const UTXOS_PER_TX_OVERESTIMATE: usize = 64;

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
	#[allow(dead_code)]
	cache_size: u16,
	multi_asset_cache: MultiAssetCache,
}

impl MidnightCNightObservationDataSourceImpl {
	pub fn new(
		pool: PgPool,
		metrics_opt: Option<MidnightDataSourceMetrics>,
		cache_size: u16,
	) -> Self {
		let multi_asset_cache = MultiAssetCache::new(pool.clone());
		Self { pool, metrics_opt, cache_size, multi_asset_cache }
	}
}

observed_async_trait!(
impl MidnightCNightObservationDataSource for MidnightCNightObservationDataSourceImpl {
	async fn get_utxos_up_to_capacity(
		&self,
		config: &CNightAddresses,
		start_position: &CardanoPosition,
		current_tip: McBlockHash,
		bound: &ObservationBound,
		spec_version: u32,
	) -> Result<ObservedUtxos, Box<dyn std::error::Error + Send + Sync>> {
		let cnight_asset_name = config.cnight_asset_name.as_bytes();

		let mapping_validator_address = Address::from_bech32(&config.mapping_validator_address)
			.map_err(|e| {
				MidnightCNightObservationDataSourceError::MappingValidatorInvalidAddress(
					e.to_string(),
				)
			})?;

		let cardano_network = mapping_validator_address.network_id().map_err(|_| {
			MidnightCNightObservationDataSourceError::CardanoNetworkError(
				config.mapping_validator_address.clone(),
			)
		})?;

		let mapping_validator_policy_id =
			EnterpriseAddress::from_address(&mapping_validator_address)
				.ok_or(MidnightCNightObservationDataSourceError::MappingValidatorInvalidAddress(
					"Not EnterpriseAddress".to_string(),
				))?
				.payment_cred()
				.to_scripthash()
				.ok_or(MidnightCNightObservationDataSourceError::MappingValidatorInvalidAddress(
					"MappingValidator address does not contain a script hash".to_string(),
				))?;

		// Resolve multi_asset.id for auth token and cNight token (cached after first call)
		let auth_token_ident = self
			.multi_asset_cache
			.resolve_ident(
				&mapping_validator_policy_id.to_bytes(),
				config.auth_token_asset_name.as_bytes(),
			)
			.await?;

		let cnight_ident = self
			.multi_asset_cache
			.resolve_ident(&config.cnight_policy_id, cnight_asset_name)
			.await?;

		// Get end position from cardano block hash
		let _block_timer = start_sub_query_timer(&self.metrics_opt, "cnight_get_block_by_hash");
		let end: CardanoPosition = crate::db::get_block_by_hash(&self.pool, current_tip.clone())
			.await?
			.ok_or(MidnightCNightObservationDataSourceError::MissingBlockReference(current_tip))?
			.into();
		drop(_block_timer);

		let (low_bounds, high_bounds) = tokio::try_join!(
			async {
				let _sq_timer = start_sub_query_timer(&self.metrics_opt, "cnight_get_low_bounds");
				crate::db::get_low_bounds(&self.pool, start_position.block_number.into())
					.await
					.map_err(Into::<Box<dyn std::error::Error + Send + Sync>>::into)
			},
			async {
				let _sq_timer = start_sub_query_timer(&self.metrics_opt, "cnight_get_high_bounds");
				crate::db::get_high_bounds(&self.pool, end.block_number.into())
					.await
					.map_err(Into::<Box<dyn std::error::Error + Send + Sync>>::into)
			},
			)?;
		let low_bounds = low_bounds.expect("Start position contains block hash that exists in database");
		let high_bounds = high_bounds.expect("End position contains block hash that exists in database");
		// Increment the end position to tx_index + 1 of the current mainchain position
		let end = end.increment();
		log::debug!("Bounds:\n{:?}\n{:?}\nfor positions:\n{:?}\n{:?}", low_bounds, high_bounds, start_position, end);

		let utxo_capacity = fetch_row_cap(bound);

		// Call db methods to get UTXOs (offset + limit) until we reach our capacity
		// TODO: (possibly) Replace this with grabbing from a queue that's filled async by an offchain thread
		// ^ We may not have to do the above if the queries are fast enough

		let paged = PagedQuery {
			start: start_position,
			end: &end,
			limit: utxo_capacity,
			offset: 0,
			low_bound: low_bounds,
			high_bound: high_bounds,
		};

		let (registration_utxos, deregistration_utxos, asset_create_utxos, asset_spend_utxos) = tokio::try_join!(
			async {
				let _sq_timer = start_sub_query_timer(&self.metrics_opt, "cnight_get_registrations");
				match auth_token_ident {
					Some(ident) => self.get_registration_utxos(
						cardano_network,
						ident,
						&config.mapping_validator_address,
						&paged,
					)
					.await
					.map_err(Into::<Box<dyn std::error::Error + Send + Sync>>::into),
					None => Ok(vec![]),
				}
			},
			async {
				let _sq_timer = start_sub_query_timer(&self.metrics_opt, "cnight_get_deregistrations");
				self.get_deregistration_utxos(
					cardano_network,
					&config.mapping_validator_address,
					&paged,
				)
				.await
				.map_err(Into::<Box<dyn std::error::Error + Send + Sync>>::into)
			},
			async {
				let _sq_timer = start_sub_query_timer(&self.metrics_opt, "cnight_get_asset_creates");
				match cnight_ident {
					Some(ident) => self.get_asset_create_utxos(
						cardano_network,
						ident,
						&paged,
					)
					.await
					.map_err(Into::<Box<dyn std::error::Error + Send + Sync>>::into),
					None => Ok(vec![]),
				}
			},
			async {
				let _sq_timer = start_sub_query_timer(&self.metrics_opt, "cnight_get_asset_spends");
				match cnight_ident {
					Some(ident) => self.get_asset_spend_utxos(
						cardano_network,
						ident,
						&paged,
					)
					.await
					.map_err(Into::<Box<dyn std::error::Error + Send + Sync>>::into),
					None => Ok(vec![]),
				}
			}
		)?;

		let mut utxos = Vec::with_capacity(
			registration_utxos.len()
				+ deregistration_utxos.len()
				+ asset_create_utxos.len()
				+ asset_spend_utxos.len(),
		);
		utxos.extend(registration_utxos);
		utxos.extend(deregistration_utxos);
		utxos.extend(asset_create_utxos);
		utxos.extend(asset_spend_utxos);

		sort_observed_utxos(&mut utxos, spec_version);

		let (utxos, cut) = admit_prefix(utxos, bound);
		match &cut {
			Cut::Cost(resume_at) if utxos.is_empty() => log::error!(
				"cNIGHT observation: the Cardano transaction at {resume_at} alone prices above the block limit; observation cannot advance past it"
			),
			Cut::Cost(resume_at) => log::warn!(
				"cNIGHT observation falling behind the Cardano tip: cNIGHT activity exceeds one block's ledger budget, admitted {} UTXOs, resuming at {resume_at}",
				utxos.len()
			),
			_ => {},
		}
		let end = cut.end(&utxos, start_position, end);
		Ok(ObservedUtxos { start: start_position.clone(), end, utxos })
	}
}
);

/// Where [`admit_prefix`] stopped.
#[derive(Debug, Clone, PartialEq)]
enum Cut {
	/// Every fetched UTXO was admitted; the batch reaches the Cardano tip.
	NotSaturated,
	/// The Cardano transaction count of an [`ObservationBound::Count`] was reached.
	TxCapacity,
	/// The pessimistic ledger cost bound was hit. The transaction at this position
	/// is the first one refused; it is re-delivered whole in the next block.
	Cost(CardanoPosition),
}

impl Cut {
	/// The inherent's `next_cardano_position` for this cut, given the admitted
	/// UTXOs, the batch start and the position just past the current Cardano tip.
	fn end(
		&self,
		admitted: &[ObservedUtxo],
		start: &CardanoPosition,
		tip: CardanoPosition,
	) -> CardanoPosition {
		match self {
			Cut::NotSaturated => tip,
			Cut::TxCapacity => admitted
				.last()
				.map_or(start.clone(), |u| u.header.tx_position.clone())
				.increment(),
			Cut::Cost(resume_at) => resume_at.clone(),
		}
	}
}

/// Row cap per UTXO kind for one fetch. Under a cost model no UTXO costs less than
/// `utxo_gas`, so more rows than the whole block affords can never be admitted and
/// the cap never truncates ahead of the cost cut.
fn fetch_row_cap(bound: &ObservationBound) -> usize {
	match bound {
		ObservationBound::Count(tx_capacity) => tx_capacity * UTXOS_PER_TX_OVERESTIMATE,
		ObservationBound::Cost(model) => (model.full_gas / model.utxo_gas) as usize + 1,
	}
}

/// Admits a prefix of sorted `utxos` at Cardano-transaction granularity.
///
/// [`ObservationBound::Count`] admits fewer than that many transactions.
/// [`ObservationBound::Cost`] admits transactions while the batch's pessimistic price,
/// counting every `AssetCreate` and `AssetSpend` as a ledger event, fits `budget_gas`;
/// a lone transaction over budget is admitted alone up to `full_gas`.
fn admit_prefix(
	mut utxos: Vec<ObservedUtxo>,
	bound: &ObservationBound,
) -> (Vec<ObservedUtxo>, Cut) {
	let mut num_txs = 0;
	let mut admitted = 0;
	let mut gas = 0u64;
	let mut cost_cut = None;
	// A strictly greater position starts a new transaction, as in the sort order.
	for group in utxos.chunk_by(|a, b| a.header.tx_position >= b.header.tx_position) {
		match bound {
			ObservationBound::Count(tx_capacity) => {
				num_txs += 1;
				if num_txs == *tx_capacity {
					break;
				}
			},
			ObservationBound::Cost(model) => {
				let group_gas = group.iter().fold(0u64, |gas, utxo| {
					let ledger_gas = match utxo.data {
						ObservedUtxoData::AssetCreate(_) => model.create_gas,
						ObservedUtxoData::AssetSpend(_) => model.destroy_gas,
						_ => 0,
					};
					gas.saturating_add(model.utxo_gas).saturating_add(ledger_gas)
				});
				let bound = gas.saturating_add(group_gas);
				if bound > model.budget_gas && (admitted > 0 || bound > model.full_gas) {
					cost_cut = Some(Cut::Cost(group[0].header.tx_position.clone()));
					break;
				}
				gas = bound;
			},
		}
		admitted += group.len();
	}
	utxos.truncate(admitted);
	let cut = cost_cut.unwrap_or(match bound {
		ObservationBound::Count(tx_capacity) if num_txs >= *tx_capacity => Cut::TxCapacity,
		_ => Cut::NotSaturated,
	});
	(utxos, cut)
}

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

	async fn get_registration_utxos(
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

			let Some(constr) = row.full_datum.and_then(|d| d.0.as_constr_plutus_data()) else {
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

	async fn get_deregistration_utxos(
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

			let Some(constr) = row.full_datum.and_then(|d| d.0.as_constr_plutus_data()) else {
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

	async fn get_asset_create_utxos(
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
				data: ObservedUtxoData::AssetCreate(CreateData {
					value: row.quantity as u128,
					owner,
					utxo_tx_hash: McTxHash(row.tx_hash.0),
					utxo_tx_index: row.utxo_index.0,
				}),
			};

			utxos.push(utxo);
		}

		Ok(utxos)
	}

	async fn get_asset_spend_utxos(
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
					value: row.quantity as u128,
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
	use midnight_primitives_cnight_observation::{
		MidnightObservationTokenMovement, ObservationCostModel, TimestampUnixMillis,
	};
	use parity_scale_codec::Encode;

	#[derive(Clone, Copy)]
	enum Kind {
		Registration,
		Create,
		Spend,
	}

	fn position(block: u32, tx: u32) -> CardanoPosition {
		CardanoPosition {
			block_hash: McBlockHash([block as u8; 32]),
			block_number: block,
			block_timestamp: TimestampUnixMillis(block as i64 * 20_000),
			tx_index_in_block: tx,
		}
	}

	fn utxo(block: u32, tx: u32, index: u16, kind: Kind) -> ObservedUtxo {
		let mut bytes = [0u8; 32];
		bytes[..4].copy_from_slice(&block.to_be_bytes());
		bytes[4..8].copy_from_slice(&tx.to_be_bytes());
		let tx_hash = McTxHash(bytes);
		let spent_tx_hash = McTxHash([9; 32]);
		let owner = CardanoRewardAddressBytes([7; 29]);
		let (utxo_tx_hash, data) = match kind {
			Kind::Registration => (
				tx_hash,
				ObservedUtxoData::Registration(RegistrationData {
					cardano_reward_address: owner,
					dust_public_key: DustPublicKeyBytes::default(),
				}),
			),
			Kind::Create => (
				tx_hash,
				ObservedUtxoData::AssetCreate(CreateData {
					value: 1,
					owner,
					utxo_tx_hash: tx_hash,
					utxo_tx_index: index,
				}),
			),
			Kind::Spend => (
				spent_tx_hash,
				ObservedUtxoData::AssetSpend(SpendData {
					value: 1,
					owner,
					utxo_tx_hash: spent_tx_hash,
					utxo_tx_index: index,
					spending_tx_hash: tx_hash,
				}),
			),
		};
		ObservedUtxo {
			header: ObservedUtxoHeader {
				tx_position: position(block, tx),
				tx_hash,
				utxo_tx_hash,
				utxo_index: UtxoIndexInTx(index),
			},
			data,
		}
	}

	/// `n` UTXOs of one kind in the Cardano transaction at (`block`, `tx`).
	fn tx(block: u32, tx: u32, n: u16, kind: Kind) -> Vec<ObservedUtxo> {
		(0..n).map(|index| utxo(block, tx, index, kind)).collect()
	}

	/// A deterministic mixed batch of `txs` Cardano transactions spread over
	/// blocks, one to four UTXOs of varying kinds each, sorted as the data source
	/// sorts them.
	fn batch(seed: u64, txs: u32) -> Vec<ObservedUtxo> {
		let mut state = seed;
		let mut next = move || {
			state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
			(state >> 33) as u32
		};
		let mut utxos = Vec::new();
		let (mut block, mut tx_index) = (1u32, 0u32);
		for _ in 0..txs {
			if next() % 3 == 0 {
				block += 1;
				tx_index = 0;
			} else {
				tx_index += 1;
			}
			for index in 0..(1 + next() % 4) as u16 {
				let kind = match next() % 4 {
					0 => Kind::Registration,
					1 | 2 => Kind::Create,
					_ => Kind::Spend,
				};
				utxos.push(utxo(block, tx_index, index, kind));
			}
		}
		utxos.sort();
		utxos
	}

	/// Verbatim copy of the count-bound truncation in nodes without the cost model,
	/// which check our inherent against theirs. Do not update.
	fn shipped_truncation(
		utxos: Vec<ObservedUtxo>,
		tx_capacity: usize,
		start_position: &CardanoPosition,
		end: CardanoPosition,
	) -> (Vec<ObservedUtxo>, CardanoPosition) {
		let utxo_capacity = tx_capacity * 64;
		let mut truncated_utxos = Vec::with_capacity(utxo_capacity);
		let mut num_txs = 0;
		let mut cur_tx: Option<CardanoPosition> = None;
		for utxo in utxos {
			if cur_tx.as_ref().is_none_or(|tx| tx < &utxo.header.tx_position) {
				num_txs += 1;
				cur_tx = Some(utxo.header.tx_position.clone());
			}
			if num_txs == tx_capacity {
				break;
			}
			truncated_utxos.push(utxo);
		}

		if num_txs < tx_capacity {
			(truncated_utxos, end)
		} else {
			let end = truncated_utxos
				.last()
				.map_or(start_position.clone(), |u| u.header.tx_position.clone())
				.increment();
			(truncated_utxos, end)
		}
	}

	fn inherent(utxos: Vec<ObservedUtxo>, next_cardano_position: CardanoPosition) -> Vec<u8> {
		MidnightObservationTokenMovement { utxos, next_cardano_position }.encode()
	}

	fn model() -> ObservationCostModel {
		ObservationCostModel {
			utxo_gas: 1,
			create_gas: 10,
			destroy_gas: 5,
			budget_gas: 100,
			full_gas: 200,
		}
	}

	fn cost() -> ObservationBound {
		ObservationBound::Cost(model())
	}

	#[test]
	fn under_the_count_bound_the_inherent_is_byte_identical_to_the_shipped_one() {
		let start = position(1, 0);
		let tip = position(99, 0).increment();
		for seed in 0..32u64 {
			for txs in [0u32, 1, 2, 7, 40] {
				let utxos = batch(seed, txs);
				let n = txs as usize;
				for tx_capacity in [0, 1, 2, 3, n / 2, n, n + 1, 200] {
					let (admitted, cut) =
						admit_prefix(utxos.clone(), &ObservationBound::Count(tx_capacity));
					let end = cut.end(&admitted, &start, tip.clone());
					let (expected, expected_end) =
						shipped_truncation(utxos.clone(), tx_capacity, &start, tip.clone());
					assert_eq!(
						inherent(admitted, end),
						inherent(expected, expected_end),
						"seed {seed}, {txs} txs, tx_capacity {tx_capacity}"
					);
				}
			}
		}
	}

	#[test]
	fn cost_cut_falls_on_a_transaction_boundary_and_resumes_there() {
		// 4 + 5 creates at 11 each are 99 of the budget of 100; the third transaction
		// would make it 121.
		let utxos =
			[tx(1, 0, 4, Kind::Create), tx(1, 1, 5, Kind::Create), tx(2, 0, 2, Kind::Create)]
				.concat();
		let (admitted, cut) = admit_prefix(utxos, &cost());
		assert_eq!(admitted.len(), 9);
		assert_eq!(cut, Cut::Cost(position(2, 0)));
		assert_eq!(cut.end(&admitted, &position(1, 0), position(9, 0)), position(2, 0));
	}

	#[test]
	fn every_utxo_is_priced_by_weight_and_creates_and_spends_by_the_ledger_too() {
		// 10 registrations are 10; 10 spends at 6 each bring it to 70 and 5 more to
		// exactly 100; one more spend would be 106.
		let utxos = [
			tx(1, 0, 10, Kind::Registration),
			tx(1, 1, 10, Kind::Spend),
			tx(1, 2, 5, Kind::Spend),
			tx(1, 3, 1, Kind::Spend),
		]
		.concat();
		let (admitted, cut) = admit_prefix(utxos, &cost());
		assert_eq!(admitted.len(), 25);
		assert_eq!(cut, Cut::Cost(position(1, 3)));
	}

	#[test]
	fn registrations_alone_are_bounded() {
		// 50 + 40 registrations are 90; 11 more would be 101.
		let utxos = [
			tx(1, 0, 50, Kind::Registration),
			tx(1, 1, 40, Kind::Registration),
			tx(1, 2, 11, Kind::Registration),
		]
		.concat();
		let (admitted, cut) = admit_prefix(utxos, &cost());
		assert_eq!(admitted.len(), 90);
		assert_eq!(cut, Cut::Cost(position(1, 2)));
	}

	#[test]
	fn lone_transaction_between_budget_and_full_is_admitted_alone() {
		// 165 exceeds the budget but fits the block; nothing may join it.
		let utxos = [tx(1, 0, 15, Kind::Create), tx(1, 1, 1, Kind::Create)].concat();
		let (admitted, cut) = admit_prefix(utxos, &cost());
		assert_eq!(admitted.len(), 15);
		assert_eq!(cut, Cut::Cost(position(1, 1)));
	}

	#[test]
	fn lone_transaction_over_the_full_block_yields_an_empty_batch_at_its_position() {
		let utxos = [tx(1, 0, 25, Kind::Create), tx(1, 1, 1, Kind::Create)].concat();
		let (admitted, cut) = admit_prefix(utxos, &cost());
		assert!(admitted.is_empty());
		assert_eq!(cut, Cut::Cost(position(1, 0)));
		assert_eq!(cut.end(&admitted, &position(1, 0), position(9, 0)), position(1, 0));
	}

	#[test]
	fn count_bound_admits_fewer_than_capacity_transactions() {
		let utxos =
			[tx(1, 0, 1, Kind::Create), tx(1, 1, 1, Kind::Create), tx(1, 2, 1, Kind::Create)]
				.concat();
		let (admitted, cut) = admit_prefix(utxos, &ObservationBound::Count(2));
		assert_eq!(admitted.len(), 1);
		assert_eq!(cut, Cut::TxCapacity);
		assert_eq!(cut.end(&admitted, &position(1, 0), position(9, 0)), position(1, 0).increment());
	}

	#[test]
	fn everything_fits_reaches_the_tip() {
		let utxos = [tx(1, 0, 3, Kind::Create), tx(2, 0, 3, Kind::Spend)].concat();
		let (admitted, cut) = admit_prefix(utxos, &cost());
		assert_eq!(admitted.len(), 6);
		assert_eq!(cut, Cut::NotSaturated);
		assert_eq!(cut.end(&admitted, &position(1, 0), position(9, 0)), position(9, 0));
	}

	#[test]
	fn fetch_row_cap_never_truncates_ahead_of_the_cost_cut() {
		let cap = fetch_row_cap(&cost());
		assert_eq!(cap, 201);
		// A full fetch of the cheapest kind always overruns the block, whether it is
		// one transaction or one UTXO per transaction, so the cut lands inside what
		// was fetched and nothing past the cap is skipped.
		let one_tx = tx(1, 0, cap as u16, Kind::Registration);
		let (admitted, cut) = admit_prefix(one_tx, &cost());
		assert!(admitted.is_empty());
		assert_eq!(cut, Cut::Cost(position(1, 0)));
		let many_txs: Vec<_> =
			(0..cap as u32).flat_map(|i| tx(1, i, 1, Kind::Registration)).collect();
		let (admitted, cut) = admit_prefix(many_txs, &cost());
		assert_eq!(admitted.len(), 100);
		assert_eq!(cut, Cut::Cost(position(1, 100)));
		assert_eq!(fetch_row_cap(&ObservationBound::Count(200)), 12_800);
	}
}
