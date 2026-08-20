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

pub mod bridge;
pub mod dust;
pub mod ledger;

mod bytes;
mod protocol_version;
mod pub_sub;
mod viewing_key;

pub use bytes::*;
pub use protocol_version::*;
pub use pub_sub::*;
pub use viewing_key::*;

use derive_more::{Deref, Display, Into};
use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
use thiserror::Error;

// Plain bytes: very simple hashes/identifiers used without serialization.
pub type BlockAuthor = ByteArray<32>;
pub type BlockHash = ByteArray<32>;
pub type IntentHash = ByteArray<32>;
pub type Nonce = ByteArray<32>;
pub type SessionId = ByteArray<32>;
pub type ViewingKeyHash = ByteArray<32>;
pub type TermsAndConditionsHash = ByteArray<32>;
pub type TokenType = ByteArray<32>;
pub type TransactionHash = ByteArray<32>;
pub type UnshieldedAddress = ByteArray<32>;

// DUST-specific types for dustGenerationStatus query.
pub type DustPublicKey = ByteVec;
pub type CardanoRewardAddress = ByteArray<29>;
pub type NightUtxoHash = ByteArray<32>;
pub type DustUtxoId = ByteVec;

// Untagged serialization: simple and/or stable types that are not expected to change.
pub type SerializedLedgerStateKey = ByteVec;
pub type SerializedTransactionIdentifier = ByteVec;
pub type SerializedZswapMerkleTreeRoot = ByteVec;
pub type SerializedDustCommitmentMerkleTreeRoot = ByteVec;
pub type SerializedDustGenerationMerkleTreeRoot = ByteVec;

// Tagged serialization: complex types that may evolve; tags allow version-awareness.
pub type SerializedContractAddress = ByteVec;
pub type SerializedContractState = ByteVec;
pub type SerializedDustTreeInsertionPath = ByteVec;
pub type SerializedLedgerEvent = ByteVec;
pub type SerializedLedgerParameters = ByteVec;
pub type SerializedTransaction = ByteVec;
pub type SerializedZswapState = ByteVec;

/// Network identifier.
#[derive(Debug, Display, Clone, PartialEq, Eq, Hash, Deref, Into, Deserialize)]
#[deref(forward)]
#[serde(try_from = "String")]
pub struct NetworkId(pub String);

impl TryFrom<String> for NetworkId {
    type Error = InvalidNetworkIdError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        if s.is_empty() {
            Err(InvalidNetworkIdError::Empty)
        } else if s.chars().any(|c| c.is_uppercase()) {
            Err(InvalidNetworkIdError::NotLowercase(s))
        } else {
            Ok(Self(s))
        }
    }
}

impl TryFrom<&str> for NetworkId {
    type Error = InvalidNetworkIdError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.to_owned().try_into()
    }
}

impl FromStr for NetworkId {
    type Err = InvalidNetworkIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.try_into()
    }
}

#[derive(Debug, Error)]
pub enum InvalidNetworkIdError {
    #[error("network ID must not be empty")]
    Empty,
    #[error("network ID must be all lowercase (was: {0})")]
    NotLowercase(String),
}

#[cfg(test)]
mod network_id_tests {
    use crate::domain::{InvalidNetworkIdError, NetworkId};

    #[test]
    fn test_valid_lowercase_network_id() {
        assert!(NetworkId::try_from("qanet".to_string()).is_ok());
    }

    #[test]
    fn test_reject_mixed_case_network_id() {
        let result = NetworkId::try_from("DevNet".to_string());
        assert!(matches!(
            result,
            Err(InvalidNetworkIdError::NotLowercase(_))
        ));
    }
}

/// A timestamp in milliseconds since the Unix epoch (Substrate Timestamp pallet convention).
/// Use when working with values from the `blocks.timestamp` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimestampMs(pub u64);

/// A timestamp in seconds since the Unix epoch (ledger convention).
/// Use when working with values from `dust_generation_info.ctime`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimestampSecs(pub u64);

