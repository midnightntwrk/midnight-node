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

//! GraphQL types for the `contractEvents` query and subscription surface (#1161, public
//! contract events per MIP-0002 / CoIP-442).
//!
//! `ContractEvent` is the polymorphic interface; concrete types per `LogEventType` variant
//! (`ShieldedSpendEvent`, `MiscContractEvent`, etc.) implement it. Clients discriminate via
//! `__typename`.

use crate::{
    domain::{
        ContractEventRow,
        storage::{
            Storage,
            contract_event::{
                ContractEventFilter as DomainContractEventFilter, FieldPrefix as DomainFieldPrefix,
            },
        },
    },
    infra::api::{
        ApiResult,
        v4::{
            HexDecodeError, HexEncodable, HexEncoded, contract_action::get_transaction_by_id,
            directives::beta, transaction::Transaction,
        },
    },
};
use async_graphql::{ComplexObject, Context, Enum, InputObject, Interface, SimpleObject};
use derive_more::Debug;
use indexer_common::domain::{
    AddressOrContract as DomainAddressOrContract, ByteVec, INDEXABLE_CONTRACT_FIELD_NAMES,
    LedgerEventAttributes, SerializedContractAddress, TransactionHash,
};
use std::marker::PhantomData;
use thiserror::Error;

/// Tagged-union helper for fields like `Either<ZswapCoinPublicKey, ContractAddress>`
/// used in standard unshielded events.
///
/// Exactly one of `userAddress` or `contractAddress` is non-null; the `kind`
/// discriminator says which.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(directive = beta::apply())]
pub struct AddressOrContract {
    /// Which of the two address fields is populated.
    pub kind: AddressOrContractKind,

    /// The hex-encoded user address; populated when kind is USER.
    pub user_address: Option<HexEncoded>,

    /// The hex-encoded contract address; populated when kind is CONTRACT.
    pub contract_address: Option<HexEncoded>,
}

/// Discriminator for `AddressOrContract`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum)]
pub enum AddressOrContractKind {
    /// The address is a user (zswap coin public key) address.
    User,

    /// The address is a contract address.
    Contract,
}

impl From<DomainAddressOrContract> for AddressOrContract {
    fn from(domain: DomainAddressOrContract) -> Self {
        match domain {
            DomainAddressOrContract::User(b) => Self {
                kind: AddressOrContractKind::User,
                user_address: Some(b.hex_encode()),
                contract_address: None,
            },
            DomainAddressOrContract::Contract(b) => Self {
                kind: AddressOrContractKind::Contract,
                user_address: None,
                contract_address: Some(b.hex_encode()),
            },
        }
    }
}

/// Closed enum of contract event types the indexer surfaces. Used in filter
/// input only, response discrimination is via `__typename`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum)]
pub enum ContractEventType {
    /// A contract spends a shielded coin.
    ShieldedSpend,

    /// A contract or user receives a shielded coin.
    ShieldedReceive,

    /// A contract mints a shielded coin.
    ShieldedMint,

    /// A contract burns a shielded coin.
    ShieldedBurn,

    /// A contract spends an unshielded coin.
    UnshieldedSpend,

    /// A contract or user receives an unshielded coin.
    UnshieldedReceive,

    /// A contract mints an unshielded coin.
    UnshieldedMint,

    /// A contract burns an unshielded coin.
    UnshieldedBurn,

    /// A contract is paused.
    Paused,

    /// A contract is unpaused.
    Unpaused,

    /// A custom event emitted by a contract.
    Misc,
}

/// Prefix filter on an indexed field of a standard event; not supported on Misc events.
#[derive(Debug, Clone, InputObject)]
pub struct FieldPrefixFilter {
    /// The indexed field name; one of `nullifier`, `commitment`, `ciphertext`, `domainSep`,
    /// `tokenType`, `sender` or `recipient`.
    pub field_name: String,

    /// The hex-encoded field value prefix. An empty string matches all values; otherwise
    /// events whose field value starts with this prefix are returned.
    pub prefix: HexEncoded,
}

/// Filter for contract events queries and subscriptions; block-range bounds live here so the
/// same shape works for both.
#[derive(Debug, Clone, InputObject)]
pub struct ContractEventFilter {
    /// The hex-encoded contract address to filter events for.
    pub contract_address: HexEncoded,

    /// Event types to narrow to; must be non-empty when given.
    pub types: Option<Vec<ContractEventType>>,

    /// Prefix matches on indexed event fields, combined with AND semantics; standard events
    /// only.
    pub field_prefixes: Option<Vec<FieldPrefixFilter>>,

    /// Lower bound on the block height an event was emitted in; on subscription this acts as
    /// a starting cursor alongside `id`.
    pub from_block: Option<u32>,

    /// Upper bound on the block height an event was emitted in; on subscription, the stream
    /// completes once the chain has reached this block.
    pub to_block: Option<u32>,

    /// The hex-encoded transaction hash to narrow to events emitted by that transaction.
    pub transaction_hash: Option<HexEncoded>,
}

/// Common interface implemented by every concrete contract event type.
#[derive(Debug, Interface)]
#[allow(clippy::duplicated_attributes)]
#[graphql(
    field(name = "id", ty = "&u64"),
    field(name = "raw", ty = "&HexEncoded"),
    field(name = "max_id", ty = "&u64"),
    field(name = "protocol_version", ty = "&u32"),
    field(name = "version", ty = "&u32"),
    field(name = "contract_address", ty = "&HexEncoded"),
    field(name = "transaction_id", ty = "&u64"),
    field(name = "transaction", ty = "ApiResult<Transaction<S>>")
)]
pub enum ContractEvent<S: Storage> {
    ShieldedSpend(ShieldedSpendEvent<S>),
    ShieldedReceive(ShieldedReceiveEvent<S>),
    ShieldedMint(ShieldedMintEvent<S>),
    ShieldedBurn(ShieldedBurnEvent<S>),
    UnshieldedSpend(UnshieldedSpendEvent<S>),
    UnshieldedReceive(UnshieldedReceiveEvent<S>),
    UnshieldedMint(UnshieldedMintEvent<S>),
    UnshieldedBurn(UnshieldedBurnEvent<S>),
    Paused(PausedEvent<S>),
    Unpaused(UnpausedEvent<S>),
    Misc(MiscContractEvent<S>),
}

