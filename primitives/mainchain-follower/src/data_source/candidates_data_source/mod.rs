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

//! Db-Sync data source used by Partner Chain committee selection
use authority_selection_inherents::*;
use db_sync_sqlx::{Address, Asset, BlockNumber, EpochNumber};
use itertools::Itertools;
use log::error;
use partner_chains_db_sync_data_sources::McFollowerMetrics;
use partner_chains_plutus_data::{
	permissioned_candidates::PermissionedCandidateDatums,
	registered_candidates::RegisterValidatorDatum,
};
use sidechain_domain::*;
use sqlx::PgPool;
use std::collections::HashMap;
use std::error::Error;

pub mod cached;
mod db_model;

#[derive(Clone, Debug)]
struct ParsedCandidate {
	utxo_info: UtxoInfo,
	datum: RegisterValidatorDatum,
	tx_inputs: Vec<UtxoId>,
}

#[derive(Debug)]
struct RegisteredCandidate {
	stake_pool_pub_key: StakePoolPublicKey,
	registration_utxo: UtxoId,
	tx_inputs: Vec<UtxoId>,
	sidechain_signature: SidechainSignature,
	mainchain_signature: MainchainSignature,
	cross_chain_signature: CrossChainSignature,
	sidechain_pub_key: SidechainPublicKey,
	cross_chain_pub_key: CrossChainPublicKey,
	keys: CandidateKeys,
	utxo_info: UtxoInfo,
}

/// Db-Sync data source serving data for Partner Chain committee selection
pub struct CandidatesDataSourceImpl {
	/// Postgres connection pool
	pool: PgPool,
	/// Prometheus metrics client
	metrics_opt: Option<McFollowerMetrics>,
	/// Configuration used by Db-Sync
	db_sync_config: db_model::DbSyncConfigurationProvider,
}

observed_async_trait!(
impl AuthoritySelectionDataSource for CandidatesDataSourceImpl {
	async fn get_ariadne_parameters(
			&self,
			epoch: McEpochNumber,
			_d_parameter_policy: PolicyId,
			permissioned_candidate_policy: PolicyId
	) -> Result<AriadneParameters, Box<dyn std::error::Error + Send + Sync>> {
		let epoch = EpochNumber::from(self.get_epoch_of_data_storage(epoch)?);
		let permissioned_candidate_asset = Asset::new(permissioned_candidate_policy);

		let candidates_output_opt =
			db_model::get_token_utxo_for_epoch(&self.pool, &permissioned_candidate_asset, epoch).await?;

		// DParameter is now read from pallet_system_parameters storage, not from mainchain.
		// This hardcoded value is unused - the actual d_parameter comes from the runtime.
		let d_parameter = DParameter { num_permissioned_candidates: 0, num_registered_candidates: 0 };

		let permissioned_candidates = match candidates_output_opt {
			None => None,
			Some(candidates_output) => {
				let candidates_datum = candidates_output.datum.ok_or(
					db_model::DataSourceError::ExpectedDataNotFound("Permissioned Candidates List Datum".to_string()),
				)?;
				Some(PermissionedCandidateDatums::try_from(candidates_datum.0)?.into())
			},
		};

		Ok(AriadneParameters { d_parameter, permissioned_candidates })
	}

	async fn get_candidates(
			&self,
			epoch: McEpochNumber,
			committee_candidate_address: MainchainAddress
	)-> Result<Vec<CandidateRegistrations>, Box<dyn std::error::Error + Send + Sync>> {
		let epoch = EpochNumber::from(self.get_epoch_of_data_storage(epoch)?);
		let candidates = self.get_registered_candidates(epoch, committee_candidate_address).await?;
		let stake_map = Self::make_stake_map(db_model::get_stake_distribution(&self.pool, epoch).await?);
		Ok(Self::group_candidates_by_mc_pub_key(candidates).into_iter().map(|(mainchain_pub_key, candidate_registrations)| {
			CandidateRegistrations {
				stake_pool_public_key: mainchain_pub_key.clone(),
				registrations: candidate_registrations.into_iter().map(Self::make_registration_data).collect(),
				stake_delegation: Self::get_stake_delegation(&stake_map, &mainchain_pub_key),
			}
		}).collect())
	}

	async fn get_epoch_nonce(&self, epoch: McEpochNumber) -> Result<Option<EpochNonce>, Box<dyn std::error::Error + Send + Sync>> {
		let epoch = self.get_epoch_of_data_storage(epoch)?;
		let nonce = db_model::get_epoch_nonce(&self.pool, EpochNumber(epoch.0)).await?;
		Ok(nonce.map(|n| EpochNonce(n.0)))
	}

	async fn data_epoch(&self, for_epoch: McEpochNumber) -> Result<McEpochNumber, Box<dyn std::error::Error + Send + Sync>> {
		self.get_epoch_of_data_storage(for_epoch)
	}
});