impl TimestampSecs {
    /// Convert to milliseconds.
    pub fn to_ms(self) -> TimestampMs {
        TimestampMs(self.0 * 1000)
    }
}

impl TimestampMs {
    /// Calculate elapsed seconds since an earlier timestamp.
    pub fn elapsed_seconds_since(self, earlier: TimestampMs) -> u64 {
        self.0.saturating_sub(earlier.0) / 1000
    }
}

/// The outcome of applying a regular transaction to the ledger state along with extracted data.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ApplyRegularTransactionOutcome {
    pub transaction_result: TransactionResult,
    pub created_unshielded_utxos: Vec<UnshieldedUtxo>,
    pub spent_unshielded_utxos: Vec<UnshieldedUtxo>,
    pub ledger_events: Vec<LedgerEvent>,
    pub fees: u128,
    /// Populated when the transaction is a `ClaimRewards(claim)` with
    /// `claim.kind == ClaimKind::CardanoBridge`. The recipient is the claim's
    /// owner (32-byte address); the amount is the claim's value.
    pub bridge_claim: Option<crate::domain::bridge::BridgeClaim>,
}

/// The outcome of applying a system transaction to the ledger state along with extracted data.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ApplySystemTransactionOutcome {
    pub created_unshielded_utxos: Vec<UnshieldedUtxo>,
    pub ledger_events: Vec<LedgerEvent>,
}

/// The result of applying a transaction to the ledger state.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionResult {
    /// All guaranteed and fallible coins succeeded.
    Success,

    /// Not all fallible coins succeeded; the value maps segemt ID to success.
    PartialSuccess(Vec<(u16, bool)>),

    /// Guaranteed coins failed.
    #[default]
    Failure,
}

/// The variant of a transaction: regular or system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Type)]
#[cfg_attr(feature = "cloud", sqlx(type_name = "TRANSACTION_VARIANT"))]
pub enum TransactionVariant {
    Regular,
    System,
}

/// A contract action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractAction {
    pub address: SerializedContractAddress,
    pub state: SerializedContractState,
    pub attributes: ContractAttributes,
}

/// Attributes for a specific contract action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractAttributes {
    Deploy,
    Call { entry_point: String },
    Update,
}

/// Token balance of a contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContractBalance {
    /// Token type identifier.
    pub token_type: TokenType,

    /// Balance amount as u128.
    pub amount: u128,
}

/// Maintenance authority of a contract, extracted from its on-chain state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractMaintenanceAuthority {
    /// The committee of verifying keys authorised to maintain the contract.
    pub committee: Vec<ContractMaintenanceVerifyingKey>,

    /// The number of committee signatures required to authorise maintenance.
    pub threshold: u32,

    /// Monotonic counter guarding against replay of maintenance operations.
    pub counter: u32,
}

/// A verifying key in a contract maintenance authority committee.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractMaintenanceVerifyingKey {
    /// The signature scheme of the key.
    pub kind: VerifyingKeyKind,

    /// Tagged-serialized verifying key bytes.
    pub key: ByteVec,
}

/// The signature scheme of a maintenance authority verifying key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyingKeyKind {
    Schnorr,
    Ecdsa,
}

/// An unshielded UTXO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnshieldedUtxo {
    pub owner: UnshieldedAddress,
    pub token_type: TokenType,
    pub value: u128,
    pub intent_hash: IntentHash,
    pub output_index: u32,
    pub ctime: Option<u64>,
    pub initial_nonce: Nonce,
    pub registered_for_dust_generation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerEvent {
    pub grouping: LedgerEventGrouping,
    pub raw: SerializedLedgerEvent,
    pub attributes: LedgerEventAttributes,
    /// Emitting `ContractCall.id` (from `contract_actions.id`) for contract
    /// events; `None` for zswap/dust events and for contract events whose
    /// originating action is not yet correlated. Mapped onto the indexed
    /// `ledger_events.contract_action_id` column; powers the nested
    /// `ContractCall.contractEvents` GraphQL surface (ticket #1162).
    pub contract_action_id: Option<u64>,
    /// Emitting contract address. Populated only for contract events
    /// (`LedgerEventGrouping::Contract`); `None` for zswap/dust events. Mapped
    /// onto the indexed `ledger_events.contract_address` column for fast
    /// filtering on the `contractEvents` query/subscription surface.
    pub contract_address: Option<SerializedContractAddress>,
}

