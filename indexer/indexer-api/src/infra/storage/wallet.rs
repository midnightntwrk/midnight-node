// This file is part of midnight-indexer.
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

use crate::{domain::storage::wallet::WalletStorage, infra::storage::Storage};
use fastrace::trace;
use futures::TryFutureExt;
use indexer_common::domain::{SessionId, ViewingKey};
use indoc::indoc;
use sqlx::types::{Uuid, time::OffsetDateTime};

impl WalletStorage for Storage {
    #[trace]
    async fn connect_wallet(&self, viewing_key: &ViewingKey) -> Result<(), sqlx::Error> {
        let id = Uuid::now_v7();
        let session_id = viewing_key.to_session_id();
        let viewing_key = viewing_key
            .encrypt(id, &self.cipher)
            .map_err(|error| sqlx::Error::Encode(error.into()))?;

        let query = indoc! {"
            INSERT INTO wallets (
                id,
                session_id,
                viewing_key,
                last_active
            )
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (session_id)
            DO UPDATE SET active = TRUE, last_active = $4
        "};

        sqlx::query(query)
            .bind(id)
            .bind(session_id.as_ref())
            .bind(&viewing_key)
            .bind(OffsetDateTime::now_utc())
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    #[trace(properties = { "session_id": "{session_id}" })]
    async fn disconnect_wallet(&self, session_id: SessionId) -> Result<(), sqlx::Error> {
        let query = indoc! {"
            UPDATE wallets
            SET active = FALSE
            WHERE session_id = $1
        "};

        sqlx::query(query)
            .bind(session_id.as_ref())
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    #[trace(properties = { "session_id": "{session_id}" })]
    async fn set_wallet_active(&self, session_id: SessionId) -> Result<(), sqlx::Error> {
        let query = indoc! {"
            UPDATE wallets
            SET active = TRUE, last_active = $2
            WHERE session_id = $1
        "};

        let result = sqlx::query(query)
            .bind(session_id.as_ref())
            .bind(OffsetDateTime::now_utc())
            .execute(&*self.pool)
            .map_ok(|_| ())
            .await;

        #[cfg(feature = "cloud")]
        let result = result.or_else(|error| {
            indexer_common::infra::sqlx::postgres::ignore_deadlock_detected(error, || ())
        });

        result
    }
}
