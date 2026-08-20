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
    domain::{DParameter, DustRegistrationEvent, TermsAndConditions},
    infra::subxt_node::{
        ContentSource, OnlineClientAtBlock, SubxtNodeError,
        runtimes::{BlockDetails, Transaction},
    },
};
use futures::TryStreamExt;
use indexer_common::domain::{
    ByteVec, DustPublicKey, SerializedContractAddress, SerializedContractState,
    TermsAndConditionsHash,
};
use itertools::Itertools;
use parity_scale_codec::Decode;
use subxt::error::RuntimeApiError;

pub async fn make_block_details(
    authorities: &mut Option<Vec<[u8; 32]>>,
    block: &OnlineClientAtBlock,
    content: Option<&ContentSource>,
) -> Result<BlockDetails, SubxtNodeError> {
    use super::runtime_1_0_0::{
        Call, Event,
        runtime_types::{
            pallet_cnight_observation::pallet::Event as CnightObservationEvent,
            pallet_midnight::pallet::Call::send_mn_transaction,
            pallet_midnight_system::pallet::{
                Call::send_mn_system_transaction, Event::SystemTransactionApplied,
            },
            pallet_partner_chains_session::pallet::Event::NewSession,
        },
        timestamp,
    };

    let extrinsics = match content {
        // Enactment block: decode this block's raw extrinsic bytes against the parent
        // (old-runtime) client. `from_bytes` is async but infallible.
        Some(content) => {
            content
                .client
                .extrinsics()
                .from_bytes(content.extrinsic_bodies.clone())
                .await
        }
        None => block
            .extrinsics()
            .fetch()
            .await
            .map_err(|error| SubxtNodeError::FetchExtrinsics(error.into()))?,
    };

    let calls = extrinsics
        .iter()
        .map(|extrinsic| {
            let call = extrinsic
                .map_err(|error| SubxtNodeError::GetNextExtrinsic(error.into()))?
                .decode_call_data_as::<Call>()
                .map_err(|error| SubxtNodeError::DecodeExtrinsicAsCall(error.into()))?;
            Ok(call)
        })
        .filter_ok(|call| {
            matches!(
                call,
                Call::Timestamp(_) | Call::Midnight(_) | Call::MidnightSystem(_)
            )
        })
        .collect::<Result<Vec<_>, SubxtNodeError>>()?;

    let timestamp = calls.iter().find_map(|call| match call {
        Call::Timestamp(timestamp::Call::set { now }) => Some(*now),
        _ => None,
    });

    let transactions = calls
        .into_iter()
        .filter_map(|call| match call {
            Call::Midnight(send_mn_transaction { midnight_tx }) => {
                Some(Transaction::Regular(midnight_tx.into()))
            }

            Call::MidnightSystem(send_mn_system_transaction { midnight_system_tx }) => {
                Some(Transaction::System(midnight_system_tx.into()))
            }

            _ => None,
        })
        .collect::<Vec<_>>();

    let mut dust_registration_events = vec![];
    let mut system_transactions_from_events = vec![];

    let events = match content {
        // Enactment block: raw event bytes are metadata-independent, so fetch them from this
        // block and re-decode against the parent (old-runtime) client. `from_bytes` is sync.
        Some(content) => {
            let raw = block
                .events()
                .fetch()
                .await
                .map_err(|error| SubxtNodeError::FetchEvents(error.into()))?
                .bytes()
                .to_vec();
            content.client.events().from_bytes(raw)
        }
        None => block
            .events()
            .fetch()
            .await
            .map_err(|error| SubxtNodeError::FetchEvents(error.into()))?,
    };

    for event in events.iter() {
        let event = event
            .map_err(|error| SubxtNodeError::GetNextEvent(error.into()))?
            .decode_as::<Event>()
            .map_err(|error| SubxtNodeError::DecodeEvent(error.into()))?;

        match event {
            Event::Session(NewSession { .. }) => {
                *authorities = None;
            }

            // System transaction created by the node (not from extrinsics).
            // These come from inherents which execute BEFORE regular transactions,
            // so they must be prepended to maintain correct execution order.
            Event::MidnightSystem(SystemTransactionApplied(transaction_applied)) => {
                system_transactions_from_events.push(Transaction::System(ByteVec::from(
                    transaction_applied.serialized_system_transaction,
                )));
            }

            // DUST registration events from NativeTokenObservation pallet.
            Event::CNightObservation(native_token_event) => match native_token_event {
                CnightObservationEvent::Registration(event) => {
                    dust_registration_events.push(DustRegistrationEvent::Registration {
                        cardano_stake_key: event.cardano_reward_address.0.into(),
                        dust_address: event.dust_public_key.0.0.into(),
                    });
                }

                CnightObservationEvent::Deregistration(event) => {
                    dust_registration_events.push(DustRegistrationEvent::Deregistration {
                        cardano_stake_key: event.cardano_reward_address.0.into(),
                        dust_address: event.dust_public_key.0.0.into(),
                    });
                }

                CnightObservationEvent::MappingAdded(event) => {
                    dust_registration_events.push(DustRegistrationEvent::MappingAdded {
                        cardano_stake_key: event.cardano_reward_address.0.into(),
                        dust_address: event.dust_public_key.0.0.into(),
                        utxo_id: event.utxo_tx_hash.0.as_ref().into(),
                        utxo_index: event.utxo_index.into(),
                    });
                }

                CnightObservationEvent::MappingRemoved(event) => {
                    dust_registration_events.push(DustRegistrationEvent::MappingRemoved {
                        cardano_stake_key: event.cardano_reward_address.0.into(),
                        dust_address: event.dust_public_key.0.0.into(),
                        utxo_id: event.utxo_tx_hash.0.as_ref().into(),
                        utxo_index: event.utxo_index.into(),
                    });
                }

                _ => {}
            },

            // The c2m-bridge pallet does not exist in node 1.0.0 metadata.
            // Decoding for `Event::C2MBridge` lives in `runtimes/v2_0_0.rs`,
            // where node 2.0.0-alpha.1 (PR #1386 et al.) added the pallet.
            _ => {}
        }
    }

    // Prepend system transactions from events (inherents) before regular transactions.
    // In Substrate, inherents execute before regular transactions in a block.
    system_transactions_from_events.extend(transactions);
    let transactions = system_transactions_from_events;

    Ok(BlockDetails {
        timestamp,
        transactions,
        dust_registration_events,
        bridge_events: vec![],
    })
}

