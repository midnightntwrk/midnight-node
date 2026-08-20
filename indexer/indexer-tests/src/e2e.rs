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

//! e2e testing library

use crate::{
    e2e::graphql::{
        BlockQuery, BlockSubscription, ConnectMutation, ContractActionQuery,
        ContractActionSubscription, DParameterHistoryQuery, DisconnectMutation,
        DustCommitmentMerkleTreeUpdateQuery, DustGenerationMerkleTreeUpdateQuery,
        DustGenerationStatusQuery, DustGenerationsQuery, DustGenerationsSubscription,
        DustLedgerEventsSubscription, DustNullifierTransactionsSubscription,
        ShieldedNullifierTransactionsSubscription, ShieldedTransactionsSubscription,
        TermsAndConditionsHistoryQuery, TransactionsQuery, UnshieldedTransactionsSubscription,
        ZswapLedgerEventsSubscription, ZswapMerkleTreeCollapsedUpdateQuery, block_query,
        block_subscription::{
            self, BlockSubscriptionBlocks, BlockSubscriptionBlocksTransactions,
            BlockSubscriptionBlocksTransactionsContractActions,
            BlockSubscriptionBlocksTransactionsDustLedgerEvents,
            BlockSubscriptionBlocksTransactionsOn,
            BlockSubscriptionBlocksTransactionsOnRegularTransaction,
            BlockSubscriptionBlocksTransactionsUnshieldedCreatedOutputs,
            BlockSubscriptionBlocksTransactionsZswapLedgerEvents,
        },
        connect_mutation,
        contract_action_query::{self},
        contract_action_subscription, disconnect_mutation,
        dust_commitment_merkle_tree_update_query, dust_generation_merkle_tree_update_query,
        dust_generation_status_query, dust_generations_query, dust_generations_subscription,
        dust_ledger_events_subscription, dust_nullifier_transactions_subscription,
        shielded_nullifier_transactions_subscription, shielded_transactions_subscription,
        transactions_query, unshielded_transactions_subscription, zswap_ledger_events_subscription,
        zswap_merkle_tree_collapsed_update_query,
    },
    graphql_ws_client,
};
use anyhow::{Context, bail};
use futures::{StreamExt, TryStreamExt, future::ok};
use graphql_client::{GraphQLQuery, Response};
use indexer_api::infra::api::v4::{
    AddressType, HexEncodable, HexEncoded, dust::DustAddress, encode_address,
    viewing_key::ViewingKey,
};
use indexer_common::domain::NetworkId;
use itertools::Itertools;
use reqwest::Client;
use serde::Serialize;
use shielded_transactions_subscription::ShieldedTransactionsSubscriptionShieldedTransactions as ShieldedTransactions;
use std::{future::ready, time::Duration};
use tokio::time::sleep;
use unshielded_transactions_subscription::UnshieldedTransactionsSubscriptionUnshieldedTransactions as UnshieldedTransactions;

const MAX_HEIGHT: usize = 32;

/// Run comprehensive e2e tests for the Indexer. It is expected that the Indexer is set up with all
/// needed dependencies, e.g. a Node, and its API is exposed securely (https and wss) or insecurely
/// (http and ws) at the given host and port.
///
/// Tests include validation of transaction fee metadata (paid_fee, estimated_fee) and segment
/// results.
pub async fn run(network_id: NetworkId, host: &str, port: u16, secure: bool) -> anyhow::Result<()> {
    println!("Starting e2e testing");

    let (api_url, ws_api_url) = {
        let core = format!("{host}:{port}/api/v4/graphql");

        if secure {
            (format!("https://{core}"), format!("wss://{core}/ws"))
        } else {
            (format!("http://{core}"), format!("ws://{core}/ws"))
        }
    };

    let api_client = Client::new();

    // Test mutations. This must come first to make the wallet indexer index the wallet that is
    // later used for the shielded transactions subscription.
    let session_id = test_connect_mutation(&api_client, &api_url, &network_id)
        .await
        .context("test connect mutation query")?;
    test_disconnect_mutation(&api_client, &api_url, &session_id)
        .await
        .context("test disconnect mutation query")?;

    // Reconnect after disconnect test — session is needed for shielded transactions subscription.
    let session_id = test_connect_mutation(&api_client, &api_url, &network_id)
        .await
        .context("test reconnect after disconnect")?;

    // Collect Indexer data using the block subscription.
    let indexer_data = IndexerData::collect(&ws_api_url)
        .await
        .context("collect Indexer data")?;

    // Test queries.
    test_block_query(&indexer_data, &api_client, &api_url)
        .await
        .context("test block query")?;
    test_transactions_query(&indexer_data, &api_client, &api_url)
        .await
        .context("test transactions query")?;
    test_contract_action_query(&indexer_data, &api_client, &api_url)
        .await
        .context("test contract action query")?;
    test_dust_generation_status_query(&api_client, &api_url)
        .await
        .context("test dust generation status query")?;
    test_dust_generations_query(&api_client, &api_url)
        .await
        .context("test dust generations query")?;
    test_dust_commitment_merkle_tree_update_query(&api_client, &api_url)
        .await
        .context("test dust commitment merkle tree update query")?;
    test_dust_generation_merkle_tree_update_query(&api_client, &api_url)
        .await
        .context("test dust generation merkle tree update query")?;
    test_governance_queries(&api_client, &api_url)
        .await
        .context("test governance queries")?;
    test_zswap_merkle_tree_collapsed_update_query(&indexer_data, &api_client, &api_url)
        .await
        .context("test zswap Merkle tree collapsed update query")?;

    // Test subscriptions (the block subscription has already been tested above).
    test_contract_actions_subscription(&indexer_data, &ws_api_url)
        .await
        .context("test contract action subscription")?;
    test_shielded_transactions_subscription(&ws_api_url, session_id.clone())
        .await
        .context("test shielded transactions subscription")?;
    test_unshielded_transactions_subscription(&indexer_data, &ws_api_url)
        .await
        .context("test unshielded transactions subscription")?;
    test_zswap_ledger_events_subscription(&indexer_data, &ws_api_url)
        .await
        .context("test zswap ledger events subscription")?;
    test_dust_ledger_events_subscription(&indexer_data, &ws_api_url)
        .await
        .context("test dust ledger events subscription")?;
    test_dust_generations_subscription(&indexer_data, &ws_api_url)
        .await
        .context("test dust generations subscription")?;
    test_shielded_nullifier_transactions_subscription(&ws_api_url)
        .await
        .context("test shielded nullifier transactions subscription")?;
    test_dust_nullifier_transactions_subscription(&ws_api_url)
        .await
        .context("test dust nullifier transactions subscription")?;

    println!("Successfully finished e2e testing");

    Ok(())
}

