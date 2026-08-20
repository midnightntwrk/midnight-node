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

mod header;
mod runtimes;

use crate::{
    domain::{
        BlockRef, SystemParametersChange,
        node::{Block, Node, RegularTransaction, SystemTransaction, Transaction},
    },
    infra::subxt_node::{header::SubstrateHeaderExt, runtimes::BlockDetails},
};
use async_stream::try_stream;
use const_hex::FromHexError;
use fastrace::trace;
use futures::{Stream, StreamExt, TryStreamExt, stream};
use http::{
    HeaderMap,
    header::{InvalidHeaderValue, USER_AGENT},
};
use indexer_common::{
    domain::{
        BlockAuthor, BlockHash, ByteVec, NodeVersion, ProtocolVersion, ProtocolVersionError,
        SerializedContractAddress,
        ledger::{self, ZswapMerkleTreeRoot},
    },
    error::BoxError,
};
use log::{debug, info, warn};
use parity_scale_codec::Decode;
use serde::Deserialize;
use std::{future::ready, pin::pin, time::Duration};
use subxt::{
    OnlineClient, SubstrateConfig,
    config::{
        Hash, RpcConfigFor,
        substrate::{ConsensusEngineId, DigestItem, SubstrateHeader},
    },
    rpcs::{
        LegacyRpcMethods,
        client::{ReconnectingRpcClient, reconnecting_rpc_client::ExponentialBackoff},
    },
    utils::H256,
};
use thiserror::Error;
use tokio::time::timeout;

type OnlineClientAtBlock = subxt::client::OnlineClientAtBlock<SubstrateConfig>;
type SubxtBlock = subxt::client::Block<SubstrateConfig>;

/// Where to decode a block's content (extrinsics + events) from. At a runtime-upgrade
/// enactment block the block's own metadata is the new runtime's, but its content was produced
/// by the old runtime; `client` is the parent (old-runtime) at-block client and `extrinsic_bodies`
/// are THIS block's raw extrinsic bytes, so content decodes against the old metadata. Raw bytes
/// are metadata-independent, so feeding this block's bytes to the parent client decodes the
/// content the old runtime actually produced.
pub(crate) struct ContentSource {
    pub(crate) client: OnlineClientAtBlock,
    pub(crate) extrinsic_bodies: Vec<Vec<u8>>,
}

/// A block with everything fetched except the author, which depends on the sequentially
/// maintained authority cache. Produced by [SubxtNode::make_raw_block], possibly concurrently
/// for multiple blocks, and completed in block order by [finish_block].
struct RawBlock {
    client: OnlineClientAtBlock,
    header: SubstrateHeader<H256>,
    state_node_version: NodeVersion,
    content_node_version: NodeVersion,
    babe_supported: bool,
    new_session: bool,
    block: Block,
}

/// Resolve the block author against the sequential authority cache and apply this block's
/// session change to the cache. Must be called in block order.
async fn finish_block(
    authorities: &mut Option<Vec<[u8; 32]>>,
    raw_block: RawBlock,
) -> Result<Block, SubxtNodeError> {
    let RawBlock {
        client,
        header,
        state_node_version,
        content_node_version,
        babe_supported,
        new_session,
        mut block,
    } = raw_block;

    // Fetch authorities if `None`, either initially or because of a `NewSession` event in the
    // previous block.
    if authorities.is_none() {
        *authorities = Some(runtimes::fetch_authorities(state_node_version, &client).await?);
    }
    block.author = authorities
        .as_ref()
        .map(|authorities| {
            extract_block_author(&header, authorities, content_node_version, babe_supported)
        })
        .transpose()?
        .flatten();

    if new_session {
        *authorities = None;
    }

    debug!(
        hash:% = block.hash,
        height = block.height,
        parent_hash:% = block.parent_hash,
        transactions_len = block.transactions.len();
        "block made"
    );

    Ok(block)
}

const AURA_ENGINE_ID: ConsensusEngineId = [b'a', b'u', b'r', b'a'];
const BABE_ENGINE_ID: ConsensusEngineId = [b'B', b'A', b'B', b'E'];

/// Name of the node runtime API reporting the active block-production engine, declared in
/// `midnight-primitives-consensus-engine` and implemented alongside the pallet driving the
/// Aura→BABE transition. Its presence in a block's runtime guarantees the correctness of Aura
/// and BABE pre-runtime digests during the transition, so BABE digests are only trusted for
/// author derivation where it exists.
const CONSENSUS_ENGINE_RUNTIME_API: &str = "ConsensusEngineApi";
const CATCH_UP_LOG_INTERVAL: u64 = 1_000;

/// One GRANDPA session worth of blocks. Blocks within this distance of the finalized tip are
/// fetched by hash (backward traversal) to avoid any risk of ingesting non-canonical blocks.
/// Blocks further back are fetched by height with parent hash verification.
const FINALIZATION_SAFETY_MARGIN: u64 = 400;