impl CandidatesDataSourceImpl {
	/// Creates new instance of the data source
	pub async fn new(
		pool: PgPool,
		metrics_opt: Option<McFollowerMetrics>,
	) -> Result<CandidatesDataSourceImpl, Box<dyn std::error::Error + Send + Sync>> {
		db_model::create_idx_ma_tx_out_ident(&pool).await?;
		db_model::create_idx_tx_out_address(&pool).await?;
		Ok(Self {
			pool: pool.clone(),
			metrics_opt,
			db_sync_config: db_model::DbSyncConfigurationProvider::new(pool),
		})
	}

	/// Creates a new caching instance of the data source
	pub fn cached(
		self,
		candidates_for_epoch_cache_size: usize,
	) -> std::result::Result<cached::CandidateDataSourceCached, Box<dyn Error + Send + Sync>> {
		cached::CandidateDataSourceCached::new_from_env(self, candidates_for_epoch_cache_size)
	}

	/// Registrations state up to this block are considered as "active", after it - as "pending".
	async fn get_last_block_for_epoch(
		&self,
		epoch: EpochNumber,
	) -> Result<Option<BlockNumber>, Box<dyn std::error::Error + Send + Sync>> {
		let block_option = db_model::get_latest_block_for_epoch(&self.pool, epoch).await?;
		Ok(block_option.map(|b| b.block_no))
	}

	async fn get_registered_candidates(
		&self,
		epoch: EpochNumber,
		committee_candidate_address: MainchainAddress,
	) -> Result<Vec<RegisteredCandidate>, Box<dyn std::error::Error + Send + Sync>> {
		let registrations_block_for_epoch = self.get_last_block_for_epoch(epoch).await?;
		let address: Address = Address(committee_candidate_address.to_string());
		let active_utxos = match registrations_block_for_epoch {
			Some(block) => {
				db_model::get_utxos_for_address(
					&self.pool,
					&address,
					block,
					self.db_sync_config.get_tx_in_config().await?,
				)
				.await?
			},
			None => vec![],
		};
		self.convert_utxos_to_candidates(&active_utxos)
	}

	fn group_candidates_by_mc_pub_key(
		candidates: Vec<RegisteredCandidate>,
	) -> HashMap<StakePoolPublicKey, Vec<RegisteredCandidate>> {
		candidates.into_iter().into_group_map_by(|c| c.stake_pool_pub_key.clone())
	}

	fn make_registration_data(c: RegisteredCandidate) -> RegistrationData {
		RegistrationData {
			registration_utxo: c.registration_utxo,
			sidechain_signature: c.sidechain_signature,
			mainchain_signature: c.mainchain_signature,
			cross_chain_signature: c.cross_chain_signature,
			sidechain_pub_key: c.sidechain_pub_key,
			cross_chain_pub_key: c.cross_chain_pub_key,
			keys: c.keys,
			utxo_info: c.utxo_info,
			tx_inputs: c.tx_inputs,
		}
	}

	fn make_stake_map(
		stake_pool_entries: Vec<db_model::StakePoolEntry>,
	) -> HashMap<MainchainKeyHash, StakeDelegation> {
		stake_pool_entries
			.into_iter()
			.map(|e| (MainchainKeyHash(e.pool_hash), StakeDelegation(e.stake.0)))
			.collect()
	}

	fn get_stake_delegation(
		stake_map: &HashMap<MainchainKeyHash, StakeDelegation>,
		stake_pool_pub_key: &StakePoolPublicKey,
	) -> Option<StakeDelegation> {
		if stake_map.is_empty() {
			None
		} else {
			Some(
				stake_map
					.get(&MainchainKeyHash::from_vkey(&stake_pool_pub_key.0))
					.cloned()
					.unwrap_or(StakeDelegation(0)),
			)
		}
	}

	// Converters
	fn convert_utxos_to_candidates(
		&self,
		outputs: &[db_model::MainchainTxOutput],
	) -> Result<Vec<RegisteredCandidate>, Box<dyn std::error::Error + Send + Sync>> {
		Self::parse_candidates(outputs)
			.into_iter()
			.map(|c| {
				match c.datum {
					RegisterValidatorDatum::V0 {
						stake_ownership,
						sidechain_pub_key,
						sidechain_signature,
						registration_utxo,
						own_pkh: _own_pkh,
						aura_pub_key,
						grandpa_pub_key,
					} => Ok(RegisteredCandidate {
						stake_pool_pub_key: stake_ownership.pub_key,
						mainchain_signature: stake_ownership.signature,
						// For now we use the same key for both cross chain and sidechain actions
						cross_chain_pub_key: CrossChainPublicKey(sidechain_pub_key.0.clone()),
						cross_chain_signature: CrossChainSignature(sidechain_signature.0.clone()),
						sidechain_signature,
						sidechain_pub_key,
						keys: CandidateKeys(vec![aura_pub_key.into(), grandpa_pub_key.into()]),
						registration_utxo,
						tx_inputs: c.tx_inputs,
						utxo_info: c.utxo_info,
					}),
					RegisterValidatorDatum::V1 {
						stake_ownership,
						sidechain_pub_key,
						sidechain_signature,
						registration_utxo,
						own_pkh: _own_pkh,
						keys,
					} => Ok(RegisteredCandidate {
						stake_pool_pub_key: stake_ownership.pub_key,
						mainchain_signature: stake_ownership.signature,
						// For now we use the same key for both cross chain and sidechain actions
						cross_chain_pub_key: CrossChainPublicKey(sidechain_pub_key.0.clone()),
						cross_chain_signature: CrossChainSignature(sidechain_signature.0.clone()),
						sidechain_signature,
						sidechain_pub_key,
						keys,
						registration_utxo,
						tx_inputs: c.tx_inputs,
						utxo_info: c.utxo_info,
					}),
				}
			})
			.collect()
	}

