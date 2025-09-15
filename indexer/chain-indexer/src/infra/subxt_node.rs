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

mod header;
mod runtimes;

use crate::{
    domain::{
        TransactionFees,
        node::{Block, BlockInfo, Node, RegularTransaction, SystemTransaction, Transaction},
    },
    infra::subxt_node::{header::SubstrateHeaderExt, runtimes::BlockDetails},
};
use async_stream::try_stream;
use fastrace::trace;
use futures::{Stream, StreamExt, TryStreamExt, stream};
use indexer_common::{
    domain::{
        BlockAuthor, BlockHash, ByteVec, ProtocolVersion, ScaleDecodeProtocolVersionError,
        ledger::{self, ZswapStateRoot},
    },
    error::BoxError,
};
use log::{debug, error, info, warn};
use serde::Deserialize;
use std::{future::ready, time::Duration};
use subxt::{
    OnlineClient, SubstrateConfig,
    backend::{
        BackendExt,
        legacy::LegacyRpcMethods,
        rpc::reconnecting_rpc_client::{ExponentialBackoff, RpcClient},
    },
    config::{
        Hasher,
        substrate::{ConsensusEngineId, DigestItem, SubstrateHeader},
    },
    ext::subxt_rpcs,
    utils::H256,
};
use thiserror::Error;

type SubxtBlock = subxt::blocks::Block<SubstrateConfig, OnlineClient<SubstrateConfig>>;

const AURA_ENGINE_ID: ConsensusEngineId = [b'a', b'u', b'r', b'a'];
const TRAVERSE_BACK_LOG_AFTER: u32 = 1_000;

/// A [Node] implementation based on subxt.
#[derive(Clone)]
pub struct SubxtNode {
    genesis_protocol_version: ProtocolVersion,
    rpc_client: RpcClient,
    default_online_client: OnlineClient<SubstrateConfig>,
    compatible_online_client: Option<(ProtocolVersion, OnlineClient<SubstrateConfig>)>,
}

impl SubxtNode {
    /// Create a new [SubxtNode] with the given [Config].
    pub async fn new(config: Config) -> Result<Self, Error> {
        let Config {
            url,
            genesis_protocol_version,
            reconnect_max_delay: retry_max_delay,
            reconnect_max_attempts: retry_max_attempts,
        } = config;

        let retry_policy = ExponentialBackoff::from_millis(10)
            .max_delay(retry_max_delay)
            .take(retry_max_attempts);
        let rpc_client = RpcClient::builder()
            .retry_policy(retry_policy)
            .build(&url)
            .await
            .map_err(|error| Error::RpcClient(error.into()))?;

        let default_online_client =
            OnlineClient::<SubstrateConfig>::from_rpc_client(rpc_client.clone()).await?;

        Ok(Self {
            rpc_client,
            genesis_protocol_version,
            default_online_client,
            compatible_online_client: None,
        })
    }

    async fn compatible_online_client(
        &mut self,
        protocol_version: ProtocolVersion,
        hash: BlockHash,
    ) -> Result<&OnlineClient<SubstrateConfig>, SubxtNodeError> {
        if !self
            .compatible_online_client
            .as_ref()
            .map(|&(v, _)| protocol_version.is_compatible(v))
            .unwrap_or_default()
        {
            let genesis_hash = self.default_online_client.genesis_hash();

            // Version must be greater or equal 15. This is a substrate/subxt detail.
            let metadata = self
                .default_online_client
                .backend()
                .metadata_at_version(15, H256(hash.0))
                .await
                .map_err(Box::new)?;

            let legacy_rpc_methods =
                LegacyRpcMethods::<SubstrateConfig>::new(self.rpc_client.to_owned().into());
            let runtime_version = legacy_rpc_methods
                .state_get_runtime_version(Some(H256(hash.0)))
                .await?;
            let runtime_version = subxt::client::RuntimeVersion {
                spec_version: runtime_version.spec_version,
                transaction_version: runtime_version.transaction_version,
            };

            let online_client = OnlineClient::<SubstrateConfig>::from_rpc_client_with(
                genesis_hash,
                runtime_version,
                metadata,
                self.rpc_client.to_owned(),
            )
            .map_err(Box::new)?;

            self.compatible_online_client = Some((protocol_version, online_client));
        }

        let compatible_online_client = self
            .compatible_online_client
            .as_ref()
            .map(|(_, c)| c)
            .expect("compatible_online_client is defined");

        Ok(compatible_online_client)
    }

