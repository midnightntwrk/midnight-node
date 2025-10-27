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
//!
//! This module uses the types and functions provided by the `db` module

pub mod cnight_observation;
pub mod cnight_observation_mock;
pub mod federated_authority_observation;
pub mod federated_authority_observation_mock;

pub use cnight_observation::{
	MidnightCNightObservationDataSourceError, MidnightCNightObservationDataSourceImpl, TxHash,
	TxPosition,
};
pub use cnight_observation_mock::CNightObservationDataSourceMock;
pub use federated_authority_observation::FederatedAuthorityObservationDataSourceImpl;
pub use federated_authority_observation_mock::FederatedAuthorityObservationDataSourceMock;

pub use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::{error::Error, str::FromStr};

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