/// All data needed for testing collected from the Indexer via the blocks subscription. To be used
/// as expected data in tests for all other API operations.
struct IndexerData {
    blocks: Vec<BlockSubscriptionBlocks>,
    transactions: Vec<BlockSubscriptionBlocksTransactions>,
    contract_actions: Vec<BlockSubscriptionBlocksTransactionsContractActions>,
    unshielded_utxos: Vec<BlockSubscriptionBlocksTransactionsUnshieldedCreatedOutputs>,
    zswap_ledger_events: Vec<BlockSubscriptionBlocksTransactionsZswapLedgerEvents>,
    dust_ledger_events: Vec<BlockSubscriptionBlocksTransactionsDustLedgerEvents>,
}

impl IndexerData {
    /// Not only collects the Indexer data needed for testing, but also validates it, e.g. that
    /// block heights start at zero and increment by one.
    async fn collect(ws_api_url: &str) -> anyhow::Result<Self> {
        // Subscribe to blocks and collect up to MAX_HEIGHT.
        let variables = block_subscription::Variables {
            block_offset: Some(block_subscription::BlockOffset::Height(0)),
        };
        let blocks = graphql_ws_client::subscribe::<BlockSubscription>(ws_api_url, variables)
            .await
            .context("subscribe to blocks")?
            .take(1 + MAX_HEIGHT)
            .map_ok(|data| data.blocks)
            .try_collect::<Vec<_>>()
            .await
            .context("collect blocks from block subscription")?;

        // Validate that block heights start at zero and increment by one.
        assert_eq!(
            blocks.iter().map(|block| block.height).collect::<Vec<_>>(),
            (0..=MAX_HEIGHT).map(|n| n as i64).collect::<Vec<_>>()
        );

        // Verify that each block references its parent and the height is incremented by one.
        blocks.windows(2).all(|blocks| {
            let hash_0 = &blocks[0].hash;
            let height_0 = blocks[0].height;

            let parent_hash_1 = blocks[1]
                .parent
                .as_ref()
                .map(|block| &block.hash)
                .expect("non-genesis block has parent");
            let parent_height_1 = blocks[1]
                .parent
                .as_ref()
                .map(|block| block.height)
                .expect("non-genesis block has parent");

            hash_0 == parent_hash_1 && height_0 == parent_height_1
        });

        // Verify that all transactions of a block reference that block and have the same protocol
        // version like the block.
        assert!(blocks.iter().all(|block| {
            block.transactions.iter().all(|transaction| {
                transaction.block.hash == block.hash
                    && transaction.protocol_version == block.protocol_version
            })
        }));

        // Collect transactions.
        let transactions = blocks
            .iter()
            .flat_map(|block| block.transactions.to_owned())
            .collect::<Vec<_>>();

        // Verify that there are transactions.
        assert!(!transactions.is_empty());

        // Verify various properties for regular transactions.
        let regular_transactions = transactions
            .iter()
            .filter_map(|transaction| match &transaction.on {
                block_subscription::BlockSubscriptionBlocksTransactionsOn::RegularTransaction(
                    transaction,
                ) => Some(transaction),
                block_subscription::BlockSubscriptionBlocksTransactionsOn::SystemTransaction => {
                    None
                }
                block_subscription::BlockSubscriptionBlocksTransactionsOn::BridgeClaimTransaction => {
                    None
                }
            });
        for transaction in regular_transactions {
            // Verify transaction segment results.
            match &transaction.transaction_result.status {
                block_subscription::TransactionResultStatus::SUCCESS => {
                    assert!(transaction.transaction_result.segments.is_none())
                }
                block_subscription::TransactionResultStatus::PARTIAL_SUCCESS => {
                    assert!(transaction.transaction_result.segments.is_some())
                }
                block_subscription::TransactionResultStatus::FAILURE => {
                    assert!(transaction.transaction_result.segments.is_none())
                }
                block_subscription::TransactionResultStatus::Other(other) => {
                    panic!("unexpected variant TransactionResultStatus {other}")
                }
            };

            // Verify fees.
            assert!(transaction.fee.parse::<u64>().is_ok());
        }

        // Verify that contract actions of a transaction reference that transaction.
        assert!(transactions.iter().all(|transaction| {
            transaction
                .contract_actions
                .iter()
                .all(|contract_action| contract_action.transaction.hash == transaction.hash)
        }));

        // Collect contract actions.
        let contract_actions = transactions
            .iter()
            .flat_map(|transaction| transaction.contract_actions.iter().cloned())
            .collect::<Vec<_>>();

        // Verify that there are contract actions.
        assert!(!contract_actions.is_empty());

        // Verify that the contract action zswap state is non-empty.
        assert!(
            contract_actions
                .iter()
                .all(|contract_action| !contract_action.zswap_state.as_ref().is_empty())
        );

        // Verify that contract calls and their deploy have the same address.
        assert!(
            contract_actions
                .iter()
                .filter_map(|contract_action| {
                    let address = &contract_action.address;
                    match &contract_action.on {
                        block_subscription::BlockSubscriptionBlocksTransactionsContractActionsOn::ContractCall(contract_call) => {
                        Some((address, &contract_call.deploy.address))
                    }
                    _ => None,
                }})
                .all(|(call_address, deploy_address)| call_address == deploy_address)
        );

        // Verify that all unshielded balances have valid token_type and amount.
        assert!(contract_actions.iter().all(|contract_action| {
            contract_action.unshielded_balances.iter().all(|balance| {
                // Validate token_type is valid hex (should be 32 bytes = 64 hex chars).
                let valid_token_type = balance.token_type.as_ref().len() == 64
                    && balance
                        .token_type
                        .as_ref()
                        .chars()
                        .all(|c| c.is_ascii_hexdigit());
                // Validate amount is parseable as u128.
                let valid_amount = balance.amount.parse::<u128>().is_ok();
                valid_token_type && valid_amount
            })
        }));

        // Collect unshielded UTXOs.
        let unshielded_utxos = transactions
            .iter()
            .flat_map(|transaction| transaction.unshielded_created_outputs.to_owned())
            .collect::<Vec<_>>();

        // Verify that there are unshielded UTXOs.
        assert!(!unshielded_utxos.is_empty());

        // Collect ledger events.
        let zswap_ledger_events = transactions
            .iter()
            .flat_map(|transaction| transaction.zswap_ledger_events.to_owned())
            .collect::<Vec<_>>();
        let dust_ledger_events = transactions
            .iter()
            .flat_map(|transaction| transaction.dust_ledger_events.to_owned())
            .collect::<Vec<_>>();

        // Verify that there are ledger events.
        assert!(!zswap_ledger_events.is_empty());
        assert!(!dust_ledger_events.is_empty());

        Ok(Self {
            blocks,
            transactions,
            contract_actions,
            unshielded_utxos,
            zswap_ledger_events,
            dust_ledger_events,
        })
    }
}