/// A [Node] implementation based on subxt.
#[derive(Clone)]
pub struct SubxtNode {
    rpc_client: ReconnectingRpcClient,
    online_client: OnlineClient<SubstrateConfig>,
    subscription_recovery_timeout: Duration,
    fetch_concurrency: usize,
}

impl SubxtNode {
    /// Create a new [SubxtNode] with the given [Config].
    pub async fn new(config: Config) -> Result<Self, Error> {
        let Config {
            url,
            reconnect_max_delay: retry_max_delay,
            reconnect_max_attempts: retry_max_attempts,
            subscription_recovery_timeout,
            fetch_concurrency,
        } = config;

        let retry_policy = ExponentialBackoff::from_millis(10)
            .max_delay(retry_max_delay)
            .take(retry_max_attempts);
        let user_agent = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")).parse()?;
        let headers = HeaderMap::from_iter([(USER_AGENT, user_agent)]);
        let rpc_client = ReconnectingRpcClient::builder()
            .set_headers(headers)
            .retry_policy(retry_policy)
            .build(&url)
            .await
            .map_err(|error| Error::RpcClient(error.into()))?;

        let online_client =
            OnlineClient::<SubstrateConfig>::from_rpc_client(rpc_client.clone()).await?;

        Ok(Self {
            rpc_client,
            online_client,
            subscription_recovery_timeout,
            fetch_concurrency,
        })
    }

    /// Subscribe to finalized blocks, filtering duplicates and disconnection errors.
    /// Subxt with its reconnecting-rpc-client feature exposes the error case, i.e. yields one `Err`
    /// item, then reconnects and continues with `Ok` items. Therefore we filter out the respective
    /// `Err` item; all other errors need to be propagated as is.
    ///
    /// The `last_height` parameter allows the caller to pass in the last successfully processed
    /// block height, which is used to properly filter duplicates after re-subscribing.
    async fn subscribe_finalized_blocks(
        &self,
        mut last_height: Option<u64>,
    ) -> Result<impl Stream<Item = Result<SubxtBlock, SubxtNodeError>> + use<>, SubxtNodeError>
    {
        let finalized_blocks = self
            .online_client
            .stream_blocks()
            .await
            .map_err(|error| SubxtNodeError::SubscribeFinalizedBlocks(error.into()))?
            .filter(move |block| {
                let pass = match block {
                    Ok(block) => {
                        let height = block.number();

                        if Some(height) <= last_height {
                            warn!(
                                hash:% = block.hash(),
                                height = block.number(),
                                last_height:?;
                                "received duplicate, possibly after reconnect"
                            );
                            false
                        } else {
                            last_height = Some(height);
                            true
                        }
                    }

                    // Filter out reconnect errors; see method comment above.
                    Err(subxt::error::BlocksError::CannotGetBlockHeader(
                        subxt::error::BackendError::Rpc(subxt::error::RpcError::ClientError(
                            subxt::rpcs::Error::DisconnectedWillReconnect(_),
                        )),
                    )) => {
                        warn!("node disconnected, reconnecting");
                        false
                    }

                    _ => true,
                };

                ready(pass)
            })
            .map_err(|error| SubxtNodeError::ReceiveBlock(error.into()));

        Ok(finalized_blocks)
    }

    async fn make_block(
        &self,
        authorities: &mut Option<Vec<[u8; 32]>>,
        block: OnlineClientAtBlock,
    ) -> Result<Block, SubxtNodeError> {
        let raw_block = self.make_raw_block(block).await?;
        finish_block(authorities, raw_block).await
    }

