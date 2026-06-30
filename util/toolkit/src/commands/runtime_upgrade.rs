// This file is part of midnight-node.
// Copyright (C) 2025-2026 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use clap::Args;
use subxt::{OnlineClient, SubstrateConfig, dynamic};
use thiserror::Error;

use crate::commands::root_call::{self, RootCallArgs};

#[derive(Error, Debug)]
pub enum RuntimeUpgradeError {
	#[error("IO error: {0}")]
	IoError(#[from] std::io::Error),
	#[error("subxt error: {0}")]
	SubxtError(#[from] subxt::Error),
	#[error("online client error: {0}")]
	OnlineClientError(#[from] subxt::error::OnlineClientError),
	#[error("online client at block error: {0}")]
	OnlineClientAtBlockError(#[from] subxt::error::OnlineClientAtBlockError),
	#[error("extrinsic error: {0}")]
	ExtrinsicError(#[from] subxt::error::ExtrinsicError),
	#[error("error executing root call: {0}")]
	RootCallError(Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Args)]
pub struct RuntimeUpgradeArgs {
	/// Path to the runtime WASM file
	#[arg(long)]
	pub wasm_file: String,

	/// Council member private keys (32-byte sr25519 seeds)
	#[arg(short, long, required_unless_present = "encode_only")]
	pub council_members: Vec<String>,

	/// Technical Committee member private keys (32-byte sr25519 seeds)
	#[arg(short, long, required_unless_present = "encode_only")]
	pub technical_committee_members: Vec<String>,

	/// RPC URL of the node
	#[arg(short, long, default_value = "ws://localhost:9944", env)]
	pub rpc_url: String,

	/// If set, print the SCALE-encoded `System::authorize_upgrade` call as `0x`-prefixed hex
	/// and exit without contacting governance. The output is suitable for passing to
	/// `toolkit batch --encoded-call ...`.
	#[arg(long)]
	pub encode_only: bool,
}

/// Build the SCALE-encoded `System::authorize_upgrade(blake2_256(wasm))` call for `args.wasm_file`.
pub async fn build_encoded_call(args: &RuntimeUpgradeArgs) -> Result<Vec<u8>, RuntimeUpgradeError> {
	let code = std::fs::read(&args.wasm_file)?;
	log::info!("Read WASM file: {} ({} bytes)", args.wasm_file, code.len());

	let code_hash = sp_crypto_hashing::blake2_256(&code);
	log::info!("Code hash: 0x{}", hex::encode(code_hash));

	let api = OnlineClient::<SubstrateConfig>::from_insecure_url(&args.rpc_url).await?;
	let authorize_upgrade_call =
		dynamic::tx("System", "authorize_upgrade", vec![dynamic::Value::from_bytes(&code_hash)]);
	Ok(api.tx().await?.call_data(&authorize_upgrade_call)?)
}

pub async fn execute(args: RuntimeUpgradeArgs) -> Result<(), RuntimeUpgradeError> {
	let encoded_call = build_encoded_call(&args).await?;

	if args.encode_only {
		println!("0x{}", hex::encode(&encoded_call));
		return Ok(());
	}

	log::info!("Executing authorize_upgrade via federated authority governance.");
	root_call::execute(RootCallArgs {
		rpc_url: args.rpc_url,
		council_keys: args.council_members,
		tc_keys: args.technical_committee_members,
		encoded_call: Some(encoded_call),
		encoded_call_file: None,
	})
	.await
	.map_err(RuntimeUpgradeError::RootCallError)?;

	log::info!(
		"Runtime upgrade authorized. Run `toolkit apply-authorized-upgrade --wasm-file {}` to apply.",
		args.wasm_file
	);
	Ok(())
}
