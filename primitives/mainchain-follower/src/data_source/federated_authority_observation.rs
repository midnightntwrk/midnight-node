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

use crate::FederatedAuthorityObservationDataSource;
use derive_new::new;
use midnight_primitives_federated_authority_observation::FederatedAuthorityData;
use partner_chains_db_sync_data_sources::McFollowerMetrics;
use sidechain_domain::McBlockHash;
pub use sqlx::PgPool;

#[derive(new)]
pub struct FederatedAuthorityObservationDataSourceImpl {
	pub pool: PgPool,
	pub metrics_opt: Option<McFollowerMetrics>,
	#[allow(dead_code)]
	cache_size: u16,
}

#[async_trait::async_trait]
impl FederatedAuthorityObservationDataSource for FederatedAuthorityObservationDataSourceImpl {
	async fn get_federated_authority_data(
		&self,
		mc_block_hash: &McBlockHash,
	) -> Result<FederatedAuthorityData, Box<dyn std::error::Error + Send + Sync>> {
		// TODO: federated-authority-observation
		// Replaced when queried from Cardano

		Ok(FederatedAuthorityData {
			council_authorities: vec![],
			technical_committee_authorities: vec![],
			mc_block_hash: mc_block_hash.clone(),
		})
	}
}

impl FederatedAuthorityObservationDataSourceImpl {
	// TODO: federated-authority-observation
}
