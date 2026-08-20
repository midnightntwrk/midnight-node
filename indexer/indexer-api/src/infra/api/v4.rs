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

pub mod block;
pub mod bridge;
pub mod contract;
pub mod contract_action;
pub mod contract_event;
pub mod dataloader;
pub mod directives;
pub mod dust;
pub mod dust_generations;
pub mod ledger_events;
pub mod merkle_tree_collapsed_update;
pub mod mutation;
pub mod query;
pub mod spo;
pub mod subscription;
pub mod system_parameters;
pub mod transaction;
pub mod unshielded;
pub mod viewing_key;
pub mod ws_deflate;

use crate::{
    domain::{
        LedgerStateCache,
        storage::{NoopStorage, Storage},
    },
    infra::api::{
        ApiResult, ContextExt, Metrics, OptionExt, ResultExt, SubscriptionConfig,
        progress_cache::ProgressCache,
        quota::{PerConnectionCounter, SubscriptionQuotas},
        v4::{
            block::BlockOffset,
            dataloader::{
                BlockByHashLoader, ContractActionsByTransactionIdLoader,
                ContractEventsByContractActionIdLoader, TransactionByIdLoader,
                TransactionsByBlockIdLoader,
            },
            mutation::Mutation,
            query::Query,
            subscription::Subscription,
        },
    },
};
use async_graphql::{
    Context, Data, Schema, SchemaBuilder, dataloader::DataLoader, http::ALL_WEBSOCKET_PROTOCOLS,
    scalar,
};
use async_graphql_axum::{GraphQLProtocol, GraphQLRequest, GraphQLResponse, GraphQLWebSocket};
use axum::{
    Extension, Router,
    extract::WebSocketUpgrade,
    response::Response,
    routing::{get, post},
};
use bech32::{Bech32, Bech32m, Hrp};
use const_hex::FromHexError;
use derive_more::{AsRef, Debug, Display};
use indexer_common::domain::{
    ByteArrayLenError, ByteVec, CardanoRewardAddress as DomainCardanoRewardAddress, NetworkId,
    NoopSubscriber, SessionId, Subscriber,
};
use serde::{Deserialize, Serialize};
use std::{
    any::type_name,
    sync::{Arc, atomic::AtomicBool},
};
use thiserror::Error;

/// Wrapper around hex-encoded bytes.
#[derive(Debug, Display, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, AsRef)]
#[debug("{_0}")]
#[as_ref(str)]
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

pub trait HexEncodable
where
    Self: AsRef<[u8]>,
{
    /// Hex-encode these bytes.
    fn hex_encode(&self) -> HexEncoded {
        HexEncoded(const_hex::encode(self.as_ref()))
    }
}

impl<T> HexEncodable for T where T: AsRef<[u8]> {}

/// A Midnight address type.
pub enum AddressType {
    Unshielded,
    SecretEncryptionKey,
    Dust,
}

impl AddressType {
    fn hrp(&self, network_id: &NetworkId) -> String {
        let prefix = self.hrp_prefix();

        if network_id.eq_ignore_ascii_case("mainnet") {
            prefix.to_string()
        } else {
            format!("{prefix}_{network_id}")
        }
    }

    fn hrp_prefix(&self) -> &'static str {
        match self {
            AddressType::Unshielded => "mn_addr",
            AddressType::SecretEncryptionKey => "mn_shield-esk",
            AddressType::Dust => "mn_dust",
        }
    }
}