impl LedgerEvent {
    fn zswap_input(raw: SerializedLedgerEvent, nullifier: ByteVec) -> Self {
        Self {
            grouping: LedgerEventGrouping::Zswap,
            raw,
            attributes: LedgerEventAttributes::ZswapInput { nullifier },
            contract_action_id: None,
            contract_address: None,
        }
    }

    fn zswap_output(raw: SerializedLedgerEvent) -> Self {
        Self {
            grouping: LedgerEventGrouping::Zswap,
            raw,
            attributes: LedgerEventAttributes::ZswapOutput,
            contract_action_id: None,
            contract_address: None,
        }
    }

    fn param_change(raw: SerializedLedgerEvent) -> Self {
        Self {
            grouping: LedgerEventGrouping::Dust,
            raw,
            attributes: LedgerEventAttributes::ParamChange,
            contract_action_id: None,
            contract_address: None,
        }
    }

    fn dust_initial_utxo(
        raw: SerializedLedgerEvent,
        output: dust::QualifiedDustOutput,
        generation_info: dust::DustGenerationInfo,
        generation_index: u64,
    ) -> Self {
        Self {
            grouping: LedgerEventGrouping::Dust,
            raw,
            attributes: LedgerEventAttributes::DustInitialUtxo {
                output,
                generation_info,
                generation_index,
            },
            contract_action_id: None,
            contract_address: None,
        }
    }

    fn dust_generation_dtime_update(
        raw: SerializedLedgerEvent,
        generation_info: dust::DustGenerationInfo,
        generation_index: u64,
        tree_insertion_path: SerializedDustTreeInsertionPath,
    ) -> Self {
        Self {
            grouping: LedgerEventGrouping::Dust,
            raw,
            attributes: LedgerEventAttributes::DustGenerationDtimeUpdate {
                generation_info,
                generation_index,
                tree_insertion_path,
            },
            contract_action_id: None,
            contract_address: None,
        }
    }

    fn dust_spend_processed(
        raw: SerializedLedgerEvent,
        nullifier: ByteVec,
        commitment: ByteVec,
    ) -> Self {
        Self {
            grouping: LedgerEventGrouping::Dust,
            raw,
            attributes: LedgerEventAttributes::DustSpendProcessed {
                nullifier,
                commitment,
            },
            contract_action_id: None,
            contract_address: None,
        }
    }

    /// Construct a contract event from already-typed attributes. Use when the
    /// chain-indexer has parsed `VersionedLogItem` into a known `LogEventType`
    /// variant via `make_ledger_events_v9` (see ticket #1158).
    ///
    /// `contract_action_id` is the `contract_actions.id` of the originating
    /// `ContractCall`. The ledger decode path passes `None` (the id is only
    /// assigned at save time); the chain-indexer correlates events with
    /// actions when saving them, populating the indexed
    /// `ledger_events.contract_action_id` column which powers
    /// `ContractCall.contractEvents` (ticket #1162). `Some` is reserved for
    /// callers that already know the id (e.g. future upstream attribution).
    pub fn contract_event(
        raw: SerializedLedgerEvent,
        contract_address: SerializedContractAddress,
        contract_action_id: Option<u64>,
        attributes: LedgerEventAttributes,
    ) -> Self {
        debug_assert!(
            attributes.contract_entry_point().is_some(),
            "contract_event() called with non-contract attributes"
        );
        Self {
            grouping: LedgerEventGrouping::Contract,
            raw,
            attributes,
            contract_action_id,
            contract_address: Some(contract_address),
        }
    }

