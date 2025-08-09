// This file is part of midnight-node.
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

use clap::Parser;
use sc_cli::{CliConfiguration, SharedParams};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use sp_sidechain::GetGenesisUtxo;
use std::{io::Write, sync::Arc};

#[derive(Debug, Clone, Parser)]
pub struct SidechainParamsCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	pub shared_params: SharedParams,
}
impl SidechainParamsCmd {
	pub async fn run<B, C>(&self, client: Arc<C>) -> sc_cli::Result<()>
	where
		B: BlockT,
		C: ProvideRuntimeApi<B> + Send + Sync + 'static,
		C::Api: GetGenesisUtxo<B>,
		C: HeaderBackend<B>,
	{
		let api = client.runtime_api();
		let best_block = client.info().best_hash;
		let genesis_utxo = api.genesis_utxo(best_block).unwrap();
		let output = serde_json::to_string_pretty(&genesis_utxo).unwrap();
		std::io::stdout().write_all(output.as_bytes()).unwrap();
		Ok(())
	}
}

impl CliConfiguration for SidechainParamsCmd {
	fn shared_params(&self) -> &SharedParams {
		&self.shared_params
	}
}