/// Test the connect mutation.
async fn test_connect_mutation(
    api_client: &Client,
    api_url: &str,
    network_id: &NetworkId,
) -> anyhow::Result<HexEncoded> {
    // Valid viewing key.
    let viewing_key = ViewingKey::from(viewing_key(network_id));
    let variables = connect_mutation::Variables { viewing_key };
    let response = send_query::<ConnectMutation>(api_client, api_url, variables).await?;
    let session_id = response.connect;

    // Invalid viewing key.
    let variables = connect_mutation::Variables {
        viewing_key: ViewingKey("invalid".to_string()),
    };
    let response = send_query::<ConnectMutation>(api_client, api_url, variables).await;
    assert!(response.is_err());

    Ok(session_id)
}

/// Test the disconnect mutation.
async fn test_disconnect_mutation(
    api_client: &Client,
    api_url: &str,
    session_id: &HexEncoded,
) -> anyhow::Result<()> {
    // Valid session ID from a previous connect.
    let variables = disconnect_mutation::Variables {
        session_id: session_id.clone(),
    };
    let response = send_query::<DisconnectMutation>(api_client, api_url, variables).await;
    assert!(response.is_ok());

    // Unknown session ID.
    let variables = disconnect_mutation::Variables {
        session_id: [0u8; 32].hex_encode(),
    };
    let response = send_query::<DisconnectMutation>(api_client, api_url, variables).await;
    assert!(response.is_err());

    // Invalid session ID (wrong length).
    let variables = disconnect_mutation::Variables {
        session_id: [42; 1].hex_encode(),
    };
    let response = send_query::<DisconnectMutation>(api_client, api_url, variables).await;
    assert!(response.is_err());

    Ok(())
}

/// Test the block query.
async fn test_block_query(
    indexer_data: &IndexerData,
    api_client: &Client,
    api_url: &str,
) -> anyhow::Result<()> {
    for expected_block in &indexer_data.blocks {
        // Existing hash.
        let variables = block_query::Variables {
            block_offset: Some(block_query::BlockOffset::Hash(
                expected_block.hash.to_owned(),
            )),
        };
        let block = send_query::<BlockQuery>(api_client, api_url, variables)
            .await?
            .block
            .expect("there is a block");
        assert_eq!(block.to_json_value(), expected_block.to_json_value());

        // Existing height.
        let variables = block_query::Variables {
            block_offset: Some(block_query::BlockOffset::Height(expected_block.height)),
        };
        let block = send_query::<BlockQuery>(api_client, api_url, variables)
            .await?
            .block
            .expect("there is a block");
        assert_eq!(block.to_json_value(), expected_block.to_json_value());
    }

    // No offset which yields the last block; as the node proceeds, that is unknown an only its
    // height can be verified to be larger or equal the collected ones.
    let variables = block_query::Variables { block_offset: None };
    let block = send_query::<BlockQuery>(api_client, api_url, variables)
        .await?
        .block
        .expect("there is a block");
    assert!(block.height >= MAX_HEIGHT as i64);

    // Unknown hash.
    let variables = block_query::Variables {
        block_offset: Some(block_query::BlockOffset::Hash([42; 32].hex_encode())),
    };
    let block = send_query::<BlockQuery>(api_client, api_url, variables)
        .await?
        .block;
    assert!(block.is_none());

    // Unknown height.
    let variables = block_query::Variables {
        block_offset: Some(block_query::BlockOffset::Height(u32::MAX as i64)),
    };
    let block = send_query::<BlockQuery>(api_client, api_url, variables)
        .await?
        .block;
    assert!(block.is_none());

    Ok(())
}

/// Test the transactions query, including fee metadata and segment results validation.
async fn test_transactions_query(
    indexer_data: &IndexerData,
    api_client: &Client,
    api_url: &str,
) -> anyhow::Result<()> {
    for expected_transaction in &indexer_data.transactions {
        // Existing hash.
        // Notice that transaction hashes are not unique, e.g. hashes of failed transactions might
        // be also used for later transactions. Hence the query might return more than one
        // transaction and we have to verify that the expected transaction is contained in that
        // collection.
        let variables = transactions_query::Variables {
            transaction_offset: transactions_query::TransactionOffset::Hash(
                expected_transaction.hash.to_owned(),
            ),
        };
        let transactions = send_query::<TransactionsQuery>(api_client, api_url, variables)
            .await?
            .transactions;

        // Verify expected transaction is in results.
        let transaction_values = transactions
            .iter()
            .map(|t| t.to_json_value())
            .collect::<Vec<_>>();
        assert!(transaction_values.contains(&expected_transaction.to_json_value()));

        // Existing identifier for regular transactions.
        if let block_subscription::BlockSubscriptionBlocksTransactionsOn::RegularTransaction(
            BlockSubscriptionBlocksTransactionsOnRegularTransaction { identifiers, .. },
        ) = &expected_transaction.on
        {
            for identifier in identifiers {
                let variables = transactions_query::Variables {
                    transaction_offset: transactions_query::TransactionOffset::Identifier(
                        identifier.to_owned(),
                    ),
                };
                let transactions = send_query::<TransactionsQuery>(api_client, api_url, variables)
                    .await?
                    .transactions;

                // Verify expected transaction is in results.
                let transaction_values = transactions
                    .iter()
                    .map(|t| t.to_json_value())
                    .collect::<Vec<_>>();
                assert!(transaction_values.contains(&expected_transaction.to_json_value()));
            }
        }
    }

    // Unknown hash.
    let variables = transactions_query::Variables {
        transaction_offset: transactions_query::TransactionOffset::Hash([42; 32].hex_encode()),
    };
    let transactions = send_query::<TransactionsQuery>(api_client, api_url, variables)
        .await?
        .transactions;
    assert!(transactions.is_empty());

    // Unknown identifier.
    let variables = transactions_query::Variables {
        transaction_offset: transactions_query::TransactionOffset::Identifier(
            [42; 32].hex_encode(),
        ),
    };
    let transactions = send_query::<TransactionsQuery>(api_client, api_url, variables)
        .await?
        .transactions;
    assert!(transactions.is_empty());

    Ok(())
}