    /// Extract the indexed-field rows for the sidecar storage. Returns
    /// `(field_name, field_value)` pairs covering every "hint: indexed" field
    /// from CoIP-442 Appendix A for the matching variant. Empty for
    /// non-contract events and for `Paused`/`Unpaused`/`Misc` (no indexable
    /// fields per the design).
    pub fn indexable_contract_fields(&self) -> Vec<(&'static str, ByteVec)> {
        match &self.attributes {
            LedgerEventAttributes::ContractShieldedSpend { nullifier, .. } => {
                vec![("nullifier", nullifier.clone())]
            }

            LedgerEventAttributes::ContractShieldedReceive {
                commitment,
                ciphertext,
                ..
            } => {
                let mut fields = vec![("commitment", commitment.clone())];
                if let Some(ciphertext) = ciphertext {
                    fields.push(("ciphertext", ciphertext.clone()));
                }
                fields
            }

            LedgerEventAttributes::ContractShieldedMint {
                commitment,
                domain_sep,
                ..
            } => vec![
                ("commitment", commitment.clone()),
                ("domainSep", domain_sep.clone()),
            ],

            LedgerEventAttributes::ContractShieldedBurn { nullifier, .. } => {
                vec![("nullifier", nullifier.clone())]
            }

            LedgerEventAttributes::ContractUnshieldedSpend {
                sender,
                domain_sep,
                token_type,
                ..
            } => vec![
                ("sender", sender.to_bytes()),
                ("domainSep", domain_sep.clone()),
                ("tokenType", token_type.clone()),
            ],

            LedgerEventAttributes::ContractUnshieldedReceive {
                recipient,
                domain_sep,
                token_type,
                ..
            } => vec![
                ("recipient", recipient.to_bytes()),
                ("domainSep", domain_sep.clone()),
                ("tokenType", token_type.clone()),
            ],

            LedgerEventAttributes::ContractUnshieldedMint {
                domain_sep,
                token_type,
                ..
            } => vec![
                ("domainSep", domain_sep.clone()),
                ("tokenType", token_type.clone()),
            ],

            LedgerEventAttributes::ContractUnshieldedBurn {
                sender, token_type, ..
            } => vec![
                ("sender", sender.to_bytes()),
                ("tokenType", token_type.clone()),
            ],

            LedgerEventAttributes::ContractPaused { .. }
            | LedgerEventAttributes::ContractUnpaused { .. }
            | LedgerEventAttributes::ContractMisc { .. }
            | LedgerEventAttributes::ZswapInput { .. }
            | LedgerEventAttributes::ZswapOutput
            | LedgerEventAttributes::ParamChange
            | LedgerEventAttributes::DustInitialUtxo { .. }
            | LedgerEventAttributes::DustGenerationDtimeUpdate { .. }
            | LedgerEventAttributes::DustSpendProcessed { .. } => vec![],
        }
    }
}

/// Every field name `LedgerEvent::indexable_contract_fields` can emit; the closed set of valid
/// `fieldName` values for the contract events field-prefix filter.
pub const INDEXABLE_CONTRACT_FIELD_NAMES: [&str; 7] = [
    "nullifier",
    "commitment",
    "ciphertext",
    "domainSep",
    "tokenType",
    "sender",
    "recipient",
];

impl AddressOrContract {
    /// Flat byte representation for sidecar storage (32 bytes). Indexer
    /// filters by the raw bytes regardless of whether the address is a user
    /// or contract; the `kind` discriminator is in the JSONB payload only.
    fn to_bytes(&self) -> ByteVec {
        match self {
            Self::User(bytes) => bytes.clone(),
            Self::Contract(bytes) => bytes.clone(),
        }
    }
}