// ============================================================================
// Shared base data extracted once per row, then folded into each concrete type.
// ============================================================================

struct Base {
    id: u64,
    raw: HexEncoded,
    max_id: u64,
    protocol_version: u32,
    version: u32,
    contract_address: HexEncoded,
    transaction_id: u64,
}

impl Base {
    fn from_row(row: &ContractEventRow, version: u32) -> Self {
        Self {
            id: row.id,
            raw: row.raw.hex_encode(),
            max_id: row.max_id,
            protocol_version: row.protocol_version.into(),
            version,
            contract_address: row.contract_address.hex_encode(),
            transaction_id: row.transaction_id,
        }
    }
}

// ============================================================================
// Shielded concrete types
// ============================================================================

#[derive(Debug, SimpleObject)]
#[graphql(complex, directive = beta::apply())]
pub struct ShieldedSpendEvent<S>
where
    S: Storage,
{
    /// The ID of this contract event.
    pub id: u64,

    /// The hex-encoded serialized event payload.
    pub raw: HexEncoded,

    /// The maximum ID of all contract events.
    pub max_id: u64,

    /// The protocol version.
    pub protocol_version: u32,

    /// The event schema version, as declared by the emitting contract's compiler.
    pub version: u32,

    /// The hex-encoded address of the emitting contract.
    pub contract_address: HexEncoded,

    /// The ID of the transaction this event was emitted from.
    pub transaction_id: u64,
    /// The hex-encoded nullifier; filterable via `fieldPrefixes`.
    pub nullifier: HexEncoded,
    #[graphql(skip)]
    _s: PhantomData<S>,
}

#[ComplexObject]
impl<S> ShieldedSpendEvent<S>
where
    S: Storage,
{
    /// The transaction this event was emitted from.
    async fn transaction(&self, cx: &Context<'_>) -> ApiResult<Transaction<S>> {
        get_transaction_by_id(self.transaction_id, cx).await
    }
}

#[derive(Debug, SimpleObject)]
#[graphql(complex, directive = beta::apply())]
pub struct ShieldedReceiveEvent<S>
where
    S: Storage,
{
    /// The ID of this contract event.
    pub id: u64,

    /// The hex-encoded serialized event payload.
    pub raw: HexEncoded,

    /// The maximum ID of all contract events.
    pub max_id: u64,

    /// The protocol version.
    pub protocol_version: u32,

    /// The event schema version, as declared by the emitting contract's compiler.
    pub version: u32,

    /// The hex-encoded address of the emitting contract.
    pub contract_address: HexEncoded,

    /// The ID of the transaction this event was emitted from.
    pub transaction_id: u64,
    /// The hex-encoded commitment; filterable via `fieldPrefixes`.
    pub commitment: HexEncoded,
    /// The hex-encoded optional ciphertext of the shielded coin receipt, up to 512 bytes;
    /// filterable via `fieldPrefixes`.
    pub ciphertext: Option<HexEncoded>,

    /// The hex-encoded receiving contract address; set when received by a contract, null for
    /// user recipients. Named to avoid collision with the emitting `contractAddress`.
    pub receiving_contract_address: Option<HexEncoded>,
    #[graphql(skip)]
    _s: PhantomData<S>,
}

#[ComplexObject]
impl<S> ShieldedReceiveEvent<S>
where
    S: Storage,
{
    /// The transaction this event was emitted from.
    async fn transaction(&self, cx: &Context<'_>) -> ApiResult<Transaction<S>> {
        get_transaction_by_id(self.transaction_id, cx).await
    }
}

#[derive(Debug, SimpleObject)]
#[graphql(complex, directive = beta::apply())]
pub struct ShieldedMintEvent<S>
where
    S: Storage,
{
    /// The ID of this contract event.
    pub id: u64,

    /// The hex-encoded serialized event payload.
    pub raw: HexEncoded,

    /// The maximum ID of all contract events.
    pub max_id: u64,

    /// The protocol version.
    pub protocol_version: u32,

    /// The event schema version, as declared by the emitting contract's compiler.
    pub version: u32,

    /// The hex-encoded address of the emitting contract.
    pub contract_address: HexEncoded,

    /// The ID of the transaction this event was emitted from.
    pub transaction_id: u64,
    /// The hex-encoded commitment; filterable via `fieldPrefixes`.
    pub commitment: HexEncoded,
    /// The hex-encoded domain separator; filterable via `fieldPrefixes`.
    pub domain_sep: HexEncoded,

    /// The optional amount as a decimal string; hidden in some shielded mints.
    pub amount: Option<String>,
    #[graphql(skip)]
    _s: PhantomData<S>,
}

#[ComplexObject]
impl<S> ShieldedMintEvent<S>
where
    S: Storage,
{
    /// The transaction this event was emitted from.
    async fn transaction(&self, cx: &Context<'_>) -> ApiResult<Transaction<S>> {
        get_transaction_by_id(self.transaction_id, cx).await
    }
}

#[derive(Debug, SimpleObject)]
#[graphql(complex, directive = beta::apply())]
pub struct ShieldedBurnEvent<S>
where
    S: Storage,
{
    /// The ID of this contract event.
    pub id: u64,

    /// The hex-encoded serialized event payload.
    pub raw: HexEncoded,

    /// The maximum ID of all contract events.
    pub max_id: u64,

    /// The protocol version.
    pub protocol_version: u32,

    /// The event schema version, as declared by the emitting contract's compiler.
    pub version: u32,

    /// The hex-encoded address of the emitting contract.
    pub contract_address: HexEncoded,

    /// The ID of the transaction this event was emitted from.
    pub transaction_id: u64,
    /// The hex-encoded nullifier; filterable via `fieldPrefixes`.
    pub nullifier: HexEncoded,
    /// The optional amount as a decimal string; hidden in some shielded burns.
    pub amount: Option<String>,
    #[graphql(skip)]
    _s: PhantomData<S>,
}