pub async fn fetch_authorities(
    block: &OnlineClientAtBlock,
) -> Result<Vec<[u8; 32]>, SubxtNodeError> {
    let authorities = block
        .storage()
        .entry(super::runtime_1_0_0::storage().aura().authorities())
        .map_err(|error| SubxtNodeError::FetchAuthorities(error.into()))?
        .fetch(())
        .await
        .map_err(|error| SubxtNodeError::FetchAuthorities(error.into()))?
        .decode()
        .map_err(|error| SubxtNodeError::DecodeAuthorities(error.into()))?;
    let authorities = authorities.0.into_iter().map(|a| a.0).collect();

    Ok(authorities)
}

pub fn decode_slot(mut slot: &[u8]) -> Result<u64, SubxtNodeError> {
    let slot = super::runtime_1_0_0::runtime_types::sp_consensus_slots::Slot::decode(&mut slot)
        .map(|x| x.0)?;
    Ok(slot)
}

pub async fn get_contract_state(
    address: SerializedContractAddress,
    block: &OnlineClientAtBlock,
) -> Result<SerializedContractState, SubxtNodeError> {
    let get_state = super::runtime_1_0_0::runtime_apis()
        .midnight_runtime_api()
        .get_contract_state(address.as_slice().into());

    let state = block
        .runtime_apis()
        .call(get_state)
        .await
        .map_err(|error| SubxtNodeError::GetContractState(address.clone(), error.into()))?
        .map_err(|error| SubxtNodeError::GetContractState(address, format!("{error:?}").into()))?
        .into();

    Ok(state)
}

