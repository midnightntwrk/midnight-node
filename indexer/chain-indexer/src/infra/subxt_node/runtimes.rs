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

// To see how this is generated, look in build.rs
include!(concat!(env!("OUT_DIR"), "/generated_runtime.rs"));

use crate::infra::subxt_node::SubxtNodeError;
use indexer_common::domain::{
    BlockHash, ByteVec, PROTOCOL_VERSION_000_016_000, ProtocolVersion,
    ledger::{SerializedContractAddress, SerializedContractState},
};
use itertools::Itertools;
use parity_scale_codec::Decode;
use subxt::{OnlineClient, SubstrateConfig, blocks::Extrinsics, events::Events, utils::H256};

/// Runtime specific block details.
pub struct BlockDetails {
    pub timestamp: Option<u64>,
    pub transactions: Vec<Transaction>,
}

/// Runtime specific (serialized) transaction.
pub enum Transaction {
    Regular(ByteVec),
    System(ByteVec),
}

/// Make block details depending on the given protocol version.
pub async fn make_block_details(
    extrinsics: Extrinsics<SubstrateConfig, OnlineClient<SubstrateConfig>>,
    events: Events<SubstrateConfig>,
    authorities: &mut Option<Vec<[u8; 32]>>,
    protocol_version: ProtocolVersion,
) -> Result<BlockDetails, SubxtNodeError> {
    // TODO Replace this often repeated pattern with a macro?
    if protocol_version.is_compatible(PROTOCOL_VERSION_000_016_000) {
        make_block_details_runtime_0_16(extrinsics, events, authorities).await
    } else {
        Err(SubxtNodeError::InvalidProtocolVersion(protocol_version))
    }
}

/// Fetch authorities depending on the given protocol version.
pub async fn fetch_authorities(
    block_hash: BlockHash,
    protocol_version: ProtocolVersion,
    online_client: &OnlineClient<SubstrateConfig>,
) -> Result<Option<Vec<[u8; 32]>>, SubxtNodeError> {
    if protocol_version.is_compatible(PROTOCOL_VERSION_000_016_000) {
        fetch_authorities_runtime_0_16(block_hash, online_client).await
    } else {
        Err(SubxtNodeError::InvalidProtocolVersion(protocol_version))
    }
}

/// Decode slot depending on the given protocol version.
pub fn decode_slot(slot: &[u8], protocol_version: ProtocolVersion) -> Result<u64, SubxtNodeError> {
    if protocol_version.is_compatible(PROTOCOL_VERSION_000_016_000) {
        decode_slot_runtime_0_16(slot)
    } else {
        Err(SubxtNodeError::InvalidProtocolVersion(protocol_version))
    }
}

/// Get contract state depending on the given protocol version.
pub async fn get_contract_state(
    address: SerializedContractAddress,
    block_hash: BlockHash,
    protocol_version: ProtocolVersion,
    online_client: &OnlineClient<SubstrateConfig>,
) -> Result<SerializedContractState, SubxtNodeError> {
    if protocol_version.is_compatible(PROTOCOL_VERSION_000_016_000) {
        get_contract_state_runtime_0_16(address, block_hash, online_client).await
    } else {
        Err(SubxtNodeError::InvalidProtocolVersion(protocol_version))
    }
}

pub async fn get_zswap_state_root(
    block_hash: BlockHash,
    protocol_version: ProtocolVersion,
    online_client: &OnlineClient<SubstrateConfig>,
) -> Result<Vec<u8>, SubxtNodeError> {
    if protocol_version.is_compatible(PROTOCOL_VERSION_000_016_000) {
        get_zswap_state_root_runtime_0_16(block_hash, online_client).await
    } else {
        Err(SubxtNodeError::InvalidProtocolVersion(protocol_version))
    }
}

/// Get cost for the given serialized transaction depending on the given protocol version.
pub async fn get_transaction_cost(
    transaction: impl AsRef<[u8]>,
    block_hash: BlockHash,
    protocol_version: ProtocolVersion,
    online_client: &OnlineClient<SubstrateConfig>,
) -> Result<u128, SubxtNodeError> {
    if protocol_version.is_compatible(PROTOCOL_VERSION_000_016_000) {
        get_transaction_cost_runtime_0_16(transaction.as_ref(), block_hash, online_client).await
    } else {
        Err(SubxtNodeError::InvalidProtocolVersion(protocol_version))
    }
}