/// Test the contract action query.
async fn test_contract_action_query(
    indexer_data: &IndexerData,
    api_client: &Client,
    api_url: &str,
) -> anyhow::Result<()> {
    for expected_contract_action in &indexer_data.contract_actions {
        // Existing block hash.
        let variables = contract_action_query::Variables {
            address: expected_contract_action.address.to_owned(),
            contract_action_offset: Some(contract_action_query::ContractActionOffset::BlockOffset(
                contract_action_query::BlockOffset::Hash(
                    expected_contract_action.transaction.block.hash.to_owned(),
                ),
            )),
        };
        let contract_action = send_query::<ContractActionQuery>(api_client, api_url, variables)
            .await?
            .contract_action
            .expect("there is a contract action");
        assert_eq!(
            contract_action.to_json_value(),
            expected_contract_action.to_json_value()
        );

        // Existing block height.
        let variables = contract_action_query::Variables {
            address: expected_contract_action.address.to_owned(),
            contract_action_offset: Some(contract_action_query::ContractActionOffset::BlockOffset(
                contract_action_query::BlockOffset::Height(
                    expected_contract_action.transaction.block.height.to_owned(),
                ),
            )),
        };
        let contract_action = send_query::<ContractActionQuery>(api_client, api_url, variables)
            .await?
            .contract_action
            .expect("there is a contract action");
        assert_eq!(
            contract_action.to_json_value(),
            expected_contract_action.to_json_value()
        );

        // Existing transaction hash.
        let variables = contract_action_query::Variables {
            address: expected_contract_action.address.to_owned(),
            contract_action_offset: Some(
                contract_action_query::ContractActionOffset::TransactionOffset(
                    contract_action_query::TransactionOffset::Hash(
                        expected_contract_action.transaction.hash.to_owned(),
                    ),
                ),
            ),
        };
        let contract_action = send_query::<ContractActionQuery>(api_client, api_url, variables)
            .await?
            .contract_action
            .expect("there is a contract action");
        assert_eq!(
            contract_action.to_json_value(),
            expected_contract_action.to_json_value()
        );

        // Existing transaction identifier.
        // The query will not necessarily return the expected contract action, but the most recent
        // one (with the highest ID); hence we can only compare addresses.
        if let block_subscription::BlockSubscriptionBlocksTransactionsContractActionsTransactionOn::RegularTransaction(transaction) = &expected_contract_action.transaction.on {
            for identifier in &transaction.identifiers  {
                let variables = contract_action_query::Variables {
                    address: expected_contract_action.address.to_owned(),
                    contract_action_offset: Some(
                        contract_action_query::ContractActionOffset::TransactionOffset(
                            contract_action_query::TransactionOffset::Identifier(identifier.to_owned()),
                        ),
                    ),
                };
                let contract_action = send_query::<ContractActionQuery>(api_client, api_url, variables)
                    .await?
                    .contract_action
                    .expect("there is a contract action");
                assert_eq!(contract_action.address, expected_contract_action.address);
            }
        }

        // Unknown block hash.
        let variables = contract_action_query::Variables {
            address: expected_contract_action.address.to_owned(),
            contract_action_offset: Some(contract_action_query::ContractActionOffset::BlockOffset(
                contract_action_query::BlockOffset::Hash([42; 32].hex_encode()),
            )),
        };
        let contract_action = send_query::<ContractActionQuery>(api_client, api_url, variables)
            .await?
            .contract_action;
        assert!(contract_action.is_none());

        // Unknown block height.
        let variables = contract_action_query::Variables {
            address: expected_contract_action.address.to_owned(),
            contract_action_offset: Some(contract_action_query::ContractActionOffset::BlockOffset(
                contract_action_query::BlockOffset::Height(MAX_HEIGHT as i64 + 42),
            )),
        };
        let contract_action = send_query::<ContractActionQuery>(api_client, api_url, variables)
            .await?
            .contract_action;
        assert!(contract_action.is_none());

        // Unknown transaction hash.
        let variables = contract_action_query::Variables {
            address: expected_contract_action.address.to_owned(),
            contract_action_offset: Some(
                contract_action_query::ContractActionOffset::TransactionOffset(
                    contract_action_query::TransactionOffset::Hash([42; 32].hex_encode()),
                ),
            ),
        };
        let contract_action = send_query::<ContractActionQuery>(api_client, api_url, variables)
            .await?
            .contract_action;
        assert!(contract_action.is_none());

        // Unknown transaction identifier.
        let variables = contract_action_query::Variables {
            address: expected_contract_action.address.to_owned(),
            contract_action_offset: Some(
                contract_action_query::ContractActionOffset::TransactionOffset(
                    contract_action_query::TransactionOffset::Identifier([42; 32].hex_encode()),
                ),
            ),
        };
        let contract_action = send_query::<ContractActionQuery>(api_client, api_url, variables)
            .await?
            .contract_action;
        assert!(contract_action.is_none());
    }

    Ok(())
}

