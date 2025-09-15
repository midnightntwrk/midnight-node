use anyhow::Context;
use chain_indexer::{
    domain::node::{self, Node},
    infra::subxt_node::{Config, SubxtNode},
};
use clap::Parser;
use futures::{Stream, StreamExt, TryStreamExt};
use indexer_common::domain::PROTOCOL_VERSION_000_016_000;
use std::{pin::Pin, time::Duration};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Cli::parse().run().await
}

/// This program connects to a node and prints blocks and their transactions.
#[derive(Debug, Parser)]
#[command()]
struct Cli {
    /// The node URL; defaults to "ws://localhost:9944".
    #[arg(long, default_value = "ws://localhost:9944")]
    node: String,

    /// How many blocks to skip; none if omitted.
    #[arg(long)]
    skip: Option<usize>,

    /// How many blocks to take; unlimited if omitted.
    #[arg(long)]
    take: Option<usize>,
}

impl Cli {
    async fn run(self) -> anyhow::Result<()> {
        let config = Config {
            url: self.node,
            genesis_protocol_version: PROTOCOL_VERSION_000_016_000,
            reconnect_max_delay: Duration::from_secs(1),
            reconnect_max_attempts: 1,
        };
        let mut node = SubxtNode::new(config).await.context("create SubxtNode")?;

        let blocks = node.finalized_blocks(None);
        let mut blocks: Pin<Box<dyn Stream<Item = Result<node::Block, _>> + Send>> =
            Box::pin(blocks);

        if let Some(n) = self.skip {
            blocks = Box::pin(blocks.skip(n));
        }

        if let Some(n) = self.take {
            blocks = Box::pin(blocks.take(n));
        }

        while let Some(block) = blocks.try_next().await.context("get next block")? {
            println!("## BLOCK: height={}, \thash={}", block.height, block.hash);
            for transaction in block.transactions {
                match transaction {
                    node::Transaction::Regular(transaction) => {
                        println!(
                            "    ## REGULAR TRANSACTION: hash={}, \t{transaction:?}",
                            transaction.hash
                        );
                    }

                    node::Transaction::System(transaction) => {
                        println!(
                            "    ## SYSTEN TRANSACTION: hash={}, \t{transaction:?}",
                            transaction.hash
                        );
                    }
                }
            }
        }
        Ok(())
    }
}
