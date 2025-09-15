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

use crate::domain::{
    ByteArray, ByteVec, NetworkId, PROTOCOL_VERSION_000_016_000, ProtocolVersion,
    ledger::{
        Error, IntentV6, SerializableV6Ext, SerializedContractAddress, TaggedSerializableV6Ext,
        TransactionV6,
    },
};
use fastrace::trace;
use midnight_base_crypto_v6::{hash::HashOutput as HashOutputV6, time::Timestamp as TimestampV6};
use midnight_coin_structure_v6::{
    coin::UserAddress as UserAddressV6, contract::ContractAddress as ContractAddressV6,
};
use midnight_ledger_v6::{
    semantics::{
        TransactionContext as TransactionContextV6, TransactionResult as TransactionResultV6,
    },
    structure::{LedgerState as LedgerStateV6, SystemTransaction as LedgerSystemTransactionV6},
    verify::WellFormedStrictness as WellFormedStrictnessV6,
};
use midnight_onchain_runtime_v6::context::BlockContext as BlockContextV6;
use midnight_serialize_v6::{Deserializable, tagged_deserialize as tagged_deserialize_v6};
use midnight_storage_v6::DefaultDB as DefaultDBV6;
use midnight_transient_crypto_v6::merkle_tree::{
    MerkleTreeCollapsedUpdate as MerkleTreeCollapsedUpdateV6,
    MerkleTreeDigest as MerkleTreeDigestV6,
};
use midnight_zswap_v6::ledger::State as ZswapStateV6;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub type IntentHash = ByteArray<32>;
pub type RawTokenType = ByteArray<32>;
pub type RawUnshieldedAddress = ByteArray<32>;
pub type SerializedLedgerState = ByteVec;
pub type SerializedTransaction = ByteVec;
pub type SerializedZswapState = ByteVec;
pub type SerializedZswapStateRoot = ByteVec;

/// Facade for `LedgerState` from `midnight_ledger` across supported (protocol) versions.
#[derive(Debug, Clone)]
pub enum LedgerState {
    V6(LedgerStateV6<DefaultDBV6>),
}

impl LedgerState {
    #[allow(missing_docs)]
    pub fn new(network_id: NetworkId) -> Self {
        Self::V6(LedgerStateV6::new(network_id))
    }

    /// Deserialize the given serialized ledger state using the given protocol version.
    #[trace(properties = { "protocol_version": "{protocol_version}" })]
    pub fn deserialize(
        ledger_state: impl AsRef<[u8]>,
        protocol_version: ProtocolVersion,
    ) -> Result<Self, Error> {
        if protocol_version.is_compatible(PROTOCOL_VERSION_000_016_000) {
            let ledger_state = tagged_deserialize_v6(&mut ledger_state.as_ref())
                .map_err(|error| Error::Io("cannot deserialize LedgerStateV6", error))?;
            Ok(Self::V6(ledger_state))
        } else {
            Err(Error::InvalidProtocolVersion(protocol_version))
        }
    }

    /// Serialize this ledger state.
    #[trace]
    pub fn serialize(&self) -> Result<SerializedLedgerState, Error> {
        match self {
            Self::V6(ledger_state) => ledger_state
                .tagged_serialize_v6()
                .map_err(|error| Error::Io("cannot serialize LedgerStateV6", error)),
        }
    }

    /// Apply the given serialized regular transaction to this ledger state and return the
    /// transaction result as well as the created and spent unshielded UTXOs.
    #[trace]
    pub fn apply_regular_transaction(
        &mut self,
        transaction: &SerializedTransaction,
        block_parent_hash: ByteArray<32>,
        block_timestamp: u64,
    ) -> Result<(TransactionResult, Vec<UnshieldedUtxo>, Vec<UnshieldedUtxo>), Error> {
        match self {
            Self::V6(ledger_state) => {
                let ledger_transaction = tagged_deserialize_v6::<TransactionV6>(
                    &mut transaction.as_ref(),
                )
                .map_err(|error| Error::Io("cannot deserialize LedgerTransactionV6", error))?;

                let cx = TransactionContextV6 {
                    ref_state: ledger_state.clone(),
                    block_context: BlockContextV6 {
                        tblock: timestamp_v6(block_timestamp),
                        tblock_err: 30,
                        parent_block_hash: HashOutputV6(block_parent_hash.0),
                    },
                    whitelist: None,
                };

                // Assume midnight-node has already validated included transactions.
                let mut strictness = WellFormedStrictnessV6::default();
                strictness.enforce_balancing = false;
                strictness.enforce_limits = false;
                strictness.verify_contract_proofs = false;
                strictness.verify_native_proofs = false;
                strictness.verify_signatures = false;
                let verified_transaction = ledger_transaction
                    .well_formed(&cx.ref_state, strictness, cx.block_context.tblock)
                    .map_err(|error| Error::MalformedTransaction(error.into()))?;

                let (ledger_state, transaction_result) =
                    ledger_state.apply(&verified_transaction, &cx);
                *self = Self::V6(ledger_state);

                let transaction_result = match transaction_result {
                    TransactionResultV6::Success(_) => TransactionResult::Success,

                    TransactionResultV6::PartialSuccess(segments, _) => {
                        let segments = segments
                            .into_iter()
                            .map(|(id, result)| (id, result.is_ok()))
                            .collect::<Vec<_>>();
                        TransactionResult::PartialSuccess(segments)
                    }

                    TransactionResultV6::Failure(_) => TransactionResult::Failure,
                };

                let (created_unshielded_utxos, spent_unshielded_utxos) =
                    extract_unshielded_utxos_v6(ledger_transaction, &transaction_result);

                Ok((
                    transaction_result,
                    created_unshielded_utxos,
                    spent_unshielded_utxos,
                ))
            }
        }
    }