#[ComplexObject]
impl<S> ShieldedBurnEvent<S>
where
    S: Storage,
{
    /// The transaction this event was emitted from.
    async fn transaction(&self, cx: &Context<'_>) -> ApiResult<Transaction<S>> {
        get_transaction_by_id(self.transaction_id, cx).await
    }
}

// ============================================================================
// Unshielded concrete types (CoIP-442 head: Spend/Receive feature both
// domainSep + tokenType; Burn features tokenType only; Mint features both).
// ============================================================================

#[derive(Debug, SimpleObject)]
#[graphql(complex, directive = beta::apply())]
pub struct UnshieldedSpendEvent<S>
where
    S: Storage,
{
    /// The ID of this contract event.
    pub id: u64,

    /// The hex-encoded serialized event payload.
    pub raw: HexEncoded,

    /// The maximum ID of all contract events.
    pub max_id: u64,

    /// The protocol version.
    pub protocol_version: u32,

    /// The event schema version, as declared by the emitting contract's compiler.
    pub version: u32,

    /// The hex-encoded address of the emitting contract.
    pub contract_address: HexEncoded,

    /// The ID of the transaction this event was emitted from.
    pub transaction_id: u64,
    /// The spending user or contract address; filterable via `fieldPrefixes`.
    pub sender: AddressOrContract,

    /// The hex-encoded domain separator; filterable via `fieldPrefixes`.
    pub domain_sep: HexEncoded,

    /// The hex-encoded token type; filterable via `fieldPrefixes`.
    pub token_type: HexEncoded,

    /// The amount as a decimal string.
    pub amount: String,
    #[graphql(skip)]
    _s: PhantomData<S>,
}

#[ComplexObject]
impl<S> UnshieldedSpendEvent<S>
where
    S: Storage,
{
    /// The transaction this event was emitted from.
    async fn transaction(&self, cx: &Context<'_>) -> ApiResult<Transaction<S>> {
        get_transaction_by_id(self.transaction_id, cx).await
    }
}

#[derive(Debug, SimpleObject)]
#[graphql(complex, directive = beta::apply())]
pub struct UnshieldedReceiveEvent<S>
where
    S: Storage,
{
    /// The ID of this contract event.
    pub id: u64,

    /// The hex-encoded serialized event payload.
    pub raw: HexEncoded,

    /// The maximum ID of all contract events.
    pub max_id: u64,

    /// The protocol version.
    pub protocol_version: u32,

    /// The event schema version, as declared by the emitting contract's compiler.
    pub version: u32,

    /// The hex-encoded address of the emitting contract.
    pub contract_address: HexEncoded,

    /// The ID of the transaction this event was emitted from.
    pub transaction_id: u64,
    /// The receiving user or contract address; filterable via `fieldPrefixes`.
    pub recipient: AddressOrContract,

    /// The hex-encoded domain separator; filterable via `fieldPrefixes`.
    pub domain_sep: HexEncoded,

    /// The hex-encoded token type; filterable via `fieldPrefixes`.
    pub token_type: HexEncoded,

    /// The amount as a decimal string.
    pub amount: String,
    #[graphql(skip)]
    _s: PhantomData<S>,
}

#[ComplexObject]
impl<S> UnshieldedReceiveEvent<S>
where
    S: Storage,
{
    /// The transaction this event was emitted from.
    async fn transaction(&self, cx: &Context<'_>) -> ApiResult<Transaction<S>> {
        get_transaction_by_id(self.transaction_id, cx).await
    }
}

#[derive(Debug, SimpleObject)]
#[graphql(complex, directive = beta::apply())]
pub struct UnshieldedMintEvent<S>
where
    S: Storage,
{
    /// The ID of this contract event.
    pub id: u64,

    /// The hex-encoded serialized event payload.
    pub raw: HexEncoded,

    /// The maximum ID of all contract events.
    pub max_id: u64,

    /// The protocol version.
    pub protocol_version: u32,

    /// The event schema version, as declared by the emitting contract's compiler.
    pub version: u32,

    /// The hex-encoded address of the emitting contract.
    pub contract_address: HexEncoded,

    /// The ID of the transaction this event was emitted from.
    pub transaction_id: u64,
    /// The hex-encoded domain separator; filterable via `fieldPrefixes`.
    pub domain_sep: HexEncoded,

    /// The hex-encoded token type; filterable via `fieldPrefixes`.
    pub token_type: HexEncoded,

    /// The amount as a decimal string.
    pub amount: String,
    #[graphql(skip)]
    _s: PhantomData<S>,
}

#[ComplexObject]
impl<S> UnshieldedMintEvent<S>
where
    S: Storage,
{
    /// The transaction this event was emitted from.
    async fn transaction(&self, cx: &Context<'_>) -> ApiResult<Transaction<S>> {
        get_transaction_by_id(self.transaction_id, cx).await
    }
}

#[derive(Debug, SimpleObject)]
#[graphql(complex, directive = beta::apply())]
pub struct UnshieldedBurnEvent<S>
where
    S: Storage,
{
    /// The ID of this contract event.
    pub id: u64,

    /// The hex-encoded serialized event payload.
    pub raw: HexEncoded,

    /// The maximum ID of all contract events.
    pub max_id: u64,

    /// The protocol version.
    pub protocol_version: u32,

    /// The event schema version, as declared by the emitting contract's compiler.
    pub version: u32,

    /// The hex-encoded address of the emitting contract.
    pub contract_address: HexEncoded,

    /// The ID of the transaction this event was emitted from.
    pub transaction_id: u64,
    /// The spending user or contract address; filterable via `fieldPrefixes`.
    pub sender: AddressOrContract,

    /// The hex-encoded token type; filterable via `fieldPrefixes`.
    pub token_type: HexEncoded,

    /// The amount as a decimal string.
    pub amount: String,
    #[graphql(skip)]
    _s: PhantomData<S>,
}