/// Test the dustGenerationStatus query.
async fn test_dust_generation_status_query(
    api_client: &Client,
    api_url: &str,
) -> anyhow::Result<()> {
    // Test with empty reward addresses list.
    let variables = dust_generation_status_query::Variables {
        cardano_reward_addresses: vec![],
    };
    let response = send_query::<DustGenerationStatusQuery>(api_client, api_url, variables).await?;
    assert!(response.dust_generation_status.is_empty());

    // Test with non-existent reward addresses - should return unregistered status.
    // Using Bech32-encoded testnet reward addresses.
    let variables = dust_generation_status_query::Variables {
        cardano_reward_addresses: vec![
            "stake_test1qyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgcpttsx"
                .try_into()
                .unwrap(),
            "stake_test1qgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqstsw9hr"
                .try_into()
                .unwrap(),
        ],
    };
    let response = send_query::<DustGenerationStatusQuery>(api_client, api_url, variables).await?;
    assert_eq!(response.dust_generation_status.len(), 2);

    for status in &response.dust_generation_status {
        // All test keys should be unregistered.
        assert!(!status.registered);
        assert!(status.dust_address.is_none());
        // Unregistered addresses have zero rates and balances.
        assert_eq!(status.generation_rate, "0");
        assert_eq!(status.current_capacity, "0");
        assert_eq!(status.night_balance, "0");
    }

    Ok(())
}

/// Test the dustGenerations query (resolves #926).
async fn test_dust_generations_query(api_client: &Client, api_url: &str) -> anyhow::Result<()> {
    // Test with empty reward addresses list.
    let variables = dust_generations_query::Variables {
        cardano_reward_addresses: vec![],
    };
    let response = send_query::<DustGenerationsQuery>(api_client, api_url, variables).await?;
    assert!(response.dust_generations.is_empty());

    // Test with non-existent reward addresses — should return empty registrations.
    let variables = dust_generations_query::Variables {
        cardano_reward_addresses: vec![
            "stake_test1qyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgcpttsx"
                .try_into()
                .unwrap(),
        ],
    };
    let response = send_query::<DustGenerationsQuery>(api_client, api_url, variables).await?;
    assert_eq!(response.dust_generations.len(), 1);
    assert!(response.dust_generations[0].registrations.is_empty());

    Ok(())
}

/// Test the dustCommitmentMerkleTreeUpdate query.
async fn test_dust_commitment_merkle_tree_update_query(
    api_client: &Client,
    api_url: &str,
) -> anyhow::Result<()> {
    let variables = dust_commitment_merkle_tree_update_query::Variables {
        start_index: 0,
        end_index: 0,
    };
    let response =
        send_query::<DustCommitmentMerkleTreeUpdateQuery>(api_client, api_url, variables).await?;
    assert!(response.dust_commitment_merkle_tree_update.start_index >= 0);

    Ok(())
}

/// Test the dustGenerationMerkleTreeUpdate query.
async fn test_dust_generation_merkle_tree_update_query(
    api_client: &Client,
    api_url: &str,
) -> anyhow::Result<()> {
    let variables = dust_generation_merkle_tree_update_query::Variables {
        start_index: 0,
        end_index: 0,
    };
    let response =
        send_query::<DustGenerationMerkleTreeUpdateQuery>(api_client, api_url, variables).await?;
    assert!(response.dust_generation_merkle_tree_update.start_index >= 0);

    Ok(())
}

/// Test governance queries (D-Parameter and Terms & Conditions history).
async fn test_governance_queries(api_client: &Client, api_url: &str) -> anyhow::Result<()> {
    use crate::e2e::graphql::{d_parameter_history_query, terms_and_conditions_history_query};

    // Test D-Parameter history.
    let variables = d_parameter_history_query::Variables {};
    let response = send_query::<DParameterHistoryQuery>(api_client, api_url, variables).await?;
    assert!(!response.d_parameter_history.is_empty());
    for entry in &response.d_parameter_history {
        assert!(entry.block_height >= 0);
    }

    // Test Terms and Conditions history.
    let variables = terms_and_conditions_history_query::Variables {};
    let response =
        send_query::<TermsAndConditionsHistoryQuery>(api_client, api_url, variables).await?;
    assert!(!response.terms_and_conditions_history.is_empty());
    for entry in &response.terms_and_conditions_history {
        assert!(entry.block_height >= 0);
        assert!(!entry.url.is_empty());
    }

    Ok(())
}

