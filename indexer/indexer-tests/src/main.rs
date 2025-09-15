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

use clap::{Parser, command};
use indexer_common::domain::NetworkId;
use indexer_tests::e2e;

/// e2e tests against the Indexer API.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Cli::parse().run().await
}

/// Run the e2e tests against the Indexer API.
#[derive(Debug, Parser)]
#[command()]
struct Cli {
    /// The network ID the Indexer is deployed to.
    #[arg(long)]
    network_id: NetworkId,

    /// The Indexer API host, e.g. `localhost`.
    #[arg(long)]
    host: String,

    /// The Indexer API port, e.g. `8088`.
    #[arg(long)]
    port: u16,

    /// If set, use https and wss, else http and ws for the Indexer API.
    #[arg(long)]
    secure: bool,
}

impl Cli {
    async fn run(self) -> anyhow::Result<()> {
        let Cli {
            network_id,
            host,
            port,
            secure,
        } = self;
        e2e::run(network_id, &host, port, secure).await
    }
}