#[derive(Debug, Error)]
pub enum DecodeAddressError {
    #[error("cannot bech32m-decode address")]
    Decode(#[from] bech32::DecodeError),

    #[error("expected HRP {expected_hrp}, but was {hrp}")]
    InvalidHrp { expected_hrp: String, hrp: String },
}

#[derive(Debug, Error)]
pub enum EncodeAddressError {
    #[error("cannot bech32m-encode address")]
    Encode(#[from] bech32::EncodeError),

    #[error("expected HRP {expected_hrp}, but was {hrp}")]
    InvalidHrp { expected_hrp: String, hrp: String },
}

/// Bech32m-decode the given address string as a byte vector, thereby validate the given address
/// type and the given network ID.
pub fn decode_address(
    address: impl AsRef<str>,
    address_type: AddressType,
    network_id: &NetworkId,
) -> Result<ByteVec, DecodeAddressError> {
    let (hrp, bytes) = bech32::decode(address.as_ref())?;

    let expected_hrp = address_type.hrp(network_id);
    if hrp.as_str() != expected_hrp {
        let hrp = hrp.to_string();
        return Err(DecodeAddressError::InvalidHrp { expected_hrp, hrp });
    }

    Ok(bytes.into())
}

/// Bech32m-encode the given address bytes as a string for the given address type and network ID.
pub fn encode_address(
    address: impl AsRef<[u8]>,
    address_type: AddressType,
    network_id: &NetworkId,
) -> String {
    let hrp = Hrp::parse(&address_type.hrp(network_id)).expect("HRP for address can be parsed");
    bech32::encode::<Bech32m>(hrp, address.as_ref())
        .expect("bytes for unshielded address can be Bech32m-encoded")
}

/// Wrapper around a Bech32-encoded Cardano reward address.
#[derive(Debug, Display, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, AsRef)]
#[debug("{_0}")]
#[as_ref(str)]
pub struct CardanoRewardAddress(String);

scalar!(CardanoRewardAddress);

impl CardanoRewardAddress {
    /// Decode this Bech32 Cardano reward address into a CardanoRewardAddress.
    pub fn decode(&self) -> Result<DomainCardanoRewardAddress, DecodeCardanoRewardAddressError> {
        decode_cardano_reward_address(&self.0)
    }

    /// Decode this Bech32 Cardano reward address, validating it matches the expected network.
    pub fn decode_for_network(
        &self,
        expected_network: CardanoNetworkId,
    ) -> Result<DomainCardanoRewardAddress, DecodeCardanoRewardAddressError> {
        decode_cardano_reward_address_for_network(&self.0, expected_network)
    }
}

impl TryFrom<String> for CardanoRewardAddress {
    type Error = DecodeCardanoRewardAddressError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        // Validate by attempting to decode.
        decode_cardano_reward_address(&s)?;
        Ok(Self(s))
    }
}

impl TryFrom<&str> for CardanoRewardAddress {
    type Error = DecodeCardanoRewardAddressError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.to_owned().try_into()
    }
}

#[derive(Debug, Error)]
pub enum DecodeCardanoRewardAddressError {
    #[error("cannot bech32-decode Cardano reward address")]
    Decode(#[from] bech32::DecodeError),

    #[error("invalid HRP for Cardano reward address: {0}")]
    InvalidHrp(String),

    #[error("invalid Cardano reward address length: expected 29 bytes, was {0}")]
    InvalidLength(usize),

    #[error("wrong Cardano network: expected {expected}, was {actual}")]
    WrongNetwork {
        expected: &'static str,
        actual: &'static str,
    },
}

/// Bech32-decode a Cardano reward address string to a 29-byte CardanoRewardAddress.
/// Supports both mainnet ("stake") and testnet ("stake_test") addresses.
pub fn decode_cardano_reward_address(
    address: impl AsRef<str>,
) -> Result<DomainCardanoRewardAddress, DecodeCardanoRewardAddressError> {
    let (_, reward_address) = decode_cardano_reward_address_with_network(address)?;
    Ok(reward_address)
}

/// Bech32-decode a Cardano reward address string to a 29-byte CardanoRewardAddress,
/// also returning the network ID derived from the HRP.
pub fn decode_cardano_reward_address_with_network(
    address: impl AsRef<str>,
) -> Result<(CardanoNetworkId, DomainCardanoRewardAddress), DecodeCardanoRewardAddressError> {
    let (hrp, bytes) = bech32::decode(address.as_ref())?;

    // Validate HRP is a valid Cardano reward address.
    let network_id = CardanoNetworkId::from_hrp(hrp.as_str())
        .ok_or_else(|| DecodeCardanoRewardAddressError::InvalidHrp(hrp.to_string()))?;

    let reward_address = <[u8; 29]>::try_from(bytes.as_slice())
        .map_err(|_| DecodeCardanoRewardAddressError::InvalidLength(bytes.len()))?;

    Ok((network_id, DomainCardanoRewardAddress::from(reward_address)))
}

/// Bech32-decode a Cardano reward address string, validating that it matches the expected network.
pub fn decode_cardano_reward_address_for_network(
    address: impl AsRef<str>,
    expected_network: CardanoNetworkId,
) -> Result<DomainCardanoRewardAddress, DecodeCardanoRewardAddressError> {
    let (actual_network, reward_address) = decode_cardano_reward_address_with_network(address)?;

    if actual_network != expected_network {
        return Err(DecodeCardanoRewardAddressError::WrongNetwork {
            expected: expected_network.hrp(),
            actual: actual_network.hrp(),
        });
    }

    Ok(reward_address)
}

/// Bech32-encode a 29-byte CardanoRewardAddress to a Cardano reward address string.
pub fn encode_cardano_reward_address(
    reward_address: DomainCardanoRewardAddress,
    network: CardanoNetworkId,
) -> String {
    let hrp = Hrp::parse(network.hrp()).expect("HRP for Cardano reward address can be parsed");
    bech32::encode::<Bech32>(hrp, reward_address.as_ref())
        .expect("bytes for Cardano reward address can be Bech32-encoded")
}

/// Cardano network ID for reward addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardanoNetworkId {
    Mainnet,
    Testnet,
}