/// Test the zswapMerkleTreeCollapsedUpdate query.
async fn test_zswap_merkle_tree_collapsed_update_query(
    indexer_data: &IndexerData,
    api_client: &Client,
    api_url: &str,
) -> anyhow::Result<()> {
    let regular_transactions = indexer_data
        .transactions
        .iter()
        .filter_map(|tx| match &tx.on {
            BlockSubscriptionBlocksTransactionsOn::RegularTransaction(t) => Some(t),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        regular_transactions.len() >= 2,
        "there are at least two regular transactions"
    );

    let start_index = regular_transactions.first().unwrap().zswap_end_index;
    let end_index = regular_transactions.last().unwrap().zswap_start_index - 1;
    let variables = zswap_merkle_tree_collapsed_update_query::Variables {
        start_index,
        end_index,
    };

    let update = send_query::<ZswapMerkleTreeCollapsedUpdateQuery>(api_client, api_url, variables)
        .await?
        .zswap_merkle_tree_collapsed_update;
    assert_eq!(update.start_index, start_index);
    assert_eq!(update.end_index, end_index);
    assert!(update.protocol_version > 0);

    // Invalid range (start > end) must return an error.

    let variables = zswap_merkle_tree_collapsed_update_query::Variables {
        start_index: 1,
        end_index: 0,
    };

    let response =
        send_query::<ZswapMerkleTreeCollapsedUpdateQuery>(api_client, api_url, variables).await;
    assert!(response.is_err());

    Ok(())
}

/// Test the contract action subscription.
async fn test_contract_actions_subscription(
    indexer_data: &IndexerData,
    ws_api_url: &str,
) -> anyhow::Result<()> {
    // Map expected contract actions by address.
    let contract_actions_by_address = indexer_data
        .contract_actions
        .iter()
        .map(|c| (c.address.to_owned(), c.to_json_value()))
        .into_group_map();

    for (address, expected_contract_actions) in contract_actions_by_address {
        // No offset.
        let variables = contract_action_subscription::Variables {
            address: address.clone(),
            contract_action_subscription_offset: None,
        };
        let contract_actions =
            graphql_ws_client::subscribe::<ContractActionSubscription>(ws_api_url, variables)
                .await
                .context("subscribe to contract actions")?
                .take(expected_contract_actions.len())
                .map_ok(|data| data.contract_actions.to_json_value())
                .try_collect::<Vec<_>>()
                .await
                .context("collect blocks from contract action subscription")?;
        assert_eq!(contract_actions, expected_contract_actions);

        // Genesis hash.
        let hash = indexer_data
            .blocks
            .first()
            .map(|b| b.hash.to_owned())
            .expect("there is a first block");
        let variables = contract_action_subscription::Variables {
            address: address.clone(),
            contract_action_subscription_offset: Some(
                contract_action_subscription::BlockOffset::Hash(hash),
            ),
        };
        let contract_actions =
            graphql_ws_client::subscribe::<ContractActionSubscription>(ws_api_url, variables)
                .await
                .context("subscribe to contract actions")?
                .take(expected_contract_actions.len())
                .map_ok(|data| data.contract_actions.to_json_value())
                .try_collect::<Vec<_>>()
                .await
                .context("collect blocks from contract action subscription")?;
        assert_eq!(contract_actions, expected_contract_actions);

        // Height zero.
        let variables = contract_action_subscription::Variables {
            address,
            contract_action_subscription_offset: Some(
                contract_action_subscription::BlockOffset::Height(0),
            ),
        };
        let contract_actions =
            graphql_ws_client::subscribe::<ContractActionSubscription>(ws_api_url, variables)
                .await
                .context("subscribe to contract actions")?
                .take(expected_contract_actions.len())
                .map_ok(|data| data.contract_actions.to_json_value())
                .try_collect::<Vec<_>>()
                .await
                .context("collect blocks from contract action subscription")?;
        assert_eq!(contract_actions, expected_contract_actions);
    }

    Ok(())
}

/// Test the shielded transactions subscription.
async fn test_shielded_transactions_subscription(
    ws_api_url: &str,
    session_id: HexEncoded,
) -> anyhow::Result<()> {
    // Collect relevant transactions.
    // We know that our test Node is set up with two relevant transactions for this wallet.
    // Therefore we `take(2)` below, which should happen almost immediately because the respecive
    // wallet has connected way earlier so the Wallet Indexer should have already indexed the
    // transatcions. Just in case we limit the time to some seconds to avoid the test hanging
    // forever. The below assert would then show the failure.
    let variables = shielded_transactions_subscription::Variables { session_id };
    let relevant_transactions =
        graphql_ws_client::subscribe::<ShieldedTransactionsSubscription>(ws_api_url, variables)
            .await
            .context("subscribe to shielded transactions")?
            .map_ok(|data| data.shielded_transactions)
            .try_filter_map(|event| match event {
                ShieldedTransactions::RelevantTransaction(t) => ok(Some(t)),
                ShieldedTransactions::ShieldedTransactionsProgress(_) => ok(None),
            })
            .take(2)
            .take_until(sleep(Duration::from_secs(10)))
            .try_collect::<Vec<_>>()
            .await
            .context("collect relevant transactions from shielded transactions events")?;
    assert_eq!(relevant_transactions.len(), 2);

    // Verify that there are no index gaps.
    let mut expected_start_index = 0;
    for relevant_transaction in relevant_transactions {
        if let Some(zswap_collapsed_update) = relevant_transaction.zswap_collapsed_update {
            assert_eq!(zswap_collapsed_update.start_index, expected_start_index);
            assert!(zswap_collapsed_update.end_index >= zswap_collapsed_update.start_index);

            expected_start_index = zswap_collapsed_update.end_index + 1;
        }

        assert!(relevant_transaction.transaction.zswap_start_index == expected_start_index);
        assert!(
            relevant_transaction.transaction.zswap_end_index
                >= relevant_transaction.transaction.zswap_start_index
        );

        expected_start_index = relevant_transaction.transaction.zswap_end_index;
    }

    Ok(())
}

async fn test_unshielded_transactions_subscription(
    indexer_data: &IndexerData,
    ws_api_url: &str,
) -> anyhow::Result<()> {
    if let Some(unshielded_address) = indexer_data
        .unshielded_utxos
        .first()
        .cloned()
        .map(|a| a.owner)
    {
        let variables = unshielded_transactions_subscription::Variables {
            address: unshielded_address.clone(),
        };
        let unshielded_utxos_updates = graphql_ws_client::subscribe::<
            UnshieldedTransactionsSubscription,
        >(ws_api_url, variables)
        .await
        .context("subscribe to unshielded UTXOs")?
        .take(3)
        .map_ok(|data| data.unshielded_transactions)
        .try_filter_map(|event| match event {
            UnshieldedTransactions::UnshieldedTransaction(t) => ok(Some(t)),
            _ => ok(None),
        })
        .try_collect::<Vec<_>>()
        .await
        .context("collect unshielded UTXO events")?;

        assert!(unshielded_utxos_updates.iter().any(move |update| {
            update
                .created_utxos
                .iter()
                .any(|u| u.owner == unshielded_address)
        }));
    }

    Ok(())
}

async fn test_zswap_ledger_events_subscription(
    indexer_data: &IndexerData,
    ws_api_url: &str,
) -> anyhow::Result<()> {
    let expected_zswap_ledger_events = indexer_data
        .zswap_ledger_events
        .iter()
        .map(|event| event.to_json_value())
        .collect::<Vec<_>>();

    let variables = zswap_ledger_events_subscription::Variables { id: None };
    let zswap_ledger_events =
        graphql_ws_client::subscribe::<ZswapLedgerEventsSubscription>(ws_api_url, variables)
            .await
            .context("subscribe to zswap ledger events")?
            .take(expected_zswap_ledger_events.len())
            .map_ok(|data| data.zswap_ledger_events.to_json_value())
            .try_collect::<Vec<_>>()
            .await
            .context("collect zswap ledger events from subscription")?;

    assert_eq!(zswap_ledger_events, expected_zswap_ledger_events);

    Ok(())
}

async fn test_dust_ledger_events_subscription(
    indexer_data: &IndexerData,
    ws_api_url: &str,
) -> anyhow::Result<()> {
    let expected_dust_ledger_events = indexer_data
        .dust_ledger_events
        .iter()
        .map(|event| event.to_json_value())
        .collect::<Vec<_>>();

    let variables = dust_ledger_events_subscription::Variables { id: None };
    let dust_ledger_events =
        graphql_ws_client::subscribe::<DustLedgerEventsSubscription>(ws_api_url, variables)
            .await
            .context("subscribe to dust ledger events")?
            .map_ok(|data| data.dust_ledger_events.to_json_value());
    let dust_ledger_events =
        tokio_stream::StreamExt::timeout(dust_ledger_events, Duration::from_secs(3))
            .take_while(|timeout_result| ready(timeout_result.is_ok()))
            .filter_map(|timeout_result| ready(timeout_result.map(Some).unwrap_or(None)))
            .try_collect::<Vec<_>>()
            .await
            .context("collect dust ledger events from subscription")?;

    assert_eq!(dust_ledger_events, expected_dust_ledger_events);

    Ok(())
}

/// Test the dustGenerations subscription with a fresh dust address. Uses
/// `start_index > end_index` so the resolver's `cursor > end_index` check
/// fires on first pass (cursor starts at start_index, no entries to advance
/// it), yielding a single final `DustGenerationsProgress` event and closing.
/// Verifies wire-format compatibility for all three union variants and the
/// completion-event shape on the empty case.
async fn test_dust_generations_subscription(
    indexer_data: &IndexerData,
    ws_api_url: &str,
) -> anyhow::Result<()> {
    let network_id: NetworkId = "undeployed".try_into().unwrap();
    let dust_address = DustAddress(encode_address([0u8; 32], AddressType::Dust, &network_id));
    let block_hash = indexer_data
        .blocks
        .last()
        .expect("there is at least one indexed block")
        .hash
        .to_owned();
    let variables = dust_generations_subscription::Variables {
        dust_address,
        block_hash,
        dtime_cutoff_height: 0,
    };
    let events = graphql_ws_client::subscribe::<DustGenerationsSubscription>(ws_api_url, variables)
        .await
        .context("subscribe to dust generations")?
        .map_ok(|data| data.dust_generations.to_json_value())
        .take(1)
        .take_until(sleep(Duration::from_secs(5)))
        .try_collect::<Vec<_>>()
        .await
        .context("collect dust generations events from subscription")?;

    assert_eq!(
        events.len(),
        1,
        "expected exactly one DustGenerationsProgress event"
    );

    let event = &events[0];
    assert_eq!(
        event.get("__typename").and_then(|v| v.as_str()),
        Some("DustGenerationsProgress"),
        "expected DustGenerationsProgress for an address with no owned generations, got: {event:?}"
    );

    Ok(())
}

/// Test the shieldedNullifierTransactions subscription. Verifies the input-validation
/// guards added in #1119 to mirror dust (empty array, empty-string element, and
/// fromBlock > toBlock all error), plus a valid non-matching prefix with a bounded
/// range completes empty. Each error case is wrapped in a defensive timeout so the
/// test fails fast rather than hanging if the server doesn't behave as expected.
async fn test_shielded_nullifier_transactions_subscription(ws_api_url: &str) -> anyhow::Result<()> {
    // Empty nullifierPrefixes array → client error per #1119.
    let variables = shielded_nullifier_transactions_subscription::Variables {
        nullifier_prefixes: vec![],
        from_block: Some(0),
        to_block: Some(0),
    };
    let stream = graphql_ws_client::subscribe::<ShieldedNullifierTransactionsSubscription>(
        ws_api_url, variables,
    )
    .await
    .context("subscribe with empty nullifierPrefixes")?;
    let result = tokio::time::timeout(Duration::from_secs(3), stream.try_collect::<Vec<_>>())
        .await
        .context("expected client error within 3s for empty nullifierPrefixes")?;
    assert!(
        result.is_err(),
        "expected client error for empty nullifierPrefixes, got: {result:?}"
    );

    // Empty-string prefix element → also client error per #1119.
    let variables = shielded_nullifier_transactions_subscription::Variables {
        nullifier_prefixes: vec!["".to_string().try_into().unwrap()],
        from_block: Some(0),
        to_block: Some(0),
    };
    let stream = graphql_ws_client::subscribe::<ShieldedNullifierTransactionsSubscription>(
        ws_api_url, variables,
    )
    .await
    .context("subscribe with empty-string prefix")?;
    let result = tokio::time::timeout(Duration::from_secs(3), stream.try_collect::<Vec<_>>())
        .await
        .context("expected client error within 3s for empty-string prefix")?;
    assert!(
        result.is_err(),
        "expected client error for empty-string prefix, got: {result:?}"
    );

    // Valid non-matching prefix with bounded range → completes empty.
    let variables = shielded_nullifier_transactions_subscription::Variables {
        nullifier_prefixes: vec!["00".to_string().try_into().unwrap()],
        from_block: Some(0),
        to_block: Some(0),
    };
    let events = graphql_ws_client::subscribe::<ShieldedNullifierTransactionsSubscription>(
        ws_api_url, variables,
    )
    .await
    .context("subscribe to shielded nullifier transactions")?;
    let events = tokio_stream::StreamExt::timeout(events, Duration::from_secs(3))
        .take_while(|timeout_result| ready(timeout_result.is_ok()))
        .filter_map(|timeout_result| ready(timeout_result.map(Some).unwrap_or(None)))
        .try_collect::<Vec<_>>()
        .await
        .context("collect shielded nullifier transactions from subscription")?;

    // No matching nullifiers expected in test data.
    assert!(events.is_empty());

    // fromBlock > toBlock → client error per #1119.
    let variables = shielded_nullifier_transactions_subscription::Variables {
        nullifier_prefixes: vec!["00".to_string().try_into().unwrap()],
        from_block: Some(10),
        to_block: Some(5),
    };
    let stream = graphql_ws_client::subscribe::<ShieldedNullifierTransactionsSubscription>(
        ws_api_url, variables,
    )
    .await
    .context("subscribe with fromBlock > toBlock")?;
    let result = tokio::time::timeout(Duration::from_secs(3), stream.try_collect::<Vec<_>>())
        .await
        .context("expected client error within 3s for fromBlock > toBlock")?;
    assert!(
        result.is_err(),
        "expected client error for fromBlock > toBlock, got: {result:?}"
    );

    Ok(())
}

/// Test the dustNullifierTransactions subscription. Verifies the input-validation
/// guards from #1089 (empty array and empty-string element both error) and #1095
/// (fromBlock > toBlock errors), plus a valid non-matching prefix with a bounded
/// range completes empty. Each case is wrapped in a defensive timeout so the test
/// fails fast rather than hanging if the server doesn't behave as expected.
async fn test_dust_nullifier_transactions_subscription(ws_api_url: &str) -> anyhow::Result<()> {
    // Empty nullifierLeBytesPrefixes array → client error per #1089.
    let variables = dust_nullifier_transactions_subscription::Variables {
        nullifier_le_bytes_prefixes: vec![],
        from_block: Some(0),
        to_block: Some(0),
    };
    let stream = graphql_ws_client::subscribe::<DustNullifierTransactionsSubscription>(
        ws_api_url, variables,
    )
    .await
    .context("subscribe with empty nullifierLeBytesPrefixes")?;
    let result = tokio::time::timeout(Duration::from_secs(3), stream.try_collect::<Vec<_>>())
        .await
        .context("expected client error within 3s for empty nullifierLeBytesPrefixes")?;
    assert!(
        result.is_err(),
        "expected client error for empty nullifierLeBytesPrefixes, got: {result:?}"
    );

    // Empty-string prefix element → also client error per #1089.
    let variables = dust_nullifier_transactions_subscription::Variables {
        nullifier_le_bytes_prefixes: vec!["".to_string().try_into().unwrap()],
        from_block: Some(0),
        to_block: Some(0),
    };
    let stream = graphql_ws_client::subscribe::<DustNullifierTransactionsSubscription>(
        ws_api_url, variables,
    )
    .await
    .context("subscribe with empty-string prefix")?;
    let result = tokio::time::timeout(Duration::from_secs(3), stream.try_collect::<Vec<_>>())
        .await
        .context("expected client error within 3s for empty-string prefix")?;
    assert!(
        result.is_err(),
        "expected client error for empty-string prefix, got: {result:?}"
    );

    // Valid non-matching prefix with bounded range → completes empty. Per-event
    // timeout pattern matches `test_shielded_nullifier_transactions_subscription`.
    let variables = dust_nullifier_transactions_subscription::Variables {
        nullifier_le_bytes_prefixes: vec!["00".to_string().try_into().unwrap()],
        from_block: Some(0),
        to_block: Some(0),
    };
    let events = graphql_ws_client::subscribe::<DustNullifierTransactionsSubscription>(
        ws_api_url, variables,
    )
    .await
    .context("subscribe with valid prefix")?;
    let events = tokio_stream::StreamExt::timeout(events, Duration::from_secs(3))
        .take_while(|timeout_result| ready(timeout_result.is_ok()))
        .filter_map(|timeout_result| ready(timeout_result.map(Some).unwrap_or(None)))
        .try_collect::<Vec<_>>()
        .await
        .context("collect dust nullifier transactions")?;
    assert!(events.is_empty());

    // fromBlock > toBlock → client error per #1095.
    let variables = dust_nullifier_transactions_subscription::Variables {
        nullifier_le_bytes_prefixes: vec!["00".to_string().try_into().unwrap()],
        from_block: Some(10),
        to_block: Some(5),
    };
    let stream = graphql_ws_client::subscribe::<DustNullifierTransactionsSubscription>(
        ws_api_url, variables,
    )
    .await
    .context("subscribe with fromBlock > toBlock")?;
    let result = tokio::time::timeout(Duration::from_secs(3), stream.try_collect::<Vec<_>>())
        .await
        .context("expected client error within 3s for fromBlock > toBlock")?;
    assert!(
        result.is_err(),
        "expected client error for fromBlock > toBlock, got: {result:?}"
    );

    Ok(())
}

trait SerializeExt
where
    Self: Serialize,
{
    fn to_json_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("can be JSON-serialized")
    }
}