impl LedgerEventAttributes {
    /// Entry point of the originating contract call for contract events; `None`
    /// for zswap and dust events. Used by the chain-indexer to correlate
    /// contract events with the emitting `ContractCall` (ticket #1162).
    pub fn contract_entry_point(&self) -> Option<&ByteVec> {
        match self {
            Self::ContractShieldedSpend { entry_point, .. }
            | Self::ContractShieldedReceive { entry_point, .. }
            | Self::ContractShieldedMint { entry_point, .. }
            | Self::ContractShieldedBurn { entry_point, .. }
            | Self::ContractUnshieldedSpend { entry_point, .. }
            | Self::ContractUnshieldedReceive { entry_point, .. }
            | Self::ContractUnshieldedMint { entry_point, .. }
            | Self::ContractUnshieldedBurn { entry_point, .. }
            | Self::ContractPaused { entry_point, .. }
            | Self::ContractUnpaused { entry_point, .. }
            | Self::ContractMisc { entry_point, .. } => Some(entry_point),

            Self::ZswapInput { .. }
            | Self::ZswapOutput
            | Self::ParamChange
            | Self::DustInitialUtxo { .. }
            | Self::DustGenerationDtimeUpdate { .. }
            | Self::DustSpendProcessed { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LedgerEventAttributes {
    ZswapInput {
        nullifier: ByteVec,
    },

    ZswapOutput,

    ParamChange,

    DustInitialUtxo {
        output: dust::QualifiedDustOutput,
        generation_info: dust::DustGenerationInfo,
        generation_index: u64,
    },

    DustGenerationDtimeUpdate {
        generation_info: dust::DustGenerationInfo,
        generation_index: u64,
        /// Tagged-serialised `TreeInsertionPath<DustGenerationInfo>` from the
        /// originating ledger event. Surfaced verbatim on the GraphQL API so
        /// wallets can hand it to `generating_tree.update_from_evidence(...)`.
        tree_insertion_path: SerializedDustTreeInsertionPath,
    },

    DustSpendProcessed {
        nullifier: ByteVec,
        commitment: ByteVec,
    },

    // ------------------------------------------------------------------------
    // Contract events (MIP-107 / CoIP-442). One variant per `LogEventType` from
    // `onchain-vm/src/ops.rs`. Field shapes follow CoIP-442 Appendix A head.
    // `entry_point` is the originating call's entry point (from
    // `EventDetailsV9::ContractLog.entry_point`), used by the nested
    // ContractCall.contractEvents surface for correlation.
    // ------------------------------------------------------------------------
    ContractShieldedSpend {
        version: u32,
        entry_point: ByteVec,
        nullifier: ByteVec,
    },

    ContractShieldedReceive {
        version: u32,
        entry_point: ByteVec,
        commitment: ByteVec,
        ciphertext: Option<ByteVec>,
        receiving_contract_address: Option<ByteVec>,
    },

    ContractShieldedMint {
        version: u32,
        entry_point: ByteVec,
        commitment: ByteVec,
        domain_sep: ByteVec,
        amount: Option<String>,
    },

    ContractShieldedBurn {
        version: u32,
        entry_point: ByteVec,
        nullifier: ByteVec,
        amount: Option<String>,
    },

    ContractUnshieldedSpend {
        version: u32,
        entry_point: ByteVec,
        sender: AddressOrContract,
        domain_sep: ByteVec,
        token_type: ByteVec,
        amount: String,
    },

    ContractUnshieldedReceive {
        version: u32,
        entry_point: ByteVec,
        recipient: AddressOrContract,
        domain_sep: ByteVec,
        token_type: ByteVec,
        amount: String,
    },

    ContractUnshieldedMint {
        version: u32,
        entry_point: ByteVec,
        domain_sep: ByteVec,
        token_type: ByteVec,
        amount: String,
    },

    ContractUnshieldedBurn {
        version: u32,
        entry_point: ByteVec,
        sender: AddressOrContract,
        token_type: ByteVec,
        amount: String,
    },

    ContractPaused {
        version: u32,
        entry_point: ByteVec,
    },

    ContractUnpaused {
        version: u32,
        entry_point: ByteVec,
    },

    ContractMisc {
        version: u32,
        entry_point: ByteVec,
        name: ByteVec,
        payload: ByteVec,
    },
}

/// Tagged union for fields like `Either<ZswapCoinPublicKey, ContractAddress>`
/// used in standard unshielded contract events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AddressOrContract {
    /// User wallet address (Zswap coin public key).
    User(ByteVec),
    /// Contract address.
    Contract(ByteVec),
}

/// Minimal DUST output info for backwards compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DustOutput {
    pub nonce: Nonce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "cloud", sqlx(type_name = "LEDGER_EVENT_GROUPING"))]
pub enum LedgerEventGrouping {
    Zswap,
    Dust,
    Contract,
}

#[cfg(test)]
mod contract_event_tests {
    use super::*;