    /// Subscribe to finalizded blocks, filtering duplicates and disconnection errors.
    async fn subscribe_finalized_blocks(
        &self,
    ) -> Result<impl Stream<Item = Result<SubxtBlock, subxt::Error>> + use<>, subxt::Error> {
        let mut last_block_height = None;

        let subscribe_finalized_blocks = self
            .default_online_client
            .blocks()
            .subscribe_finalized()
            .await?
            .filter(move |block| {
                let pass = match block {
                    Ok(block) => {
                        let height = block.number();

                        if Some(height) <= last_block_height {
                            warn!(
                                hash:% = block.hash(),
                                height = block.number();
                                "received duplicate, possibly after reconnect"
                            );
                            false
                        } else {
                            last_block_height = Some(height);
                            true
                        }
                    }

                    Err(subxt::Error::Rpc(subxt::error::RpcError::ClientError(
                        subxt_rpcs::Error::DisconnectedWillReconnect(_),
                    ))) => {
                        warn!("node disconnected, reconnecting");
                        false
                    }

                    _ => true,
                };

                ready(pass)
            });

        Ok(subscribe_finalized_blocks)
    }

    #[trace]
    async fn fetch_block(&self, hash: H256) -> Result<SubxtBlock, subxt::Error> {
        self.default_online_client.blocks().at(hash).await
    }

    async fn make_block(
        &mut self,
        block: SubxtBlock,
        authorities: &mut Option<Vec<[u8; 32]>>,
    ) -> Result<Block, SubxtNodeError> {
        let hash = block.hash().0.into();
        let height = block.number();
        let parent_hash = block.header().parent_hash.0.into();
        let protocol_version = block
            .header()
            .protocol_version()?
            .unwrap_or(self.genesis_protocol_version);

        info!(
            hash:%,
            height,
            parent_hash:%,
            protocol_version:%;
            "making block"
        );

        let online_client = self
            .compatible_online_client(protocol_version, hash)
            .await?;

        // Fetch authorities if `None`, either initially or because of a `NewSession` event (below).
        if authorities.is_none() {
            *authorities =
                runtimes::fetch_authorities(hash, protocol_version, online_client).await?;
        }
        let author = authorities
            .as_ref()
            .map(|authorities| extract_block_author(block.header(), authorities, protocol_version))
            .transpose()?
            .flatten();

        let zswap_state_root =
            runtimes::get_zswap_state_root(hash, protocol_version, online_client).await?;
        let zswap_state_root = ZswapStateRoot::deserialize(zswap_state_root, protocol_version)?;

        let extrinsics = block.extrinsics().await.map_err(Box::new)?;
        let events = block.events().await.map_err(Box::new)?;
        let BlockDetails {
            timestamp,
            transactions,
        } = runtimes::make_block_details(extrinsics, events, authorities, protocol_version).await?;

        let transactions = stream::iter(transactions)
            .then(|t| make_transaction(t, hash, protocol_version, online_client))
            .try_collect::<Vec<_>>()
            .await?;

        let block = Block {
            hash,
            height,
            parent_hash,
            protocol_version,
            author,
            timestamp: timestamp.unwrap_or(0),
            zswap_state_root,
            transactions,
        };

        debug!(
            hash:% = block.hash,
            height = block.height,
            parent_hash:% = block.parent_hash,
            transactions_len = block.transactions.len();
            "block made"
        );

        Ok(block)
    }
}

impl Node for SubxtNode {
    type Error = SubxtNodeError;

    async fn highest_blocks(
        &self,
    ) -> Result<impl Stream<Item = Result<BlockInfo, Self::Error>> + Send, Self::Error> {
        let highest_blocks = self
            .subscribe_finalized_blocks()
            .await
            .map_err(Box::new)?
            .map_ok(|block| BlockInfo {
                hash: block.hash().0.into(),
                height: block.number(),
            })
            .map_err(|error| Box::new(error).into());

        Ok(highest_blocks)
    }