    /// Apply the given serialized system transaction to this ledger state.
    #[trace]
    pub fn apply_system_transaction(
        &mut self,
        transaction: &SerializedTransaction,
        block_timestamp: u64,
    ) -> Result<(), Error> {
        match self {
            Self::V6(ledger_state) => {
                let ledger_transaction =
                    tagged_deserialize_v6::<LedgerSystemTransactionV6>(&mut transaction.as_ref())
                        .map_err(|error| {
                        Error::Io("cannot deserialize LedgerSystemTransactionV6", error)
                    })?;

                // TODO Handle events!
                let (ledger_state, _events) = ledger_state
                    .apply_system_tx(&ledger_transaction, timestamp_v6(block_timestamp))
                    .map_err(|error| Error::SystemTransaction(error.into()))?;
                *self = Self::V6(ledger_state);

                Ok(())
            }
        }
    }

    /// Get the first free index of the zswap state.
    pub fn zswap_first_free(&self) -> u64 {
        match self {
            Self::V6(ledger_state) => ledger_state.zswap.first_free,
        }
    }

    /// Get the merkle tree root of the zswap state.
    pub fn zswap_merkle_tree_root(&self) -> ZswapStateRoot {
        match self {
            Self::V6(ledger_state) => {
                let root = ledger_state
                    .zswap
                    .coin_coms
                    .root()
                    .expect("zswap merkle tree root should exist");
                ZswapStateRoot::V6(root)
            }
        }
    }

    /// Extract the zswap state for the given contract address.
    pub fn extract_contract_zswap_state(
        &self,
        address: &SerializedContractAddress,
    ) -> Result<SerializedZswapState, Error> {
        match self {
            Self::V6(ledger_state) => {
                let address = tagged_deserialize_v6::<ContractAddressV6>(&mut address.as_ref())
                    .map_err(|error| Error::Io("cannot deserialize ContractAddressV6", error))?;

                let mut contract_zswap_state = ZswapStateV6::new();
                contract_zswap_state.coin_coms = ledger_state.zswap.filter(&[address]);

                contract_zswap_state
                    .tagged_serialize_v6()
                    .map_err(|error| Error::Io("cannot serialize ZswapStateV6", error))
            }
        }
    }

    /// Extract the UTXOs.
    pub fn extract_utxos(&self) -> Vec<UnshieldedUtxo> {
        match self {
            Self::V6(ledger_state) => ledger_state
                .utxo
                .utxos
                .keys()
                .map(|utxo| UnshieldedUtxo {
                    value: utxo.value,
                    owner: utxo.owner.0.0.into(),
                    token_type: utxo.type_.0.0.into(),
                    intent_hash: utxo.intent_hash.0.0.into(),
                    output_index: utxo.output_no,
                })
                .collect(),
        }
    }

    /// Extract the serialized merkle-tree collapsed update for the given indices.
    pub fn collapsed_update(&self, start_index: u64, end_index: u64) -> Result<ByteVec, Error> {
        match self {
            Self::V6(ledger_state) => MerkleTreeCollapsedUpdateV6::new(
                &ledger_state.zswap.coin_coms,
                start_index,
                end_index,
            )
            .map_err(|error| Error::InvalidUpdate(error.into()))?
            .tagged_serialize_v6()
            .map_err(|error| Error::Io("cannot serialize MerkleTreeCollapsedUpdateV6", error)),
        }
    }

