// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
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

//! Shared helpers for executing calls through governance (Council + Technical Committee)
//! with Root origin via the FederatedAuthority pallet.

use subxt::{
	Metadata, OnlineClient, SubstrateConfig,
	dynamic::{self, Value},
	ext::scale_value::{At, scale::decode_as_type},
	tx::Payload,
	utils::H256,
};
use subxt_signer::sr25519::Keypair;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GovernanceError {
	#[error("subxt error: {0}")]
	SubxtError(#[from] subxt::Error),
	#[error("Proposal index not found in events")]
	ProposalIndexNotFound,
	#[error("Failed to decode call: {0}")]
	CallDecodeError(String),
}

/// Execute an encoded call through the full governance flow:
/// Council propose → vote → close → TC propose → vote → close → FederatedAuthority motion close.
pub async fn execute_governance_call(
	api: &OnlineClient<SubstrateConfig>,
	encoded_call: &[u8],
	council_keypairs: &[Keypair],
	tc_keypairs: &[Keypair],
) -> Result<(), GovernanceError> {
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
		.map_err(|e| GovernanceError::SubxtError(subxt::Error::Other(format!("{:?}", e))))?;
	let proposal_hash = sp_crypto_hashing::blake2_256(&fed_auth_call_data);
	let proposal_hash = H256(proposal_hash);

	log::info!("Proposal hash: 0x{}", hex::encode(proposal_hash.0));

	// Step 3: Council proposes
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

	// Step 4: Council members vote (need 2/3 threshold)
	log::info!("Council members voting...");
	for (i, voter) in council_keypairs.iter().take(2).enumerate() {
		log::info!("Council vote {} from 0x{}", i + 1, hex::encode(voter.public_key().0));
		vote_on_proposal(api, voter, "Council", proposal_hash, council_proposal_index, true)
			.await?;
	}

	// Step 5: Close Council proposal
	log::info!("Closing Council proposal...");
	close_proposal(api, council_proposer, "Council", proposal_hash, council_proposal_index).await?;

	// Step 6: Technical Committee proposes
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

	// Step 7: Technical Committee members vote
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

	// Step 8: Close Technical Committee proposal
	log::info!("Closing Technical Committee proposal...");
	close_proposal(api, tc_proposer, "TechnicalCommittee", proposal_hash, tech_proposal_index)
		.await?;

	log::info!("Federated authority motion approved by both councils!");

	// Step 9: Compute the motion hash and close the federated motion
	let motion_hash = sp_crypto_hashing::blake2_256(encoded_call);
	let motion_hash = H256(motion_hash);
	log::info!("Motion hash: 0x{}", hex::encode(motion_hash.0));

	log::info!("Closing federated motion to execute call with Root origin...");
	// Build motion_close args — newer runtimes require a proposal_weight_bound parameter,
	// older runtimes only take motion_hash. Detect via metadata to stay backward-compatible
	// with pre-upgrade runtimes (e.g. during hardfork tests).
	let motion_close_args = if has_motion_close_weight_bound(api) {
		let proposal_weight_bound = Value::named_composite(vec![
			("ref_time", Value::u128(1_000_000_000_000)),
			("proof_size", Value::u128(1_000_000)),
		]);
		vec![Value::from_bytes(&motion_hash.0), proposal_weight_bound]
	} else {
		vec![Value::from_bytes(&motion_hash.0)]
	};
	let close_motion_call = dynamic::tx("FederatedAuthority", "motion_close", motion_close_args);

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
) -> Result<(), GovernanceError> {
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
) -> Result<(), GovernanceError> {
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
) -> Result<u32, GovernanceError> {
	for event in events.iter() {
		let event = event?;
		if event.pallet_name() == pallet && event.variant_name() == "Proposed" {
			let fields = event.field_values().map_err(|e| GovernanceError::SubxtError(e.into()))?;

			if let Some(proposal_index_value) = fields.at("proposal_index") {
				if let Some(index) = proposal_index_value.as_u128() {
					return Ok(index as u32);
				}
			}
		}
	}
	Err(GovernanceError::ProposalIndexNotFound)
}

/// Decode SCALE-encoded call bytes into a Value using runtime metadata.
fn decode_call_to_value(
	encoded_call: &[u8],
	metadata: &Metadata,
) -> Result<Value, GovernanceError> {
	let call_ty_id = metadata.outer_enums().call_enum_ty();

	let value = decode_as_type(&mut &encoded_call[..], call_ty_id, metadata.types())
		.map_err(|e| GovernanceError::CallDecodeError(format!("{:?}", e)))?;

	Ok(value.remove_context())
}

/// Check whether the runtime's `FederatedAuthority::motion_close` accepts a
/// `proposal_weight_bound` parameter (2 fields) or only `motion_hash` (1 field).
fn has_motion_close_weight_bound(api: &OnlineClient<SubstrateConfig>) -> bool {
	let metadata = api.metadata();
	let Ok(pallet) = metadata.pallet_by_name_err("FederatedAuthority") else {
		return false;
	};
	let Some(call_ty_id) = pallet.call_ty_id() else {
		return false;
	};
	let Some(ty) = metadata.types().resolve(call_ty_id) else {
		return false;
	};
	let scale_info::TypeDef::Variant(variant) = &ty.type_def else {
		return false;
	};
	variant
		.variants
		.iter()
		.find(|v| v.name == "motion_close")
		.is_some_and(|v| v.fields.len() > 1)
}
