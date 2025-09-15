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

//! e2e testing library

use crate::{
    e2e::graphql::{
        BlockQuery, BlockSubscription, ConnectMutation, ContractActionQuery,
        ContractActionSubscription, DisconnectMutation, ShieldedTransactionsSubscription,
        TransactionsQuery, UnshieldedTransactionsSubscription, block_query,
        block_subscription::{
            self, BlockSubscriptionBlocks as BlockSubscriptionBlock,
            BlockSubscriptionBlocksTransactions as BlockSubscriptionTransaction,
            BlockSubscriptionBlocksTransactionsContractActions as BlockSubscriptionContractAction,
            BlockSubscriptionBlocksTransactionsUnshieldedCreatedOutputs as BlockSubscriptionUnshieldedUtxo,
            TransactionResultStatus as BlockSubscriptionTransactionResultStatus,
        },
        connect_mutation,
        contract_action_query::{
            self, ContractActionQueryContractAction,
            TransactionResultStatus as ContractActionQueryTransactionResultStatus,
        },
        contract_action_subscription, disconnect_mutation, shielded_transactions_subscription,
        transactions_query, unshielded_transactions_subscription,
    },
    graphql_ws_client,
};
use anyhow::{Context, Ok, bail};
use futures::{StreamExt, TryStreamExt, future::ok};
use graphql_client::{GraphQLQuery, Response};
use indexer_api::infra::api::v1::{
    AsBytesExt, HexEncoded, transaction::TransactionResultStatus, viewing_key::ViewingKey,
};
use indexer_common::domain::{NetworkId, PROTOCOL_VERSION_000_016_000};
use itertools::Itertools;
use reqwest::Client;
use serde::Serialize;
use std::time::{Duration, Instant};

const MAX_HEIGHT: usize = 30;

/// Run comprehensive e2e tests for the Indexer. It is expected that the Indexer is set up with all
/// needed dependencies, e.g. a Node, and its API is exposed securely (https and wss) or insecurely
/// (http and ws) at the given host and port.
///
/// Tests include validation of transaction fee metadata (paid_fee, estimated_fee) and segment
/// results.
pub async fn run(network_id: NetworkId, host: &str, port: u16, secure: bool) -> anyhow::Result<()> {
    println!("Starting e2e testing");

    let (api_url, ws_api_url) = {
        let core = format!("{host}:{port}/api/v1/graphql");

        if secure {
            (format!("https://{core}"), format!("wss://{core}/ws"))
        } else {
            (format!("http://{core}"), format!("ws://{core}/ws"))
        }
    };

    let api_client = Client::new();

    // Collect Indexer data using the block subscription.
    let indexer_data = IndexerData::collect(&ws_api_url, network_id)
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

    // Test mutations.
    test_connect_mutation(&api_client, &api_url, network_id)
        .await
        .context("test connect mutation query")?;
    test_disconnect_mutation(&api_client, &api_url)
        .await
        .context("test disconnect mutation query")?;

    // Test subscriptions (the block subscription has already been tested above).
    test_contract_actions_subscription(&indexer_data, &ws_api_url)
        .await
        .context("test contract action subscription")?;
    test_shielded_transactions_subscription(&ws_api_url, network_id)
        .await
        .context("test shielded transactions subscription")?;
    test_unshielded_transactions_subscription(&indexer_data, &ws_api_url)
        .await
        .context("test unshielded transactions subscription")?;

    println!("Successfully finished e2e testing");

    Ok(())
}

/// All data needed for testing collected from the Indexer via the blocks subscription. To be used
/// as expected data in tests for all other API operations.
struct IndexerData {
    blocks: Vec<BlockSubscriptionBlock>,
    transactions: Vec<BlockSubscriptionTransaction>,
    contract_actions: Vec<BlockSubscriptionContractAction>,
    unshielded_utxos: Vec<BlockSubscriptionUnshieldedUtxo>,
}