    /// Fetch and assemble everything of a block that does not depend on the sequential
    /// authority cache, so it can run concurrently for multiple blocks. [finish_block]
    /// resolves the author and applies session changes in block order.
    async fn make_raw_block(&self, block: OnlineClientAtBlock) -> Result<RawBlock, SubxtNodeError> {
        let hash = block.block_hash().0.into();
        let height = block.block_number();
        let header = block_header(&block).await?;
        let parent_hash = header.parent_hash.0.into();
        let protocol_version = header
            .protocol_version()?
            .ok_or(SubxtNodeError::MissingProtocolVersionHeader)?;
        // Two runtime versions are in play at a runtime-upgrade enactment block, and every call
        // below must pick the one matching what it touches:
        //
        // - `content_node_version` decodes bytes produced by the runtime that BUILT this block, as
        //   recorded in the MNSV digest: extrinsics, events and header digests.
        // - `state_node_version` addresses the runtime present in this block's STATE. At an
        //   enactment block `set_code` landed inside this very block, so every RPC at this hash
        //   (runtime API, storage, metadata) already executes against the next runtime, whose
        //   version is therefore newer than the MNSV digest's.
        //
        // Away from enactment blocks the two are equal; getting the pairing wrong there is
        // invisible, which is exactly why each call site names the version it needs.
        let content_node_version = protocol_version.node_version();
        let state_node_version = ProtocolVersion::try_from(block.spec_version())?.node_version();
        let ledger_version = protocol_version.ledger_version();

        if content_node_version != state_node_version {
            info!(
                hash:%,
                height,
                content_node_version:%,
                state_node_version:%;
                "runtime upgrade enacted in this block; block contents and block state are on \
                 different runtimes"
            );
        }

        debug!(
            hash:%,
            height,
            parent_hash:%,
            protocol_version:?,
            content_node_version:%,
            state_node_version:%,
            ledger_version:%;
            "making block"
        );

        // The state metadata can only be newer than the runtime that authored the
        // block, so this can never enable BABE recognition too late.
        let babe_supported = block
            .metadata_ref()
            .runtime_api_trait_by_name(CONSENSUS_ENGINE_RUNTIME_API)
            .is_some();

        // The node's ledger-9 host API detects the v8 StateKey at the 8->9 enactment block and
        // dispatches this read to the v8 bridge. The MNSV protocol version remains the right
        // decoder here: that block's committed ledger state is still v8 until apply+1.
        let zswap_merkle_tree_root =
            runtimes::get_zswap_merkle_tree_root(state_node_version, &block).await?;
        let zswap_merkle_tree_root =
            ZswapMerkleTreeRoot::deserialize(zswap_merkle_tree_root, ledger_version)?;

        // At a runtime-upgrade enactment block the block reports the next runtime, so subxt binds
        // it to the next runtime's metadata even though its extrinsics and events were produced by
        // the previous runtime. Decode the content against the parent block's client (which carries
        // the previous runtime's metadata), fed this block's raw, metadata-independent bytes. Away
        // from enactment blocks the two runtimes are equal and no parent client is needed.
        let content_source = if content_node_version != state_node_version {
            let parent = block
                .online_client()
                .at_block(header.parent_hash)
                .await
                .map_err(|error| {
                    SubxtNodeError::GetOnlineClientAt(header.parent_hash, error.into())
                })?;
            let legacy_rpc_methods = LegacyRpcMethods::<RpcConfigFor<SubstrateConfig>>::new(
                self.rpc_client.to_owned().into(),
            );
            let extrinsic_bodies = legacy_rpc_methods
                .chain_get_block(Some(block.block_hash()))
                .await
                .map_err(SubxtNodeError::FetchBlockBody)?
                .ok_or(SubxtNodeError::BlockBodyNotFound)?
                .block
                .extrinsics
                .into_iter()
                .map(|bytes| bytes.0)
                .collect::<Vec<_>>();
            Some(ContentSource {
                client: parent,
                extrinsic_bodies,
            })
        } else {
            None
        };

        let BlockDetails {
            timestamp,
            new_session,
            transactions,
            mut dust_registration_events,
            bridge_events,
        } = runtimes::make_block_details(content_node_version, &block, content_source.as_ref())
            .await?;

        // At genesis, Substrate does not emit events (Parity PR #5463). Fetch cNight
        // registrations from pallet storage instead.
        // Also fetch the ledger state root for genesis ledger state detection.
        let ledger_state_root = if height == 0 {
            let genesis_registrations =
                runtimes::fetch_genesis_cnight_registrations(state_node_version, &block).await?;
            dust_registration_events.extend(genesis_registrations);

            runtimes::get_ledger_state_root(state_node_version, &block)
                .await?
                .map(Into::into)
        } else {
            None
        };

        let transactions = stream::iter(transactions)
            .then(|t| make_transaction(t, protocol_version, state_node_version, &block))
            .try_collect::<Vec<_>>()
            .await?;

        Ok(RawBlock {
            header,
            state_node_version,
            content_node_version,
            babe_supported,
            new_session,
            block: Block {
                hash,
                height,
                parent_hash,
                protocol_version,
                author: None,
                timestamp: timestamp.unwrap_or(0),
                zswap_merkle_tree_root,
                ledger_state_root,
                transactions,
                dust_registration_events,
                bridge_events,
            },
            client: block,
        })
    }

    #[trace]
    async fn block_at(&self, hash: H256) -> Result<OnlineClientAtBlock, SubxtNodeError> {
        self.online_client
            .at_block(hash)
            .await
            .map_err(|error| SubxtNodeError::GetOnlineClientAt(hash, error.into()))
    }

    #[trace]
    async fn block_at_height(&self, height: u64) -> Result<OnlineClientAtBlock, SubxtNodeError> {
        self.online_client
            .at_block(height)
            .await
            .map_err(|error| SubxtNodeError::GetOnlineClientAtHeight(height, error.into()))
    }
}

impl Node for SubxtNode {
    type Error = SubxtNodeError;