    fn finalized_blocks<'a>(
        &'a mut self,
        after: Option<BlockInfo>,
    ) -> impl Stream<Item = Result<Block, Self::Error>> + use<'a> {
        let (after_hash, after_height) = after
            .map(|BlockInfo { hash, height }| (hash, height))
            .unzip();
        debug!(
            after_hash:?,
            after_height:?;
            "subscribing to finalized blocks"
        );

        let after_hash = after_hash.unwrap_or_default();
        let mut authorities = None;

        try_stream! {
            let mut finalized_blocks = self.subscribe_finalized_blocks().await.map_err(Box::new)?;

            // First we receive the first finalized block.
            let Some(first_block) = receive_block(&mut finalized_blocks)
                .await
                .map_err(Box::new)?
            else {
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
                // If we have not already stored the first finalized block, we fetch all blocks
                // walking backwards from the one with the parent hash of the first finalized block
                // until we arrive at the highest stored block (excluded) or at genesis (included).
                // For these we store the hashes; one hash is 32 bytes, i.e. one year is ~ 156MB.
                let genesis_parent_hash = self
                    .fetch_block(self.default_online_client.genesis_hash())
                    .await
                    .map_err(Box::new)?
                    .header()
                    .parent_hash;

                let capacity = match after_height {
                    Some(highest_height) if highest_height < first_block.number() => {
                        (first_block.number() - highest_height) as usize + 1
                    }
                    _ => first_block.number() as usize + 1,
                };
                info!(
                    highest_stored_height:? = after_height,
                    first_finalized_height = first_block.number();
                    "traversing back via parent hashes, this may take some time"
                );

                let mut hashes = Vec::with_capacity(capacity);
                let mut parent_hash = first_block.header().parent_hash;
                while parent_hash.0 != after_hash.0 && parent_hash != genesis_parent_hash {
                    let block = self.fetch_block(parent_hash).await.map_err(Box::new)?;
                    if block.number() % TRAVERSE_BACK_LOG_AFTER == 0 {
                        info!(
                            highest_stored_height:? = after_height,
                            current_height = block.number(),
                            first_finalized_height = first_block.number();
                            "traversing back via parent hashes"
                        );
                    }
                    parent_hash = block.header().parent_hash;
                    hashes.push(block.hash());
                }

                // We fetch and yield the blocks for the stored block hashes.
                for hash in hashes.into_iter().rev() {
                    let block = self.fetch_block(hash).await.map_err(Box::new)?;
                    debug!(
                        hash:% = block.hash(),
                        height = block.number(),
                        parent_hash:% = block.header().parent_hash;
                        "block fetched"
                    );
                    yield self.make_block(block, &mut authorities).await?;
                }

                // Then we yield the first finalized block.
                yield self.make_block(first_block, &mut authorities).await?;
            }

            // Finally we emit all other finalized ones.
            while let Some(block) = receive_block(&mut finalized_blocks)
                .await
                .map_err(Box::new)?
            {
                debug!(
                    hash:% = block.hash(),
                    height = block.number(),
                    parent_hash:% = block.header().parent_hash;
                    "block received"
                );

                yield self.make_block(block, &mut authorities).await?;
            }
        }
    }
}

/// Config for node connection.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub url: String,

    pub genesis_protocol_version: ProtocolVersion,

    #[serde(with = "humantime_serde")]
    pub reconnect_max_delay: Duration,

    pub reconnect_max_attempts: usize,
}

