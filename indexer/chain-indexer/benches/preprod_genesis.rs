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

//! `LedgerState::from_genesis` on live-node genesis bytes (preprod by default,
//! override with `BENCH_NODE_URL`). Network hit once at setup; bytes cached.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use indexer_common::{
    domain::{LedgerVersion, ledger::LedgerState},
    infra::ledger_db,
};
use serde::Deserialize;
use std::{env, time::Duration};
use tokio::runtime::Runtime;

const DEFAULT_NODE_URL: &str = "https://rpc.preprod.midnight.network";

#[derive(Deserialize)]
struct RpcResponse {
    result: SystemProperties,
}

#[derive(Deserialize)]
struct SystemProperties {
    genesis_state: String,
}

fn init_ledger_db(rt: &Runtime) -> tempfile::TempDir {
    let temp_dir = tempfile::tempdir().expect("create tempdir");
    let sqlite_file = temp_dir
        .path()
        .join("ledger-db.sqlite")
        .display()
        .to_string();
    rt.block_on(async {
        ledger_db::init(ledger_db::Config {
            cache_max_nodes: 1_024,
            cnn_url: sqlite_file,
        })
        .await
        .expect("init ledger_db");
    });
    temp_dir
}

fn fetch_genesis_state(rt: &Runtime, node_url: &str) -> Option<Vec<u8>> {
    rt.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .ok()?;
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "system_properties",
            "params": []
        });
        let resp: RpcResponse = client
            .post(node_url)
            .json(&body)
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;
        let hex = resp.result.genesis_state.trim_start_matches("0x");
        const_hex::decode(hex).ok()
    })
}

fn bench_from_genesis_preprod(c: &mut Criterion) {
    let rt = Runtime::new().expect("tokio runtime");
    let _temp_dir = init_ledger_db(&rt);

    let node_url = env::var("BENCH_NODE_URL").unwrap_or_else(|_| DEFAULT_NODE_URL.to_owned());
    let Some(genesis_bytes) = fetch_genesis_state(&rt, &node_url) else {
        eprintln!("skipping preprod_genesis: cannot reach {node_url}");
        return;
    };

    c.bench_function(
        &format!(
            "LedgerState::from_genesis ({} bytes, {node_url})",
            genesis_bytes.len()
        ),
        |b| {
            b.iter(|| {
                LedgerState::from_genesis(black_box(&genesis_bytes), LedgerVersion::V8)
                    .expect("from_genesis")
            })
        },
    );
}

criterion_group!(benches, bench_from_genesis_preprod);
criterion_main!(benches);