    fn bv(bytes: &[u8]) -> ByteVec {
        ByteVec::from(bytes.to_vec())
    }

    fn shielded_spend_attrs() -> LedgerEventAttributes {
        LedgerEventAttributes::ContractShieldedSpend {
            version: 1,
            entry_point: bv(b"spend"),
            nullifier: bv(&[0xaa; 32]),
        }
    }

    fn shielded_receive_attrs() -> LedgerEventAttributes {
        LedgerEventAttributes::ContractShieldedReceive {
            version: 1,
            entry_point: bv(b"receive"),
            commitment: bv(&[0xbb; 32]),
            ciphertext: Some(bv(&[0xcc; 64])),
            receiving_contract_address: None,
        }
    }

    fn shielded_mint_attrs() -> LedgerEventAttributes {
        LedgerEventAttributes::ContractShieldedMint {
            version: 1,
            entry_point: bv(b"mint"),
            commitment: bv(&[0x11; 32]),
            domain_sep: bv(&[0x22; 32]),
            amount: Some("1000".to_string()),
        }
    }

    fn shielded_burn_attrs() -> LedgerEventAttributes {
        LedgerEventAttributes::ContractShieldedBurn {
            version: 1,
            entry_point: bv(b"burn"),
            nullifier: bv(&[0x33; 32]),
            amount: None,
        }
    }

    fn unshielded_spend_attrs() -> LedgerEventAttributes {
        LedgerEventAttributes::ContractUnshieldedSpend {
            version: 1,
            entry_point: bv(b"u_spend"),
            sender: AddressOrContract::User(bv(&[0x44; 32])),
            domain_sep: bv(&[0x55; 32]),
            token_type: bv(&[0x66; 32]),
            amount: "500".to_string(),
        }
    }

    fn unshielded_receive_attrs() -> LedgerEventAttributes {
        LedgerEventAttributes::ContractUnshieldedReceive {
            version: 1,
            entry_point: bv(b"u_recv"),
            recipient: AddressOrContract::Contract(bv(&[0x77; 32])),
            domain_sep: bv(&[0x88; 32]),
            token_type: bv(&[0x99; 32]),
            amount: "501".to_string(),
        }
    }

    fn unshielded_mint_attrs() -> LedgerEventAttributes {
        LedgerEventAttributes::ContractUnshieldedMint {
            version: 1,
            entry_point: bv(b"u_mint"),
            domain_sep: bv(&[0xaa; 32]),
            token_type: bv(&[0xbb; 32]),
            amount: "502".to_string(),
        }
    }

    fn unshielded_burn_attrs() -> LedgerEventAttributes {
        LedgerEventAttributes::ContractUnshieldedBurn {
            version: 1,
            entry_point: bv(b"u_burn"),
            sender: AddressOrContract::User(bv(&[0xcc; 32])),
            token_type: bv(&[0xdd; 32]),
            amount: "503".to_string(),
        }
    }

