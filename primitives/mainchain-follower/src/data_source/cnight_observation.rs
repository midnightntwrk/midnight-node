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
	CNightAddresses, CardanoPosition, CardanoRewardAddressBytes, DustPublicKeyBytes,
	ObservationBatch, ObservationCostModel, ObservedUtxos,
};
use sidechain_domain::{McBlockHash, McTxHash};
pub use sqlx::PgPool;
use std::fmt::Debug;

/// Page limit per UTXO kind under [`ObservationBound::Count`] (API version 1 runtimes
/// and genesis only), in UTXOs per Cardano transaction. Fixed: nodes without the cost
/// model check our inherent against their own, fetched with this limit.
const UTXOS_PER_TX_OVERESTIMATE: usize = 64;

/// Bytes of the smallest row of any UTXO kind: a transaction input, CBOR
/// `[hash32, index < 24]`. `maxTxSize` over this bounds one transaction's rows.
const MIN_ROW_BYTES: i64 = 36;

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
	#[error("db-sync returned {rows} rows for a query limited to {limit}")]
	RowLimitExceeded { rows: usize, limit: usize },
	#[error("db-sync has no epoch_param row for the epoch of Cardano block {0}")]
	MissingEpochParams(u32),
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
		start_offset: u32,
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

		let (low_bounds, high_bounds, max_rows_per_tx) = tokio::try_join!(
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
			async {
				let ObservationBound::Cost(_) = bound else { return Ok(0) };
				let _sq_timer = start_sub_query_timer(&self.metrics_opt, "cnight_get_max_tx_size");
				let block = start_position.block_number;
				let max_tx_size = crate::db::get_max_tx_size(&self.pool, block)
					.await?
					.ok_or(MidnightCNightObservationDataSourceError::MissingEpochParams(block))?;
				Ok::<_, Box<dyn std::error::Error + Send + Sync>>((max_tx_size / MIN_ROW_BYTES) as usize)
			},
			)?;
		let low_bounds = low_bounds.expect("Start position contains block hash that exists in database");
		let high_bounds = high_bounds.expect("End position contains block hash that exists in database");
		// Increment the end position to tx_index + 1 of the current mainchain position
		let end = end.increment();
		log::debug!("Bounds:\n{:?}\n{:?}\nfor positions:\n{:?}\n{:?}", low_bounds, high_bounds, start_position, end);

		// Call db methods to get UTXOs (offset + limit) until we reach our capacity
		// TODO: (possibly) Replace this with grabbing from a queue that's filled async by an offchain thread
		// ^ We may not have to do the above if the queries are fast enough

		let query = |ledger_gas: fn(&ObservationCostModel) -> u64| PagedQuery {
			start: start_position,
			end: &end,
			limit: page_limit(bound, start_offset as usize, max_rows_per_tx, ledger_gas),
			offset: 0,
			low_bound: low_bounds,
			high_bound: high_bounds,
		};
		let mapping_query = query(|_| 0);
		let create_query = query(|model| model.create_gas);
		let spend_query = query(|model| model.destroy_gas);

		let (registration_utxos, deregistration_utxos, asset_create_utxos, asset_spend_utxos) = tokio::try_join!(
			async {
				let _sq_timer = start_sub_query_timer(&self.metrics_opt, "cnight_get_registrations");
				match auth_token_ident {
					Some(ident) => self.get_registration_utxos(
						cardano_network,
						ident,
						&config.mapping_validator_address,
						&mapping_query,
					)
					.await
					.map_err(Into::<Box<dyn std::error::Error + Send + Sync>>::into),
					None => Ok(UtxoPage::default()),
				}
			},
			async {
				let _sq_timer = start_sub_query_timer(&self.metrics_opt, "cnight_get_deregistrations");
				self.get_deregistration_utxos(
					cardano_network,
					&config.mapping_validator_address,
					&mapping_query,
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
						&create_query,
					)
					.await
					.map_err(Into::<Box<dyn std::error::Error + Send + Sync>>::into),
					None => Ok(UtxoPage::default()),
				}
			},
			async {
				let _sq_timer = start_sub_query_timer(&self.metrics_opt, "cnight_get_asset_spends");
				match cnight_ident {
					Some(ident) => self.get_asset_spend_utxos(
						cardano_network,
						ident,
						&spend_query,
					)
					.await
					.map_err(Into::<Box<dyn std::error::Error + Send + Sync>>::into),
					None => Ok(UtxoPage::default()),
				}
			}
		)?;

		let pages = [registration_utxos, deregistration_utxos, asset_create_utxos, asset_spend_utxos];
		let frontier = match bound {
			ObservationBound::Count(_) => None,
			ObservationBound::Cost(_) => earliest_cut_short(&pages),
		};
		// Concatenation order matters: the legacy sort is stable over it.
		let mut utxos = ObservationBatch::new(
			pages.into_iter().flat_map(|page| page.utxos).collect(),
			spec_version,
		);
		skip_observed(&mut utxos, start_position, start_offset as usize);

		let (utxos, cut) = admit_prefix(utxos, bound, frontier.as_ref());
		match &cut {
			Cut::Cost(resume_at) if utxos.last().is_some_and(|u| u.header.tx_position == *resume_at) => log::info!(
				"cNIGHT observation batch ends inside the Cardano transaction at {resume_at}; resuming there"
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

/// One UTXO kind's page: the UTXOs decoded from its rows and, when the rows filled the
/// row cap, the position of the last row. Rows are dropped while decoding, so only the
/// raw row count says whether the cap was hit.
#[derive(Default)]
struct UtxoPage {
	utxos: Vec<ObservedUtxo>,
	cut_short_at: Option<CardanoPosition>,
}

fn check_row_limit(
	rows: usize,
	query: &PagedQuery<'_>,
) -> Result<(), MidnightCNightObservationDataSourceError> {
	if rows > query.limit {
		return Err(MidnightCNightObservationDataSourceError::RowLimitExceeded {
			rows,
			limit: query.limit,
		});
	}
	Ok(())
}

/// The first position whose UTXOs may be incomplete: a kind that filled its row cap
/// may have more rows in the transaction of its last row or later, and SQL orders the
/// rows inside a transaction differently from the observation order.
fn earliest_cut_short(pages: &[UtxoPage]) -> Option<CardanoPosition> {
	pages
		.iter()
		.filter_map(|page| page.cut_short_at.clone())
		.min_by(|a, b| a.partial_cmp(b).expect("CardanoPosition order is total"))
}

/// Drops the first `start_offset` UTXOs of the transaction at `start`, which earlier
/// batches already observed.
fn skip_observed(utxos: &mut ObservationBatch, start: &CardanoPosition, start_offset: usize) {
	let observed = utxos
		.iter()
		.take(start_offset)
		.take_while(|utxo| utxo.header.tx_position == *start)
		.count();
	utxos.remove_prefix(observed);
}

/// Where [`admit_prefix`] stopped.
#[derive(Debug, Clone, PartialEq)]
enum Cut {
	/// Every fetched UTXO was admitted; the batch reaches the Cardano tip.
	NotSaturated,
	/// The Cardano transaction count of an [`ObservationBound::Count`] was reached.
	TxCapacity,
	/// The pessimistic ledger cost bound was hit at a UTXO of the transaction at this
	/// position. If earlier UTXOs of that transaction were admitted, the runtime records
	/// how many and the next batch resumes after them.
	Cost(CardanoPosition),
	/// The page frontier: the transaction at this position may be incomplete, so the
	/// next batch fetches it afresh.
	Frontier(CardanoPosition),
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
			Cut::Cost(resume_at) | Cut::Frontier(resume_at) => resume_at.clone(),
		}
	}
}