impl IndexerData {
    /// Not only collects the Indexer data needed for testing, but also validates it, e.g. that
    /// block heights start at zero and increment by one.
    async fn collect(ws_api_url: &str, network_id: NetworkId) -> anyhow::Result<Self> {
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

        // Verify that each block correctly references its parent and the height is increased by
        // one.
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

        // Verify that all transactions reference the correct block and have the same protocol
        // version.
        assert!(blocks.iter().all(|block| {
            block.transactions.iter().all(|transaction| {
                transaction.block.hash == block.hash
                    && transaction.protocol_version == block.protocol_version
            })
        }));

        // Verify that all transactions have fee information
        assert!(blocks.iter().all(|block| {
            block.transactions.iter().all(|transaction| {
                // Fees should always be present and non-empty strings
                !transaction.fees.paid_fees.is_empty()
                    && !transaction.fees.estimated_fees.is_empty()
            })
        }));

        // Verify that segment results are consistent with transaction results
        assert!(blocks.iter().all(|block| {
            block.transactions.iter().all(|transaction| {
                match transaction.transaction_result.status {
                    BlockSubscriptionTransactionResultStatus::SUCCESS => {
                        transaction.transaction_result.segments.is_none()
                    }

                    BlockSubscriptionTransactionResultStatus::FAILURE => {
                        // Failure transactions should have no segment results
                        transaction.transaction_result.segments.is_none()
                    }

                    BlockSubscriptionTransactionResultStatus::PARTIAL_SUCCESS => {
                        transaction.transaction_result.segments.is_some()
                    }

                    _ => true,
                }
            })
        }));

        // Collect transactions.
        let transactions = blocks
            .iter()
            .flat_map(|block| block.transactions.to_owned())
            .collect::<Vec<_>>();

        // Verify that all transactions have valid fee information
        assert!(
            transactions.iter().all(|transaction| {
                // Fees should be parseable as numbers
                transaction.fees.paid_fees.parse::<u64>().is_ok()
                    && transaction.fees.estimated_fees.parse::<u64>().is_ok()
            }),
            "All transactions should have valid numeric fee values"
        );

        // Verify that all contract actions reference the correct transaction.
        assert!(transactions.iter().all(|transaction| {
            transaction
                .contract_actions
                .iter()
                .all(|contract_action| contract_action.transaction_hash() == transaction.hash)
        }));

        // Collect contract actions.
        let contract_actions = transactions
            .iter()
            .flat_map(|transaction| transaction.contract_actions.to_owned())
            .collect::<Vec<_>>();

        // Verify that contract calls and their deploy have the same address.
        contract_actions
            .iter()
            .filter_map(|contract_action| match contract_action {
                BlockSubscriptionContractAction::ContractCall(c) => {
                    Some((&c.address, &c.deploy.address))
                }
                _ => None,
            })
            .all(|(a1, a2)| a1 == a2);

        // Collect unshielded UTXOs.
        let unshielded_utxos = transactions
            .iter()
            .flat_map(|transaction| transaction.unshielded_created_outputs.to_owned())
            .collect::<Vec<_>>();

        // Test genesis UTXOs for non-MainNet networks.
        // MainNet has no pre-funded accounts (clean genesis), while test/dev networks
        // contain pre-funded UTXOs for testing purposes. This validation ensures the
        // genesis UTXO extraction workaround works correctly on networks where it's needed.
        if network_id != NetworkId::MainNet {
            let genesis_block = blocks
                .iter()
                .find(|block| block.height == 0)
                .context("genesis block not found")?;

            // Genesis block should have at least one transaction.
            assert!(!genesis_block.transactions.is_empty());

            // let genesis_transaction = &genesis_block.transactions[0];

            // // Genesis transaction should have created unshielded UTXOs.
            // assert!(!genesis_transaction.unshielded_created_outputs.is_empty());

            // // Verify genesis UTXOs have expected properties.
            // for utxo in &genesis_transaction.unshielded_created_outputs {
            //     // Genesis UTXOs should have positive values.
            //     assert!(utxo.value.parse::<u128>().unwrap_or(0) > 0);

            //     // Genesis UTXOs should have valid owner addresses (non-empty string).
            //     assert!(!utxo.owner.0.is_empty());

            //     // Genesis UTXOs should have valid token types.
            //     // Token type validation: attempt to decode as 32-byte array.
            //     // For native tokens, this is typically all zeros.
            //     assert!(utxo.token_type.hex_decode::<RawTokenType>().is_ok());
            // }
        }

        Ok(Self {
            blocks,
            transactions,
            contract_actions,
            unshielded_utxos,
        })
    }
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