    async fn highest_blocks(
        &self,
    ) -> Result<impl Stream<Item = Result<BlockRef, Self::Error>> + Send, Self::Error> {
        let highest_blocks = self
            .subscribe_finalized_blocks(None)
            .await?
            .map_ok(|block| BlockRef {
                hash: block.hash().0.into(),
                height: block.number(),
            });

        Ok(highest_blocks)
    }

    fn finalized_blocks<'a>(
        &'a mut self,
        after: Option<BlockRef>,
    ) -> impl Stream<Item = Result<Block, Self::Error>> + use<'a> {
        let (after_hash, after_height) = after
            .map(|BlockRef { hash, height }| (hash, height))
            .unzip();
        debug!(
            after_hash:?,
            after_height:?;
            "subscribing to finalized blocks"
        );

        let after_hash = after_hash.unwrap_or_default();
        let mut authorities = None;

        try_stream! {
            let mut finalized_blocks = self.subscribe_finalized_blocks(after_height).await?;
            let mut last_yielded_height = after_height;

            // First we receive the first finalized block.
            let Some(first_block) = receive_block(&mut finalized_blocks).await? else {
                return;
            };
            debug!(
                hash:% = first_block.hash(),
                height = first_block.number(),
                parent_hash:% = first_block.header().parent_hash;
                "block received"
            );

            // Then we fetch and yield earlier blocks and then yield the first finalized block,
            // unless the highest stored block matches the first finalized block.
            if first_block.hash().0 != after_hash.0 {
                let start_height = after_height.map(|h| h + 1).unwrap_or(0);
                let end_height = first_block.number();

                // Blocks older than FINALIZATION_SAFETY_MARGIN from the finalized tip are
                // guaranteed to be finalized by an earlier GRANDPA round, so they can be
                // fetched by height with parent hash verification. Blocks within the safety
                // margin are fetched by hash (backward traversal) to avoid any risk of
                // ingesting non-canonical blocks near the tip.
                let safe_height = end_height
                    .saturating_sub(FINALIZATION_SAFETY_MARGIN)
                    .max(start_height);

                // Initialize from the stored block hash so the first forward-fetched block
                // is verified against it too.
                let mut last_forward_hash = after_height.map(|_| H256(after_hash.0));
                // Fetch blocks for multiple heights concurrently; `buffered` preserves height
                // order. Author resolution and parent-hash verification stay sequential below.
                let fetch_node = self.clone();
                let raw_blocks = stream::iter(start_height..safe_height)
                    .map(move |height| {
                        let fetch_node = fetch_node.clone();
                        async move {
                            let block = fetch_node.block_at_height(height).await?;
                            fetch_node.make_raw_block(block).await
                        }
                    })
                    .buffered(self.fetch_concurrency.max(1));
                let mut raw_blocks = pin!(raw_blocks);

                let mut height = start_height;
                while let Some(raw_block) = raw_blocks.next().await {
                    if height % CATCH_UP_LOG_INTERVAL == 0 {
                        info!(
                            highest_stored_height:? = after_height,
                            current_height = height,
                            first_finalized_height = end_height;
                            "catching up by height"
                        );
                    }
                    let raw_block = raw_block?;
                    let block_hash = raw_block.client.block_hash();
                    let made_block = finish_block(&mut authorities, raw_block).await?;
                    if let Some(expected_parent) = last_forward_hash
                        && made_block.parent_hash.0 != expected_parent.0
                    {
                        Err(SubxtNodeError::ParentHashMismatch(
                            height,
                            expected_parent,
                            H256(made_block.parent_hash.0),
                        ))?;
                    }
                    last_forward_hash = Some(block_hash);
                    height += 1;
                    yield made_block;
                }

                let stop_hash = last_forward_hash.unwrap_or(H256(after_hash.0));
                let genesis = self.block_at(self.online_client.genesis_hash()).await?;
                let genesis_parent_hash = block_header(&genesis).await?.parent_hash;

                let mut hashes = Vec::with_capacity(FINALIZATION_SAFETY_MARGIN as usize);
                let mut parent_hash = first_block.header().parent_hash;
                while parent_hash != stop_hash && parent_hash != genesis_parent_hash {
                    let parent = self.block_at(parent_hash).await?;
                    parent_hash = block_header(&parent).await?.parent_hash;
                    hashes.push(parent.block_hash());
                }

                for hash in hashes.into_iter().rev() {
                    let block = self.block_at(hash).await?;
                    yield self.make_block(&mut authorities, block).await?;
                }

                // Then we yield the first finalized block.
                let first_block = first_block.at().await.map_err(|error| {
                    SubxtNodeError::GetOnlineClientAt(first_block.hash(), error.into())
                })?;
                let first_block = self.make_block(&mut authorities, first_block).await?;
                last_yielded_height = Some(first_block.height);
                yield first_block;
            }

            // Finally we emit all other finalized ones.
            // If no block is received within the recovery timeout, re-subscribe to recover
            // from potentially stuck subscriptions (e.g., after a reconnect).
            let recovery_timeout = self.subscription_recovery_timeout;
            loop {
                match timeout(recovery_timeout, receive_block(&mut finalized_blocks)).await {
                    Ok(Ok(Some(block))) => {
                        debug!(
                            hash:% = block.hash(),
                            height = block.number(),
                            parent_hash:% = block.header().parent_hash;
                            "block received"
                        );
                        let block = block.at().await.map_err(|error| {
                            SubxtNodeError::GetOnlineClientAt(block.hash(), error.into())
                        })?;
                        let block = self.make_block(&mut authorities, block).await?;
                        last_yielded_height = Some(block.height);
                        yield block;
                    }

                    // Stream completed normally.
                    Ok(Ok(None)) => break,

                    // Stream completed with error.
                    Ok(Err(e)) => Err(e)?,

                    // Timeout: no block received within recovery_timeout => resubscribe.
                    Err(_) => {
                        warn!(
                            last_yielded_height:?,
                            recovery_timeout:?;
                            "subscription appears stuck, re-subscribing"
                        );
                        finalized_blocks =
                            self.subscribe_finalized_blocks(last_yielded_height).await?;
                    }
                }
            }
        }
    }

    async fn fetch_system_parameters(
        &self,
        block_hash: BlockHash,
        block_height: u64,
        timestamp: u64,
        _node_version: NodeVersion,
    ) -> Result<SystemParametersChange, Self::Error> {
        let block = self.block_at(H256(block_hash.0)).await?;

        // Both are storage/runtime-API reads, so both need the runtime in this block's state,
        // which at an enactment block is already the next one; see `make_block`. The
        // `_node_version` parameter carries the MNSV based version and is deliberately unused.
        let state_node_version = ProtocolVersion::try_from(block.spec_version())?.node_version();

        let (d_parameter, terms_and_conditions) = tokio::try_join!(
            runtimes::get_d_parameter(state_node_version, &block),
            runtimes::get_terms_and_conditions(state_node_version, &block),
        )?;

        Ok(SystemParametersChange {
            block_height,
            block_hash,
            timestamp,
            d_parameter: Some(d_parameter),
            terms_and_conditions,
        })
    }

    async fn fetch_genesis_ledger_state(&self) -> Result<ByteVec, Self::Error> {
        let legacy_rpc_methods = LegacyRpcMethods::<RpcConfigFor<SubstrateConfig>>::new(
            self.rpc_client.to_owned().into(),
        );
        let properties = legacy_rpc_methods
            .system_properties()
            .await
            .map_err(SubxtNodeError::FetchSystemProperties)?;

        let genesis_ledger_state = properties
            .get("genesis_state")
            .and_then(|value| value.as_str())
            .map(Ok)
            .unwrap_or_else(|| Err(SubxtNodeError::GenesisLedgerStateNotFound))?;

        let genesis_ledger_state = genesis_ledger_state
            .strip_prefix("0x")
            .unwrap_or(genesis_ledger_state);
        let genesis_ledger_state = const_hex::decode(genesis_ledger_state)
            .map_err(SubxtNodeError::HexDecodeGenesisLedgerState)?;
        let genesis_ledger_state = ByteVec::from(genesis_ledger_state);

        info!(
            genesis_ledger_state_len = genesis_ledger_state.len();
            "fetched genesis ledger state from system properties"
        );

        Ok(genesis_ledger_state)
    }
}