    fn paused_attrs() -> LedgerEventAttributes {
        LedgerEventAttributes::ContractPaused {
            version: 1,
            entry_point: bv(b"pause"),
        }
    }

    fn unpaused_attrs() -> LedgerEventAttributes {
        LedgerEventAttributes::ContractUnpaused {
            version: 1,
            entry_point: bv(b"unpause"),
        }
    }

    fn misc_attrs() -> LedgerEventAttributes {
        LedgerEventAttributes::ContractMisc {
            version: 1,
            entry_point: bv(b"misc"),
            name: bv(&[0xee; 32]),
            payload: bv(&[0xff; 64]),
        }
    }

    #[test]
    fn contract_event_constructor_sets_contract_grouping() {
        let attrs = shielded_spend_attrs();
        let event =
            LedgerEvent::contract_event(bv(b"raw"), bv(&[0x01; 32]), Some(42), attrs.clone());
        assert!(matches!(event.grouping, LedgerEventGrouping::Contract));
        assert_eq!(event.contract_address, Some(bv(&[0x01; 32])));
        assert_eq!(event.contract_action_id, Some(42));
        assert_eq!(event.attributes, attrs);
    }

    #[test]
    fn contract_event_constructor_accepts_none_contract_action_id() {
        let event =
            LedgerEvent::contract_event(bv(b"raw"), bv(&[0x02; 32]), None, shielded_spend_attrs());
        assert!(matches!(event.grouping, LedgerEventGrouping::Contract));
        assert!(event.contract_action_id.is_none());
    }

    #[test]
    fn existing_event_constructors_leave_contract_envelope_fields_unset() {
        let zswap_in = LedgerEvent::zswap_input(bv(b"raw"), bv(&[0; 32]));
        let zswap_out = LedgerEvent::zswap_output(bv(b"raw"));
        let param = LedgerEvent::param_change(bv(b"raw"));
        let dust_spend = LedgerEvent::dust_spend_processed(bv(b"raw"), bv(&[0; 32]), bv(&[0; 32]));

        for e in [&zswap_in, &zswap_out, &param, &dust_spend] {
            assert!(e.contract_address.is_none());
            assert!(e.contract_action_id.is_none());
        }

        assert!(matches!(zswap_in.grouping, LedgerEventGrouping::Zswap));
        assert!(matches!(zswap_out.grouping, LedgerEventGrouping::Zswap));
        assert!(matches!(param.grouping, LedgerEventGrouping::Dust));
        assert!(matches!(dust_spend.grouping, LedgerEventGrouping::Dust));
    }

