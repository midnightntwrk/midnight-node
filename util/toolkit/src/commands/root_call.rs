// This file is part of midnight-node.
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

//! Execute a call through governance (Council + Technical Committee) with Root origin.
//!
//! This command allows executing arbitrary runtime calls through the federated authority
//! governance mechanism using proper governance.

use std::str::FromStr;

use crate::cli_parsers as cli;
use clap::Args;
use subxt::{OnlineClient, SubstrateConfig};
use subxt_signer::sr25519::Keypair;
use thiserror::Error;

#[derive(Args)]
pub struct RootCallArgs {
	/// RPC URL of the node
	#[arg(long, env = "RPC_URL", default_value = "ws://127.0.0.1:9944")]
	pub rpc_url: String,

	/// Council member private keys as hex strings (32-byte sr25519 seeds).
	/// Defaults to Ferdie, Dave, Eve (dev network council members).
	#[arg(
		long = "council-keys",
		num_args = 1..,
		default_values_t = [
			"42438b7883391c05512a938e36c2df0131e088b3756d6aa7a755fbff19d2f842".to_string(),
			"868020ae0687dda7d57565093a69090211449845a7e11453612800b663307246".to_string(),
			"786ad0e2df456fe43dd1f91ebca22e235bc162e0bb8d53c633e8c85b2af68b7a".to_string(),
		]
	)]
	pub council_keys: Vec<String>,

	/// Technical Committee member private keys as hex strings (32-byte sr25519 seeds).
	/// Defaults to Bob, Charlie, Alice (dev network TC members).
	#[arg(
		long = "tc-keys",
		num_args = 1..,
		default_values_t = [
			"398f0c28f98885e046333d4a41c19cee4c37368a9832c6502f6cfd182e2aef89".to_string(),
			"bc1ede780f784bb6991a585e4f6e61522c14e1cae6ad0895fb57b9a205a8f938".to_string(),
			"e5be9a5092b81bca64be81d212e7f2f9eba183bb7a90954f7b76361f6edb5c0a".to_string(),
		]
	)]
	pub tc_keys: Vec<String>,

	/// Encoded call as hex string (e.g., 0x...)
	#[arg(long, conflicts_with = "encoded_call_file", value_parser = cli::hex_bytes)]
	pub encoded_call: Option<Vec<u8>>,

	/// Path to file containing the encoded call hex string
	#[arg(long, conflicts_with = "encoded_call")]
	pub encoded_call_file: Option<String>,
}

#[derive(Error, Debug)]
pub enum RootCallError {
	#[error("subxt error: {0}")]
	SubxtError(#[from] subxt::Error),
	#[error("signer error: {0}")]
	SignerError(#[from] subxt_signer::sr25519::Error),
	#[error("hex decode error: {0}")]
	HexError(#[from] hex::FromHexError),
	#[error("IO error: {0}")]
	IoError(#[from] std::io::Error),
	#[error("No encoded call provided. Use --encoded-call or --encoded-call-file")]
	NoEncodedCall,
	#[error("Call execution failed")]
	CallExecutionFailed,
	#[error("governance error: {0}")]
	GovernanceError(#[from] governance_helpers::GovernanceError),
	#[error("Need at least 2 council keys for 2/3 threshold voting")]
	NotEnoughCouncilKeys,
	#[error("Need at least 2 technical committee keys for 2/3 threshold voting")]
	NotEnoughTcKeys,
	#[error("Kepair parse error")]
	KeypairParseError(#[from] midnight_node_ledger_helpers::KeypairParseError),
}

pub async fn execute(args: RootCallArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	// Validate we have enough keys
	if args.council_keys.len() < 2 {
		return Err(RootCallError::NotEnoughCouncilKeys.into());
	}
	if args.tc_keys.len() < 2 {
		return Err(RootCallError::NotEnoughTcKeys.into());
	}

	// Get the encoded call
	let encoded_call = get_encoded_call(&args)?;
	log::info!("Encoded call ({}  bytes): 0x{}", encoded_call.len(), hex::encode(&encoded_call));

	// Parse council keypairs
	let council_keypairs: Vec<Keypair> =
		args.council_keys.iter().map(|k| get_signer(k)).collect::<Result<Vec<_>, _>>()?;

	// Parse TC keypairs
	let tc_keypairs: Vec<Keypair> =
		args.tc_keys.iter().map(|k| get_signer(k)).collect::<Result<Vec<_>, _>>()?;

	log::info!("Council members: {}", council_keypairs.len());
	for (i, kp) in council_keypairs.iter().enumerate() {
		log::info!("  Council[{}]: 0x{}", i, hex::encode(kp.public_key().0));
	}

	log::info!("Technical Committee members: {}", tc_keypairs.len());
	for (i, kp) in tc_keypairs.iter().enumerate() {
		log::info!("  TC[{}]: 0x{}", i, hex::encode(kp.public_key().0));
	}

	// Connect to the node
	log::info!("Connecting to node at {}", args.rpc_url);
	let api = OnlineClient::<SubstrateConfig>::from_insecure_url(&args.rpc_url).await?;

	// Execute the governance flow
	governance_helpers::execute_governance_call(
		&api,
		&encoded_call,
		&council_keypairs,
		&tc_keypairs,
	)
	.await?;

	log::info!("Call executed successfully through governance!");
	Ok(())
}

fn get_encoded_call(args: &RootCallArgs) -> Result<Vec<u8>, RootCallError> {
	if let Some(ref call) = args.encoded_call {
		Ok(call.clone())
	} else if let Some(ref path) = args.encoded_call_file {
		let hex_str = std::fs::read_to_string(path)?.trim().to_string();
		// Remove 0x prefix if present
		let hex_str = hex_str.strip_prefix("0x").unwrap_or(&hex_str);
		Ok(hex::decode(hex_str)?)
	} else {
		return Err(RootCallError::NoEncodedCall);
	}
}

fn get_signer(key_str: &str) -> Result<Keypair, RootCallError> {
	Ok(midnight_node_ledger_helpers::Keypair::from_str(key_str)?.0)
}