/// Limit of one UTXO kind's page, whose UTXOs each cost `utxo_gas + ledger_gas`.
///
/// Covers the skipped UTXOs plus more than the budget affords, so the cost cut, not
/// the limit, usually ends a batch. The `max_rows_per_tx` floor, at least the rows
/// of one kind the start transaction can carry, makes a full page reach past it,
/// so the cursor advances even when every row before the frontier fails to decode.
fn page_limit(
	bound: &ObservationBound,
	start_offset: usize,
	max_rows_per_tx: usize,
	ledger_gas: fn(&ObservationCostModel) -> u64,
) -> usize {
	match bound {
		ObservationBound::Count(tx_capacity) => tx_capacity * UTXOS_PER_TX_OVERESTIMATE,
		ObservationBound::Cost(model) => {
			let affordable = model.budget_gas / model.utxo_gas.saturating_add(ledger_gas(model));
			start_offset + (affordable as usize).max(max_rows_per_tx) + 1
		},
	}
}

/// Admits a prefix of `utxos`, sorted in the observation order.
///
/// [`ObservationBound::Count`] admits fewer than that many whole transactions.
/// [`ObservationBound::Cost`] admits UTXOs, before the `frontier`, while the
/// batch's pessimistic price, counting every `AssetCreate` and `AssetSpend` as a
/// ledger event, fits `budget_gas`. It may stop inside a Cardano transaction, so a
/// transaction of any size drains over as many blocks as it needs. The first UTXO is
/// always admitted.
fn admit_prefix(
	mut utxos: ObservationBatch,
	bound: &ObservationBound,
	frontier: Option<&CardanoPosition>,
) -> (ObservationBatch, Cut) {
	let mut admitted = 0;
	let cut = match bound {
		ObservationBound::Count(tx_capacity) => {
			let mut num_txs = 0;
			// A strictly greater position starts a new transaction, as in the sort order.
			for group in utxos.chunk_by(|a, b| a.header.tx_position >= b.header.tx_position) {
				num_txs += 1;
				if num_txs == *tx_capacity {
					break;
				}
				admitted += group.len();
			}
			if num_txs >= *tx_capacity { Cut::TxCapacity } else { Cut::NotSaturated }
		},
		ObservationBound::Cost(model) => {
			let mut gas = 0u64;
			let mut cut = Cut::NotSaturated;
			for utxo in utxos.iter() {
				let position = &utxo.header.tx_position;
				if let Some(frontier) = frontier.filter(|frontier| position >= *frontier) {
					cut = Cut::Frontier(frontier.clone());
					break;
				}
				let ledger_gas = match utxo.data {
					ObservedUtxoData::AssetCreate(_) => model.create_gas,
					ObservedUtxoData::AssetSpend(_) => model.destroy_gas,
					_ => 0,
				};
				let bound = gas.saturating_add(model.utxo_gas).saturating_add(ledger_gas);
				if bound > model.budget_gas && admitted > 0 {
					cut = Cut::Cost(position.clone());
					break;
				}
				gas = bound;
				admitted += 1;
			}
			cut
		},
	};
	utxos.truncate(admitted);
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
	) -> Result<UtxoPage, MidnightCNightObservationDataSourceError> {
		let rows = get_registrations(&self.pool, address, auth_token_ident, query).await?;

		check_row_limit(rows.len(), query)?;
		let filled_cap = rows.len() == query.limit;
		let mut utxos = Vec::new();
		let mut cut_short_at = None;
		let mut rows = rows.into_iter().peekable();

		while let Some(row) = rows.next() {
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
			if filled_cap && rows.peek().is_none() {
				cut_short_at = Some(header.tx_position.clone());
			}

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

		Ok(UtxoPage { utxos, cut_short_at })
	}

	async fn get_deregistration_utxos(
		&self,
		cardano_network: u8,
		address: &str,
		query: &PagedQuery<'_>,
	) -> Result<UtxoPage, MidnightCNightObservationDataSourceError> {
		let rows = get_deregistrations(&self.pool, address, query).await?;

		check_row_limit(rows.len(), query)?;
		let filled_cap = rows.len() == query.limit;
		let mut utxos = Vec::new();
		let mut cut_short_at = None;
		let mut rows = rows.into_iter().peekable();

		while let Some(row) = rows.next() {
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
			if filled_cap && rows.peek().is_none() {
				cut_short_at = Some(header.tx_position.clone());
			}

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

		Ok(UtxoPage { utxos, cut_short_at })
	}

	async fn get_asset_create_utxos(
		&self,
		cardano_network: u8,
		ident: i64,
		query: &PagedQuery<'_>,
	) -> Result<UtxoPage, MidnightCNightObservationDataSourceError> {
		let rows = crate::db::get_asset_creates(&self.pool, ident, query).await?;

		check_row_limit(rows.len(), query)?;
		let filled_cap = rows.len() == query.limit;
		let mut utxos = Vec::new();
		let mut cut_short_at = None;
		let mut rows = rows.into_iter().peekable();

		while let Some(row) = rows.next() {
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
			if filled_cap && rows.peek().is_none() {
				cut_short_at = Some(header.tx_position.clone());
			}

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

		Ok(UtxoPage { utxos, cut_short_at })
	}

	async fn get_asset_spend_utxos(
		&self,
		cardano_network: u8,
		ident: i64,
		query: &PagedQuery<'_>,
	) -> Result<UtxoPage, MidnightCNightObservationDataSourceError> {
		let rows = crate::db::get_asset_spends(&self.pool, ident, query).await?;

		check_row_limit(rows.len(), query)?;
		let filled_cap = rows.len() == query.limit;
		let mut utxos = Vec::new();
		let mut cut_short_at = None;
		let mut rows = rows.into_iter().peekable();

		while let Some(row) = rows.next() {
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
			if filled_cap && rows.peek().is_none() {
				cut_short_at = Some(header.tx_position.clone());
			}

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

		Ok(UtxoPage { utxos, cut_short_at })
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use midnight_primitives_cnight_observation::{
		DATA_ORDERED_UTXOS_SPEC_VERSION, MidnightObservationTokenMovement, ObservationCostModel,
		TimestampUnixMillis, cmp_observed_utxos_by_data,
	};
	use parity_scale_codec::Encode;

	#[derive(Clone, Copy)]
	enum Kind {
		Registration,
		Deregistration,
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
			Kind::Deregistration => (
				spent_tx_hash,
				ObservedUtxoData::Deregistration(DeregistrationData {
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

	fn sorted(utxos: Vec<ObservedUtxo>) -> ObservationBatch {
		ObservationBatch::new(utxos, DATA_ORDERED_UTXOS_SPEC_VERSION)
	}

	fn inherent(utxos: ObservationBatch, next_cardano_position: CardanoPosition) -> Vec<u8> {
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
				let legacy =
					ObservationBatch::new(utxos.clone(), DATA_ORDERED_UTXOS_SPEC_VERSION - 1);
				let n = txs as usize;
				for tx_capacity in [0, 1, 2, 3, n / 2, n, n + 1, 200] {
					let (admitted, cut) =
						admit_prefix(legacy.clone(), &ObservationBound::Count(tx_capacity), None);
					let end = cut.end(&admitted, &start, tip.clone());
					let (expected, expected_end) =
						shipped_truncation(utxos.clone(), tx_capacity, &start, tip.clone());
					assert_eq!(
						inherent(admitted, end),
						inherent(
							ObservationBatch::new(expected, DATA_ORDERED_UTXOS_SPEC_VERSION - 1),
							expected_end
						),
						"seed {seed}, {txs} txs, tx_capacity {tx_capacity}"
					);
				}
			}
		}
	}

	/// `batch` plus, for some UTXOs, the second UTXO the same Cardano output yields: a
	/// create beside a registration, a spend beside a deregistration and vice versa.
	fn batch_with_shared_outputs(seed: u64, txs: u32) -> Vec<ObservedUtxo> {
		let mut utxos = batch(seed, txs);
		let twins: Vec<_> = utxos
			.iter()
			.enumerate()
			.filter(|(i, _)| (*i as u64 + seed).is_multiple_of(3))
			.map(|(_, u)| {
				let block = u.header.tx_position.block_number;
				let tx = u.header.tx_position.tx_index_in_block;
				let index = u.header.utxo_index.0;
				let twin = match u.data {
					ObservedUtxoData::Registration(_) => Kind::Create,
					ObservedUtxoData::AssetCreate(_) => Kind::Registration,
					ObservedUtxoData::Deregistration(_) => Kind::Spend,
					ObservedUtxoData::AssetSpend(_) => Kind::Deregistration,
				};
				utxo(block, tx, index, twin)
			})
			.collect();
		utxos.extend(twins);
		utxos
	}

	fn shuffle(utxos: &mut [ObservedUtxo], seed: u64) {
		let mut state = seed ^ 0x9e37_79b9_7f4a_7c15;
		for i in (1..utxos.len()).rev() {
			state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
			utxos.swap(i, (state >> 33) as usize % (i + 1));
		}
	}

	#[test]
	fn observation_order_is_strict_and_independent_of_input_order() {
		for seed in 0..32u64 {
			let utxos = batch_with_shared_outputs(seed, 40);
			let mut first = utxos.clone();
			shuffle(&mut first, seed);
			first.sort_by(cmp_observed_utxos_by_data);
			let mut second = utxos.clone();
			shuffle(&mut second, seed + 1000);
			second.sort_by(cmp_observed_utxos_by_data);
			assert_eq!(first, second, "seed {seed}: the order must not depend on input order");
			for pair in first.windows(2) {
				assert!(
					cmp_observed_utxos_by_data(&pair[0], &pair[1]).is_lt(),
					"seed {seed}: distinct UTXOs compare unequal"
				);
			}
		}
	}

	#[test]
	fn cost_cut_between_transactions_resumes_at_the_next_one() {
		// 4 + 5 creates at 11 each are 99 of the budget of 100; one more would be 110.
		let utxos =
			[tx(1, 0, 4, Kind::Create), tx(1, 1, 5, Kind::Create), tx(2, 0, 2, Kind::Create)]
				.concat();
		let (admitted, cut) = admit_prefix(sorted(utxos), &cost(), None);
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
		let (admitted, cut) = admit_prefix(sorted(utxos), &cost(), None);
		assert_eq!(admitted.len(), 25);
		assert_eq!(cut, Cut::Cost(position(1, 3)));
	}

	#[test]
	fn a_transaction_over_the_budget_is_cut_inside_it() {
		// 50 + 40 registrations are 90; 10 of the next transaction's 11 fit the budget.
		let utxos = [
			tx(1, 0, 50, Kind::Registration),
			tx(1, 1, 40, Kind::Registration),
			tx(1, 2, 11, Kind::Registration),
		]
		.concat();
		let (admitted, cut) = admit_prefix(sorted(utxos), &cost(), None);
		assert_eq!(admitted.len(), 100);
		assert_eq!(admitted.last().unwrap().header.tx_position, position(1, 2));
		assert_eq!(cut, Cut::Cost(position(1, 2)));
		assert_eq!(cut.end(&admitted, &position(1, 0), position(9, 0)), position(1, 2));
	}

	#[test]
	fn the_first_utxo_is_admitted_even_over_the_budget() {
		let expensive = ObservationCostModel { create_gas: 1_000, ..model() };
		let utxos = tx(1, 0, 3, Kind::Create);
		let (admitted, cut) = admit_prefix(sorted(utxos), &ObservationBound::Cost(expensive), None);
		assert_eq!(admitted.len(), 1);
		assert_eq!(cut, Cut::Cost(position(1, 0)));
	}

	#[test]
	fn the_frontier_stops_the_batch_before_a_possibly_incomplete_transaction() {
		let utxos = [tx(1, 0, 3, Kind::Registration), tx(1, 1, 3, Kind::Registration)].concat();
		let (admitted, cut) = admit_prefix(sorted(utxos), &cost(), Some(&position(1, 1)));
		assert_eq!(admitted.len(), 3);
		assert_eq!(cut, Cut::Frontier(position(1, 1)));
		assert_eq!(cut.end(&admitted, &position(1, 0), position(9, 0)), position(1, 1));
	}

	#[test]
	fn the_frontier_is_the_lowest_last_row_among_kinds_that_filled_the_cap() {
		let pages = [
			UtxoPage { utxos: vec![], cut_short_at: Some(position(3, 0)) },
			UtxoPage::default(),
			UtxoPage { utxos: vec![], cut_short_at: Some(position(2, 5)) },
			UtxoPage { utxos: vec![], cut_short_at: None },
		];
		assert_eq!(earliest_cut_short(&pages), Some(position(2, 5)));
		assert_eq!(earliest_cut_short(&[UtxoPage::default()]), None);
	}

	#[test]
	fn the_offset_skips_only_observed_utxos_of_the_start_transaction() {
		let utxos = [tx(1, 0, 5, Kind::Create), tx(1, 1, 2, Kind::Create)].concat();
		let mut skipped = sorted(utxos.clone());
		skip_observed(&mut skipped, &position(1, 0), 3);
		assert_eq!(skipped[..], utxos[3..]);
		// An offset past the transaction never reaches into the next one.
		let mut skipped = sorted(utxos.clone());
		skip_observed(&mut skipped, &position(1, 0), 9);
		assert_eq!(skipped[..], utxos[5..]);
	}

	#[test]
	fn count_bound_admits_fewer_than_capacity_transactions() {
		let utxos =
			[tx(1, 0, 1, Kind::Create), tx(1, 1, 1, Kind::Create), tx(1, 2, 1, Kind::Create)]
				.concat();
		let (admitted, cut) = admit_prefix(sorted(utxos), &ObservationBound::Count(2), None);
		assert_eq!(admitted.len(), 1);
		assert_eq!(cut, Cut::TxCapacity);
		assert_eq!(cut.end(&admitted, &position(1, 0), position(9, 0)), position(1, 0).increment());
	}

	#[test]
	fn everything_fits_reaches_the_tip() {
		let utxos = [tx(1, 0, 3, Kind::Create), tx(2, 0, 3, Kind::Spend)].concat();
		let (admitted, cut) = admit_prefix(sorted(utxos), &cost(), None);
		assert_eq!(admitted.len(), 6);
		assert_eq!(cut, Cut::NotSaturated);
		assert_eq!(cut.end(&admitted, &position(1, 0), position(9, 0)), position(9, 0));
	}

	#[test]
	fn page_limit_covers_the_skipped_utxos_and_more_of_its_kind_than_the_budget_affords() {
		let cheap = ObservationBound::Cost(ObservationCostModel { budget_gas: 100_000, ..model() });
		assert_eq!(page_limit(&cheap, 0, 455, |_| 0), 100_001);
		assert_eq!(page_limit(&cheap, 7, 455, |_| 0), 100_008);
		assert_eq!(page_limit(&cheap, 0, 455, |model| model.create_gas), 9_091);
		// Never fewer than a Cardano transaction's rows past the skipped UTXOs.
		assert_eq!(page_limit(&cost(), 7, 455, |model| model.create_gas), 463);
		assert_eq!(page_limit(&ObservationBound::Count(200), 7, 455, |_| 0), 12_800);
		// A full fetch of one kind always overruns the budget, so the cost cut lands
		// inside what was fetched.
		let rows =
			tx(1, 0, page_limit(&cheap, 0, 455, |model| model.create_gas) as u16, Kind::Create);
		let (admitted, cut) = admit_prefix(sorted(rows), &cheap, None);
		assert_eq!(admitted.len(), 9_090);
		assert_eq!(cut, Cut::Cost(position(1, 0)));
	}

	/// The runtime's offset rule, as `apply_observations` applies it.
	fn next_offset(
		admitted: &[ObservedUtxo],
		prev: &CardanoPosition,
		prev_offset: u32,
		next: &CardanoPosition,
	) -> u32 {
		let ends_inside =
			admitted.iter().rev().take_while(|u| u.header.tx_position == *next).count() as u32;
		if next == prev { prev_offset + ends_inside } else { ends_inside }
	}

	/// One block's batch as `get_utxos_up_to_capacity` builds it, with `rows` in the
	/// queries' order standing in for db-sync. `dropped` rows fail to decode.
	fn observe(
		rows: &[ObservedUtxo],
		max_rows_per_tx: usize,
		dropped: impl Fn(&ObservedUtxo) -> bool,
		cursor: &CardanoPosition,
		offset: u32,
		bound: &ObservationBound,
	) -> (ObservationBatch, Cut) {
		let ledger_gas: [fn(&ObservationCostModel) -> u64; 4] =
			[|_| 0, |_| 0, |model| model.destroy_gas, |model| model.create_gas];
		let pages: Vec<_> = (0..4u8)
			.map(|rank| {
				let limit =
					page_limit(bound, offset as usize, max_rows_per_tx, ledger_gas[rank as usize]);
				let page: Vec<_> = rows
					.iter()
					.filter(|u| u.data.ordering_rank() == rank && u.header.tx_position >= *cursor)
					.take(limit)
					.collect();
				UtxoPage {
					cut_short_at: (page.len() == limit)
						.then(|| page.last().unwrap().header.tx_position.clone()),
					utxos: page.into_iter().filter(|u| !dropped(u)).cloned().collect(),
				}
			})
			.collect();
		let frontier = earliest_cut_short(&pages);
		let mut utxos = sorted(pages.into_iter().flat_map(|page| page.utxos).collect());
		skip_observed(&mut utxos, cursor, offset as usize);
		admit_prefix(utxos, bound, frontier.as_ref())
	}

	#[test]
	fn big_and_undecodable_transactions_drain_over_several_blocks_each_utxo_once() {
		// Runtime prices: about 159 spends or 110 creates fit the budget. Mainnet has
		// carried a Cardano transaction with 427 cNIGHT spends, and creates to
		// enterprise addresses are fetched but decode to nothing.
		let model = ObservationCostModel {
			utxo_gas: 87_319_504,
			create_gas: 8_960_000_002,
			destroy_gas: 6_166_908_418,
			budget_gas: 1_000_000_000_000,
			full_gas: 2_000_000_000_000,
		};
		let bound = ObservationBound::Cost(model.clone());
		let rows = [
			tx(5, 1, 230, Kind::Create),
			tx(5, 2, 400, Kind::Create),
			tx(5, 3, 427, Kind::Spend),
			tx(5, 4, 3, Kind::Create),
			tx(6, 0, 1, Kind::Spend),
		]
		.concat();
		let dropped =
			|u: &ObservedUtxo| [position(5, 1), position(5, 2)].contains(&u.header.tx_position);
		let tip = position(6, 0).increment();
		let (mut cursor, mut offset) = (position(5, 0), 0u32);
		let mut observed = Vec::new();
		let mut blocks = 0;
		loop {
			blocks += 1;
			assert!(blocks < 10, "observation must keep advancing");
			// A 16 KiB `maxTxSize`.
			let (admitted, cut) = observe(&rows, 455, dropped, &cursor, offset, &bound);
			let price: u64 = admitted
				.iter()
				.map(|u| {
					model.utxo_gas
						+ match u.data {
							ObservedUtxoData::AssetSpend(_) => model.destroy_gas,
							ObservedUtxoData::AssetCreate(_) => model.create_gas,
							_ => 0,
						}
				})
				.sum();
			assert!(price <= model.budget_gas);
			let end = cut.end(&admitted, &cursor, tip.clone());
			offset = next_offset(&admitted, &cursor, offset, &end);
			cursor = end;
			observed.extend(admitted);
			if cut == Cut::NotSaturated {
				break;
			}
		}
		let decodable: Vec<_> = rows.into_iter().filter(|u| !dropped(u)).collect();
		assert_eq!(observed, decodable);
		// One block passes the undecodable creates, three drain the spends.
		assert_eq!(blocks, 4);
	}
}