    /// To be called after applying transactions.
    pub fn post_apply_transactions(&mut self, block_timestamp: u64) {
        match self {
            Self::V6(ledger_state) => {
                let timestamp = timestamp_v6(block_timestamp);
                let ledger_state = ledger_state.post_block_update(timestamp);
                *self = Self::V6(ledger_state);
            }
        }
    }
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

/// An unshielded UTXO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnshieldedUtxo {
    pub owner: RawUnshieldedAddress,
    pub token_type: RawTokenType,
    pub value: u128,
    pub intent_hash: IntentHash,
    pub output_index: u32,
}

/// Facade for zswap state root across supported (protocol) versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZswapStateRoot {
    V6(MerkleTreeDigestV6),
}

impl ZswapStateRoot {
    /// Deserialize the given serialized zswap state root using the given protocol version.
    #[trace(properties = { "protocol_version": "{protocol_version}" })]
    pub fn deserialize(
        zswap_state_root: impl AsRef<[u8]>,
        protocol_version: ProtocolVersion,
    ) -> Result<Self, Error> {
        if protocol_version.is_compatible(PROTOCOL_VERSION_000_016_000) {
            let digest = MerkleTreeDigestV6::deserialize(&mut zswap_state_root.as_ref(), 0)
                .map_err(|error| Error::Io("cannot deserialize MerkleTreeDigestV6", error))?;
            Ok(Self::V6(digest))
        } else {
            Err(Error::InvalidProtocolVersion(protocol_version))
        }
    }

    /// Serialize this zswap state root.
    #[trace]
    pub fn serialize(&self) -> Result<SerializedZswapStateRoot, Error> {
        match self {
            Self::V6(digest) => digest
                .serialize_v6()
                .map_err(|error| Error::Io("cannot serialize zswap merkle tree root", error)),
        }
    }
}

fn timestamp_v6(block_timestamp: u64) -> TimestampV6 {
    TimestampV6::from_secs(block_timestamp / 1000)
}

fn extract_unshielded_utxos_v6(
    ledger_transaction: TransactionV6,
    transaction_result: &TransactionResult,
) -> (Vec<UnshieldedUtxo>, Vec<UnshieldedUtxo>) {
    match ledger_transaction {
        TransactionV6::Standard(transaction) => {
            let successful_segments = match &transaction_result {
                TransactionResult::Success => transaction.segments().into_iter().collect(),

                TransactionResult::PartialSuccess(segments) => segments
                    .iter()
                    .filter_map(|(id, success)| success.then_some(id))
                    .copied()
                    .collect(),

                TransactionResult::Failure => HashSet::new(),
            };

            let mut outputs = vec![];
            let mut inputs = vec![];

            for segment_id in transaction.segments() {
                // Guaranteed phase.
                if segment_id == 0 {
                    for intent in transaction.intents.values() {
                        extend_v6(&mut outputs, &mut inputs, segment_id, &intent, true);
                    }

                // Fallible phase.
                } else if let Some(intent) = transaction.intents.get(&segment_id)
                    && successful_segments.contains(&segment_id)
                {
                    extend_v6(&mut outputs, &mut inputs, segment_id, &intent, false);
                }
            }

            (outputs, inputs)
        }

        TransactionV6::ClaimRewards(_) => (vec![], vec![]),
    }
}

fn extend_v6(
    outputs: &mut Vec<UnshieldedUtxo>,
    inputs: &mut Vec<UnshieldedUtxo>,
    segment_id: u16,
    intent: &IntentV6,
    guaranteed: bool,
) {
    let intent_hash = intent
        .erase_proofs()
        .erase_signatures()
        .intent_hash(segment_id);
    let intent_hash = intent_hash.0.0.into();

    let intent_outputs = if guaranteed {
        intent.guaranteed_outputs()
    } else {
        intent.fallible_outputs()
    };
    let intent_outputs = intent_outputs
        .into_iter()
        .enumerate()
        .map(|(output_index, output)| UnshieldedUtxo {
            owner: output.owner.0.0.into(),
            token_type: output.type_.0.0.into(),
            value: output.value,
            intent_hash,
            output_index: output_index as u32,
        });
    outputs.extend(intent_outputs);

    let intent_inputs = if guaranteed {
        intent.guaranteed_inputs()
    } else {
        intent.fallible_inputs()
    };
    let intent_inputs = intent_inputs.into_iter().map(|spend| UnshieldedUtxo {
        owner: UserAddressV6::from(spend.owner).0.0.into(),
        token_type: spend.type_.0.0.into(),
        value: spend.value,
        intent_hash,
        output_index: spend.output_no,
    });
    inputs.extend(intent_inputs);
}