/// Error possibly returned by [SubxtNode::new].
#[derive(Debug, Error)]
pub enum Error {
    #[error("cannot create reconnecting subxt RPC client")]
    RpcClient(#[source] BoxError),

    #[error("cannot create subxt online client")]
    OnlineClient(#[from] subxt::Error),
}

/// Error possibly returned by each item of the [Block]s stream.
#[derive(Debug, Error)]
pub enum SubxtNodeError {
    #[error(transparent)]
    Subxt(#[from] Box<subxt::Error>),

    #[error(transparent)]
    SubxtRcps(#[from] subxt::ext::subxt_rpcs::Error),

    #[error("cannot scale decode")]
    ScaleDecode(#[from] parity_scale_codec::Error),

    #[error(transparent)]
    DecodeProtocolVersion(#[from] ScaleDecodeProtocolVersionError),

    #[error(transparent)]
    Ledger(#[from] ledger::Error),

    #[error("cannot get contract state: {0}")]
    GetContractState(String),

    #[error("cannot get zswap state root: {0}")]
    GetZswapStateRoot(String),

    #[error("cannot get transaction cost: {0}")]
    GetTransactionCost(String),

    #[error("block with hash {0} not found")]
    BlockNotFound(BlockHash),

    #[error("invalid protocol version {0}")]
    InvalidProtocolVersion(ProtocolVersion),
}

#[trace]
async fn receive_block(
    finalized_blocks: &mut (impl Stream<Item = Result<SubxtBlock, subxt::Error>> + Unpin),
) -> Result<Option<SubxtBlock>, subxt::Error> {
    finalized_blocks.try_next().await
}

/// Check an authority set against a block header's digest logs to determine the author of that
/// block.
fn extract_block_author<H>(
    header: &SubstrateHeader<u32, H>,
    authorities: &[[u8; 32]],
    protocol_version: ProtocolVersion,
) -> Result<Option<BlockAuthor>, SubxtNodeError>
where
    H: Hasher,
{
    if authorities.is_empty() {
        return Ok(None);
    }

    let block_author = header
        .digest
        .logs
        .iter()
        .find_map(|log| match log {
            DigestItem::PreRuntime(AURA_ENGINE_ID, inner) => Some(inner.as_slice()),
            _ => None,
        })
        .map(|slot| runtimes::decode_slot(slot, protocol_version))
        .transpose()?
        .and_then(|slot| {
            let index = slot % authorities.len() as u64;
            authorities.get(index as usize).copied().map(Into::into)
        });

    Ok(block_author)
}

async fn make_transaction(
    transaction: runtimes::Transaction,
    block_hash: BlockHash,
    protocol_version: ProtocolVersion,
    online_client: &OnlineClient<SubstrateConfig>,
) -> Result<Transaction, SubxtNodeError> {
    match transaction {
        runtimes::Transaction::Regular(transaction) => {
            make_regular_transaction(transaction, block_hash, protocol_version, online_client).await
        }

        runtimes::Transaction::System(transaction) => {
            make_system_transaction(transaction, protocol_version).await
        }
    }
}

async fn make_regular_transaction(
    transaction: ByteVec,
    block_hash: BlockHash,
    protocol_version: ProtocolVersion,
    online_client: &OnlineClient<SubstrateConfig>,
) -> Result<Transaction, SubxtNodeError> {
    let ledger_transaction = ledger::Transaction::deserialize(&transaction, protocol_version)?;

    let hash = ledger_transaction.hash();

    let identifiers = ledger_transaction.identifiers()?;

    let contract_actions = ledger_transaction
        .contract_actions(|address| async move {
            runtimes::get_contract_state(address, block_hash, protocol_version, online_client).await
        })
        .await?
        .into_iter()
        .map(Into::into)
        .collect();

    let fees = match runtimes::get_transaction_cost(
        &transaction,
        block_hash,
        protocol_version,
        online_client,
    )
    .await
    {
        Ok(fees) => TransactionFees {
            paid_fees: fees,
            estimated_fees: fees,
        },

        Err(error) => {
            warn!(
                error:%, block_hash:%, transaction_size = transaction.len();
                "cannot get runtime API fees, using fallback"
            );
            TransactionFees::from_ledger_transaction(&ledger_transaction, transaction.len())
        }
    };

    let transaction = RegularTransaction {
        hash,
        protocol_version,
        identifiers,
        contract_actions,
        raw: transaction,
        paid_fees: fees.paid_fees,
        estimated_fees: fees.estimated_fees,
    };

    Ok(Transaction::Regular(transaction))
}

async fn make_system_transaction(
    transaction: ByteVec,
    protocol_version: ProtocolVersion,
) -> Result<Transaction, SubxtNodeError> {
    let ledger_transaction =
        ledger::SystemTransaction::deserialize(&transaction, protocol_version)?;

    let hash = ledger_transaction.hash();

    let transaction = SystemTransaction {
        hash,
        protocol_version,
        raw: transaction,
    };

    Ok(Transaction::System(transaction))
}
