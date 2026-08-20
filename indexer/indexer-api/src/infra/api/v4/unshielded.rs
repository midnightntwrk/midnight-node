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

//! GraphQL types for contract unshielded token balances.
use crate::{
    domain::{self, storage::Storage},
    infra::api::{
        ApiResult, ContextExt, OptionExt, ResultExt,
        v4::{
            AddressType, DecodeAddressError, HexEncodable, HexEncoded,
            block::BlockOffset,
            decode_address, encode_address,
            transaction::{Transaction, TransactionOffset},
        },
    },
};
use async_graphql::{ComplexObject, Context, OneofObject, SimpleObject, scalar};
use derive_more::{Debug, From};
use indexer_common::domain::{ByteArrayLenError, NetworkId};
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use thiserror::Error;

/// Represents an unshielded UTXO.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(complex)]
pub struct UnshieldedUtxo<S>
where
    S: Storage,
{
    /// Owner Bech32m-encoded address.
    owner: UnshieldedAddress,

    /// Token hex-encoded serialized token type.
    token_type: HexEncoded,

    /// UTXO value (quantity) as a string to support u128.
    value: String,

    /// The hex-encoded serialized intent hash.
    intent_hash: HexEncoded,

    /// Index of this output within its creating transaction.
    output_index: u32,

    /// The creation time in seconds.
    ctime: Option<u64>,

    /// The hex-encoded initial nonce for DUST generation tracking.
    initial_nonce: HexEncoded,

    /// Whether this UTXO is registered for DUST generation.
    registered_for_dust_generation: bool,

    #[graphql(skip)]
    creating_transaction_id: u64,

    #[graphql(skip)]
    spending_transaction_id: Option<u64>,

    #[graphql(skip)]
    _s: PhantomData<S>,
}

#[ComplexObject]
impl<S> UnshieldedUtxo<S>
where
    S: Storage,
{
    /// Transaction that created this UTXO.
    async fn created_at_transaction(&self, cx: &Context<'_>) -> ApiResult<Transaction<S>> {
        let id = self.creating_transaction_id;

        let transaction = cx
            .get_transaction_by_id_loader::<S>()
            .load_one(id)
            .await
            .map_err_into_server_error(|| format!("get transaction by id {id}"))?
            .some_or_server_error(|| format!("transaction with id {id} not found"))?;

        Ok(transaction.into())
    }

    /// Transaction that spent this UTXO.
    async fn spent_at_transaction(&self, cx: &Context<'_>) -> ApiResult<Option<Transaction<S>>> {
        let Some(id) = self.spending_transaction_id else {
            return Ok(None);
        };

        let transaction = cx
            .get_transaction_by_id_loader::<S>()
            .load_one(id)
            .await
            .map_err_into_server_error(|| format!("get transaction by id {id}"))?
            .some_or_server_error(|| format!("transaction with id {id} not found"))?;

        Ok(Some(transaction.into()))
    }
}

impl<S> From<(domain::UnshieldedUtxo, &NetworkId)> for UnshieldedUtxo<S>
where
    S: Storage,
{
    fn from((utxo, network_id): (domain::UnshieldedUtxo, &NetworkId)) -> Self {
        let domain::UnshieldedUtxo {
            creating_transaction_id,
            spending_transaction_id,
            owner,
            token_type,
            value,
            intent_hash,
            output_index,
            ctime,
            initial_nonce,
            registered_for_dust_generation,
        } = utxo;

        Self {
            creating_transaction_id,
            spending_transaction_id,
            owner: encode_address(owner, AddressType::Unshielded, network_id).into(),
            token_type: token_type.hex_encode(),
            value: value.to_string(),
            intent_hash: intent_hash.hex_encode(),
            output_index,
            ctime,
            initial_nonce: initial_nonce.hex_encode(),
            registered_for_dust_generation,
            _s: PhantomData,
        }
    }
}

/// Either a block offset or a transaction offset.
#[derive(Debug, OneofObject)]
pub enum UnshieldedOffset {
    /// Either a block hash or a block height.
    BlockOffset(BlockOffset),

    /// Either a transaction hash or a transaction identifier.
    TransactionOffset(TransactionOffset),
}

/// Bech32m-encoded unshielded address.
///
/// The format depends on the network ID:
/// - Mainnet: `mn_addr` + bech32m data (no network ID suffix)
/// - Other networks: `mn_addr_` + network-id + bech32m data
#[derive(Debug, Clone, PartialEq, Eq, Hash, From, Serialize, Deserialize)]
pub struct UnshieldedAddress(pub String);

scalar!(UnshieldedAddress);

impl UnshieldedAddress {
    /// Converts this API address into a domain address, validating the bech32m format and
    /// network ID.
    pub fn try_into_domain(
        &self,
        network_id: &NetworkId,
    ) -> Result<indexer_common::domain::UnshieldedAddress, UnshieldedAddressFormatError> {
        let bytes = decode_address(&self.0, AddressType::Unshielded, network_id)?;
        let address = bytes.0.try_into()?;

        Ok(address)
    }
}

#[derive(Debug, Error)]
pub enum UnshieldedAddressFormatError {
    #[error("cannot bech32m-decode unshielded address")]
    Decode(#[from] DecodeAddressError),

    #[error("cannot convert into unshielded address")]
    ByteArrayLen(#[from] ByteArrayLenError),
}

/// Represents a token balance held by a contract.
/// This type is exposed through the GraphQL API to allow clients to query
/// unshielded token balances for any contract action (Deploy, Call, Update).
#[derive(Debug, Clone, PartialEq, Eq, SimpleObject)]
pub struct ContractBalance {
    /// Hex-encoded token type identifier.
    pub token_type: HexEncoded,

    /// Balance amount as string to support larger integer values (up to 16 bytes).
    pub amount: String,
}

impl From<domain::ContractBalance> for ContractBalance {
    fn from(balance: domain::ContractBalance) -> Self {
        let domain::ContractBalance { token_type, amount } = balance;
        Self {
            token_type: token_type.hex_encode(),
            amount: amount.to_string(),
        }
    }
}