/// Config for node connection.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub url: String,

    #[serde(with = "humantime_serde")]
    pub reconnect_max_delay: Duration,

    pub reconnect_max_attempts: usize,

    /// Timeout for receiving a valid block after a reconnect or duplicate event.
    /// If no valid block is received within this duration, the subscription is considered
    /// stuck and will be re-established. Defaults to 30 seconds.
    #[serde(
        with = "humantime_serde",
        default = "default_subscription_recovery_timeout"
    )]
    pub subscription_recovery_timeout: Duration,

    /// Number of blocks fetched concurrently while catching up by height. Author resolution
    /// stays sequential, so this only bounds in-flight block fetches. Defaults to 8.
    #[serde(default = "default_fetch_concurrency")]
    pub fetch_concurrency: usize,
}

fn default_subscription_recovery_timeout() -> Duration {
    Duration::from_secs(30)
}

fn default_fetch_concurrency() -> usize {
    8
}

/// Error possibly returned by [SubxtNode::new].
#[derive(Debug, Error)]
pub enum Error {
    #[error("cannot create reconnecting subxt RPC client")]
    RpcClient(#[source] BoxError),

    #[error("cannot create subxt online client")]
    OnlineClient(#[from] subxt::error::OnlineClientError),

    #[error("cannot create HTTP header")]
    InvalidHeaderValue(#[from] InvalidHeaderValue),
}

/// Error possibly returned by each item of the [Block]s stream.
#[derive(Debug, Error)]
pub enum SubxtNodeError {
    #[error("cannot subscribe to finalized blocks")]
    SubscribeFinalizedBlocks(#[source] Box<subxt::error::BlocksError>),

    #[error("cannot receive finalized block")]
    ReceiveBlock(#[source] Box<subxt::error::BlocksError>),

    #[error("cannot get online client at block {0}")]
    GetOnlineClientAt(H256, #[source] Box<subxt::error::OnlineClientAtBlockError>),

    #[error("cannot get online client at block height {0}")]
    GetOnlineClientAtHeight(u64, #[source] Box<subxt::error::OnlineClientAtBlockError>),

    #[error("parent hash mismatch at height {0}: expected {1}, was {2}")]
    ParentHashMismatch(u64, H256, H256),

    #[error("cannot fetch extrinsics")]
    FetchExtrinsics(#[source] Box<subxt::error::ExtrinsicError>),

    #[error("cannot fetch block body via legacy RPC")]
    FetchBlockBody(#[source] subxt::rpcs::Error),

    #[error("block body not found")]
    BlockBodyNotFound,

    #[error("cannot fetch events")]
    FetchEvents(#[source] Box<subxt::error::EventsError>),

    #[error("cannot get block header")]
    GetBlockHeader(#[source] Box<subxt::error::BlockError>),

    #[error("protocol version header missing from block")]
    MissingProtocolVersionHeader,

    #[error("cannot get next extrinsic")]
    GetNextExtrinsic(#[source] Box<subxt::error::ExtrinsicDecodeErrorAt>),

    #[error("cannot decode extrinsic as call")]
    DecodeExtrinsicAsCall(#[source] Box<subxt::error::ExtrinsicError>),

    #[error("cannot get next event")]
    GetNextEvent(#[source] Box<subxt::error::EventsError>),

    #[error("cannot decode subxt event as midnight event")]
    DecodeEvent(#[source] Box<subxt::error::EventsError>),

    #[error("cannot decode bridge recipient from c2m-bridge event")]
    DecodeBridgeRecipient(#[from] indexer_common::domain::bridge::BridgeRecipientError),

    #[error("cannot fetch authorities")]
    FetchAuthorities(#[source] Box<subxt::error::StorageError>),

    #[error("cannot decode authorities")]
    DecodeAuthorities(#[source] Box<subxt::error::StorageValueError>),

    #[error("invalid BABE pre-runtime digest variant tag {0}")]
    InvalidBabePreDigestTag(u8),

    #[error("cannot fetch genesis cNight registrations")]
    FetchGenesisCnightRegistrations(#[source] Box<subxt::error::StorageError>),

    #[error("cannot decode genesis cNight registrations")]
    DecodeGenesisCnightRegistrations(#[source] Box<subxt::error::StorageValueError>),

    #[error("cannot decode genesis cNight registration key")]
    DecodeGenesisCnightRegistrationKey(#[source] Box<subxt::error::StorageKeyError>),

    #[error("cannot get contract state for address {0}")]
    GetContractState(SerializedContractAddress, #[source] BoxError),

    #[error("cannot get zswap state root")]
    GetZswapStateRoot(#[source] BoxError),

    #[error("cannot get D-Parameter")]
    GetDParameter(#[source] BoxError),

    #[error("cannot get Terms and Conditions")]
    GetTermsAndConditions(#[source] BoxError),

    #[error("cannot hex decode genesis ledger state")]
    HexDecodeGenesisLedgerState(#[source] FromHexError),

    #[error("cannot get ledger state root")]
    GetLedgerStateRoot(#[source] BoxError),

    #[error("cannot fetch system properties")]
    FetchSystemProperties(#[source] subxt::rpcs::Error),

    #[error("no String type genesis ledger state in system parameters")]
    GenesisLedgerStateNotFound,

    #[error(transparent)]
    ProtocolVersion(#[from] ProtocolVersionError),

    #[error("cannot scale decode")]
    ScaleDecode(#[from] parity_scale_codec::Error),

    #[error(transparent)]
    Ledger(#[from] ledger::Error),
}

#[trace]
async fn receive_block(
    finalized_blocks: &mut (impl Stream<Item = Result<SubxtBlock, SubxtNodeError>> + Unpin),
) -> Result<Option<SubxtBlock>, SubxtNodeError> {
    finalized_blocks.try_next().await
}

/// Check an authority set against a block header's digest logs to determine the author of that
/// block.
fn extract_block_author<H>(
    header: &SubstrateHeader<H>,
    authorities: &[[u8; 32]],
    content_node_version: NodeVersion,
    babe_supported: bool,
) -> Result<Option<BlockAuthor>, SubxtNodeError>
where
    H: Hash,
{
    author_from_digest_logs(
        &header.digest.logs,
        authorities,
        content_node_version,
        babe_supported,
    )
}

/// Determine the block author from the pre-runtime digest logs, taking the first log with a
/// recognized consensus engine that yields an author, in digest order (mirroring polkadot-js
/// `extractAuthor`): Aura carries the slot (the author is the slot modulo the authority-set
/// length), BABE carries the authority index explicitly in all of its pre-digest variants. BABE
/// digests are only recognized if `babe_supported`, i.e. if the block's runtime guarantees
/// their correctness (see [CONSENSUS_ENGINE_RUNTIME_API]); otherwise they are skipped like any
/// unrecognized engine.
fn author_from_digest_logs(
    logs: &[DigestItem],
    authorities: &[[u8; 32]],
    content_node_version: NodeVersion,
    babe_supported: bool,
) -> Result<Option<BlockAuthor>, SubxtNodeError> {
    if authorities.is_empty() {
        return Ok(None);
    }

    for log in logs {
        let DigestItem::PreRuntime(engine_id, pre_digest) = log else {
            continue;
        };

        let author = match *engine_id {
            AURA_ENGINE_ID => {
                let slot = runtimes::decode_slot(pre_digest, content_node_version)?;
                let index = slot % authorities.len() as u64;
                authorities.get(index as usize).copied().map(Into::into)
            }

            BABE_ENGINE_ID if babe_supported => babe_author(pre_digest, authorities)?,

            _ => None,
        };

        if author.is_some() {
            return Ok(author);
        }
    }

    Ok(None)
}

/// Determine the block author from a BABE pre-runtime digest. An out-of-range authority index
/// means the cached authority set does not match the block's epoch; report an unknown author
/// instead of failing block processing.
fn babe_author(
    pre_digest: &[u8],
    authorities: &[[u8; 32]],
) -> Result<Option<BlockAuthor>, SubxtNodeError> {
    let index = decode_babe_authority_index(pre_digest)?;

    let author = usize::try_from(index)
        .ok()
        .and_then(|index| authorities.get(index))
        .copied()
        .map(Into::into);

    Ok(author)
}

/// Extract the authority index from a BABE pre-runtime digest. All `PreDigest` variants
/// (`Primary` = 1, `SecondaryPlain` = 2, `SecondaryVRF` = 3, see `sp_consensus_babe::digests`)
/// lead with the SCALE-encoded `authority_index: u32` right after the variant tag, so only that
/// prefix is decoded and the remainder (slot, VRF signature) is ignored.
fn decode_babe_authority_index(mut pre_digest: &[u8]) -> Result<u32, SubxtNodeError> {
    let tag = u8::decode(&mut pre_digest)?;
    if !(1..=3).contains(&tag) {
        return Err(SubxtNodeError::InvalidBabePreDigestTag(tag));
    }

    Ok(u32::decode(&mut pre_digest)?)
}

async fn make_transaction(
    transaction: runtimes::Transaction,
    protocol_version: ProtocolVersion,
    state_node_version: NodeVersion,
    block: &OnlineClientAtBlock,
) -> Result<Transaction, SubxtNodeError> {
    match transaction {
        runtimes::Transaction::Regular(transaction) => {
            make_regular_transaction(transaction, protocol_version, state_node_version, block).await
        }

        runtimes::Transaction::System(transaction) => {
            make_system_transaction(transaction, protocol_version).await
        }
    }
}

async fn make_regular_transaction(
    transaction: ByteVec,
    protocol_version: ProtocolVersion,
    state_node_version: NodeVersion,
    block: &OnlineClientAtBlock,
) -> Result<Transaction, SubxtNodeError> {
    let ledger_transaction =
        ledger::Transaction::deserialize(&transaction, protocol_version.ledger_version())?;

    let hash = ledger_transaction.hash();

    let identifiers = ledger_transaction.identifiers()?;

    let contract_actions = ledger_transaction
        .contract_actions(|address| async move {
            runtimes::get_contract_state(address, state_node_version, block).await
        })
        .await?
        .into_iter()
        .map(Into::into)
        .collect();

    let transaction = RegularTransaction {
        hash,
        protocol_version,
        identifiers,
        contract_actions,
        raw: transaction,
    };

    Ok(Transaction::Regular(transaction))
}

async fn make_system_transaction(
    transaction: ByteVec,
    protocol_version: ProtocolVersion,
) -> Result<Transaction, SubxtNodeError> {
    let ledger_transaction =
        ledger::SystemTransaction::deserialize(&transaction, protocol_version.ledger_version())?;

    let hash = ledger_transaction.hash();

    let transaction = SystemTransaction {
        hash,
        protocol_version,
        raw: transaction,
    };

    Ok(Transaction::System(transaction))
}

#[trace]
async fn block_header(
    block: &OnlineClientAtBlock,
) -> Result<SubstrateHeader<H256>, SubxtNodeError> {
    block
        .block_header()
        .await
        .map_err(|error| SubxtNodeError::GetBlockHeader(error.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::Encode;

    const AUTHORITIES: [[u8; 32]; 3] = [[1; 32], [2; 32], [3; 32]];

    /// A BABE pre-digest prefix: variant tag, then the SCALE-encoded authority index, then
    /// trailing payload (slot, VRF signature) which must be ignored.
    fn babe_pre_digest(tag: u8, authority_index: u32) -> Vec<u8> {
        let mut pre_digest = vec![tag];
        pre_digest.extend(authority_index.encode());
        pre_digest.extend([0xff; 8]);
        pre_digest
    }

    #[test]
    fn author_from_aura_digest() {
        let logs = vec![DigestItem::PreRuntime(AURA_ENGINE_ID, 4u64.encode())];

        let author = author_from_digest_logs(&logs, &AUTHORITIES, NodeVersion::V2_0, false)
            .expect("author can be determined");

        assert_eq!(author, Some([2; 32].into()));
    }

    #[test]
    fn babe_author_for_all_variants() {
        for tag in 1..=3 {
            let author = babe_author(&babe_pre_digest(tag, 2), &AUTHORITIES)
                .expect("author can be determined");

            assert_eq!(author, Some([3; 32].into()));
        }
    }

    #[test]
    fn babe_digest_is_skipped_if_babe_not_supported() {
        let logs = vec![DigestItem::PreRuntime(
            BABE_ENGINE_ID,
            babe_pre_digest(2, 2),
        )];
        let author = author_from_digest_logs(&logs, &AUTHORITIES, NodeVersion::V2_0, false)
            .expect("skipped digest is not an error");
        assert_eq!(author, None);

        let logs = vec![
            DigestItem::PreRuntime(BABE_ENGINE_ID, babe_pre_digest(2, 2)),
            DigestItem::PreRuntime(AURA_ENGINE_ID, 4u64.encode()),
        ];
        let author = author_from_digest_logs(&logs, &AUTHORITIES, NodeVersion::V2_0, false)
            .expect("author can be determined");
        assert_eq!(author, Some([2; 32].into()));
    }

    #[test]
    fn first_pre_runtime_digest_in_digest_order_wins() {
        let logs = vec![
            DigestItem::PreRuntime(BABE_ENGINE_ID, babe_pre_digest(2, 2)),
            DigestItem::PreRuntime(AURA_ENGINE_ID, 4u64.encode()),
        ];
        let author = author_from_digest_logs(&logs, &AUTHORITIES, NodeVersion::V2_0, true)
            .expect("author can be determined");
        assert_eq!(author, Some([3; 32].into()));

        let logs = vec![
            DigestItem::PreRuntime(AURA_ENGINE_ID, 4u64.encode()),
            DigestItem::PreRuntime(BABE_ENGINE_ID, babe_pre_digest(2, 2)),
        ];
        let author = author_from_digest_logs(&logs, &AUTHORITIES, NodeVersion::V2_0, true)
            .expect("author can be determined");
        assert_eq!(author, Some([2; 32].into()));
    }

    #[test]
    fn unrecognized_engine_is_skipped() {
        let logs = vec![
            DigestItem::PreRuntime(*b"test", vec![0xaa]),
            DigestItem::PreRuntime(AURA_ENGINE_ID, 4u64.encode()),
        ];

        let author = author_from_digest_logs(&logs, &AUTHORITIES, NodeVersion::V2_0, true)
            .expect("author can be determined");

        assert_eq!(author, Some([2; 32].into()));
    }

    #[test]
    fn babe_out_of_range_authority_index_yields_no_author() {
        let author = babe_author(&babe_pre_digest(2, 7), &AUTHORITIES)
            .expect("out-of-range index is not an error");

        assert_eq!(author, None);
    }

    #[test]
    fn invalid_babe_pre_digest_tag_is_an_error() {
        for tag in [0, 4] {
            let author = babe_author(&babe_pre_digest(tag, 2), &AUTHORITIES);

            assert!(matches!(
                author,
                Err(SubxtNodeError::InvalidBabePreDigestTag(t)) if t == tag
            ));
        }
    }

    #[test]
    fn truncated_babe_pre_digest_is_an_error() {
        let author = babe_author(&[1, 0xaa], &AUTHORITIES);

        assert!(matches!(author, Err(SubxtNodeError::ScaleDecode(_))));
    }

    #[test]
    fn no_pre_runtime_digest_yields_no_author() {
        let author = author_from_digest_logs(&[], &AUTHORITIES, NodeVersion::V2_0, true)
            .expect("no digest is not an error");

        assert_eq!(author, None);
    }

    #[test]
    fn empty_authorities_yield_no_author() {
        let logs = vec![DigestItem::PreRuntime(AURA_ENGINE_ID, 4u64.encode())];

        let author = author_from_digest_logs(&logs, &[], NodeVersion::V2_0, true)
            .expect("empty authorities are not an error");

        assert_eq!(author, None);
    }
}
