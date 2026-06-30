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

//! Apply a previously-authorized runtime upgrade by submitting `System::apply_authorized_upgrade`.
//!
//! This is the unsigned-side step of the runtime-upgrade flow: governance has already
//! authorized the code hash via `System::authorize_upgrade`, and any account can now
//! submit the WASM bytes to actually swap the code.

use std::str::FromStr;

use clap::Args;
use subxt::{OnlineClient, SubstrateConfig, dynamic};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApplyAuthorizedUpgradeError {
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
	#[error("events error: {0}")]
	EventsError(#[from] subxt::error::EventsError),
	#[error("keypair parse error: {0}")]
	KeypairParseError(#[from] midnight_node_ledger_helpers::KeypairParseError),
	#[error("runtime upgrade failed: CodeUpdated event not found")]
	CodeUpdateNotFound,
}

#[derive(Args)]
pub struct ApplyAuthorizedUpgradeArgs {
	/// Path to the runtime WASM file. Must match the code hash previously authorized via
	/// `System::authorize_upgrade`.
	#[arg(long)]
	pub wasm_file: String,

	/// Signer key for submitting the apply step (any funded account)
	#[arg(long, default_value = "//Alice")]
	pub signer_key: String,

	/// RPC URL of the node
	#[arg(short, long, default_value = "ws://localhost:9944", env)]
	pub rpc_url: String,
}

pub async fn execute(args: ApplyAuthorizedUpgradeArgs) -> Result<(), ApplyAuthorizedUpgradeError> {
	let code = std::fs::read(&args.wasm_file)?;
	log::info!("Read WASM file: {} ({} bytes)", args.wasm_file, code.len());

	let api = OnlineClient::<SubstrateConfig>::from_insecure_url(&args.rpc_url).await?;
	let signer = midnight_node_ledger_helpers::Keypair::from_str(&args.signer_key)?.0;

	log::info!("Submitting System::apply_authorized_upgrade...");
	let apply_upgrade_call =
		dynamic::tx("System", "apply_authorized_upgrade", vec![dynamic::Value::from_bytes(&code)]);

	let apply_events = api
		.tx()
		.await?
		.sign_and_submit_then_watch_default(&apply_upgrade_call, &signer)
		.await?
		.wait_for_finalized_success()
		.await?;

	for event in apply_events.iter() {
		let event = event?;
		if event.pallet_name() == "System" && event.event_name() == "CodeUpdated" {
			log::info!("Code update success: {:?}", event);
			log::info!("Runtime upgrade applied successfully!");
			return Ok(());
		}
	}
	Err(ApplyAuthorizedUpgradeError::CodeUpdateNotFound)
}
