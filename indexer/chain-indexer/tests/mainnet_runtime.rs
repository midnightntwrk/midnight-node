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

//! Blocks produced by the node 1.0 runtime — the runtime mainnet upgraded to at block
//! 1_774_492 (protocol version 1_000_000) — must be ingestible. Indexer versions without
//! node 1.0 support fail here, either with `ProtocolVersionError::Unsupported(1_000_000)`
//! or with subxt metadata validation errors.

#![cfg(any(feature = "cloud", feature = "standalone"))]

use anyhow::Context;
use chain_indexer::{
    domain::{
        BlockRef,
        node::{Node, Transaction},
    },
    infra::subxt_node::{Config, SubxtNode},
};
use fs_extra::dir::{CopyOptions, copy};
use futures::TryStreamExt;
use std::{fs, path::Path, pin::pin, time::Duration};
use testcontainers::{
    GenericImage, ImageExt,
    core::{Mount, WaitFor},
    runners::AsyncRunner,
};
use walkdir::WalkDir;

/// The node version running the same runtime (identical metadata) as mainnet after the
/// upgrade at block 1_774_492.
const NODE_VERSION: &str = "1.0.0";

#[tokio::test(flavor = "multi_thread")]
async fn test_finalized_blocks_node_1_0() -> anyhow::Result<()> {
    let _ledger_db = init_ledger_db().await?;

    let node_dir = Path::new(&format!("{}/../.node", env!("CARGO_MANIFEST_DIR")))
        .join(NODE_VERSION)
        .canonicalize()
        .context("create path to node directory")?;
    let temp_dir = tempfile::tempdir().context("create tempdir")?;
    copy(&node_dir, &temp_dir, &CopyOptions::default())
        .context("copy .node directory into tempdir")?;

    // The node container runs as non-root user (appuser), so the bind-mounted directory
    // must be writable by all users.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let chain_dir = temp_dir.path().join(NODE_VERSION).join("chain");
        if chain_dir.exists() {
            for entry in WalkDir::new(&chain_dir) {
                let entry = entry.context("walk chain directory")?;
                let path = entry.path();
                let mode = if path.is_dir() { 0o777 } else { 0o666 };
                fs::set_permissions(path, fs::Permissions::from_mode(mode))
                    .with_context(|| format!("set permissions on {}", path.display()))?;
            }
        }
    }

    let node_path = temp_dir.path().join(NODE_VERSION).display().to_string();
    let node_container = GenericImage::new("midnightntwrk/midnight-node", NODE_VERSION)
        .with_wait_for(WaitFor::message_on_stderr("9944"))
        .with_mount(Mount::bind_mount(node_path, "/node"))
        .with_env_var("SHOW_CONFIG", "false")
        .with_env_var("CFG_PRESET", "dev")
        .start()
        .await
        .context("start node container")?;
    let node_port = node_container
        .get_host_port_ipv4(9944)
        .await
        .context("get node port")?;

    let config = Config {
        url: format!("ws://localhost:{node_port}"),
        reconnect_max_delay: Duration::from_secs(1),
        reconnect_max_attempts: 1,
        subscription_recovery_timeout: Duration::from_secs(30),
    };
    let mut node = SubxtNode::new(config).await.context("create SubxtNode")?;

    let blocks = node.finalized_blocks(None);
    let mut blocks = pin!(blocks);
    for _ in 0..3 {
        let block = blocks
            .try_next()
            .await
            .context("get next finalized block")?
            .context("stream of finalized blocks must not end")?;
        assert_eq!(u32::from(block.protocol_version), 1_000_000);
    }

    Ok(())
}