macro_rules! make_block_details {
    ($module:ident) => {
        paste::paste! {
            async fn [<make_block_details_ $module>](
                extrinsics: Extrinsics<SubstrateConfig, OnlineClient<SubstrateConfig>>,
                events: Events<SubstrateConfig>,
                authorities: &mut Option<Vec<[u8; 32]>>,
            ) -> Result<BlockDetails, SubxtNodeError> {
                use self::$module::{
                    Call, Event, midnight, midnight_system, timestamp,
                    runtime_types::pallet_partner_chains_session::pallet::Event::NewSession,
                };

                let calls = extrinsics
                    .iter()
                    .map(|extrinsic| {
                        let call = extrinsic.as_root_extrinsic::<Call>().map_err(Box::new)?;
                        Ok(call)
                    })
                    .filter_ok(|call|
                        matches!(
                            call,
                            Call::Timestamp(_) | Call::Midnight(_) | Call::MidnightSystem(_)
                        )
                    )
                    .collect::<Result<Vec<_>, SubxtNodeError>>()?;

                let timestamp = calls.iter().find_map(|call| match call {
                    Call::Timestamp(timestamp::Call::set { now }) => Some(*now),
                    _ => None,
                });

                let transactions = calls
                    .into_iter()
                    .filter_map(|call| match call {
                        Call::Midnight(
                            midnight::Call::send_mn_transaction { midnight_tx }
                        ) => {
                            Some(Transaction::Regular(midnight_tx.into()))
                        }

                        Call::MidnightSystem(
                            midnight_system::Call::send_mn_system_transaction { midnight_system_tx }
                        ) => {
                            Some(Transaction::System(midnight_system_tx.into()))
                        }

                        _ => None,
                    })
                    .collect();

                for event in events.iter().flatten() {
                    let event = event.as_root_event::<Event>();
                    if let Ok(Event::Session(NewSession { .. })) = event {
                        *authorities = None;
                    }
                }

                Ok(BlockDetails {
                    timestamp,
                    transactions,
                })
            }
        }
    };
}

make_block_details!(runtime_0_16);

macro_rules! fetch_authorities {
    ($module:ident) => {
        paste::paste! {
            async fn [<fetch_authorities_ $module>](
                block_hash: BlockHash,
                online_client: &OnlineClient<SubstrateConfig>,
            ) -> Result<Option<Vec<[u8; 32]>>, SubxtNodeError> {
                let authorities = online_client
                    .storage()
                    .at(H256(block_hash.0))
                    .fetch(&$module::storage().aura().authorities())
                    .await
                    .map_err(Box::new)?
                    .map(|authorities| authorities.0.into_iter().map(|public| public.0).collect());

                Ok(authorities)
            }
        }
    };
}

fetch_authorities!(runtime_0_16);

macro_rules! decode_slot {
    ($module:ident) => {
        paste::paste! {
            fn [<decode_slot_ $module>](mut slot: &[u8]) -> Result<u64, SubxtNodeError> {
                let slot = $module::runtime_types::sp_consensus_slots::Slot::decode(&mut slot)
                    .map(|x| x.0)?;
                Ok(slot)
            }
        }
    };
}

decode_slot!(runtime_0_16);

macro_rules! get_contract_state {
    ($module:ident) => {
        paste::paste! {
            async fn [<get_contract_state_ $module>](
                address: SerializedContractAddress,
                block_hash: BlockHash,
                online_client: &OnlineClient<SubstrateConfig>,
            ) -> Result<SerializedContractState, SubxtNodeError> {
                // This returns the serialized contract state.
                let get_state = $module::apis()
                    .midnight_runtime_api()
                    .get_contract_state(address.into());

                let state = online_client
                    .runtime_api()
                    .at(H256(block_hash.0))
                    .call(get_state)
                    .await
                    .map_err(Box::new)?
                    .map_err(|error| SubxtNodeError::GetContractState(format!("{error:?}")))?
                    .into();

                Ok(state)
            }
        }
    };
}

get_contract_state!(runtime_0_16);

macro_rules! get_zswap_state_root {
    ($module:ident) => {
        paste::paste! {
            async fn [<get_zswap_state_root_ $module>](
                block_hash: BlockHash,
                online_client: &OnlineClient<SubstrateConfig>,
            ) -> Result<Vec<u8>, SubxtNodeError> {
                let get_zswap_state_root = $module::apis()
                    .midnight_runtime_api()
                    .get_zswap_state_root();

                let root = online_client
                    .runtime_api()
                    .at(H256(block_hash.0))
                    .call(get_zswap_state_root)
                    .await
                    .map_err(Box::new)?
                    .map_err(|error| SubxtNodeError::GetZswapStateRoot(format!("{error:?}")))?;

                Ok(root)

            }
        }
    };
}

get_zswap_state_root!(runtime_0_16);

macro_rules! get_transaction_cost {
    ($module:ident) => {
        paste::paste! {
            async fn [<get_transaction_cost_ $module>](
                transaction: &[u8],
                block_hash: BlockHash,
                online_client: &OnlineClient<SubstrateConfig>,
            ) -> Result<u128, SubxtNodeError> {
                let get_transaction_cost = $module::apis()
                    .midnight_runtime_api()
                    .get_transaction_cost(transaction.to_owned());

                let (storage_cost, gas_cost) = online_client
                    .runtime_api()
                    .at(H256(block_hash.0))
                    .call(get_transaction_cost)
                    .await
                    .map_err(Box::new)?
                    .map_err(|error| SubxtNodeError::GetTransactionCost(format!("{error:?}")))?;

                // Combine storage cost and gas cost for total fee
                // StorageCost = u128, GasCost = u64
                let total_cost = storage_cost.saturating_add(gas_cost as u128);
                Ok(total_cost)
            }
        }
    };
}

get_transaction_cost!(runtime_0_16);
