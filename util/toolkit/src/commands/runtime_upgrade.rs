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
use std::time::Duration;

use clap::Args;
use subxt::{
	OnlineClient, SubstrateConfig, dynamic,
	rpcs::{RpcClient, rpc_params},
};
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
	#[error("transaction finalized error: {0}")]
	TransactionFinalizedError(#[from] subxt::error::TransactionFinalizedSuccessError),
	#[error("transaction progress error: {0}")]
	TransactionProgressError(#[from] subxt::error::TransactionProgressError),
	#[error("rpc error: {0}")]
	RpcError(#[from] subxt::rpcs::Error),
	#[error("events error: {0}")]
	EventsError(#[from] subxt::error::EventsError),
	#[error("keypair parse error: {0}")]
	KeypairParseError(#[from] midnight_node_ledger_helpers::KeypairParseError),
	#[error("error executing root call: {0}")]
	RootCallError(Box<dyn std::error::Error + Send + Sync>),
	#[error("runtime upgrade failed: CodeUpdated event not found")]
	CodeUpdateNotFound,
	#[error("timed out waiting for the apply_authorized_upgrade transaction to finalize")]
	ApplyFinalizeTimeout,
	#[error(
		"runtime upgrade did not enact: spec_version stayed at {0} after applying the code"
	)]
	UpgradeNotEnacted(u32),
}

/// Query the on-chain runtime spec_version via the raw `state_getRuntimeVersion` RPC.
///
/// We avoid subxt's typed metadata here on purpose: across a runtime upgrade the
/// client's metadata switches to the new runtime, so decoding anything encoded by
/// the old runtime (e.g. the `System.CodeUpdated` event in the apply block) is
/// unreliable. The raw JSON spec_version has no such dependency.
async fn spec_version(rpc: &RpcClient) -> Result<u32, RuntimeUpgradeError> {
	let version: serde_json::Value =
		rpc.request("state_getRuntimeVersion", rpc_params![]).await?;
	Ok(version.get("specVersion").and_then(|v| v.as_u64()).unwrap_or(0) as u32)
}

#[derive(Args)]
pub struct RuntimeUpgradeArgs {
	/// Path to the runtime WASM file
	#[arg(long)]
	pub wasm_file: String,

	/// Council member private keys (32-byte sr25519 seeds)
	#[arg(short, long, required = true)]
	pub council_members: Vec<String>,

	/// Technical Committee member private keys (32-byte sr25519 seeds)
	#[arg(short, long, required = true)]
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
	let rpc_client = RpcClient::from_insecure_url(&args.rpc_url).await?;
	let api = OnlineClient::<SubstrateConfig>::from_rpc_client(rpc_client.clone()).await?;
	let authorize_upgrade_call =
		dynamic::tx("System", "authorize_upgrade", vec![dynamic::Value::from_bytes(&code_hash)]);
	let encoded_call = api.tx().await?.call_data(&authorize_upgrade_call)?;

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

	let pre_spec_version = spec_version(&rpc_client).await?;
	log::info!("Pre-upgrade spec_version: {pre_spec_version}");

	// Wait only for finalization — NOT `wait_for_finalized_success`, which eagerly
	// decodes the block's events. `apply_authorized_upgrade` swaps the on-chain
	// code, and subxt's metadata follows it to the new runtime, so decoding the
	// old runtime's `System.CodeUpdated` event in that block fails. Confirm the
	// upgrade succeeded by observing the spec_version bump below instead.
	let submit = async {
		api.tx()
			.await?
			.sign_and_submit_then_watch_default(&apply_upgrade_call, &signer)
			.await?
			.wait_for_finalized()
			.await
			.map_err(RuntimeUpgradeError::from)
	};
	tokio::time::timeout(Duration::from_secs(120), submit)
		.await
		.map_err(|_| RuntimeUpgradeError::ApplyFinalizeTimeout)??;

	// Step 6: Confirm the upgrade enacted by polling for the spec_version bump.
	// The new code takes effect on the block after the apply block, so poll a few
	// block times.
	for _ in 0..20 {
		tokio::time::sleep(Duration::from_secs(3)).await;
		let cur = spec_version(&rpc_client).await?;
		if cur > pre_spec_version {
			log::info!(
				"Runtime upgrade completed successfully! spec_version {pre_spec_version} -> {cur}"
			);
			return Ok(());
		}
	}

	Err(RuntimeUpgradeError::UpgradeNotEnacted(pre_spec_version))
}
