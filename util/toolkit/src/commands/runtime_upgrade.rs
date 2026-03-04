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

use std::str::FromStr;

use clap::Args;
use subxt::{OnlineClient, SubstrateConfig, dynamic, tx::Payload};
use thiserror::Error;

use crate::commands::root_call::{self, RootCallArgs};

#[derive(Error, Debug)]
pub enum RuntimeUpgradeError {
	#[error("IO error: {0}")]
	IoError(#[from] std::io::Error),
	#[error("subxt error: {0}")]
	SubxtError(#[from] subxt::Error),
	#[error("keypair parse error: {0}")]
	KeypairParseError(#[from] midnight_node_ledger_helpers::KeypairParseError),
	#[error("error executing root call: {0}")]
	RootCallError(Box<dyn std::error::Error + Send + Sync>),
	#[error("runtime upgrade failed: CodeUpdated event not found")]
	CodeUpdateNotFound,
}

#[derive(Args)]
pub struct RuntimeUpgradeArgs {
	/// Path to the runtime WASM file
	#[arg(long)]
	pub wasm_file: String,

	/// Council member private keys (32-byte sr25519 seeds)
	#[arg(short, required = true)]
	pub council_members: Vec<String>,

	/// Technical Committee member private keys (32-byte sr25519 seeds)
	#[arg(short, required = true)]
	pub technical_committee_members: Vec<String>,

	/// RPC URL of the node
	#[arg(short, long, default_value = "ws://localhost:9944", env)]
	pub rpc_url: String,

	/// Signer key for the apply step (any funded account)
	#[arg(long, default_value = "//Alice")]
	pub signer_key: String,
}

pub async fn execute(args: RuntimeUpgradeArgs) -> Result<(), RuntimeUpgradeError> {
	// Step 1: Read the WASM file
	let code = std::fs::read(&args.wasm_file)?;
	log::info!("Read WASM file: {} ({} bytes)", args.wasm_file, code.len());

	// Step 2: Compute blake2-256 hash of the WASM code
	let code_hash = sp_crypto_hashing::blake2_256(&code);
	log::info!("Code hash: 0x{}", hex::encode(code_hash));

	// Step 3: Build System::authorize_upgrade call and encode it
	let api = OnlineClient::<SubstrateConfig>::from_insecure_url(&args.rpc_url).await?;
	let authorize_upgrade_call =
		dynamic::tx("System", "authorize_upgrade", vec![dynamic::Value::from_bytes(&code_hash)]);
	let encoded_call = authorize_upgrade_call
		.encode_call_data(&api.metadata())
		.map_err(|e| RuntimeUpgradeError::SubxtError(subxt::Error::Other(format!("{e:?}"))))?;

	// Step 4: Execute the authorization through governance
	log::info!("Executing authorize_upgrade via federated authority governance.");
	root_call::execute(RootCallArgs {
		rpc_url: args.rpc_url.clone(),
		council_keys: args.council_members,
		tc_keys: args.technical_committee_members,
		encoded_call: Some(encoded_call),
		encoded_call_file: None,
	})
	.await
	.map_err(RuntimeUpgradeError::RootCallError)?;

	// Step 5: Apply the authorized upgrade
	log::info!("Applying authorized upgrade...");
	let signer = midnight_node_ledger_helpers::Keypair::from_str(&args.signer_key)?.0;
	let apply_upgrade_call =
		dynamic::tx("System", "apply_authorized_upgrade", vec![dynamic::Value::from_bytes(&code)]);

	let apply_events = api
		.tx()
		.sign_and_submit_then_watch_default(&apply_upgrade_call, &signer)
		.await?
		.wait_for_finalized_success()
		.await?;

	// Step 6: Verify CodeUpdated event
	let mut success = false;
	for event in apply_events.iter() {
		let event = event?;
		if event.pallet_name() == "System" && event.variant_name() == "CodeUpdated" {
			log::info!("Code update success: {:?}", event);
			success = true;
			break;
		}
	}
	if !success {
		return Err(RuntimeUpgradeError::CodeUpdateNotFound);
	}

	log::info!("Runtime upgrade completed successfully!");
	Ok(())
}
