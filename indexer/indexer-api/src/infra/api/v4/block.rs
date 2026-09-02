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
    domain::{self, storage::Storage},
    infra::api::{
        ApiResult, ContextExt, OptionExt, ResultExt,
        v4::{
            HexEncodable, HexEncoded,
            directives::beta,
            system_parameters::{DParameter, SystemParameters, TermsAndConditions},
            transaction::Transaction,
        },
    },
};
use async_graphql::{ComplexObject, Context, OneofObject, SimpleObject};
use derive_more::Debug;
use indexer_common::domain::BlockHash;
use std::marker::PhantomData;

/// A block with its relevant data.
#[derive(Debug, SimpleObject)]
#[graphql(complex)]
pub struct Block<S>
where
    S: Storage,
{
    /// The block hash.
    hash: HexEncoded,

    /// The block height.
    height: u32,

    /// The protocol version.
    protocol_version: u32,

    /// The UNIX timestamp.
    timestamp: u64,

    /// The hex-encoded block author.
    author: Option<HexEncoded>,

    /// The hex-encoded serialized zswap state Merkle tree root.
    #[debug(skip)]
    zswap_merkle_tree_root: HexEncoded,

    /// The hex-encoded ledger parameters for this block.
    ledger_parameters: HexEncoded,

    /// The zswap commitment tree end index at this block; exclusive, i.e. the next free index.
    zswap_end_index: u64,

    /// The dust commitment tree end index at this block; exclusive, i.e. the next free index.
    #[graphql(directive = beta::apply())]
    dust_commitment_end_index: u64,

    /// The dust generation tree end index at this block; exclusive, i.e. the next free index.
    #[graphql(directive = beta::apply())]
    dust_generation_end_index: u64,

    /// The hex-encoded dust commitment Merkle tree root at this block.
    #[graphql(directive = beta::apply())]
    dust_commitment_merkle_tree_root: Option<HexEncoded>,

    /// The hex-encoded dust generation Merkle tree root at this block.
    #[graphql(directive = beta::apply())]
    dust_generation_merkle_tree_root: Option<HexEncoded>,

    #[graphql(skip)]
    id: u64,

    #[graphql(skip)]
    raw_hash: BlockHash,

    #[graphql(skip)]
    parent_hash: BlockHash,

    #[graphql(skip)]
    _s: PhantomData<S>,
}

