use anyhow::Context;
use chain_indexer::{
    domain::node::Node,
    infra::subxt_node::{Config, SubxtNode},
};
use futures::{StreamExt, TryStreamExt};
use indexer_common::domain::PROTOCOL_VERSION_000_016_000;
use std::{pin::pin, time::Duration};

/// Simple test to verify connection to midnight-node and basic block retrieval.
/// Note: This test bypasses the full indexing pipeline and calls the node interface
/// directly via `node.finalized_blocks()`. As a result, it doesn't trigger the
/// genesis UTXO extraction that happens in the zswap transaction processing layer.
///
/// For proper genesis UTXO extraction testing, use the e2e tests which go through
/// the complete indexing pipeline.
///
/// Background:
/// - Genesis blocks don't emit UnshieldedTokens events due to Substrate PR #5463.
/// - Genesis UTXO extraction is integrated into zswap transaction processing.
/// - Full extraction only occurs when blocks are processed through the indexing pipeline.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Note: logging is disabled for this simple test

    let config = Config {
        url: "ws://localhost:9944".to_string(),
        genesis_protocol_version: PROTOCOL_VERSION_000_016_000,
        reconnect_max_delay: Duration::from_secs(1),
        reconnect_max_attempts: 3,
    };
    let mut node = SubxtNode::new(config).await.context("create SubxtNode")?;

    let blocks = node.finalized_blocks(None).take(3);
    let mut blocks = pin!(blocks);

    while let Some(block) = blocks.try_next().await.context("get next block")? {
        println!("## BLOCK: height={}, \thash={}", block.height, block.hash);

        // For genesis block, note that UTXO extraction doesn't happen in this test
        if block.height == 0 {
            println!("*** GENESIS BLOCK DETECTED ***");

            // let utxo_count = block
            //     .transactions
            //     .first()
            //     .map(|t| t.created_unshielded_utxos.len())
            //     .unwrap_or(0);

            // println!("*** UTXOs: {utxo_count} (extraction requires full indexing pipeline) ***");
        }

        // for transaction in &block.transactions {
        //     println!(
        //         "    ## TRANSACTION: hash={}, created_utxos={}, spent_utxos={}",
        //         transaction.hash(),
        //         transaction.created_unshielded_utxos.len(),
        //         transaction.spent_unshielded_utxos.len()
        //     );
        // }
    }

    Ok(())
}