    #[test]
    fn indexable_contract_fields_shielded_spend_returns_nullifier_only() {
        let event =
            LedgerEvent::contract_event(bv(b"raw"), bv(&[0x01; 32]), None, shielded_spend_attrs());
        let fields = event.indexable_contract_fields();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].0, "nullifier");
    }

    #[test]
    fn indexable_contract_fields_shielded_receive_includes_ciphertext_when_present() {
        let with_ct = LedgerEvent::contract_event(
            bv(b"raw"),
            bv(&[0x01; 32]),
            None,
            shielded_receive_attrs(),
        );
        let names: Vec<_> = with_ct
            .indexable_contract_fields()
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(names, vec!["commitment", "ciphertext"]);

        let no_ct_attrs = LedgerEventAttributes::ContractShieldedReceive {
            version: 1,
            entry_point: bv(b"r"),
            commitment: bv(&[0; 32]),
            ciphertext: None,
            receiving_contract_address: None,
        };
        let no_ct = LedgerEvent::contract_event(bv(b"raw"), bv(&[0x01; 32]), None, no_ct_attrs);
        let names: Vec<_> = no_ct
            .indexable_contract_fields()
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(names, vec!["commitment"]);
    }

    #[test]
    fn indexable_contract_fields_unshielded_spend_three_fields() {
        let event = LedgerEvent::contract_event(
            bv(b"raw"),
            bv(&[0x01; 32]),
            None,
            unshielded_spend_attrs(),
        );
        let names: Vec<_> = event
            .indexable_contract_fields()
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(names, vec!["sender", "domainSep", "tokenType"]);
    }

    #[test]
    fn indexable_contract_fields_paused_unpaused_misc_are_empty() {
        for attrs in [paused_attrs(), unpaused_attrs(), misc_attrs()] {
            let event = LedgerEvent::contract_event(bv(b"raw"), bv(&[0x01; 32]), None, attrs);
            assert!(event.indexable_contract_fields().is_empty());
        }
    }

    #[test]
    fn indexable_contract_fields_legacy_event_types_are_empty() {
        let zswap_in = LedgerEvent::zswap_input(bv(b"raw"), bv(&[0; 32]));
        let param = LedgerEvent::param_change(bv(b"raw"));
        assert!(zswap_in.indexable_contract_fields().is_empty());
        assert!(param.indexable_contract_fields().is_empty());
    }

    #[test]
    fn address_or_contract_to_bytes_returns_inner() {
        let user = AddressOrContract::User(bv(&[0xab; 32]));
        let contract = AddressOrContract::Contract(bv(&[0xcd; 32]));
        assert_eq!(user.to_bytes().as_ref(), &[0xab; 32]);
        assert_eq!(contract.to_bytes().as_ref(), &[0xcd; 32]);
    }

    #[test]
    fn indexable_contract_field_names_cover_every_emitted_field() {
        let all_attrs = [
            shielded_spend_attrs(),
            shielded_receive_attrs(),
            shielded_mint_attrs(),
            shielded_burn_attrs(),
            unshielded_spend_attrs(),
            unshielded_receive_attrs(),
            unshielded_mint_attrs(),
            unshielded_burn_attrs(),
            paused_attrs(),
            unpaused_attrs(),
            misc_attrs(),
        ];
        for attrs in all_attrs {
            let event = LedgerEvent::contract_event(bv(b"raw"), bv(&[0x01; 32]), None, attrs);
            for (name, _) in event.indexable_contract_fields() {
                assert!(INDEXABLE_CONTRACT_FIELD_NAMES.contains(&name));
            }
        }
    }

    #[test]
    fn contract_entry_point_is_some_for_every_contract_variant_and_none_otherwise() {
        let contract_attrs = [
            shielded_spend_attrs(),
            shielded_receive_attrs(),
            shielded_mint_attrs(),
            shielded_burn_attrs(),
            unshielded_spend_attrs(),
            unshielded_receive_attrs(),
            unshielded_mint_attrs(),
            unshielded_burn_attrs(),
            paused_attrs(),
            unpaused_attrs(),
            misc_attrs(),
        ];
        for attrs in contract_attrs {
            assert!(attrs.contract_entry_point().is_some(), "{attrs:?}");
        }

        let zswap_input = LedgerEventAttributes::ZswapInput {
            nullifier: bv(&[0; 32]),
        };
        assert!(zswap_input.contract_entry_point().is_none());
        assert!(
            LedgerEventAttributes::ParamChange
                .contract_entry_point()
                .is_none()
        );
    }

    #[test]
    fn ledger_event_attributes_roundtrip_via_json() {
        let attrs = [
            shielded_spend_attrs(),
            shielded_receive_attrs(),
            shielded_mint_attrs(),
            shielded_burn_attrs(),
            unshielded_spend_attrs(),
            unshielded_receive_attrs(),
            unshielded_mint_attrs(),
            unshielded_burn_attrs(),
            paused_attrs(),
            unpaused_attrs(),
            misc_attrs(),
        ];
        for original in attrs {
            let json = serde_json::to_string(&original).expect("serialize");
            let decoded: LedgerEventAttributes = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(original, decoded, "round-trip mismatch");
        }
    }
}