impl<T> SerializeExt for T where T: Serialize {}

async fn send_query<T>(
    api_client: &Client,
    api_url: &str,
    variables: T::Variables,
) -> anyhow::Result<T::ResponseData>
where
    T: GraphQLQuery,
{
    let query = T::build_query(variables);

    let response = api_client
        .post(api_url)
        .json(&query)
        .send()
        .await
        .context("send query")?
        .error_for_status()
        .context("response for query")?
        .json::<Response<T::ResponseData>>()
        .await
        .context("JSON-decode query response")?;

    if let Some(errors) = response.errors {
        let errors = errors.into_iter().map(|e| e.message).join(", ");
        bail!(errors)
    }

    let data = response
        .data
        .expect("if there are no errors, there must be data");

    Ok(data)
}

fn viewing_key(network_id: &NetworkId) -> &'static str {
    match network_id.as_ref() {
        "undeployed" => {
            "mn_shield-esk_undeployed1dlyj7u8juj68fd4psnkqhjxh32sec0q480vzswg8kd485e2kljcs9ete5h"
        }
        "devnet" => {
            "mn_shield-esk_devnet1dlyj7u8juj68fd4psnkqhjxh32sec0q480vzswg8kd485e2kljcs64mtz0"
        }
        "testnet" => {
            "mn_shield-esk_testnet1dlyj7u8juj68fd4psnkqhjxh32sec0q480vzswg8kd485e2kljcs8fw0p6"
        }
        "mainnet" => "mn_shield-esk1dlyj7u8juj68fd4psnkqhjxh32sec0q480vzswg8kd485e2kljcsucf6ww",
        other => panic!("unexpected network ID {other}"),
    }
}

mod graphql {
    use graphql_client::GraphQLQuery;
    use indexer_api::infra::api::v4::{
        CardanoRewardAddress, HexEncoded, dust::DustAddress, mutation::Unit,
        unshielded::UnshieldedAddress, viewing_key::ViewingKey,
    };

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct BlockQuery;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct TransactionsQuery;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct ContractActionQuery;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct UnshieldedTransactionsSubscription;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct ConnectMutation;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct DisconnectMutation;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct BlockSubscription;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct ContractActionSubscription;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct ShieldedTransactionsSubscription;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct ZswapLedgerEventsSubscription;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct DustLedgerEventsSubscription;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct DustGenerationsSubscription;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct ShieldedNullifierTransactionsSubscription;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct DustNullifierTransactionsSubscription;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct DustGenerationStatusQuery;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct ZswapMerkleTreeCollapsedUpdateQuery;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct DustGenerationsQuery;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct DustCommitmentMerkleTreeUpdateQuery;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct DustGenerationMerkleTreeUpdateQuery;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct DParameterHistoryQuery;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v4.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct TermsAndConditionsHistoryQuery;
}
