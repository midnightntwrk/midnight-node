// This file is part of midnight-indexer.
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

use crate::{
    domain::storage::Storage,
    infra::api::{
        ApiResult, ContextExt, OptionExt, ResultExt,
        v4::{HexEncodable, HexEncoded, decode_session_id, viewing_key::ViewingKey},
    },
};
use async_graphql::{Context, InputObject, Object, scalar};
use fastrace::trace;
use log::debug;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

pub struct Mutation<S> {
    _s: PhantomData<S>,
}

impl<S> Default for Mutation<S> {
    fn default() -> Self {
        Self { _s: PhantomData }
    }
}

#[Object]
impl<S> Mutation<S>
where
    S: Storage,
{
    /// Connect the wallet with the given viewing key and return a session ID.
    #[trace]
    async fn connect(
        &self,
        cx: &Context<'_>,
        viewing_key: ViewingKey,
        options: Option<ConnectOptions>,
    ) -> ApiResult<HexEncoded> {
        let viewing_key = viewing_key
            .try_into_domain(cx.get_network_id())
            .map_err_into_client_error(|| "invalid viewing key")?;

        let start_index = options
            .and_then(|o| o.start_index)
            .map(|i| {
                u64::try_from(i)
                    .ok()
                    .some_or_client_error(|| "startIndex must not be negative")
            })
            .transpose()?;

        let storage = cx.get_storage::<S>();

        let session_id = storage
            .connect_wallet(&viewing_key, start_index)
            .await
            .map_err_into_server_error(|| "connect wallet")?;

        let wallet_id = storage
            .resolve_session_id(session_id)
            .await
            .map_err_into_server_error(|| "resolve session ID")?
            .some_or_client_error(|| "unknown or expired session ID")?;

        debug!(wallet_id:%; "wallet connected");

        Ok(session_id.hex_encode())
    }

    /// Disconnect the wallet with the given session ID.
    #[trace]
    async fn disconnect(&self, cx: &Context<'_>, session_id: HexEncoded) -> ApiResult<Unit> {
        let session_id =
            decode_session_id(session_id).map_err_into_client_error(|| "invalid session ID")?;

        let storage = cx.get_storage::<S>();

        let wallet_id = storage
            .resolve_session_id(session_id)
            .await
            .map_err_into_server_error(|| "resolve session ID")?
            .some_or_client_error(|| "unknown or expired session ID")?;

        storage
            .disconnect_wallet(session_id)
            .await
            .map_err_into_server_error(|| "disconnect wallet")?;

        debug!(wallet_id:%; "wallet disconnected");

        Ok(Unit)
    }
}

/// Options for the connect mutation.
#[derive(Debug, Clone, InputObject)]
pub struct ConnectOptions {
    /// Transaction index to start searching for relevant transactions (inclusive).
    start_index: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit;

scalar!(Unit);
