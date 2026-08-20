// This file is part of midnight-indexer.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{
    domain::{
        storage::system_parameters::SystemParametersStorage,
        system_parameters::{DParameter, TermsAndConditions},
    },
    infra::storage::Storage,
};
use fastrace::trace;
use indoc::indoc;

impl SystemParametersStorage for Storage {
    #[trace(properties = { "block_height": "{block_height}" })]
    async fn get_terms_and_conditions_at(
        &self,
        block_height: u32,
    ) -> Result<Option<TermsAndConditions>, sqlx::Error> {
        let query = indoc! {"
            SELECT
                block_height,
                block_hash,
                timestamp,
                hash,
                url
            FROM system_parameters_terms_and_conditions
            WHERE block_height <= $1
            ORDER BY block_height DESC
            LIMIT 1
        "};

        sqlx::query_as::<_, TermsAndConditions>(query)
            .bind(block_height as i64)
            .fetch_optional(&*self.pool)
            .await
    }

    #[trace(properties = { "block_height": "{block_height}" })]
    async fn get_d_parameter_at(
        &self,
        block_height: u32,
    ) -> Result<Option<DParameter>, sqlx::Error> {
        let query = indoc! {"
            SELECT
                block_height,
                block_hash,
                timestamp,
                num_permissioned_candidates,
                num_registered_candidates
            FROM system_parameters_d
            WHERE block_height <= $1
            ORDER BY block_height DESC
            LIMIT 1
        "};

        sqlx::query_as::<_, DParameter>(query)
            .bind(block_height as i64)
            .fetch_optional(&*self.pool)
            .await
    }

    #[trace]
    async fn get_terms_and_conditions_history(
        &self,
    ) -> Result<Vec<TermsAndConditions>, sqlx::Error> {
        let query = indoc! {"
            SELECT
                block_height,
                block_hash,
                timestamp,
                hash,
                url
            FROM system_parameters_terms_and_conditions
            ORDER BY block_height DESC
        "};

        sqlx::query_as::<_, TermsAndConditions>(query)
            .fetch_all(&*self.pool)
            .await
    }

    #[trace]
    async fn get_d_parameter_history(&self) -> Result<Vec<DParameter>, sqlx::Error> {
        let query = indoc! {"
            SELECT
                block_height,
                block_hash,
                timestamp,
                num_permissioned_candidates,
                num_registered_candidates
            FROM system_parameters_d
            ORDER BY block_height DESC
        "};

        sqlx::query_as::<_, DParameter>(query)
            .fetch_all(&*self.pool)
            .await
    }
}
