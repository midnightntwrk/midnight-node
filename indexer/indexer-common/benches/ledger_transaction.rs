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

//! Transaction deserialisation and wallet-relevance filtering.

use bip32::{DerivationPath, XPrv};
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use indexer_common::{
    domain::{LedgerVersion, ViewingKey, ledger::Transaction},
    infra::ledger_db,
};
use midnight_zswap_v8::keys::{SecretKeys, Seed};
use std::{fs, str::FromStr};
use tokio::runtime::Runtime;

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

fn viewing_key(n: u8) -> ViewingKey {
    let mut seed = [0; 32];
    seed[31] = n;

    let derivation_path = DerivationPath::from_str("m/44'/2400'/0'/3/0").expect("derivation path");
    let derived_seed: [u8; 32] = XPrv::derive_from_path(seed, &derivation_path)
        .expect("derive key")
        .private_key()
        .to_bytes()
        .into();

    SecretKeys::from(Seed::from(derived_seed))
        .encryption_secret_key
        .repr()
        .into()
}

fn bench_transaction_deserialize(c: &mut Criterion) {
    let rt = Runtime::new().expect("tokio runtime");
    let _temp_dir = init_ledger_db(&rt);

    let tx_1_2_2 = fs::read(format!("{}/tests/tx_1_2_2.raw", env!("CARGO_MANIFEST_DIR")))
        .expect("read tx_1_2_2.raw");

    let tx_1_2_3 = fs::read(format!("{}/tests/tx_1_2_3.raw", env!("CARGO_MANIFEST_DIR")))
        .expect("read tx_1_2_3.raw");

    let mut group = c.benchmark_group("Transaction::deserialize");
    group.bench_function("tx_1_2_2", |b| {
        b.iter(|| {
            Transaction::deserialize(black_box(&tx_1_2_2), LedgerVersion::V9).expect("deserialize")
        })
    });
    group.bench_function("tx_1_2_3", |b| {
        b.iter(|| {
            Transaction::deserialize(black_box(&tx_1_2_3), LedgerVersion::V9).expect("deserialize")
        })
    });
    group.finish();
}

fn bench_transaction_relevant(c: &mut Criterion) {
    let rt = Runtime::new().expect("tokio runtime");
    let _temp_dir = init_ledger_db(&rt);

    let tx_1_2_2_bytes = fs::read(format!("{}/tests/tx_1_2_2.raw", env!("CARGO_MANIFEST_DIR")))
        .expect("read tx_1_2_2.raw");
    let tx_1_2_2 =
        Transaction::deserialize(&tx_1_2_2_bytes, LedgerVersion::V9).expect("deserialize tx_1_2_2");

    let tx_1_2_3_bytes = fs::read(format!("{}/tests/tx_1_2_3.raw", env!("CARGO_MANIFEST_DIR")))
        .expect("read tx_1_2_3.raw");
    let tx_1_2_3 =
        Transaction::deserialize(&tx_1_2_3_bytes, LedgerVersion::V9).expect("deserialize tx_1_2_3");

    let vk_1 = viewing_key(1);
    let vk_2 = viewing_key(2);
    let vk_3 = viewing_key(3);

    let mut group = c.benchmark_group("Transaction::relevant");

    // tx_1_2_2 matches vk_1 and vk_2 but not vk_3.
    group.bench_function("tx_1_2_2 vk_1 (match)", |b| {
        b.iter(|| tx_1_2_2.relevant(black_box(vk_1)))
    });
    group.bench_function("tx_1_2_2 vk_3 (no match)", |b| {
        b.iter(|| tx_1_2_2.relevant(black_box(vk_3)))
    });

    // tx_1_2_3 matches vk_1 and vk_3 but not vk_2.
    group.bench_function("tx_1_2_3 vk_1 (match)", |b| {
        b.iter(|| tx_1_2_3.relevant(black_box(vk_1)))
    });
    group.bench_function("tx_1_2_3 vk_2 (no match)", |b| {
        b.iter(|| tx_1_2_3.relevant(black_box(vk_2)))
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_transaction_deserialize,
    bench_transaction_relevant
);
criterion_main!(benches);
