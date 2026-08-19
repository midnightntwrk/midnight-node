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

//! Data sources implementations that read from db-sync postgres.
//!
//! This module uses the types and functions provided by the `db` module

pub mod candidates_data_source;
pub mod cnight_observation;
pub mod cnight_observation_mock;
pub mod federated_authority_observation;
pub mod federated_authority_observation_mock;
pub mod metrics;

pub use candidates_data_source::CandidatesDataSourceImpl;
pub use candidates_data_source::cached::CandidateDataSourceCached;
pub use candidates_data_source::get_epoch_for_block_hash;
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
			log::debug!(
				"Database connection details: host={}, port={}, database={}; error: {e}",
				connect_options.get_host(),
				connect_options.get_port(),
				connect_options.get_database().unwrap_or("cexplorer"),
			);
			PostgresConnectionError(e.to_string())
		})?;
	Ok(pool)
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("Could not connect to database; error: {0}")]
struct PostgresConnectionError(String);

#[cfg(test)]
mod tests {
	use super::*;
	use sqlx::Error::PoolTimedOut;

	#[tokio::test]
	async fn connection_error_redacts_host_port_and_database() {
		let test_connection_string = "postgres://postgres:randompsw@localhost:4432/cexplorer_test";
		let actual_connection_error =
			get_connection(test_connection_string, std::time::Duration::from_millis(1)).await;
		let error_message = actual_connection_error.unwrap_err().to_string();

		let expected = PostgresConnectionError(PoolTimedOut.to_string()).to_string();
		assert_eq!(expected, error_message);

		assert!(!error_message.contains("localhost"), "error must not contain host");
		assert!(!error_message.contains("4432"), "error must not contain port");
		assert!(!error_message.contains("cexplorer_test"), "error must not contain database name");
		assert!(!error_message.contains("randompsw"), "error must not contain password");
		assert!(
			error_message.contains("Could not connect to database"),
			"error must indicate connection failure"
		);
	}
}