#[ComplexObject]
impl<S> UnshieldedBurnEvent<S>
where
    S: Storage,
{
    /// The transaction this event was emitted from.
    async fn transaction(&self, cx: &Context<'_>) -> ApiResult<Transaction<S>> {
        get_transaction_by_id(self.transaction_id, cx).await
    }
}

// ============================================================================
// Signal-only events (empty payload structs `Paused`, `Unpaused`).
// ============================================================================

#[derive(Debug, SimpleObject)]
#[graphql(complex, directive = beta::apply())]
pub struct PausedEvent<S>
where
    S: Storage,
{
    /// The ID of this contract event.
    pub id: u64,

    /// The hex-encoded serialized event payload.
    pub raw: HexEncoded,

    /// The maximum ID of all contract events.
    pub max_id: u64,

    /// The protocol version.
    pub protocol_version: u32,

    /// The event schema version, as declared by the emitting contract's compiler.
    pub version: u32,

    /// The hex-encoded address of the emitting contract.
    pub contract_address: HexEncoded,

    /// The ID of the transaction this event was emitted from.
    pub transaction_id: u64,
    #[graphql(skip)]
    _s: PhantomData<S>,
}

#[ComplexObject]
impl<S> PausedEvent<S>
where
    S: Storage,
{
    /// The transaction this event was emitted from.
    async fn transaction(&self, cx: &Context<'_>) -> ApiResult<Transaction<S>> {
        get_transaction_by_id(self.transaction_id, cx).await
    }
}

#[derive(Debug, SimpleObject)]
#[graphql(complex, directive = beta::apply())]
pub struct UnpausedEvent<S>
where
    S: Storage,
{
    /// The ID of this contract event.
    pub id: u64,

    /// The hex-encoded serialized event payload.
    pub raw: HexEncoded,

    /// The maximum ID of all contract events.
    pub max_id: u64,

    /// The protocol version.
    pub protocol_version: u32,

    /// The event schema version, as declared by the emitting contract's compiler.
    pub version: u32,

    /// The hex-encoded address of the emitting contract.
    pub contract_address: HexEncoded,

    /// The ID of the transaction this event was emitted from.
    pub transaction_id: u64,
    #[graphql(skip)]
    _s: PhantomData<S>,
}

#[ComplexObject]
impl<S> UnpausedEvent<S>
where
    S: Storage,
{
    /// The transaction this event was emitted from.
    async fn transaction(&self, cx: &Context<'_>) -> ApiResult<Transaction<S>> {
        get_transaction_by_id(self.transaction_id, cx).await
    }
}

// ============================================================================
// Custom (Misc) event — opaque payload up to 256 bytes.
// ============================================================================

#[derive(Debug, SimpleObject)]
#[graphql(complex, directive = beta::apply())]
pub struct MiscContractEvent<S>
where
    S: Storage,
{
    /// The ID of this contract event.
    pub id: u64,

    /// The hex-encoded serialized event payload.
    pub raw: HexEncoded,

    /// The maximum ID of all contract events.
    pub max_id: u64,

    /// The protocol version.
    pub protocol_version: u32,

    /// The event schema version, as declared by the emitting contract's compiler.
    pub version: u32,

    /// The hex-encoded address of the emitting contract.
    pub contract_address: HexEncoded,

    /// The ID of the transaction this event was emitted from.
    pub transaction_id: u64,
    /// The hex-encoded contract-defined event name (32 bytes).
    pub name: HexEncoded,

    /// The hex-encoded opaque event payload, up to 256 bytes; consumers decode it with their
    /// contract's descriptor.
    pub payload: HexEncoded,
    #[graphql(skip)]
    _s: PhantomData<S>,
}

#[ComplexObject]
impl<S> MiscContractEvent<S>
where
    S: Storage,
{
    /// The transaction this event was emitted from.
    async fn transaction(&self, cx: &Context<'_>) -> ApiResult<Transaction<S>> {
        get_transaction_by_id(self.transaction_id, cx).await
    }
}

// ============================================================================
// TryFrom: discriminate via attributes, fold base + per-variant fields.
// ============================================================================

