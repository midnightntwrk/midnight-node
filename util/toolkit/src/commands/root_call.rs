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
use subxt::{
	Metadata, OnlineClient, SubstrateConfig,
	dynamic::{self, Value},
	ext::scale_value::{At, scale::decode_as_type},
	tx::Payload,
	utils::H256,
};
use subxt_signer::sr25519::Keypair;
use thiserror::Error;

#[derive(Args)]
pub struct RootCallArgs {
	/// RPC URL of the node
	#[arg(long, env = "RPC_URL", default_value = "ws://127.0.0.1:9944")]
	pub rpc_url: String,

	/// Council member private keys as hex strings (32-byte sr25519 seeds)
	#[arg(long = "council-keys", required = true, num_args = 1..)]
	pub council_keys: Vec<String>,

	/// Technical Committee member private keys as hex strings (32-byte sr25519 seeds)
	#[arg(long = "tc-keys", required = true, num_args = 1..)]
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
	#[error("Proposal index not found in events")]
	ProposalIndexNotFound,
	#[error("Call execution failed")]
	CallExecutionFailed,
	#[error("Need at least 2 council keys for 2/3 threshold voting")]
	NotEnoughCouncilKeys,
	#[error("Need at least 2 technical committee keys for 2/3 threshold voting")]
	NotEnoughTcKeys,
	#[error("Kepair parse error")]
	KeypairParseError(#[from] midnight_node_ledger_helpers::KeypairParseError),
	#[error("Failed to decode call: {0}")]
	CallDecodeError(String),
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
	execute_governance_call(&api, &encoded_call, &council_keypairs, &tc_keypairs).await?;

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

/// Decode SCALE-encoded call bytes into a Value using runtime metadata
fn decode_call_to_value(encoded_call: &[u8], metadata: &Metadata) -> Result<Value, RootCallError> {
	// Get the RuntimeCall type ID from metadata
	let call_ty_id = metadata.outer_enums().call_enum_ty();

	// Decode the bytes into a Value<u32> (with type ID context)
	let value = decode_as_type(&mut &encoded_call[..], call_ty_id, metadata.types())
		.map_err(|e| RootCallError::CallDecodeError(format!("{:?}", e)))?;

	// Convert Value<u32> to Value<()> by removing type ID context
	Ok(value.remove_context())
}

async fn execute_governance_call(
	api: &OnlineClient<SubstrateConfig>,
	encoded_call: &[u8],
	council_keypairs: &[Keypair],
	tc_keypairs: &[Keypair],
) -> Result<(), RootCallError> {
	// The encoded_call is already the full SCALE-encoded call
	// We need to decode it into a Value and wrap it in FederatedAuthority::motion_approve

	// Step 1: Decode the encoded call bytes into a Value using metadata
	let call_value = decode_call_to_value(encoded_call, &api.metadata())?;
	log::info!("Decoded call successfully");

	// Step 2: Create the FederatedAuthority::motion_approve call wrapping our decoded call
	let fed_auth_call =
		dynamic::tx("FederatedAuthority", "motion_approve", vec![call_value.clone()]).into_value();

	// Compute the proposal hash for the federated authority call
	let fed_auth_tx = dynamic::tx("FederatedAuthority", "motion_approve", vec![call_value.clone()]);
	let fed_auth_call_data = fed_auth_tx
		.encode_call_data(&api.metadata())
		.map_err(|e| RootCallError::SubxtError(subxt::Error::Other(format!("{:?}", e))))?;
	let proposal_hash = sp_crypto_hashing::blake2_256(&fed_auth_call_data);
	let proposal_hash = H256(proposal_hash);

	log::info!("Proposal hash: 0x{}", hex::encode(proposal_hash.0));

	// Step 2: Council proposes
	log::info!("Council proposing federated motion approval...");
	let council_proposer = &council_keypairs[0];

	let council_proposal = dynamic::tx(
		"Council",
		"propose",
		vec![Value::u128(2), fed_auth_call.clone(), Value::u128(10000)],
	);

	let council_propose_events = api
		.tx()
		.sign_and_submit_then_watch_default(&council_proposal, council_proposer)
		.await?
		.wait_for_finalized_success()
		.await?;

	let council_proposal_index = extract_proposal_index(&council_propose_events, "Council")?;
	log::info!(
		"Council proposal created with hash: 0x{} and index: {}",
		hex::encode(proposal_hash.0),
		council_proposal_index
	);

	// Step 3: Council members vote (need 2/3 threshold)
	log::info!("Council members voting...");
	for (i, voter) in council_keypairs.iter().take(2).enumerate() {
		log::info!("Council vote {} from 0x{}", i + 1, hex::encode(voter.public_key().0));
		vote_on_proposal(api, voter, "Council", proposal_hash, council_proposal_index, true)
			.await?;
	}

	// Step 4: Close Council proposal
	log::info!("Closing Council proposal...");
	close_proposal(api, council_proposer, "Council", proposal_hash, council_proposal_index).await?;

	// Step 5: Technical Committee proposes
	log::info!("Technical Committee proposing federated motion approval...");
	let tc_proposer = &tc_keypairs[0];

	let tech_proposal = dynamic::tx(
		"TechnicalCommittee",
		"propose",
		vec![Value::u128(2), fed_auth_call, Value::u128(10000)],
	);

	let tech_propose_events = api
		.tx()
		.sign_and_submit_then_watch_default(&tech_proposal, tc_proposer)
		.await?
		.wait_for_finalized_success()
		.await?;

	let tech_proposal_index = extract_proposal_index(&tech_propose_events, "TechnicalCommittee")?;
	log::info!(
		"Technical Committee proposal created with hash: 0x{} and index: {}",
		hex::encode(proposal_hash.0),
		tech_proposal_index
	);

	// Step 6: Technical Committee members vote
	log::info!("Technical Committee members voting...");
	for (i, voter) in tc_keypairs.iter().take(2).enumerate() {
		log::info!("TC vote {} from 0x{}", i + 1, hex::encode(voter.public_key().0));
		vote_on_proposal(
			api,
			voter,
			"TechnicalCommittee",
			proposal_hash,
			tech_proposal_index,
			true,
		)
		.await?;
	}

	// Step 7: Close Technical Committee proposal
	log::info!("Closing Technical Committee proposal...");
	close_proposal(api, tc_proposer, "TechnicalCommittee", proposal_hash, tech_proposal_index)
		.await?;

	log::info!("Federated authority motion approved by both councils!");

	// Step 8: Compute the motion hash and close the federated motion
	let motion_hash = sp_crypto_hashing::blake2_256(encoded_call);
	let motion_hash = H256(motion_hash);
	log::info!("Motion hash: 0x{}", hex::encode(motion_hash.0));

	log::info!("Closing federated motion to execute call with Root origin...");
	let close_motion_call =
		dynamic::tx("FederatedAuthority", "motion_close", vec![Value::from_bytes(&motion_hash.0)]);

	// Anyone can close the motion, use first council member
	api.tx()
		.sign_and_submit_then_watch_default(&close_motion_call, council_proposer)
		.await?
		.wait_for_finalized_success()
		.await?;

	log::info!("Federated motion closed, call executed with Root origin!");

	Ok(())
}

async fn vote_on_proposal(
	api: &OnlineClient<SubstrateConfig>,
	signer: &Keypair,
	pallet: &str,
	proposal_hash: H256,
	proposal_index: u32,
	approve: bool,
) -> Result<(), RootCallError> {
	let vote_call = dynamic::tx(
		pallet,
		"vote",
		vec![
			Value::from_bytes(&proposal_hash.0),
			Value::u128(proposal_index as u128),
			Value::bool(approve),
		],
	);

	api.tx()
		.sign_and_submit_then_watch_default(&vote_call, signer)
		.await?
		.wait_for_finalized_success()
		.await?;

	Ok(())
}

async fn close_proposal(
	api: &OnlineClient<SubstrateConfig>,
	signer: &Keypair,
	pallet: &str,
	proposal_hash: H256,
	proposal_index: u32,
) -> Result<(), RootCallError> {
	let weight_value = Value::named_composite(vec![
		("ref_time", Value::u128(10_000_000_000)),
		("proof_size", Value::u128(65536)),
	]);

	let close_call = dynamic::tx(
		pallet,
		"close",
		vec![
			Value::from_bytes(&proposal_hash.0),
			Value::u128(proposal_index as u128),
			weight_value,
			Value::u128(10000),
		],
	);

	api.tx()
		.sign_and_submit_then_watch_default(&close_call, signer)
		.await?
		.wait_for_finalized_success()
		.await?;

	Ok(())
}

fn extract_proposal_index(
	events: &subxt::blocks::ExtrinsicEvents<SubstrateConfig>,
	pallet: &str,
) -> Result<u32, RootCallError> {
	for event in events.iter() {
		let event = event?;
		if event.pallet_name() == pallet && event.variant_name() == "Proposed" {
			// Use subxt's field_values() to decode the event fields using metadata
			let fields = event.field_values().map_err(|e| RootCallError::SubxtError(e.into()))?;

			// The Proposed event has fields: account, proposal_index, proposal_hash, threshold
			// Access proposal_index by field name
			if let Some(proposal_index_value) = fields.at("proposal_index") {
				if let Some(index) = proposal_index_value.as_u128() {
					return Ok(index as u32);
				}
			}
		}
	}
	Err(RootCallError::ProposalIndexNotFound)
}
