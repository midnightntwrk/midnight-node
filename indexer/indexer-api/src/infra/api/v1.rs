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

pub mod block;
pub mod contract_action;
pub mod mutation;
pub mod query;
pub mod subscription;
pub mod transaction;
pub mod unshielded;
pub mod viewing_key;

use crate::{
    domain::{
        LedgerStateCache,
        storage::{NoopStorage, Storage},
    },
    infra::api::{
        ApiResult, Metrics, OptionExt, ResultExt,
        v1::{block::BlockOffset, mutation::Mutation, query::Query, subscription::Subscription},
    },
};
use async_graphql::{Schema, SchemaBuilder, scalar};
use async_graphql_axum::{GraphQL, GraphQLSubscription};
use axum::{Router, routing::post_service};
use const_hex::FromHexError;
use derive_more::{Debug, Display};
use indexer_common::domain::{
    ByteArrayLenError, ByteVec, LedgerStateStorage, NetworkId, NoopLedgerStateStorage,
    NoopSubscriber, SessionId, Subscriber,
};
use log::error;
use serde::{Deserialize, Serialize};
use std::{
    any::type_name,
    sync::{Arc, atomic::AtomicBool},
};
use thiserror::Error;

/// Wrapper around hex-encoded bytes.
#[derive(Debug, Display, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[debug("{_0}")]
pub struct HexEncoded(String);

scalar!(HexEncoded);

impl HexEncoded {
    /// Hex-decode this [HexEncoded] into some type that can be made from bytes.
    pub fn hex_decode<T>(&self) -> Result<T, HexDecodeError>
    where
        T: TryFrom<ByteVec>,
    {
        let bytes = ByteVec::from(const_hex::decode(&self.0)?);
        let decoded = bytes
            .try_into()
            .map_err(|_| HexDecodeError::Convert(type_name::<T>()))?;
        Ok(decoded)
    }
}

#[derive(Debug, Error)]
pub enum HexDecodeError {
    #[error("cannot hex-decode")]
    Decode(#[from] FromHexError),

    #[error("cannot convert to {0}")]
    Convert(&'static str),
}

// Needed to derive `Interface` for `ContractAction`. Weird!
impl From<&HexEncoded> for HexEncoded {
    fn from(value: &HexEncoded) -> Self {
        value.to_owned()
    }
}

impl TryFrom<String> for HexEncoded {
    type Error = FromHexError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        const_hex::decode(&s)?;
        Ok(Self(s))
    }
}

impl TryFrom<&str> for HexEncoded {
    type Error = FromHexError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        const_hex::decode(s)?;
        Ok(Self(s.to_owned()))
    }
}

pub trait AsBytesExt
where
    Self: AsRef<[u8]>,
{
    /// Hex-encode these bytes.
    fn hex_encode(&self) -> HexEncoded {
        HexEncoded(const_hex::encode(self.as_ref()))
    }
}

impl<T> AsBytesExt for T where T: AsRef<[u8]> {}

/// Export the GraphQL schema in SDL format.
pub fn export_schema() -> String {
    // Once traits with async functions are object safe, `NoopStorage` can be replaced with
    // `<Box<dyn Storage>`.
    schema_builder::<NoopStorage, NoopSubscriber, NoopLedgerStateStorage>()
        .finish()
        .sdl()
}

pub fn make_app<S, B, Z>(
    network_id: NetworkId,
    zswap_state_cache: LedgerStateCache,
    storage: S,
    ledger_state_storage: Z,
    subscriber: B,
    max_complexity: usize,
    max_depth: usize,
) -> Router<Arc<AtomicBool>>
where
    S: Storage,
    B: Subscriber,
    Z: LedgerStateStorage,
{
    let metrics = Metrics::default();

    let schema = schema_builder::<S, B, Z>()
        .data(network_id)
        .data(zswap_state_cache)
        .data(storage)
        .data(ledger_state_storage)
        .data(subscriber)
        .data(metrics)
        .limit_complexity(max_complexity)
        .limit_depth(max_depth)
        .limit_recursive_depth(max_depth)
        .finish();

    Router::new()
        .route("/graphql", post_service(GraphQL::new(schema.clone())))
        .route_service("/graphql/ws", GraphQLSubscription::new(schema))
}

fn schema_builder<S, B, Z>() -> SchemaBuilder<Query<S>, Mutation<S>, Subscription<S, B, Z>>
where
    S: Storage,
    B: Subscriber,
    Z: LedgerStateStorage,
{
    Schema::build(
        Query::<S>::default(),
        Mutation::<S>::default(),
        Subscription::<S, B, Z>::default(),
    )
}

fn decode_session_id(session_id: HexEncoded) -> Result<SessionId, DecodeSessionIdError> {
    let session_id = session_id.hex_decode::<Vec<u8>>()?;
    let session_id = SessionId::try_from(session_id.as_slice())?;
    Ok(session_id)
}

#[derive(Debug, Error)]
enum DecodeSessionIdError {
    #[error("cannot hex-decode session ID")]
    HexDecode(#[from] HexDecodeError),

    #[error("cannot convert into session ID")]
    ByteArrayLen(#[from] ByteArrayLenError),
}

/// Resolve the block height for the given optional block offset. If it is a block height, it is
/// simple, if it is a hash, the block is loaded and its height returned. If the block offset is
/// omitted, the last block is loaded and its height returned.
async fn resolve_height(offset: Option<BlockOffset>, storage: &impl Storage) -> ApiResult<u32> {
    match offset {
        Some(offset) => match offset {
            BlockOffset::Hash(hash) => {
                let hash = hash
                    .hex_decode()
                    .map_err_into_client_error(|| "invalid block hash")?;

                let block = storage
                    .get_block_by_hash(hash)
                    .await
                    .map_err_into_server_error(|| format!("get block by hash {hash}"))?
                    .ok_or_client_error(|| format!("block with hash {hash} not found"))?;

                Ok(block.height)
            }

            BlockOffset::Height(height) => {
                storage
                    .get_block_by_height(height)
                    .await
                    .map_err_into_server_error(|| "get block by height")?
                    .ok_or_client_error(|| format!("block with height {height} not found"))?;

                Ok(height)
            }
        },

        None => {
            let latest_block = storage
                .get_latest_block()
                .await
                .map_err_into_server_error(|| "get latest block")?;
            let height = latest_block.map(|block| block.height).unwrap_or_default();

            Ok(height)
        }
    }
}