impl<S> TryFrom<ContractEventRow> for ContractEvent<S>
where
    S: Storage,
{
    type Error = Box<UnexpectedContractEvent>;

    fn try_from(row: ContractEventRow) -> Result<Self, Self::Error> {
        match &row.attributes {
            LedgerEventAttributes::ContractShieldedSpend {
                version, nullifier, ..
            } => {
                let base = Base::from_row(&row, *version);
                Ok(ContractEvent::ShieldedSpend(ShieldedSpendEvent {
                    id: base.id,
                    raw: base.raw,
                    max_id: base.max_id,
                    protocol_version: base.protocol_version,
                    version: base.version,
                    contract_address: base.contract_address,
                    transaction_id: base.transaction_id,
                    nullifier: nullifier.hex_encode(),
                    _s: PhantomData,
                }))
            }

            LedgerEventAttributes::ContractShieldedReceive {
                version,
                commitment,
                ciphertext,
                receiving_contract_address,
                ..
            } => {
                let base = Base::from_row(&row, *version);
                Ok(ContractEvent::ShieldedReceive(ShieldedReceiveEvent {
                    id: base.id,
                    raw: base.raw,
                    max_id: base.max_id,
                    protocol_version: base.protocol_version,
                    version: base.version,
                    contract_address: base.contract_address,
                    transaction_id: base.transaction_id,
                    commitment: commitment.hex_encode(),
                    ciphertext: ciphertext.as_ref().map(|b| b.hex_encode()),
                    receiving_contract_address: receiving_contract_address
                        .as_ref()
                        .map(|b| b.hex_encode()),
                    _s: PhantomData,
                }))
            }

            LedgerEventAttributes::ContractShieldedMint {
                version,
                commitment,
                domain_sep,
                amount,
                ..
            } => {
                let base = Base::from_row(&row, *version);
                Ok(ContractEvent::ShieldedMint(ShieldedMintEvent {
                    id: base.id,
                    raw: base.raw,
                    max_id: base.max_id,
                    protocol_version: base.protocol_version,
                    version: base.version,
                    contract_address: base.contract_address,
                    transaction_id: base.transaction_id,
                    commitment: commitment.hex_encode(),
                    domain_sep: domain_sep.hex_encode(),
                    amount: amount.clone(),
                    _s: PhantomData,
                }))
            }

            LedgerEventAttributes::ContractShieldedBurn {
                version,
                nullifier,
                amount,
                ..
            } => {
                let base = Base::from_row(&row, *version);
                Ok(ContractEvent::ShieldedBurn(ShieldedBurnEvent {
                    id: base.id,
                    raw: base.raw,
                    max_id: base.max_id,
                    protocol_version: base.protocol_version,
                    version: base.version,
                    contract_address: base.contract_address,
                    transaction_id: base.transaction_id,
                    nullifier: nullifier.hex_encode(),
                    amount: amount.clone(),
                    _s: PhantomData,
                }))
            }

            LedgerEventAttributes::ContractUnshieldedSpend {
                version,
                sender,
                domain_sep,
                token_type,
                amount,
                ..
            } => {
                let base = Base::from_row(&row, *version);
                Ok(ContractEvent::UnshieldedSpend(UnshieldedSpendEvent {
                    id: base.id,
                    raw: base.raw,
                    max_id: base.max_id,
                    protocol_version: base.protocol_version,
                    version: base.version,
                    contract_address: base.contract_address,
                    transaction_id: base.transaction_id,
                    sender: sender.clone().into(),
                    domain_sep: domain_sep.hex_encode(),
                    token_type: token_type.hex_encode(),
                    amount: amount.clone(),
                    _s: PhantomData,
                }))
            }

            LedgerEventAttributes::ContractUnshieldedReceive {
                version,
                recipient,
                domain_sep,
                token_type,
                amount,
                ..
            } => {
                let base = Base::from_row(&row, *version);
                Ok(ContractEvent::UnshieldedReceive(UnshieldedReceiveEvent {
                    id: base.id,
                    raw: base.raw,
                    max_id: base.max_id,
                    protocol_version: base.protocol_version,
                    version: base.version,
                    contract_address: base.contract_address,
                    transaction_id: base.transaction_id,
                    recipient: recipient.clone().into(),
                    domain_sep: domain_sep.hex_encode(),
                    token_type: token_type.hex_encode(),
                    amount: amount.clone(),
                    _s: PhantomData,
                }))
            }

            LedgerEventAttributes::ContractUnshieldedMint {
                version,
                domain_sep,
                token_type,
                amount,
                ..
            } => {
                let base = Base::from_row(&row, *version);
                Ok(ContractEvent::UnshieldedMint(UnshieldedMintEvent {
                    id: base.id,
                    raw: base.raw,
                    max_id: base.max_id,
                    protocol_version: base.protocol_version,
                    version: base.version,
                    contract_address: base.contract_address,
                    transaction_id: base.transaction_id,
                    domain_sep: domain_sep.hex_encode(),
                    token_type: token_type.hex_encode(),
                    amount: amount.clone(),
                    _s: PhantomData,
                }))
            }

            LedgerEventAttributes::ContractUnshieldedBurn {
                version,
                sender,
                token_type,
                amount,
                ..
            } => {
                let base = Base::from_row(&row, *version);
                Ok(ContractEvent::UnshieldedBurn(UnshieldedBurnEvent {
                    id: base.id,
                    raw: base.raw,
                    max_id: base.max_id,
                    protocol_version: base.protocol_version,
                    version: base.version,
                    contract_address: base.contract_address,
                    transaction_id: base.transaction_id,
                    sender: sender.clone().into(),
                    token_type: token_type.hex_encode(),
                    amount: amount.clone(),
                    _s: PhantomData,
                }))
            }

            LedgerEventAttributes::ContractPaused { version, .. } => {
                let base = Base::from_row(&row, *version);
                Ok(ContractEvent::Paused(PausedEvent {
                    id: base.id,
                    raw: base.raw,
                    max_id: base.max_id,
                    protocol_version: base.protocol_version,
                    version: base.version,
                    contract_address: base.contract_address,
                    transaction_id: base.transaction_id,
                    _s: PhantomData,
                }))
            }

            LedgerEventAttributes::ContractUnpaused { version, .. } => {
                let base = Base::from_row(&row, *version);
                Ok(ContractEvent::Unpaused(UnpausedEvent {
                    id: base.id,
                    raw: base.raw,
                    max_id: base.max_id,
                    protocol_version: base.protocol_version,
                    version: base.version,
                    contract_address: base.contract_address,
                    transaction_id: base.transaction_id,
                    _s: PhantomData,
                }))
            }

            LedgerEventAttributes::ContractMisc {
                version,
                name,
                payload,
                ..
            } => {
                let base = Base::from_row(&row, *version);
                Ok(ContractEvent::Misc(MiscContractEvent {
                    id: base.id,
                    raw: base.raw,
                    max_id: base.max_id,
                    protocol_version: base.protocol_version,
                    version: base.version,
                    contract_address: base.contract_address,
                    transaction_id: base.transaction_id,
                    name: name.hex_encode(),
                    payload: payload.hex_encode(),
                    _s: PhantomData,
                }))
            }

            other => Err(Box::new(UnexpectedContractEvent(other.clone()))),
        }
    }
}