/// Ingest the exact mainnet blocks at the runtime upgrade boundary via the public mainnet
/// RPC. This covers both v4.0.x outage failure modes on the very blocks where they occurred:
/// block 1_774_491 contains a contract call, so ingesting it exercises the node 0.22
/// `get_contract_state` runtime API against a chain that already runs the 1.0 runtime
/// ("The static Runtime API address used is not compatible with the live chain" on v4.0.x);
/// block 1_774_492 is the first block built by the 1.0 runtime ("unsupported protocol
/// version 1000000" on v4.0.x).
///
/// Requires network access, hence ignored by default; run explicitly:
/// `cargo nextest run -p chain-indexer --features cloud --run-ignored all -E
/// 'test(test_mainnet_runtime_upgrade_boundary)'`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires network access to the public mainnet RPC"]
async fn test_mainnet_runtime_upgrade_boundary() -> anyhow::Result<()> {
    const CONTRACT_ADDRESS: &str =
        "9ef16e583fbc361ba6016b2751e6f26a5ab2bbf2f7102ea5e28dc8810696eb9c";

    let _ledger_db = init_ledger_db().await?;

    let config = Config {
        url: "wss://rpc.mainnet.midnight.network".to_string(),
        reconnect_max_delay: Duration::from_secs(1),
        reconnect_max_attempts: 3,
        subscription_recovery_timeout: Duration::from_secs(30),
    };
    let mut node = SubxtNode::new(config).await.context("create SubxtNode")?;

    let after = BlockRef {
        hash: const_hex::decode_to_array::<_, 32>(
            "e23dc07f65b1194d134b6d9b3c2f7433329d0512896a1c4543048a166d4fabd9",
        )
        .expect("valid block hash")
        .into(),
        height: 1_774_490,
    };
    let blocks = node.finalized_blocks(Some(after));
    let mut blocks = pin!(blocks);

    let block = blocks
        .try_next()
        .await
        .context("get mainnet block 1_774_491")?
        .context("stream of finalized blocks must not end")?;
    assert_eq!(block.height, 1_774_491);
    assert_eq!(u32::from(block.protocol_version), 22_000);
    let contract_action = block
        .transactions
        .iter()
        .filter_map(|transaction| match transaction {
            Transaction::Regular(transaction) => Some(&transaction.contract_actions),
            Transaction::System(_) => None,
        })
        .flatten()
        .find(|contract_action| {
            contract_action.address.as_ref()
                == const_hex::decode(CONTRACT_ADDRESS).expect("valid address")
        })
        .context("mainnet block 1_774_491 must contain the known contract call")?;
    assert!(!contract_action.state.as_ref().is_empty());

    let block = blocks
        .try_next()
        .await
        .context("get mainnet block 1_774_492")?
        .context("stream of finalized blocks must not end")?;
    assert_eq!(block.height, 1_774_492);
    assert_eq!(u32::from(block.protocol_version), 1_000_000);

    Ok(())
}

#[cfg(feature = "cloud")]
async fn init_ledger_db()
-> anyhow::Result<testcontainers::ContainerAsync<testcontainers_modules::postgres::Postgres>> {
    use indexer_common::infra::{
        ledger_db, migrations,
        pool::postgres::{Config, PostgresPool},
    };
    use sqlx::postgres::PgSslMode;
    use testcontainers_modules::postgres::Postgres;

    let postgres_container = Postgres::default()
        .with_db_name("indexer")
        .with_user("indexer")
        .with_password("postgres")
        .with_tag("17.1-alpine")
        .start()
        .await
        .context("start Postgres container")?;
    let postgres_port = postgres_container
        .get_host_port_ipv4(5432)
        .await
        .context("get Postgres port")?;

    let config = Config {
        host: "localhost".to_string(),
        port: postgres_port,
        dbname: "indexer".to_string(),
        user: "indexer".to_string(),
        password: "postgres".to_string().into(),
        sslmode: PgSslMode::Prefer,
        max_connections: 10,
        idle_timeout: Duration::from_secs(60),
        max_lifetime: Duration::from_secs(5 * 60),
    };
    let pool = PostgresPool::new(config)
        .await
        .context("create PostgresPool")?;
    migrations::postgres::run(&pool)
        .await
        .context("run Postgres migrations")?;

    ledger_db::init(
        ledger_db::Config {
            cache_max_nodes: 1_024,
        },
        pool,
    );

    Ok(postgres_container)
}

#[cfg(feature = "standalone")]
async fn init_ledger_db() -> anyhow::Result<tempfile::TempDir> {
    use indexer_common::infra::ledger_db;

    let temp_dir = tempfile::tempdir().context("create tempdir")?;
    let cnn_url = temp_dir
        .path()
        .join("ledger-db.sqlite")
        .display()
        .to_string();

    ledger_db::init(ledger_db::Config {
        cache_max_nodes: 1_024,
        cnn_url,
    })
    .await
    .context("init ledger db")?;

    Ok(temp_dir)
}
