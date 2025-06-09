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

//! Data sources implementations that read from db-sync postgres.

use crate::MidnightNativeTokenObservationDataSource;
use crate::db_model::{PolicyUtxoRow, get_datum_for_address};
use db_sync_follower::{
	block::DbSyncBlockDataSourceConfig, metrics::McFollowerMetrics, observed_async_trait,
};
use derive_new::new;
use figment::Figment;
use figment::providers::Env;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
pub use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::{error::Error, fmt::Debug, str::FromStr};

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectionConfig {
	pub(crate) db_sync_postgres_connection_string: SecretString,
}

impl ConnectionConfig {
	pub fn from_env() -> Result<Self, Box<dyn Error + Send + Sync + 'static>> {
		let config: Self = Figment::new()
			.merge(Env::raw())
			.extract()
			.map_err(|e| format!("Failed to read main chain follower connection: {e}"))?;
		Ok(config)
	}
}

pub async fn get_connection(
	connection_string: &str,
	acquire_timeout: std::time::Duration,
) -> Result<PgPool, Box<dyn Error + Send + Sync + 'static>> {
	let connect_options = PgConnectOptions::from_str(connection_string)?;
	let pool = PgPoolOptions::new()
		.max_connections(5)
		.acquire_timeout(acquire_timeout)
		.connect_with(connect_options.clone())
		.await
		.map_err(|e| {
			PostgresConnectionError(
				connect_options.get_host().to_string(),
				connect_options.get_port(),
				connect_options.get_database().unwrap_or("cexplorer").to_string(),
				e.to_string(),
			)
			.to_string()
		})?;
	Ok(pool)
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("Could not connect to database: postgres://***:***@{0}:{1}/{2}; error: {3}")]
struct PostgresConnectionError(String, u16, String, String);

pub async fn get_connection_from_env() -> Result<PgPool, Box<dyn Error + Send + Sync + 'static>> {
	let config = ConnectionConfig::from_env()?;
	get_connection(
		config.db_sync_postgres_connection_string.expose_secret(),
		std::time::Duration::from_secs(30),
	)
	.await
}

#[derive(new)]
pub struct MidnightNativeTokenObservationDataSourceImpl {
	pub pool: PgPool,
	pub metrics_opt: Option<McFollowerMetrics>,
	db_sync_block_data_source_config: DbSyncBlockDataSourceConfig,
	#[allow(dead_code)]
	cache_size: u16,
}

observed_async_trait!(
	impl MidnightNativeTokenObservationDataSource for MidnightNativeTokenObservationDataSourceImpl {
	async fn get_night_generates_dust_registrants_datum(
		&self,
		address: &str,
		min_block_no: i64,
		max_block_no: i64,
	) -> Result<Vec<(Vec<u8>, Vec<u8>)>, Box<dyn std::error::Error + Send + Sync>> {
		let data_in_block_range = get_datum_for_address(&self.pool, address, min_block_no, max_block_no)
		.await
		.map_err(|e| format!("Failed to fetch data: {e}"))?;

		let mut cardano_address_dust_address_pairs = Vec::new();

		for datum in data_in_block_range {
			let constr = datum.as_constr_plutus_data()
				.ok_or("Expected ConstrPlutusData in datum")?;
			let list = constr.data();

			let cardano_wallet = list.get(0);
			let dust_address = list.get(1);
			let cardano_wallet_bytes = match cardano_wallet.as_bytes() {
				Some(bytes) => {
					match String::from_utf8(bytes.clone()) {
						Ok(bech32) => {
							cardano_serialization_lib::Address::from_bech32(&bech32)
								.map_err(|_| "Invalid bech32 address in datum")?
								.to_bytes()
						}
						Err(_) => bytes.clone(),
					}
				}
				None => return Err("Cardano wallet field is not bytes".into()),
			};

			let dust_address_bytes = dust_address.as_bytes()
				.ok_or("Dust address field is not bytes")?;

			cardano_address_dust_address_pairs.push((cardano_wallet_bytes, dust_address_bytes));
		}

		Ok(cardano_address_dust_address_pairs)
	}

	async fn get_spends_for_asset_in_block_range(
		&self,
		policy_id_hex: &str,
		asset_name: &str,
		min_block_no: i64,
		max_block_no: i64,
	) -> Result<Vec<PolicyUtxoRow>, Box<dyn std::error::Error + Send + Sync>> {
		let result = crate::db_model::get_spends_for_asset_in_block_range(
			&self.pool,
			policy_id_hex,
			asset_name,
			min_block_no,
			max_block_no,
		).await?;
		Ok(result)
	}

	async fn get_latest_block_no(&self) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
		let block = crate::db_model::get_latest_block_no(&self.pool).await?;
		let security_parameter = self.db_sync_block_data_source_config.cardano_security_parameter  as i64 ;
		let block_margin = self.db_sync_block_data_source_config.block_stability_margin as i64;
		Ok(block - (security_parameter + block_margin))
	}
});

#[cfg(test)]
mod tests {
	use super::*;
	use sqlx::Error::PoolTimedOut;

	#[tokio::test]
	async fn display_passwordless_connection_string_on_connection_error() {
		let expected_connection_error = PostgresConnectionError(
			"localhost".to_string(),
			4432,
			"cexplorer_test".to_string(),
			PoolTimedOut.to_string(),
		);
		let test_connection_string = "postgres://postgres:randompsw@localhost:4432/cexplorer_test";
		let actual_connection_error =
			get_connection(test_connection_string, std::time::Duration::from_millis(1)).await;
		assert_eq!(
			expected_connection_error.to_string(),
			actual_connection_error.unwrap_err().to_string()
		);
	}
}
