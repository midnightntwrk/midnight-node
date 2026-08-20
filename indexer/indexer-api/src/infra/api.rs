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

pub mod progress_cache;
pub mod quota;
pub mod v4;

use crate::{
    domain::{Api, LedgerStateCache, storage::Storage},
    infra::api::{
        progress_cache::{ProgressCache, ProgressCacheConfig},
        quota::{PerConnectionCounter, QuotaConfig, SubscriptionQuotas},
        v4::dataloader::{
            BlockByHashLoader, ContractActionsByTransactionIdLoader,
            ContractEventsByContractActionIdLoader, TransactionByIdLoader,
            TransactionsByBlockIdLoader,
        },
    },
};
use async_graphql::{Context, dataloader::DataLoader};
use axum::{
    Router,
    body::Body,
    extract::{OriginalUri, State},
    http::{StatusCode, Uri},
    response::{IntoResponse, Redirect, Response},
    routing::{any, get},
};
use derive_more::Debug;
use fastrace_axum::FastraceLayer;
use indexer_common::{
    domain::{NetworkId, Subscriber},
    error::StdErrorExt,
};
use log::{error, info, warn};
use metrics::{Gauge, gauge};
use serde::Deserialize;
use std::{
    convert::Infallible,
    error::Error as StdError,
    fmt::{self, Display},
    io,
    net::IpAddr,
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use thiserror::Error;
use tokio::{
    net::TcpListener,
    signal::unix::{SignalKind, signal},
};
use tower::ServiceBuilder;
use tower_http::{compression::CompressionLayer, cors::CorsLayer, limit::RequestBodyLimitLayer};

/// Attention: This could change if the used libraries change!
/// See https://docs.rs/http-body-util/0.1.2/src/http_body_util/limited.rs.html#93.
const LENGTH_LIMIT_EXCEEDED_BODY: &[u8] =
    b"Io(Custom { kind: Other, error: \"length limit exceeded\" })";

pub struct AxumApi<S, B> {
    config: Config,
    storage: S,
    subscriber: B,
}

impl<S, B> AxumApi<S, B> {
    pub fn new(config: Config, storage: S, subscriber: B) -> Self {
        Self {
            config,
            storage,
            subscriber,
        }
    }
}

impl<S, B> Api for AxumApi<S, B>
where
    S: Storage,
    B: Subscriber,
{
    type Error = AxumApiError;

    /// Serve the API.
    async fn serve(
        self,
        network_id: NetworkId,
        caught_up: Arc<AtomicBool>,
    ) -> Result<(), Self::Error> {
        let Config {
            address,
            port,
            request_body_limit,
            max_complexity,
            max_depth,
            subscription_config,
            quota_config,
        } = self.config;

        let app = make_app(
            caught_up,
            network_id,
            self.storage,
            self.subscriber,
            request_body_limit as usize,
            max_complexity,
            max_depth,
            subscription_config,
            quota_config,
        );

        let listener = TcpListener::bind((address, port))
            .await
            .map_err(AxumApiError::Bind)?;
        info!(address:?, port; "listening to TCP connections");

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .map_err(AxumApiError::Serve)
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Config {
    pub address: IpAddr,

    pub port: u16,
    #[serde(with = "byte_unit_serde")]
    pub request_body_limit: u64,

    pub max_complexity: usize,

    pub max_depth: usize,

    #[serde(rename = "subscription")]
    pub subscription_config: SubscriptionConfig,

    #[serde(rename = "quota")]
    pub quota_config: QuotaConfig,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct SubscriptionConfig {
    blocks: BlocksSubscriptionConfig,
    contract_actions: ContractActionsSubscriptionConfig,
    contract_events: ContractEventsSubscriptionConfig,
    pub dust_generations: DustGenerationsSubscriptionConfig,
    dust_ledger_events: DustLedgerEventsSubscriptionConfig,
    pub dust_nullifier_transactions: DustNullifierTransactionsSubscriptionConfig,
    progress_cache: ProgressCacheConfig,
    pub shielded_nullifier_transactions: ShieldedNullifierTransactionsSubscriptionConfig,
    shielded_transactions: ShieldedTransactionsSubscriptionConfig,
    unshielded_transactions: UnshieldedTransactionsSubscriptionConfig,
    zswap_ledger_events: ZswapLedgerEventsSubscriptionConfig,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct BlocksSubscriptionConfig {
    batch_size: NonZeroU32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ContractActionsSubscriptionConfig {
    batch_size: NonZeroU32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ContractEventsSubscriptionConfig {
    batch_size: NonZeroU32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct DustGenerationsSubscriptionConfig {
    pub batch_size: NonZeroU32,

    /// Maximum age in blocks of the snapshot `block_hash`; older ones are rejected with a
    /// re-subscribe hint. Must stay well below chain-indexer's ledger_state_retention, which
    /// bounds how long a block's ledger state remains loadable.
    #[serde(default = "max_snapshot_age_default")]
    pub max_snapshot_age: u32,
}

fn max_snapshot_age_default() -> u32 {
    500
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct DustLedgerEventsSubscriptionConfig {
    batch_size: NonZeroU32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct DustNullifierTransactionsSubscriptionConfig {
    pub batch_size: NonZeroU32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ShieldedNullifierTransactionsSubscriptionConfig {
    pub batch_size: NonZeroU32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ShieldedTransactionsSubscriptionConfig {
    batch_size: NonZeroU32,

    #[serde(with = "humantime_serde")]
    progress_update_interval: Duration,

    #[serde(with = "humantime_serde")]
    keep_wallet_alive_interval: Duration,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct UnshieldedTransactionsSubscriptionConfig {
    batch_size: NonZeroU32,

    #[serde(with = "humantime_serde")]
    progress_update_interval: Duration,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ZswapLedgerEventsSubscriptionConfig {
    batch_size: NonZeroU32,
}

#[derive(Debug, Error)]
pub enum AxumApiError {
    #[error("cannot bind tcp listener")]
    Bind(#[source] io::Error),

    #[error("cannot serve API")]
    Serve(#[source] io::Error),
}

/// API related metrics.
struct Metrics {
    /// Number of currently connected wallets via the wallet subscription. Incremented when a
    /// wallet subscription starts, decremented when it ends.
    wallets_connected: Gauge,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            wallets_connected: gauge!("indexer_wallets_connected"),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn make_app<S, B>(
    caught_up: Arc<AtomicBool>,
    network_id: NetworkId,
    storage: S,
    subscriber: B,
    request_body_limit: usize,
    max_complexity: usize,
    max_depth: usize,
    subscription_config: SubscriptionConfig,
    quota_config: QuotaConfig,
) -> Router
where
    S: Storage,
    B: Subscriber,
{
    let ledger_state_cache = LedgerStateCache::default();
    let quotas = SubscriptionQuotas::new(quota_config);
    let progress_cache = ProgressCache::new(subscription_config.progress_cache);

    let v4_app = v4::make_app(
        network_id,
        ledger_state_cache,
        storage,
        subscriber,
        max_complexity,
        max_depth,
        subscription_config,
        quotas,
        progress_cache,
    );

    // For some reason the FastraceLayer and RequestBodyLimitLayer cannot be put into a
    // ServiceBuilder together, so we use `Router::layer` with the inverted (bottom to top) order.
    Router::new()
        .route("/live", get(live))
        .route("/ready", get(ready))
        .nest("/api/v3", v4_app.clone()) // v3 is an alias to v4 for backwards compatibility.
        .nest("/api/v4", v4_app)
        .route("/api/{*rest}", any(redirect_api_to_latest))
        .with_state(caught_up)
        .layer(FastraceLayer::default())
        .layer(
            ServiceBuilder::new()
                .layer(RequestBodyLimitLayer::new(request_body_limit))
                .and_then(transform_lentgh_limit_exceeded),
        )
        .layer(CorsLayer::permissive())
        .layer(CompressionLayer::new())
}

// Returns 200 when reachable. If the runtime is parked (e.g. storage-core deadlock during
// `creating dust generations collapsed update`), this handler does not run; kubelet's
// liveness probe times out and the pod is terminated and recreated.
async fn live() -> impl IntoResponse {
    StatusCode::OK.into_response()
}

async fn ready(State(caught_up): State<Arc<AtomicBool>>) -> impl IntoResponse {
    if !caught_up.load(Ordering::Acquire) {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "indexer has not yet caught up with the node",
        )
            .into_response()
    } else {
        StatusCode::OK.into_response()
    }
}

async fn redirect_api_to_latest(OriginalUri(uri): OriginalUri) -> Response {
    // Avoid redirecting paths already under a known API version, otherwise the redirect prepends
    // `/api/v4` again and the client follows itself into an infinite loop. Unrecognised paths
    // under a known version should 404 like any other unknown route.
    let path = uri.path();
    if path.starts_with("/api/v3/") || path.starts_with("/api/v4/") {
        return StatusCode::NOT_FOUND.into_response();
    }

    redirect_to_latest(uri, "/api").into_response()
}

fn redirect_to_latest(uri: Uri, target: &str) -> Redirect {
    let mut path = uri.path().replacen(target, "/api/v4", 1);

    if let Some(query) = uri.query() {
        path.push('?');
        path.push_str(query);
    }

    Redirect::permanent(&path)
}

/// This is a workaround for async-graphql swallowing `LengthLimitError`s returned by the
/// `RequestBodyLimitLayer` for requests that are too large but do not expose that via the
/// `Content-Length` header which results in responses with status code 400 instead of 413.
async fn transform_lentgh_limit_exceeded(response: Response<Body>) -> Result<Response, Infallible> {
    if response.status() == StatusCode::BAD_REQUEST {
        let (mut head, body) = response.into_parts();

        let Ok(bytes) = axum::body::to_bytes(body, LENGTH_LIMIT_EXCEEDED_BODY.len()).await else {
            warn!("cannot consume response body");
            return Ok(Response::from_parts(head, Body::empty()));
        };

        if &*bytes == LENGTH_LIMIT_EXCEEDED_BODY {
            head.status = StatusCode::PAYLOAD_TOO_LARGE;
            Ok(Response::from_parts(
                head,
                Body::from("length limit exceeded"),
            ))
        } else {
            Ok(Response::from_parts(head, Body::from(bytes)))
        }
    } else {
        Ok::<_, Infallible>(response)
    }
}

async fn shutdown_signal() {
    signal(SignalKind::terminate())
        .expect("SIGTERM handler can be registered")
        .recv()
        .await;
}

trait ContextExt {
    fn get_network_id(&self) -> &NetworkId;

    fn get_storage<S>(&self) -> &S
    where
        S: Storage;

    fn get_block_by_hash_loader<S>(&self) -> &DataLoader<BlockByHashLoader<S>>
    where
        S: Storage;

    fn get_transaction_by_id_loader<S>(&self) -> &DataLoader<TransactionByIdLoader<S>>
    where
        S: Storage;

    fn get_transactions_by_block_id_loader<S>(&self) -> &DataLoader<TransactionsByBlockIdLoader<S>>
    where
        S: Storage;

    fn get_contract_actions_by_transaction_id_loader<S>(
        &self,
    ) -> &DataLoader<ContractActionsByTransactionIdLoader<S>>
    where
        S: Storage;

    fn get_contract_events_by_contract_action_id_loader<S>(
        &self,
    ) -> &DataLoader<ContractEventsByContractActionIdLoader<S>>
    where
        S: Storage;

    fn get_subscriber<B>(&self) -> &B
    where
        B: Subscriber;

    fn get_ledger_state_cache(&self) -> &LedgerStateCache;

    fn get_metrics(&self) -> &Metrics;

    fn get_subscription_config(&self) -> &SubscriptionConfig;

    fn get_subscription_quotas(&self) -> &SubscriptionQuotas;

    fn get_progress_cache(&self) -> &ProgressCache;

    fn get_per_connection_counter(&self) -> &Arc<AtomicUsize>;
}

impl ContextExt for Context<'_> {
    fn get_network_id(&self) -> &NetworkId {
        self.data::<NetworkId>()
            .expect("NetworkId is stored in Context")
    }

    fn get_storage<S>(&self) -> &S
    where
        S: Storage,
    {
        self.data::<S>().expect("Storage is stored in Context")
    }

    fn get_block_by_hash_loader<S>(&self) -> &DataLoader<BlockByHashLoader<S>>
    where
        S: Storage,
    {
        self.data::<DataLoader<BlockByHashLoader<S>>>()
            .expect("BlockByHashLoader is stored in Context")
    }

    fn get_transaction_by_id_loader<S>(&self) -> &DataLoader<TransactionByIdLoader<S>>
    where
        S: Storage,
    {
        self.data::<DataLoader<TransactionByIdLoader<S>>>()
            .expect("TransactionByIdLoader is stored in Context")
    }

    fn get_transactions_by_block_id_loader<S>(&self) -> &DataLoader<TransactionsByBlockIdLoader<S>>
    where
        S: Storage,
    {
        self.data::<DataLoader<TransactionsByBlockIdLoader<S>>>()
            .expect("TransactionsByBlockIdLoader is stored in Context")
    }

    fn get_contract_actions_by_transaction_id_loader<S>(
        &self,
    ) -> &DataLoader<ContractActionsByTransactionIdLoader<S>>
    where
        S: Storage,
    {
        self.data::<DataLoader<ContractActionsByTransactionIdLoader<S>>>()
            .expect("ContractActionsByTransactionIdLoader is stored in Context")
    }

    fn get_contract_events_by_contract_action_id_loader<S>(
        &self,
    ) -> &DataLoader<ContractEventsByContractActionIdLoader<S>>
    where
        S: Storage,
    {
        self.data::<DataLoader<ContractEventsByContractActionIdLoader<S>>>()
            .expect("ContractEventsByContractActionIdLoader is stored in Context")
    }

    fn get_subscriber<B>(&self) -> &B
    where
        B: Subscriber,
    {
        self.data::<B>().expect("Subscriber is stored in Context")
    }

    fn get_ledger_state_cache(&self) -> &LedgerStateCache {
        self.data::<LedgerStateCache>()
            .expect("LedgerStateCache is stored in Context")
    }

    fn get_metrics(&self) -> &Metrics {
        self.data::<Metrics>()
            .expect("Metrics is stored in Context")
    }

    fn get_subscription_config(&self) -> &SubscriptionConfig {
        self.data::<SubscriptionConfig>()
            .expect("SubscriptionConfig is stored in Context")
    }

    fn get_subscription_quotas(&self) -> &SubscriptionQuotas {
        self.data::<SubscriptionQuotas>()
            .expect("SubscriptionQuotas is stored in Context")
    }

    fn get_progress_cache(&self) -> &ProgressCache {
        self.data::<ProgressCache>()
            .expect("ProgressCache is stored in Context")
    }

    fn get_per_connection_counter(&self) -> &Arc<AtomicUsize> {
        &self
            .data::<PerConnectionCounter>()
            .expect("PerConnectionCounter is stored in per-connection Data via on_connection_init")
            .0
    }
}

trait ResultExt<T> {
    fn map_err_into_client_error<S>(self, message: impl Fn() -> S) -> ApiResult<T>
    where
        S: ToString;

    fn map_err_into_server_error<S>(self, message: impl Fn() -> S) -> ApiResult<T>
    where
        S: ToString;
}

impl<T, E> ResultExt<T> for Result<T, E>
where
    E: StdError + Send + Sync + 'static,
{
    fn map_err_into_client_error<S>(self, message: impl Fn() -> S) -> ApiResult<T>
    where
        S: ToString,
    {
        self.map_err(|error| ApiError::client(message(), error))
    }

    fn map_err_into_server_error<S>(self, message: impl Fn() -> S) -> ApiResult<T>
    where
        S: ToString,
    {
        self.map_err(|error| ApiError::server(message(), error))
    }
}

trait OptionExt<T> {
    fn some_or_server_error<S>(self, message: impl Fn() -> S) -> Result<T, ApiError>
    where
        S: ToString;

    fn some_or_client_error<S>(self, message: impl Fn() -> S) -> Result<T, ApiError>
    where
        S: ToString;
}

impl<T> OptionExt<T> for Option<T> {
    fn some_or_server_error<S>(self, message: impl Fn() -> S) -> Result<T, ApiError>
    where
        S: ToString,
    {
        self.ok_or_else(|| ApiError::Server(InnerApiError(message().to_string(), None)))
    }

    fn some_or_client_error<S>(self, message: impl Fn() -> S) -> Result<T, ApiError>
    where
        S: ToString,
    {
        self.ok_or_else(|| ApiError::Client(InnerApiError(message().to_string(), None)))
    }
}

type ApiResult<T> = Result<T, ApiError>;

/// The error type all API handlers must return.
#[derive(Debug, Clone)]
pub enum ApiError {
    /// A client error, caused by invalid input.
    Client(InnerApiError),

    /// An internal server error.
    Server(InnerApiError),
}

impl ApiError {
    fn client(message: impl ToString, error: impl StdError + Send + Sync + 'static) -> Self {
        Self::Client(InnerApiError(message.to_string(), Some(Arc::new(error))))
    }

    fn server(message: impl ToString, error: impl StdError + Send + Sync + 'static) -> Self {
        Self::Server(InnerApiError(message.to_string(), Some(Arc::new(error))))
    }
}

/// For client errors, write the full error chain and for server errors, log the full error chain
/// and write "Internal Server Error".
impl Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Client(error) => write!(f, "{}", error.as_chain()),

            ApiError::Server(error) => {
                error!(error = error.as_chain(); "internal server error");
                write!(f, "Internal Server Error")
            }
        }
    }
}

impl StdError for ApiError {}

#[derive(Debug, Clone, Error)]
#[error("{0}")]
pub struct InnerApiError(String, #[source] Option<Arc<dyn StdError + Send + Sync>>);

#[cfg(test)]
mod tests {
    use crate::infra::api::redirect_api_to_latest;
    use axum::{
        extract::OriginalUri,
        http::{StatusCode, Uri, header},
    };

    /// Regression for #1085, an unrecognised path under `/api/v4` must not redirect to a
    /// `/api/v4/v4/...` URL (which would loop indefinitely). Same guard applies to `/api/v3`.
    #[tokio::test]
    async fn unknown_versioned_paths_return_404() {
        for path in ["/api/v4/schema", "/api/v3/schema"] {
            let uri: Uri = path.parse().unwrap();
            let response = redirect_api_to_latest(OriginalUri(uri)).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    /// Unversioned `/api/...` paths still get the existing redirect to the latest version.
    #[tokio::test]
    async fn unversioned_api_path_redirects_to_latest() {
        let uri: Uri = "/api/foo".parse().unwrap();
        let response = redirect_api_to_latest(OriginalUri(uri)).await;
        assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/api/v4/foo"
        );
    }
}