        // Verify expected transaction is in results
        let transaction_values = transactions
            .iter()
            .map(|t| t.to_json_value())
            .collect::<Vec<_>>();
        assert!(transaction_values.contains(&expected_transaction.to_json_value()));

        // Validate fee metadata and segment results for all returned transactions
        for transaction in &transactions {
            // Verify fee information is present and valid
            assert!(
                !transaction.fees.paid_fees.is_empty(),
                "paid_fee should not be empty"
            );
            assert!(
                !transaction.fees.estimated_fees.is_empty(),
                "estimated_fee should not be empty"
            );

            // Verify fees are valid numeric strings (DUST amounts)
            let paid_fee: u64 = transaction
                .fees
                .paid_fees
                .parse()
                .expect("paid_fee should be a valid number");
            let estimated_fee: u64 = transaction
                .fees
                .estimated_fees
                .parse()
                .expect("estimated_fee should be a valid number");

            // Fees should be reasonable values (not impossibly large)
            assert!(
                paid_fee <= 1_000_000_000_000,
                "paid_fee should be reasonable"
            );
            assert!(
                estimated_fee <= 1_000_000_000_000,
                "estimated_fee should be reasonable"
            );

            // Verify segment results structure matches transaction status
            match transaction.transaction_result.status {
                transactions_query::TransactionResultStatus::SUCCESS => {
                    assert!(
                        transaction.transaction_result.segments.is_none(),
                        "SUCCESS transactions should have no segment results"
                    );
                }

                transactions_query::TransactionResultStatus::FAILURE => {
                    assert!(
                        transaction.transaction_result.segments.is_none(),
                        "FAILURE transactions should have no segment results"
                    );
                }

                transactions_query::TransactionResultStatus::PARTIAL_SUCCESS => {
                    assert!(
                        transaction.transaction_result.segments.is_some(),
                        "PARTIAL_SUCCESS transactions should have segment results"
                    );
                }

                _ => {}
            }
        }

