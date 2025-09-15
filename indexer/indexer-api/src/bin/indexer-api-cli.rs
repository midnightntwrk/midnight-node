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

#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![cfg_attr(coverage_nightly, coverage(off))]

use clap::{Parser, Subcommand, command};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Cli::parse().run()
}

#[derive(Debug, Parser)]
#[command()]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    PrintApiSchemaV1,
}

impl Cli {
    fn run(self) -> anyhow::Result<()> {
        match self.command {
            Command::PrintApiSchemaV1 => {
                let schema = indexer_api::infra::api::v1::export_schema();
                println!("{schema}");
            }
        };
        Ok(())
    }
}