impl CardanoNetworkId {
    fn hrp(&self) -> &'static str {
        match self {
            CardanoNetworkId::Mainnet => "stake",
            CardanoNetworkId::Testnet => "stake_test",
        }
    }

    fn from_hrp(hrp: &str) -> Option<Self> {
        match hrp {
            "stake" => Some(CardanoNetworkId::Mainnet),
            "stake_test" => Some(CardanoNetworkId::Testnet),
            _ => None,
        }
    }
}

impl From<&NetworkId> for CardanoNetworkId {
    fn from(network_id: &NetworkId) -> Self {
        if network_id.eq_ignore_ascii_case("mainnet") {
            CardanoNetworkId::Mainnet
        } else {
            CardanoNetworkId::Testnet
        }
    }
}

/// Export the GraphQL schema in SDL format.
pub fn export_schema() -> String {
    // Once traits with async functions are object safe, `NoopStorage` can be replaced with
    // `<Box<dyn Storage>`.
    schema_builder::<NoopStorage, NoopSubscriber>()
        .finish()
        .sdl()
}

#[allow(clippy::too_many_arguments)]
pub fn make_app<S, B>(
    network_id: NetworkId,
    ledger_state_cache: LedgerStateCache,
    storage: S,
    subscriber: B,
    max_complexity: usize,
    max_depth: usize,
    subscription_config: SubscriptionConfig,
    quotas: SubscriptionQuotas,
    progress_cache: ProgressCache,
) -> Router<Arc<AtomicBool>>
where
    S: Storage,
    B: Subscriber,
{
    let metrics = Metrics::default();

    let schema = schema_builder::<S, B>()
        .data(network_id)
        .data(ledger_state_cache)
        .data(DataLoader::new(
            BlockByHashLoader::new(storage.clone()),
            tokio::spawn,
        ))
        .data(DataLoader::new(
            TransactionByIdLoader::new(storage.clone()),
            tokio::spawn,
        ))
        .data(DataLoader::new(
            TransactionsByBlockIdLoader::new(storage.clone()),
            tokio::spawn,
        ))
        .data(DataLoader::new(
            ContractActionsByTransactionIdLoader::new(storage.clone()),
            tokio::spawn,
        ))
        .data(DataLoader::new(
            ContractEventsByContractActionIdLoader::new(storage.clone()),
            tokio::spawn,
        ))
        .data(storage)
        .data(subscriber)
        .data(metrics)
        .data(subscription_config)
        .data(quotas)
        .data(progress_cache)
        .limit_complexity(max_complexity)
        .limit_depth(max_depth)
        .limit_recursive_depth(max_depth)
        .finish();

    Router::new()
        .route("/graphql", post(graphql_no_batch::<S, B>))
        .route("/graphql/ws", get(graphql_ws::<S, B>))
        .layer(Extension(schema))
}

