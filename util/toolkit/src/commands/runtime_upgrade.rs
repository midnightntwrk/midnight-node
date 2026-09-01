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
	#[error("runtime upgrade did not enact: spec_version stayed at {0} after applying the code")]
	UpgradeNotEnacted(u32),
}

/// Query the on-chain runtime spec_version via the raw `state_getRuntimeVersion` RPC.
///
/// We avoid subxt's typed metadata here on purpose: across a runtime upgrade the
/// client's metadata switches to the new runtime, so decoding anything encoded by
/// the old runtime (e.g. the `System.CodeUpdated` event in the apply block) is
/// unreliable. The raw JSON spec_version has no such dependency.
/// (finalized_height, spec_version at the finalized head).
///
/// We track both because `state_getRuntimeVersion(finalized)` reports the code
/// *stored at* that block — which flips to the new runtime already at the
/// `apply_authorized_upgrade` block, even though that block *executed* under the
/// old runtime (its `MNSV` digest — how the toolkit fetcher classifies a block's
/// ledger version — is still the old spec). The first block that actually
/// executes the new runtime is `apply + 1`. So "spec flipped at the finalized
/// head" is one block early; callers must additionally wait for the finalized
/// height to advance past that point before the fetcher will see a ledger-9 block.
async fn finalized_state(rpc: &RpcClient) -> Result<(u64, u32), RuntimeUpgradeError> {
	let hash: serde_json::Value = rpc.request("chain_getFinalizedHead", rpc_params![]).await?;
	let header: serde_json::Value =
		rpc.request("chain_getHeader", rpc_params![hash.clone()]).await?;
	let version: serde_json::Value =
		rpc.request("state_getRuntimeVersion", rpc_params![hash]).await?;
	let height = header
		.get("number")
		.and_then(|n| n.as_str())
		.and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
		.unwrap_or(0);
	let spec = version.get("specVersion").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
	Ok((height, spec))
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

/// Run the runtime-upgrade command. `rpc_request_timeout` is the per-request
/// RPC timeout applied to every node connection this command makes.
pub async fn execute(
	args: RuntimeUpgradeArgs,
	rpc_request_timeout: Duration,
) -> Result<(), RuntimeUpgradeError> {
	// Step 1: Read the WASM file
	let code = std::fs::read(&args.wasm_file)?;
	log::info!("Read WASM file: {} ({} bytes)", args.wasm_file, code.len());

	// Step 2: Compute blake2-256 hash of the WASM code
	let code_hash = sp_crypto_hashing::blake2_256(&code);
	log::info!("Code hash: 0x{}", hex::encode(code_hash));

	// Step 3: Build System::authorize_upgrade call and encode it
	let rpc_client =
		crate::client::rpc_client_with_timeout(&args.rpc_url, rpc_request_timeout).await?;
	let api = OnlineClient::<SubstrateConfig>::from_rpc_client(rpc_client.clone()).await?;
	let authorize_upgrade_call =
		dynamic::tx("System", "authorize_upgrade", vec![dynamic::Value::from_bytes(&code_hash)]);
	let encoded_call = api.tx().await?.call_data(&authorize_upgrade_call)?;

	// Step 4: Execute the authorization through governance
	log::info!("Executing authorize_upgrade via federated authority governance.");
	root_call::execute(
		RootCallArgs {
			rpc_url: args.rpc_url.clone(),
			council_keys: args.council_members,
			tc_keys: args.technical_committee_members,
			encoded_call: Some(encoded_call),
			encoded_call_file: None,
		},
		rpc_request_timeout,
	)
	.await
	.map_err(RuntimeUpgradeError::RootCallError)?;

	// Step 5: Apply the authorized upgrade
	log::info!("Applying authorized upgrade...");
	let signer = midnight_node_ledger_helpers::Keypair::from_str(&args.signer_key)?.0;
	let apply_upgrade_call =
		dynamic::tx("System", "apply_authorized_upgrade", vec![dynamic::Value::from_bytes(&code)]);

	let (_, pre_spec_version) = finalized_state(&rpc_client).await?;
	log::info!("Pre-upgrade spec_version (finalized): {pre_spec_version}");

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

	// Step 6: Confirm the upgrade is not just applied but *executing* at a
	// finalized block. `state_getRuntimeVersion(finalized)` reports the stored
	// code, which flips to the new runtime already at the apply block — but that
	// block's `MNSV` execution-version digest (how the fetcher classifies it) is
	// still the old spec. The first block that runs the new runtime is apply+1.
	// So: note the finalized height where the stored spec first exceeds pre, then
	// wait for the finalized height to advance past it (apply+1 finalized). Only
	// then will a downstream `fetch` see a ledger-9-classified block.
	let mut flip_height: Option<u64> = None;
	for _ in 0..60 {
		tokio::time::sleep(Duration::from_secs(3)).await;
		let (height, spec) = finalized_state(&rpc_client).await?;
		if spec > pre_spec_version {
			match flip_height {
				None => flip_height = Some(height),
				Some(h0) if height > h0 => {
					log::info!(
						"Runtime upgrade completed successfully! spec_version {pre_spec_version} -> {spec}; \
						 new runtime executing since finalized #{}, now finalized #{height}",
						h0 + 1,
					);
					return Ok(());
				},
				_ => {},
			}
		}
	}

	Err(RuntimeUpgradeError::UpgradeNotEnacted(pre_spec_version))
}