	fn parse_candidates(outputs: &[db_model::MainchainTxOutput]) -> Vec<ParsedCandidate> {
		let results: Vec<std::result::Result<ParsedCandidate, String>> = outputs
			.iter()
			.map(|output| {
				let datum = output.datum.clone().ok_or(format!(
					"Missing registration datum for {:?}",
					output.clone().utxo_id
				))?;
				let register_validator_datum =
					RegisterValidatorDatum::try_from(datum).map_err(|_| {
						format!("Invalid registration datum for {:?}", output.clone().utxo_id)
					})?;
				Ok(ParsedCandidate {
					utxo_info: UtxoInfo {
						utxo_id: output.utxo_id,
						epoch_number: output.tx_epoch_no.into(),
						block_number: output.tx_block_no.into(),
						slot_number: output.tx_slot_no.into(),
						tx_index_within_block: McTxIndexInBlock(output.tx_index_in_block.0),
					},
					datum: register_validator_datum,
					tx_inputs: output.tx_inputs.clone(),
				})
			})
			.collect();
		results
			.into_iter()
			.filter_map(|r| match r {
				Ok(candidate) => Some(candidate.clone()),
				Err(msg) => {
					error!("{msg}");
					None
				},
			})
			.collect()
	}

	fn get_epoch_of_data_storage(
		&self,
		epoch_of_data_usage: McEpochNumber,
	) -> Result<McEpochNumber, Box<dyn std::error::Error + Send + Sync>> {
		offset_data_epoch(&epoch_of_data_usage).map_err(|offset| {
			db_model::DataSourceError::BadRequest(format!(
				"Minimum supported epoch of data usage is {offset}, but {} was provided",
				epoch_of_data_usage.0
			))
			.into()
		})
	}
}

/// Logs each method invocation and each returned result.
/// Has to be made at the level of trait, because otherwise #[async_trait] is expanded first.
/// '&self' matching yields "__self" identifier not found error, so "&$self:tt" is required.
/// Works only if return type is Result.
/// Note: Metrics tracking is disabled because McFollowerMetrics methods are crate-private in partner-chains.
macro_rules! observed_async_trait {
	(impl $(<$($type_param:tt),+>)? $trait_name:ident $(<$($type_arg:ident),+>)? for $target_type:ty
		$(where $($where_type:ident : $where_bound:tt ,)+)?

		{
		$(type $type_name:ident = $type:ty;)*
		$(async fn $method:ident(&$self:tt $(,$param_name:ident: $param_type:ty)* $(,)?) -> $res:ty $body:block)*
	})=> {
		#[async_trait::async_trait]
		impl $(<$($type_param),+>)? $trait_name $(<$($type_arg),+>)? for $target_type
		$(where $($where_type : $where_bound ,)+)?

		{
		$(type $type_name = $type;)*
		$(
			async fn $method(&$self $(,$param_name: $param_type)*,) -> $res {
				let method_name = stringify!($method);
				let _ = &$self.metrics_opt; // Silence unused field warning
				let params: Vec<String> = vec![$(format!("{:?}", $param_name.clone()),)*];
				log::debug!("{} called with parameters: {:?}", method_name, params);
				let result = $body;
				match &result {
					Ok(value) => {
						log::debug!("{} returns {:?}", method_name, value);
					},
					Err(error) => {
						log::error!("{} failed with {:?}", method_name, error);
					},
				};
				result
			}
		)*
		}
	};
}

pub(crate) use observed_async_trait;

/// Returns the epoch number for a given block hash from the database.
/// This is useful for converting a Cardano block hash to an epoch number.
pub async fn get_epoch_for_block_hash(
	pool: &sqlx::PgPool,
	block_hash: &McBlockHash,
) -> Result<Option<McEpochNumber>, Box<dyn Error + Send + Sync>> {
	Ok(db_model::get_epoch_for_block_hash(pool, &block_hash.0)
		.await?
		.map(|e| McEpochNumber(e.0)))
}
