// This file is part of midnight-ledger.
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

use crate::dust::{DustCommitment, DustGenerationInfo, DustNullifier, QualifiedDustOutput};
use crate::structure::{LedgerParameters, TransactionHash};
use base_crypto::time::Timestamp;
use coin_structure::coin::{
    Commitment as CoinCommitment, Info as CoinInfo, Nullifier as CoinNullifier,
};
use coin_structure::contract::ContractAddress;
use coin_structure::transfer::Recipient;
use derive_where::derive_where;
use onchain_runtime::state::{ContractState, EntryPointBuf, StateValue};
use serialize::{Deserializable, Serializable, Tagged, tag_enforcement_test};
#[cfg(test)]
use storage::db::InMemoryDB;
use storage::{
    Storable,
    arena::{ArenaKey, Sp},
    db::DB,
    storable::Loader,
};
use transient_crypto::merkle_tree::TreeInsertionPath;
use zswap::{CoinCiphertext, keys::SecretKeys as ZswapSecretKeys};

#[derive_where(PartialEq, Eq, Clone, Debug)]
#[derive(Storable)]
#[storable(db = D)]
#[tag = "event[v9]"]
pub struct Event<D: DB> {
    pub source: EventSource,
    pub content: EventDetails<D>,
}
tag_enforcement_test!(Event<InMemoryDB>);

#[derive(PartialEq, Eq, Clone, Debug, Serializable, Storable)]
#[storable(base)]
#[tag = "event-source[v1]"]
pub struct EventSource {
    pub transaction_hash: TransactionHash,
    pub logical_segment: u16,
    pub physical_segment: u16,
}
tag_enforcement_test!(EventSource);

#[derive(PartialEq, Eq, Clone, Debug, Serializable, Storable)]
#[storable(base)]
#[tag = "zswap-preimage-evidence[v2]"]
pub enum ZswapPreimageEvidence {
    Ciphertext(Box<CoinCiphertext>),
    PublicPreimage {
        coin: CoinInfo,
        recipient: Recipient,
    },
    None,
}
tag_enforcement_test!(ZswapPreimageEvidence);

impl ZswapPreimageEvidence {
    pub fn try_with_keys(&self, secret_keys: &ZswapSecretKeys) -> Option<CoinInfo> {
        match self {
            ZswapPreimageEvidence::Ciphertext(ciph) => secret_keys.try_decrypt(ciph),
            ZswapPreimageEvidence::PublicPreimage {
                coin,
                recipient: Recipient::User(pk),
            } if *pk == secret_keys.coin_public_key() => Some(*coin),
            _ => None,
        }
    }
}

#[derive_where(PartialEq, Eq, Clone, Debug)]
#[derive(Storable)]
#[storable(db = D)]
#[tag = "event-details[v9]"]
#[non_exhaustive]
pub enum EventDetails<D: DB> {
    ZswapInput {
        nullifier: CoinNullifier,
        contract: Option<Sp<ContractAddress, D>>,
    },
    ZswapOutput {
        commitment: CoinCommitment,
        preimage_evidence: ZswapPreimageEvidence,
        contract: Option<Sp<ContractAddress, D>>,
        mt_index: u64,
    },
    ContractDeploy {
        address: ContractAddress,
        initial_state: ContractState<D>,
    },
    ContractLog {
        address: ContractAddress,
        entry_point: EntryPointBuf,
        logged_item: StateValue<D>,
    },
    ParamChange(Sp<LedgerParameters, D>),
    DustInitialUtxo {
        output: QualifiedDustOutput,
        generation: DustGenerationInfo,
        generation_index: u64,
        block_time: Timestamp,
    },
    DustGenerationDtimeUpdate {
        update: TreeInsertionPath<DustGenerationInfo>,
        block_time: Timestamp,
    },
    DustSpendProcessed {
        commitment: DustCommitment,
        commitment_index: u64,
        nullifier: DustNullifier,
        v_fee: u128,
        declared_time: Timestamp,
        block_time: Timestamp,
    },
}
tag_enforcement_test!(EventDetails<InMemoryDB>);