#[ComplexObject]
impl<S> Block<S>
where
    S: Storage,
{
    /// The parent of this block.
    async fn parent(&self, cx: &Context<'_>) -> ApiResult<Option<Block<S>>> {
        let block = cx
            .get_block_by_hash_loader::<S>()
            .load_one(self.parent_hash)
            .await
            .map_err_into_server_error(|| format!("get block by hash {}", self.parent_hash))?;

        Ok(block.map(Into::into))
    }

    /// The transactions within this block.
    async fn transactions(&self, cx: &Context<'_>) -> ApiResult<Vec<Transaction<S>>> {
        let transactions = cx
            .get_transactions_by_block_id_loader::<S>()
            .load_one(self.id)
            .await
            .map_err_into_server_error(|| format!("get transactions by block id {}", self.id))?
            .unwrap_or_default();

        Ok(transactions.into_iter().map(Into::into).collect())
    }

    /// The system parameters (governance) at this block height.
    async fn system_parameters(&self, cx: &Context<'_>) -> ApiResult<SystemParameters> {
        let storage = cx.get_storage::<S>();

        let d_param = storage
            .get_d_parameter_at(self.height)
            .await
            .map_err_into_server_error(|| {
                format!("get D-parameter at block height {}", self.height)
            })?
            .map(DParameter::from)
            .unwrap_or(DParameter {
                num_permissioned_candidates: 0,
                num_registered_candidates: 0,
            });

        let terms_and_conditions = storage
            .get_terms_and_conditions_at(self.height)
            .await
            .map_err_into_server_error(|| format!("get T&C at block height {}", self.height))?
            .map(TermsAndConditions::from);

        Ok(SystemParameters {
            d_parameter: d_param,
            terms_and_conditions,
        })
    }

    /// The zswap commitment tree filtered to the given contract address, resolved from this
    /// block's ledger state; null if the contract does not exist at this block. Hex-encoded.
    /// For building transactions, compose with `ledgerParameters` and `contract { state }` in
    /// one request, anchored to the same block; use the latest block: older trees age out of
    /// the ledger's root window, and blocks beyond the chain-indexer's ledger state retention
    /// window no longer have a loadable ledger state and yield an error.
    #[graphql(directive = beta::apply())]
    async fn contract_zswap_state(
        &self,
        cx: &Context<'_>,
        address: HexEncoded,
    ) -> ApiResult<Option<HexEncoded>> {
        let storage = cx.get_storage::<S>();

        let address = &address
            .hex_decode()
            .map_err_into_client_error(|| "invalid address")?;

        // Null when the contract does not exist as of this block.
        if !storage
            .contract_action_exists_by_address_as_of_block_hash(address, self.raw_hash)
            .await
            .map_err_into_server_error(|| {
                format!(
                    "check contract existence for address {address} as of block {}",
                    self.hash
                )
            })?
        {
            return Ok(None);
        }

        let (_, _, protocol_version, ledger_state_key) = storage
            .get_ledger_state_at(self.raw_hash)
            .await
            .map_err_into_server_error(|| format!("get ledger state at block {}", self.hash))?
            .some_or_server_error(|| format!("no ledger state for block {}", self.hash))?;

        // Verify the root node is still in the ledger DB before loading: loading culled state
        // panics inside storage-core rather than returning an error, so this check is what
        // turns a block beyond the chain-indexer's ledger_state_retention into a clean client
        // error.
        domain::LedgerState::root_loadable(&ledger_state_key, protocol_version.ledger_version())
            .map_err_into_server_error(|| "check ledger state availability")?
            .then_some(())
            .some_or_client_error(
                || "ledger state for this block is no longer available, query a more recent block",
            )?;

        let ledger_state =
            domain::LedgerState::load(&ledger_state_key, protocol_version.ledger_version())
                .map_err_into_server_error(|| "load ledger state")?;

        let zswap_state = ledger_state
            .extract_contract_zswap_state(address)
            .map_err_into_server_error(|| format!("extract zswap state for contract {address}"))?;

        Ok(Some(zswap_state.hex_encode()))
    }
}

impl<S> From<domain::Block> for Block<S>
where
    S: Storage,
{
    fn from(value: domain::Block) -> Self {
        let domain::Block {
            id,
            hash,
            height,
            protocol_version,
            author,
            timestamp,
            parent_hash,
            zswap_merkle_tree_root,
            ledger_parameters,
            zswap_end_index,
            dust_commitment_end_index,
            dust_generation_end_index,
            dust_commitment_merkle_tree_root,
            dust_generation_merkle_tree_root,
        } = value;

        Block {
            hash: hash.hex_encode(),
            height,
            protocol_version: protocol_version.into(),
            author: author.map(|author| author.hex_encode()),
            zswap_merkle_tree_root: zswap_merkle_tree_root.hex_encode(),
            ledger_parameters: ledger_parameters.hex_encode(),
            timestamp,
            zswap_end_index,
            dust_commitment_end_index,
            dust_generation_end_index,
            dust_commitment_merkle_tree_root: dust_commitment_merkle_tree_root
                .map(|root| root.hex_encode()),
            dust_generation_merkle_tree_root: dust_generation_merkle_tree_root
                .map(|root| root.hex_encode()),
            id,
            raw_hash: hash,
            parent_hash,
            _s: PhantomData,
        }
    }
}

/// Either a block hash or a block height.
#[derive(Debug, OneofObject)]
pub enum BlockOffset {
    /// A hex-encoded block hash.
    Hash(HexEncoded),

    /// A block height.
    Height(u32),
}