/// Custom WebSocket handler that wires `on_connection_init` to attach a [`PerConnectionCounter`]
/// to each connection's async-graphql `Data`. The default `GraphQLSubscription` axum service does
/// not expose `on_connection_init`, so we open the WebSocket directly via `GraphQLWebSocket`.
///
/// In addition to the standard GraphQL WebSocket subprotocols this handler offers the opt-in
/// [`ws_deflate::GRAPHQL_TRANSPORT_WS_DEFLATE`] subprotocol: clients selecting it exchange
/// zlib-compressed payloads as binary frames (see [`ws_deflate`] for the wire format), all other
/// clients are byte-for-byte unaffected.
#[allow(clippy::type_complexity)]
async fn graphql_ws<S, B>(
    Extension(schema): Extension<Schema<Query<S>, Mutation<S>, Subscription<S, B>>>,
    protocol: GraphQLProtocol,
    upgrade: WebSocketUpgrade,
) -> Response
where
    S: Storage,
    B: Subscriber,
{
    upgrade
        .protocols(
            std::iter::once(ws_deflate::GRAPHQL_TRANSPORT_WS_DEFLATE)
                .chain(ALL_WEBSOCKET_PROTOCOLS.iter().copied()),
        )
        .on_upgrade(move |stream| async move {
            let deflate = stream.protocol().is_some_and(|p| {
                p.as_bytes() == ws_deflate::GRAPHQL_TRANSPORT_WS_DEFLATE.as_bytes()
            });

            if deflate {
                serve_graphql_ws(ws_deflate::DeflateWebSocket::new(stream), schema, protocol).await
            } else {
                serve_graphql_ws(stream, schema, protocol).await
            }
        })
}

/// Runs the GraphQL-over-WebSocket protocol on the given (possibly compression-wrapped) socket,
/// attaching a fresh [`PerConnectionCounter`] on connection init.
async fn serve_graphql_ws<St, S, B>(
    stream: St,
    schema: Schema<Query<S>, Mutation<S>, Subscription<S, B>>,
    protocol: GraphQLProtocol,
) where
    St: futures::Stream<Item = Result<axum::extract::ws::Message, axum::Error>>
        + futures::Sink<axum::extract::ws::Message, Error = axum::Error>,
    S: Storage,
    B: Subscriber,
{
    GraphQLWebSocket::new(stream, schema, protocol)
        .on_connection_init(|_payload: serde_json::Value| async move {
            let mut data = Data::default();
            data.insert(PerConnectionCounter::default());
            Ok(data)
        })
        .serve()
        .await
}

// This prevents batch requests, because `GraphQLRequest` only accepts single requests.
#[allow(clippy::type_complexity)]
async fn graphql_no_batch<S, B>(
    Extension(schema): Extension<Schema<Query<S>, Mutation<S>, Subscription<S, B>>>,
    request: GraphQLRequest,
) -> GraphQLResponse
where
    S: Storage,
    B: Subscriber,
{
    schema.execute(request.into_inner()).await.into()
}

fn schema_builder<S, B>() -> SchemaBuilder<Query<S>, Mutation<S>, Subscription<S, B>>
where
    S: Storage,
    B: Subscriber,
{
    Schema::build(
        Query::<S>::default(),
        Mutation::<S>::default(),
        Subscription::<S, B>::default(),
    )
    .extension(async_graphql::extensions::Tracing)
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
/// simple, if it is a hash, the block is loaded via the DataLoader and its height returned. If the
/// block offset is omitted, the last block is loaded and its height returned.
async fn resolve_height<S: Storage>(
    offset: Option<BlockOffset>,
    cx: &Context<'_>,
) -> ApiResult<u32> {
    match offset {
        Some(offset) => match offset {
            BlockOffset::Hash(hash) => {
                let hash = hash
                    .hex_decode()
                    .map_err_into_client_error(|| "invalid block hash")?;

                let block = cx
                    .get_block_by_hash_loader::<S>()
                    .load_one(hash)
                    .await
                    .map_err_into_server_error(|| format!("get block by hash {hash}"))?
                    .some_or_client_error(|| format!("block with hash {hash} not found"))?;

                Ok(block.height)
            }

            BlockOffset::Height(height) => {
                cx.get_storage::<S>()
                    .get_block_by_height(height)
                    .await
                    .map_err_into_server_error(|| "get block by height")?
                    .some_or_client_error(|| format!("block with height {height} not found"))?;

                Ok(height)
            }
        },

        None => {
            let latest_block = cx
                .get_storage::<S>()
                .get_latest_block()
                .await
                .map_err_into_server_error(|| "get latest block")?;
            let height = latest_block.map(|block| block.height).unwrap_or_default();

            Ok(height)
        }
    }
}