        // Existing identifier.
        for identifier in &expected_transaction.identifiers {
            let variables = transactions_query::Variables {
                transaction_offset: transactions_query::TransactionOffset::Identifier(
                    identifier.to_owned(),
                ),
            };
            let transactions = send_query::<TransactionsQuery>(api_client, api_url, variables)
                .await?
                .transactions;

            // Verify expected transaction is in results
            let transaction_values = transactions
                .iter()
                .map(|t| t.to_json_value())
                .collect::<Vec<_>>();
            assert!(transaction_values.contains(&expected_transaction.to_json_value()));

            // Also validate fee metadata for identifier queries
            for transaction in &transactions {
                assert!(!transaction.fees.paid_fees.is_empty());
                assert!(!transaction.fees.estimated_fees.is_empty());
                assert!(transaction.fees.paid_fees.parse::<u64>().is_ok());
                assert!(transaction.fees.estimated_fees.parse::<u64>().is_ok());
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
    for expected_contract_action in indexer_data
        .contract_actions
        .iter()
        .filter(|c| c.transaction_transaction_result_status() != TransactionResultStatus::Failure)
    {
        // Existing block hash.
        let variables = contract_action_query::Variables {
            address: expected_contract_action.address(),
            contract_action_offset: Some(contract_action_query::ContractActionOffset::BlockOffset(
                contract_action_query::BlockOffset::Hash(expected_contract_action.block_hash()),
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
            address: expected_contract_action.address(),
            contract_action_offset: Some(contract_action_query::ContractActionOffset::BlockOffset(
                contract_action_query::BlockOffset::Height(expected_contract_action.block_height()),
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
            address: expected_contract_action.address(),
            contract_action_offset: Some(
                contract_action_query::ContractActionOffset::TransactionOffset(
                    contract_action_query::TransactionOffset::Hash(
                        expected_contract_action.transaction_hash(),
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
        for identifier in expected_contract_action.identifiers() {
            let variables = contract_action_query::Variables {
                address: expected_contract_action.address(),
                contract_action_offset: Some(
                    contract_action_query::ContractActionOffset::TransactionOffset(
                        contract_action_query::TransactionOffset::Identifier(identifier),
                    ),
                ),
            };
            let contract_action = send_query::<ContractActionQuery>(api_client, api_url, variables)
                .await?
                .contract_action
                .expect("there is a contract action");
            assert_eq!(
                contract_action.address(),
                expected_contract_action.address()
            );
        }

        // Unknown block hash.
        let variables = contract_action_query::Variables {
            address: expected_contract_action.address(),
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
            address: expected_contract_action.address(),
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
            address: expected_contract_action.address(),
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
            address: expected_contract_action.address(),
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

/// Test the connect mutation.
async fn test_connect_mutation(
    api_client: &Client,
    api_url: &str,
    network_id: NetworkId,
) -> anyhow::Result<()> {
    // Valid viewing key.
    let viewing_key = ViewingKey::from(viewing_key(network_id));
    let variables = connect_mutation::Variables { viewing_key };
    let response = send_query::<ConnectMutation>(api_client, api_url, variables).await;
    assert!(response.is_ok());

    // Invalid viewing key.
    let variables = connect_mutation::Variables {
        viewing_key: ViewingKey("invalid".to_string()),
    };
    let response = send_query::<ConnectMutation>(api_client, api_url, variables).await;
    assert!(response.is_err());

    Ok(())
}

/// Test the disconnect mutation.
async fn test_disconnect_mutation(api_client: &Client, api_url: &str) -> anyhow::Result<()> {
    // Valid session ID.
    let session_id = indexer_common::domain::ViewingKey::from([0; 32])
        .to_session_id()
        .hex_encode();
    let variables = disconnect_mutation::Variables { session_id };
    let response = send_query::<DisconnectMutation>(api_client, api_url, variables).await;
    assert!(response.is_ok());

    // Invalid viewing key.
    let variables = disconnect_mutation::Variables {
        session_id: [42; 1].hex_encode(),
    };
    let response = send_query::<DisconnectMutation>(api_client, api_url, variables).await;
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
        .map(|c| (c.address(), c.to_json_value()))
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

async fn test_unshielded_transactions_subscription(
    indexer_data: &IndexerData,
    ws_api_url: &str,
) -> anyhow::Result<()> {
    use unshielded_transactions_subscription::UnshieldedTransactionsSubscriptionUnshieldedTransactions as UnshieldedTransactions;

    let unshielded_address = indexer_data
        .unshielded_utxos
        .first()
        .cloned()
        .unwrap()
        .owner;

    let variables = unshielded_transactions_subscription::Variables {
        address: unshielded_address.clone(),
    };
    let unshielded_utxos_updates =
        graphql_ws_client::subscribe::<UnshieldedTransactionsSubscription>(ws_api_url, variables)
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

    Ok(())
}

/// Test the wallet subscription.
async fn test_shielded_transactions_subscription(
    ws_api_url: &str,
    network_id: NetworkId,
) -> anyhow::Result<()> {
    use shielded_transactions_subscription::ShieldedTransactionsSubscriptionShieldedTransactions as ShieldedTransactions;

    let session_id = ViewingKey::from(viewing_key(network_id))
        .try_into_domain(network_id, PROTOCOL_VERSION_000_016_000)?
        .to_session_id()
        .hex_encode();

    // Collect shielded transactions events until there are no more relevant transactions (3s
    // deadline).
    let mut relevant_transaction_timestamp = Instant::now();
    let variables = shielded_transactions_subscription::Variables { session_id };
    let events =
        graphql_ws_client::subscribe::<ShieldedTransactionsSubscription>(ws_api_url, variables)
            .await
            .context("subscribe to shielded transactions")?
            .map_ok(|data| data.shielded_transactions)
            .try_take_while(|event| {
                let duration = Instant::now() - relevant_transaction_timestamp;

                if let ShieldedTransactions::RelevantTransaction(_) = event {
                    relevant_transaction_timestamp = Instant::now()
                }

                ok(duration < Duration::from_secs(3))
            })
            .try_collect::<Vec<_>>()
            .await
            .context("collect shielded transactions events")?;

    // Filter relevant transactions only.
    let relevant_transactions = events.into_iter().filter_map(|event| match event {
        ShieldedTransactions::RelevantTransaction(relevant_transaction) => {
            Some(relevant_transaction)
        }
        _ => None,
    });

    // Verify that there are no index gaps.
    let mut expected_start_index = 0;
    for relevant_transaction in relevant_transactions {
        if let Some(collapsed_merkle_tree) = relevant_transaction.collapsed_merkle_tree {
            assert_eq!(collapsed_merkle_tree.start_index, expected_start_index);
            assert!(collapsed_merkle_tree.end_index >= collapsed_merkle_tree.start_index);

            expected_start_index = collapsed_merkle_tree.end_index + 1;
        }

        assert!(relevant_transaction.transaction.start_index == expected_start_index);
        assert!(
            relevant_transaction.transaction.end_index
                >= relevant_transaction.transaction.start_index
        );

        expected_start_index = relevant_transaction.transaction.end_index;
    }

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

trait ContractActionExt {
    fn address(&self) -> HexEncoded;
    fn block_hash(&self) -> HexEncoded;
    fn block_height(&self) -> i64;
    fn transaction_hash(&self) -> HexEncoded;
    fn transaction_transaction_result_status(&self) -> TransactionResultStatus;
    fn identifiers(&self) -> Vec<HexEncoded>;
}

impl ContractActionExt for BlockSubscriptionContractAction {
    fn address(&self) -> HexEncoded {
        let address = match self {
            BlockSubscriptionContractAction::ContractDeploy(c) => &c.address,
            BlockSubscriptionContractAction::ContractCall(c) => &c.address,
            BlockSubscriptionContractAction::ContractUpdate(c) => &c.address,
        };

        address.to_owned()
    }

    fn block_hash(&self) -> HexEncoded {
        let block_hash = match self {
            BlockSubscriptionContractAction::ContractDeploy(c) => &c.transaction.block.hash,
            BlockSubscriptionContractAction::ContractCall(c) => &c.transaction.block.hash,
            BlockSubscriptionContractAction::ContractUpdate(c) => &c.transaction.block.hash,
        };

        block_hash.to_owned()
    }

    fn block_height(&self) -> i64 {
        match self {
            BlockSubscriptionContractAction::ContractDeploy(c) => c.transaction.block.height,
            BlockSubscriptionContractAction::ContractCall(c) => c.transaction.block.height,
            BlockSubscriptionContractAction::ContractUpdate(c) => c.transaction.block.height,
        }
    }

    fn transaction_hash(&self) -> HexEncoded {
        let transaction_hash = match self {
            BlockSubscriptionContractAction::ContractDeploy(c) => &c.transaction.hash,
            BlockSubscriptionContractAction::ContractCall(c) => &c.transaction.hash,
            BlockSubscriptionContractAction::ContractUpdate(c) => &c.transaction.hash,
        };

        transaction_hash.to_owned()
    }

    fn transaction_transaction_result_status(&self) -> TransactionResultStatus {
        let status = match self {
            BlockSubscriptionContractAction::ContractCall(c) => {
                &c.transaction.transaction_result.status
            }
            BlockSubscriptionContractAction::ContractDeploy(c) => {
                &c.transaction.transaction_result.status
            }
            BlockSubscriptionContractAction::ContractUpdate(c) => {
                &c.transaction.transaction_result.status
            }
        };

        match status {
            BlockSubscriptionTransactionResultStatus::SUCCESS => TransactionResultStatus::Success,
            BlockSubscriptionTransactionResultStatus::PARTIAL_SUCCESS => {
                TransactionResultStatus::PartialSuccess
            }
            BlockSubscriptionTransactionResultStatus::FAILURE => TransactionResultStatus::Failure,
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    fn identifiers(&self) -> Vec<HexEncoded> {
        let identifiers = match self {
            BlockSubscriptionContractAction::ContractDeploy(c) => &c.transaction.identifiers,
            BlockSubscriptionContractAction::ContractCall(c) => &c.transaction.identifiers,
            BlockSubscriptionContractAction::ContractUpdate(c) => &c.transaction.identifiers,
        };

        identifiers.to_owned()
    }
}

impl ContractActionExt for ContractActionQueryContractAction {
    fn address(&self) -> HexEncoded {
        let address = match self {
            ContractActionQueryContractAction::ContractDeploy(c) => &c.address,
            ContractActionQueryContractAction::ContractCall(c) => &c.address,
            ContractActionQueryContractAction::ContractUpdate(c) => &c.address,
        };

        address.to_owned()
    }

    fn block_hash(&self) -> HexEncoded {
        let block_hash = match self {
            ContractActionQueryContractAction::ContractDeploy(c) => &c.transaction.block.hash,
            ContractActionQueryContractAction::ContractCall(c) => &c.transaction.block.hash,
            ContractActionQueryContractAction::ContractUpdate(c) => &c.transaction.block.hash,
        };

        block_hash.to_owned()
    }

    fn block_height(&self) -> i64 {
        match self {
            ContractActionQueryContractAction::ContractDeploy(c) => c.transaction.block.height,
            ContractActionQueryContractAction::ContractCall(c) => c.transaction.block.height,
            ContractActionQueryContractAction::ContractUpdate(c) => c.transaction.block.height,
        }
    }

    fn transaction_hash(&self) -> HexEncoded {
        let transaction_hash = match self {
            ContractActionQueryContractAction::ContractDeploy(c) => &c.transaction.hash,
            ContractActionQueryContractAction::ContractCall(c) => &c.transaction.hash,
            ContractActionQueryContractAction::ContractUpdate(c) => &c.transaction.hash,
        };

        transaction_hash.to_owned()
    }

    fn transaction_transaction_result_status(&self) -> TransactionResultStatus {
        let status = match self {
            ContractActionQueryContractAction::ContractCall(c) => {
                &c.transaction.transaction_result.status
            }
            ContractActionQueryContractAction::ContractDeploy(c) => {
                &c.transaction.transaction_result.status
            }
            ContractActionQueryContractAction::ContractUpdate(c) => {
                &c.transaction.transaction_result.status
            }
        };

        match status {
            ContractActionQueryTransactionResultStatus::SUCCESS => TransactionResultStatus::Success,
            ContractActionQueryTransactionResultStatus::PARTIAL_SUCCESS => {
                TransactionResultStatus::PartialSuccess
            }
            ContractActionQueryTransactionResultStatus::FAILURE => TransactionResultStatus::Failure,
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    fn identifiers(&self) -> Vec<HexEncoded> {
        let identifiers = match self {
            ContractActionQueryContractAction::ContractDeploy(c) => &c.transaction.identifiers,
            ContractActionQueryContractAction::ContractCall(c) => &c.transaction.identifiers,
            ContractActionQueryContractAction::ContractUpdate(c) => &c.transaction.identifiers,
        };

        identifiers.to_owned()
    }
}

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

fn viewing_key(network_id: NetworkId) -> &'static str {
    match network_id {
        NetworkId::Undeployed => {
            "mn_shield-esk_undeployed1d45kgmnfva58gwn9de3hy7tsw35k7m3dwdjkxun9wskkketetdmrzhf6dlyj7u8juj68fd4psnkqhjxh32sec0q480vzswg8kd485e2kljcsmxqc0u"
        }
        NetworkId::DevNet => {
            "mn_shield-esk_dev1d45kgmnfva58gwn9de3hy7tsw35k7m3dwdjkxun9wskkketetdmrzhf6dlyj7u8juj68fd4psnkqhjxh32sec0q480vzswg8kd485e2kljcs6vq5mk"
        }
        NetworkId::TestNet => {
            "mn_shield-esk_test1d45kgmnfva58gwn9de3hy7tsw35k7m3dwdjkxun9wskkketetdmrzhf6dlyj7u8juj68fd4psnkqhjxh32sec0q480vzswg8kd485e2kljcsqwtxq9"
        }
        NetworkId::MainNet => {
            "mn_shield-esk1d45kgmnfva58gwn9de3hy7tsw35k7m3dwdjkxun9wskkketetdmrzhf6dlyj7u8juj68fd4psnkqhjxh32sec0q480vzswg8kd485e2kljcsn6y6ls"
        }
    }
}

mod graphql {
    use graphql_client::GraphQLQuery;
    use indexer_api::infra::api::v1::{
        HexEncoded, mutation::Unit, unshielded::UnshieldedAddress, viewing_key::ViewingKey,
    };

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v1.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct BlockQuery;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v1.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct TransactionsQuery;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v1.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct ContractActionQuery;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v1.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct UnshieldedTransactionsSubscription;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v1.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct ConnectMutation;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v1.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct DisconnectMutation;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v1.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct BlockSubscription;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v1.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct ContractActionSubscription;

    #[derive(GraphQLQuery)]
    #[graphql(
        schema_path = "../indexer-api/graphql/schema-v1.graphql",
        query_path = "./e2e.graphql",
        response_derives = "Debug, Clone, Serialize"
    )]
    pub struct ShieldedTransactionsSubscription;
}