pub async fn get_zswap_merkle_tree_root(
    block: &OnlineClientAtBlock,
) -> Result<Vec<u8>, SubxtNodeError> {
    let get_zswap_state_root = super::runtime_1_0_0::runtime_apis()
        .midnight_runtime_api()
        .get_zswap_state_root();

    let root = block.runtime_apis().call(&get_zswap_state_root).await;

    let root = match root {
        // Retry with online client at parent block if codegen is incompatible which can happen for
        // runtime updates, because subxt uses next metadata whereas Node uses previous metadata.
        Err(RuntimeApiError::IncompatibleCodegen) => {
            let parent_hash = block
                .block_header()
                .await
                .map_err(|error| SubxtNodeError::GetBlockHeader(error.into()))?
                .parent_hash;
            let block = block
                .online_client()
                .at_block(parent_hash)
                .await
                .map_err(|error| SubxtNodeError::GetOnlineClientAt(parent_hash, error.into()))?;
            block.runtime_apis().call(get_zswap_state_root).await
        }

        other => other,
    };

    root.map_err(|error| SubxtNodeError::GetZswapStateRoot(error.into()))?
        .map_err(|error| SubxtNodeError::GetZswapStateRoot(format!("{error:?}").into()))
}

pub async fn get_ledger_state_root(
    block: &OnlineClientAtBlock,
) -> Result<Option<Vec<u8>>, SubxtNodeError> {
    let get_ledger_state_root = super::runtime_1_0_0::runtime_apis()
        .midnight_runtime_api()
        .get_ledger_state_root();

    let root = block
        .runtime_apis()
        .call(get_ledger_state_root)
        .await
        .map_err(|error| SubxtNodeError::GetLedgerStateRoot(error.into()))?
        .map_err(|error| SubxtNodeError::GetLedgerStateRoot(format!("{error:?}").into()))?;

    Ok(Some(root))
}

pub async fn get_d_parameter(block: &OnlineClientAtBlock) -> Result<DParameter, SubxtNodeError> {
    let get_d_param = super::runtime_1_0_0::runtime_apis()
        .system_parameters_api()
        .get_d_parameter();

    let d_parameter = block
        .runtime_apis()
        .call(get_d_param)
        .await
        .map_err(|error| SubxtNodeError::GetDParameter(error.into()))?;

    Ok(DParameter {
        num_permissioned_candidates: d_parameter.num_permissioned_candidates,
        num_registered_candidates: d_parameter.num_registered_candidates,
    })
}

pub async fn fetch_genesis_cnight_registrations(
    block: &OnlineClientAtBlock,
) -> Result<Vec<DustRegistrationEvent>, SubxtNodeError> {
    let query = super::runtime_1_0_0::storage()
        .c_night_observation()
        .mappings();
    block
        .storage()
        .entry(query)
        .map_err(|error| SubxtNodeError::FetchGenesisCnightRegistrations(error.into()))?
        .iter(())
        .await
        .map_err(|error| SubxtNodeError::FetchGenesisCnightRegistrations(error.into()))?
        .try_collect::<Vec<_>>()
        .await
        .map_err(|error| SubxtNodeError::FetchGenesisCnightRegistrations(error.into()))?
        .into_iter()
        .try_fold(vec![], |mut events, mapping| {
            let mapping = mapping
                .value()
                .decode()
                .map_err(|error| SubxtNodeError::DecodeGenesisCnightRegistrations(error.into()))?;

            let these_events = mapping
                .first()
                .map(|mapping| {
                    let cardano_stake_key = mapping.cardano_reward_address.0.into();
                    let dust_address = DustPublicKey::from(mapping.dust_public_key.0.0.clone());
                    let utxo_id = mapping.utxo_tx_hash.0.as_ref().into();
                    let utxo_index = mapping.utxo_index.into();

                    let events = vec![
                        DustRegistrationEvent::Registration {
                            cardano_stake_key,
                            dust_address: dust_address.clone(),
                        },
                        DustRegistrationEvent::MappingAdded {
                            cardano_stake_key,
                            dust_address,
                            utxo_id,
                            utxo_index,
                        },
                    ];

                    events
                })
                .unwrap_or_default();

            events.extend(these_events);

            Ok(events)
        })
}

pub async fn get_terms_and_conditions(
    block: &OnlineClientAtBlock,
) -> Result<Option<TermsAndConditions>, SubxtNodeError> {
    let get_tc = super::runtime_1_0_0::runtime_apis()
        .system_parameters_api()
        .get_terms_and_conditions();

    let tc = block
        .runtime_apis()
        .call(get_tc)
        .await
        .map_err(|error| SubxtNodeError::GetTermsAndConditions(error.into()))?;

    Ok(tc.map(|response| {
        let hash = TermsAndConditionsHash::from(response.hash.0);
        let url = String::from_utf8_lossy(&response.url).to_string();
        TermsAndConditions { hash, url }
    }))
}