#[derive(Debug, Error)]
#[error("unexpected ledger event for contract events surface: {0:?}")]
pub struct UnexpectedContractEvent(LedgerEventAttributes);

// ============================================================================
// GraphQL filter → domain filter conversion.
// ============================================================================

impl ContractEventFilter {
    /// Convert into the domain filter shape consumed by the storage layer, validating the
    /// user-supplied fields.
    pub fn into_domain(self) -> Result<DomainContractEventFilter, ContractEventFilterError> {
        let contract_address = self
            .contract_address
            .hex_decode::<SerializedContractAddress>()
            .map_err(ContractEventFilterError::InvalidContractAddress)?;
        if contract_address.as_ref().is_empty() {
            return Err(ContractEventFilterError::EmptyContractAddress);
        }

        let variants = match self.types {
            Some(types) if types.is_empty() => {
                return Err(ContractEventFilterError::EmptyTypes);
            }

            Some(types) => types
                .into_iter()
                .map(ContractEventType::variant_name)
                .collect(),

            None => Vec::new(),
        };

        let field_prefixes = self
            .field_prefixes
            .unwrap_or_default()
            .into_iter()
            .map(|field_prefix| {
                if !INDEXABLE_CONTRACT_FIELD_NAMES.contains(&field_prefix.field_name.as_str()) {
                    return Err(ContractEventFilterError::UnknownFieldName(
                        field_prefix.field_name,
                    ));
                }

                let prefix = field_prefix
                    .prefix
                    .hex_decode::<ByteVec>()
                    .map_err(ContractEventFilterError::InvalidFieldPrefix)?;

                Ok(DomainFieldPrefix {
                    field_name: field_prefix.field_name,
                    prefix,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        if let (Some(from_block), Some(to_block)) = (self.from_block, self.to_block)
            && from_block > to_block
        {
            return Err(ContractEventFilterError::InvalidBlockRange {
                from_block,
                to_block,
            });
        }

        let transaction_hash = self
            .transaction_hash
            .map(|transaction_hash| {
                transaction_hash
                    .hex_decode::<TransactionHash>()
                    .map_err(ContractEventFilterError::InvalidTransactionHash)
            })
            .transpose()?;

        Ok(DomainContractEventFilter {
            contract_address,
            variants,
            field_prefixes,
            from_block: self.from_block,
            to_block: self.to_block,
            transaction_hash,
        })
    }
}

impl ContractEventType {
    /// The `LEDGER_EVENT_VARIANT` value matching this type.
    fn variant_name(self) -> &'static str {
        match self {
            Self::ShieldedSpend => "ShieldedSpend",
            Self::ShieldedReceive => "ShieldedReceive",
            Self::ShieldedMint => "ShieldedMint",
            Self::ShieldedBurn => "ShieldedBurn",
            Self::UnshieldedSpend => "UnshieldedSpend",
            Self::UnshieldedReceive => "UnshieldedReceive",
            Self::UnshieldedMint => "UnshieldedMint",
            Self::UnshieldedBurn => "UnshieldedBurn",
            Self::Paused => "Paused",
            Self::Unpaused => "Unpaused",
            Self::Misc => "Misc",
        }
    }
}

#[derive(Debug, Error)]
pub enum ContractEventFilterError {
    #[error("invalid contractAddress")]
    InvalidContractAddress(#[source] HexDecodeError),

    #[error("contractAddress must be non-empty")]
    EmptyContractAddress,

    #[error("types must not be empty")]
    EmptyTypes,

    #[error("unknown fieldPrefixes.fieldName {0}")]
    UnknownFieldName(String),

    #[error("invalid fieldPrefixes.prefix")]
    InvalidFieldPrefix(#[source] HexDecodeError),

    #[error("fromBlock {from_block} must not exceed toBlock {to_block}")]
    InvalidBlockRange { from_block: u32, to_block: u32 },

    #[error("invalid transactionHash")]
    InvalidTransactionHash(#[source] HexDecodeError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::storage::NoopStorage;
    use indexer_common::domain::{ByteVec, ProtocolVersion, SerializedLedgerEvent};

    fn bv(bytes: &[u8]) -> ByteVec {
        ByteVec::from(bytes.to_vec())
    }

    fn make_row(attributes: LedgerEventAttributes) -> ContractEventRow {
        ContractEventRow {
            id: 42,
            contract_address: bv(&[0x01; 32]),
            transaction_id: 7,
            contract_action_id: Some(99),
            raw: SerializedLedgerEvent::from(vec![0xab, 0xcd]),
            attributes,
            max_id: 100,
            protocol_version: ProtocolVersion::V1_0(0),
        }
    }

    #[test]
    fn try_from_shielded_spend_yields_correct_variant() {
        let row = make_row(LedgerEventAttributes::ContractShieldedSpend {
            version: 1,
            entry_point: bv(b"spend"),
            nullifier: bv(&[0xaa; 32]),
        });

        let event = ContractEvent::<NoopStorage>::try_from(row).expect("try_from");
        assert!(matches!(event, ContractEvent::ShieldedSpend(_)));
        if let ContractEvent::ShieldedSpend(e) = event {
            assert_eq!(e.id, 42);
            assert_eq!(e.version, 1);
            assert_eq!(e.transaction_id, 7);
            assert_eq!(e.max_id, 100);
        }
    }

    #[test]
    fn try_from_unshielded_receive_includes_domain_sep() {
        let row = make_row(LedgerEventAttributes::ContractUnshieldedReceive {
            version: 2,
            entry_point: bv(b"recv"),
            recipient: indexer_common::domain::AddressOrContract::Contract(bv(&[0xbb; 32])),
            domain_sep: bv(&[0xcc; 32]),
            token_type: bv(&[0xdd; 32]),
            amount: "1000".into(),
        });

        let event = ContractEvent::<NoopStorage>::try_from(row).expect("try_from");
        if let ContractEvent::UnshieldedReceive(e) = event {
            assert_eq!(e.version, 2);
            assert_eq!(e.amount, "1000");
            assert!(matches!(e.recipient.kind, AddressOrContractKind::Contract));
            assert!(e.recipient.user_address.is_none());
            assert!(e.recipient.contract_address.is_some());
        } else {
            panic!("expected UnshieldedReceive");
        }
    }

    #[test]
    fn try_from_paused_carries_only_interface_fields() {
        let row = make_row(LedgerEventAttributes::ContractPaused {
            version: 1,
            entry_point: bv(b"pause"),
        });
        let event = ContractEvent::<NoopStorage>::try_from(row).expect("try_from");
        assert!(matches!(event, ContractEvent::Paused(_)));
    }

    #[test]
    fn try_from_non_contract_attribute_returns_error() {
        let row = make_row(LedgerEventAttributes::ZswapOutput);
        let err = ContractEvent::<NoopStorage>::try_from(row).expect_err("must error");
        assert!(format!("{err}").contains("unexpected ledger event"));
    }

    #[test]
    fn contract_event_type_variant_name_is_stable() {
        assert_eq!(
            ContractEventType::ShieldedSpend.variant_name(),
            "ShieldedSpend"
        );
        assert_eq!(ContractEventType::Misc.variant_name(), "Misc");
        assert_eq!(
            ContractEventType::UnshieldedReceive.variant_name(),
            "UnshieldedReceive"
        );
    }

    #[test]
    fn filter_into_domain_rejects_empty_contract_address() {
        let filter = ContractEventFilter {
            contract_address: HexEncoded::try_from(String::new()).expect("empty hex parses"),
            types: None,
            field_prefixes: None,
            from_block: None,
            to_block: None,
            transaction_hash: None,
        };
        let err = filter.into_domain().expect_err("should reject");
        assert!(matches!(
            err,
            ContractEventFilterError::EmptyContractAddress
        ));
    }

    #[test]
    fn filter_into_domain_threads_block_range_and_types() {
        let filter = ContractEventFilter {
            contract_address: HexEncoded::try_from("ab".repeat(32)).expect("valid hex"),
            types: Some(vec![
                ContractEventType::ShieldedSpend,
                ContractEventType::Misc,
            ]),
            field_prefixes: None,
            from_block: Some(100),
            to_block: Some(200),
            transaction_hash: None,
        };
        let domain = filter.into_domain().expect("valid");
        assert_eq!(domain.from_block, Some(100));
        assert_eq!(domain.to_block, Some(200));
        assert_eq!(domain.variants, vec!["ShieldedSpend", "Misc"]);
        assert_eq!(domain.contract_address.as_ref().len(), 32);
        assert_eq!(domain.transaction_hash, None);
    }

    #[test]
    fn filter_into_domain_threads_transaction_hash() {
        let filter = ContractEventFilter {
            contract_address: HexEncoded::try_from("ab".repeat(32)).expect("valid hex"),
            types: None,
            field_prefixes: None,
            from_block: None,
            to_block: None,
            transaction_hash: Some(HexEncoded::try_from("cd".repeat(32)).expect("valid hex")),
        };
        let domain = filter.into_domain().expect("valid");
        assert_eq!(
            domain.transaction_hash,
            Some(TransactionHash::from([0xcd; 32]))
        );
    }

    #[test]
    fn filter_into_domain_rejects_empty_types() {
        let filter = ContractEventFilter {
            contract_address: HexEncoded::try_from("ab".repeat(32)).expect("valid hex"),
            types: Some(vec![]),
            field_prefixes: None,
            from_block: None,
            to_block: None,
            transaction_hash: None,
        };
        let err = filter.into_domain().expect_err("should reject");
        assert!(matches!(err, ContractEventFilterError::EmptyTypes));
    }

    #[test]
    fn filter_into_domain_rejects_unknown_field_name() {
        let filter = ContractEventFilter {
            contract_address: HexEncoded::try_from("ab".repeat(32)).expect("valid hex"),
            types: None,
            field_prefixes: Some(vec![FieldPrefixFilter {
                field_name: "token_type".to_owned(),
                prefix: HexEncoded::try_from("ab".to_owned()).expect("valid hex"),
            }]),
            from_block: None,
            to_block: None,
            transaction_hash: None,
        };
        let err = filter.into_domain().expect_err("should reject");
        assert!(matches!(
            err,
            ContractEventFilterError::UnknownFieldName(name) if name == "token_type"
        ));
    }

    #[test]
    fn filter_into_domain_rejects_inverted_block_range() {
        let filter = ContractEventFilter {
            contract_address: HexEncoded::try_from("ab".repeat(32)).expect("valid hex"),
            types: None,
            field_prefixes: None,
            from_block: Some(200),
            to_block: Some(100),
            transaction_hash: None,
        };
        let err = filter.into_domain().expect_err("should reject");
        assert!(matches!(
            err,
            ContractEventFilterError::InvalidBlockRange {
                from_block: 200,
                to_block: 100,
            }
        ));
    }

    #[test]
    fn filter_into_domain_rejects_wrong_length_transaction_hash() {
        let filter = ContractEventFilter {
            contract_address: HexEncoded::try_from("ab".repeat(32)).expect("valid hex"),
            types: None,
            field_prefixes: None,
            from_block: None,
            to_block: None,
            transaction_hash: Some(HexEncoded::try_from("cd".repeat(3)).expect("valid hex")),
        };
        let err = filter.into_domain().expect_err("should reject");
        assert!(matches!(
            err,
            ContractEventFilterError::InvalidTransactionHash(_)
        ));
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;
    use crate::domain::storage::NoopStorage;
    use indexer_common::domain::{
        AddressOrContract as DomainAOC, ByteVec, ProtocolVersion, SerializedLedgerEvent,
    };

    fn bv(bytes: &[u8]) -> ByteVec {
        ByteVec::from(bytes.to_vec())
    }

    fn make_row(attributes: LedgerEventAttributes) -> ContractEventRow {
        ContractEventRow {
            id: 1,
            contract_address: bv(&[0x01; 32]),
            transaction_id: 1,
            contract_action_id: Some(1),
            raw: SerializedLedgerEvent::from(vec![0]),
            attributes,
            max_id: 1,
            protocol_version: ProtocolVersion::V1_0(0),
        }
    }

    #[test]
    #[allow(clippy::type_complexity)]
    fn try_from_every_variant_succeeds() {
        let cases: Vec<(
            LedgerEventAttributes,
            fn(&ContractEvent<NoopStorage>) -> bool,
        )> = vec![
            (
                LedgerEventAttributes::ContractShieldedSpend {
                    version: 1,
                    entry_point: bv(b""),
                    nullifier: bv(&[0; 32]),
                },
                |e| matches!(e, ContractEvent::ShieldedSpend(_)),
            ),
            (
                LedgerEventAttributes::ContractShieldedReceive {
                    version: 1,
                    entry_point: bv(b""),
                    commitment: bv(&[0; 32]),
                    ciphertext: None,
                    receiving_contract_address: None,
                },
                |e| matches!(e, ContractEvent::ShieldedReceive(_)),
            ),
            (
                LedgerEventAttributes::ContractShieldedMint {
                    version: 1,
                    entry_point: bv(b""),
                    commitment: bv(&[0; 32]),
                    domain_sep: bv(&[0; 32]),
                    amount: None,
                },
                |e| matches!(e, ContractEvent::ShieldedMint(_)),
            ),
            (
                LedgerEventAttributes::ContractShieldedBurn {
                    version: 1,
                    entry_point: bv(b""),
                    nullifier: bv(&[0; 32]),
                    amount: None,
                },
                |e| matches!(e, ContractEvent::ShieldedBurn(_)),
            ),
            (
                LedgerEventAttributes::ContractUnshieldedSpend {
                    version: 1,
                    entry_point: bv(b""),
                    sender: DomainAOC::User(bv(&[0; 32])),
                    domain_sep: bv(&[0; 32]),
                    token_type: bv(&[0; 32]),
                    amount: "0".into(),
                },
                |e| matches!(e, ContractEvent::UnshieldedSpend(_)),
            ),
            (
                LedgerEventAttributes::ContractUnshieldedReceive {
                    version: 1,
                    entry_point: bv(b""),
                    recipient: DomainAOC::Contract(bv(&[0; 32])),
                    domain_sep: bv(&[0; 32]),
                    token_type: bv(&[0; 32]),
                    amount: "0".into(),
                },
                |e| matches!(e, ContractEvent::UnshieldedReceive(_)),
            ),
            (
                LedgerEventAttributes::ContractUnshieldedMint {
                    version: 1,
                    entry_point: bv(b""),
                    domain_sep: bv(&[0; 32]),
                    token_type: bv(&[0; 32]),
                    amount: "0".into(),
                },
                |e| matches!(e, ContractEvent::UnshieldedMint(_)),
            ),
            (
                LedgerEventAttributes::ContractUnshieldedBurn {
                    version: 1,
                    entry_point: bv(b""),
                    sender: DomainAOC::User(bv(&[0; 32])),
                    token_type: bv(&[0; 32]),
                    amount: "0".into(),
                },
                |e| matches!(e, ContractEvent::UnshieldedBurn(_)),
            ),
            (
                LedgerEventAttributes::ContractPaused {
                    version: 1,
                    entry_point: bv(b""),
                },
                |e| matches!(e, ContractEvent::Paused(_)),
            ),
            (
                LedgerEventAttributes::ContractUnpaused {
                    version: 1,
                    entry_point: bv(b""),
                },
                |e| matches!(e, ContractEvent::Unpaused(_)),
            ),
            (
                LedgerEventAttributes::ContractMisc {
                    version: 1,
                    entry_point: bv(b""),
                    name: bv(&[0; 32]),
                    payload: bv(&[0; 32]),
                },
                |e| matches!(e, ContractEvent::Misc(_)),
            ),
        ];

        for (attrs, check) in cases {
            let row = make_row(attrs.clone());
            let event = ContractEvent::<NoopStorage>::try_from(row).unwrap_or_else(|_| {
                panic!("try_from failed for {:?}", attrs);
            });
            assert!(check(&event), "wrong variant for {:?}", attrs);
        }
    }

    #[test]
    fn address_or_contract_kind_round_trip_through_graphql_conversion() {
        let user_gql: AddressOrContract = DomainAOC::User(bv(&[0xab; 32])).into();
        let contract_gql: AddressOrContract = DomainAOC::Contract(bv(&[0xcd; 32])).into();

        assert!(matches!(user_gql.kind, AddressOrContractKind::User));
        assert!(user_gql.user_address.is_some());
        assert!(user_gql.contract_address.is_none());

        assert!(matches!(contract_gql.kind, AddressOrContractKind::Contract));
        assert!(contract_gql.user_address.is_none());
        assert!(contract_gql.contract_address.is_some());
    }

    #[test]
    fn shielded_receive_with_all_optional_fields_present() {
        let attrs = LedgerEventAttributes::ContractShieldedReceive {
            version: 1,
            entry_point: bv(b"r"),
            commitment: bv(&[0x11; 32]),
            ciphertext: Some(bv(&[0x22; 64])),
            receiving_contract_address: Some(bv(&[0x33; 32])),
        };
        let event = ContractEvent::<NoopStorage>::try_from(make_row(attrs)).unwrap();
        if let ContractEvent::ShieldedReceive(e) = event {
            assert!(e.ciphertext.is_some());
            assert!(e.receiving_contract_address.is_some());
        } else {
            panic!("wrong variant");
        }
    }
}
