use std::str::FromStr;

use bip39::Mnemonic;
use clap::Args;
use subxt::{
	OnlineClient, SubstrateConfig,
	dynamic::{self, Value},
	tx::Payload,
	utils::H256,
};
use subxt_signer::{SecretUri, SecretUriError, sr25519::Keypair};
use thiserror::Error;

use midnight_node_ledger_helpers::{
	deserialize,
	mn_ledger::structure::{LedgerParameters, SystemTransaction},
	serialize,
};

pub fn get_signer(key_str: &str) -> Result<Keypair, LedgerParametersError> {
	// Supports seed phrases
	if key_str.contains('/') {
		let uri = SecretUri::from_str(key_str)?;
		Ok(Keypair::from_uri(&uri)?)
	} else {
		let phrase = Mnemonic::parse(key_str)?;
		Ok(Keypair::from_phrase(&phrase, None)?)
	}
}

#[derive(Error, Debug)]
pub enum LedgerParametersError {
	#[error("Secret URI parse error: {0}")]
	UriParseFailed(#[from] SecretUriError),
	#[error("Subxt signer error: {0}")]
	SubxtSignerError(#[from] subxt_signer::sr25519::Error),
	#[error("Subxt error: {0}")]
	SubxtError(#[from] subxt::Error),
	#[error("BIP error: {0}")]
	BipError(#[from] bip39::Error),
	#[error("serialization error: {0}")]
	SerializationError(std::io::Error),
	#[error("Parameters update failed: Missing code updated event")]
	ParametersUpdateFailed,
	#[error("Proposal index not found in events")]
	ProposalIndexNotFound,
	#[error("Encoding error: {0}")]
	EncodingError(String),
	#[error("Failed to decode ledger parameters: {0}")]
	DecodeLedgerParameters(Box<dyn std::error::Error + Send + Sync>),
	#[error("Failed to deserialize ledger parameters: {0}")]
	DeserializeLedgerParameters(Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Args, Clone)]
pub struct UpdateLedgerParametersArgs {
	/// The new serialized ledger parameters
	#[arg(long, env)]
	parameters: String,

	/// Seed for applying the authorized update (can be any authority member)
	#[arg(short, long, env, default_value = "//Alice")]
	signer_key: String,

	/// RPC URL for sending the update
	#[arg(short, long, default_value = "ws://localhost:9944", env)]
	rpc_url: String,
}

pub async fn execute(args: UpdateLedgerParametersArgs) -> Result<(), LedgerParametersError> {
	let signer = get_signer(&args.signer_key)?;
	let bytes = hex::decode(&args.parameters.replace("0x", ""))
		.map_err(|e| LedgerParametersError::DecodeLedgerParameters(e.into()))?;
	let parameters: LedgerParameters = deserialize(&mut &bytes[..])
		.map_err(|e| LedgerParametersError::DeserializeLedgerParameters(e.into()))?;

	println!("Ledger params loaded: {:#?}", parameters);

	println!("Executing ledger parameters update via federated authority.");

	// Create a new API client
	let api = OnlineClient::<SubstrateConfig>::from_insecure_url(args.rpc_url).await?;

	// Authority member keypairs
	// Technical Committee members: Alice, Bob, Charlie
	let alice = Keypair::from_uri(&SecretUri::from_str("//Alice")?)?;
	let bob = Keypair::from_uri(&SecretUri::from_str("//Bob")?)?;
	let _charlie = Keypair::from_uri(&SecretUri::from_str("//Charlie")?)?; // Reserved for optional 3rd vote
	// Council members: Dave, Eve, Ferdie
	let dave = Keypair::from_uri(&SecretUri::from_str("//Dave")?)?;
	let eve = Keypair::from_uri(&SecretUri::from_str("//Eve")?)?;
	let _ferdie = Keypair::from_uri(&SecretUri::from_str("//Ferdie")?)?; // Reserved for optional 3rd vote

	// Step 1: Create the send system transaction call
	let system_transaction = SystemTransaction::OverwriteParameters(parameters.clone());
	let send_system_tx_call = dynamic::tx(
		"MidnightSystem",
		"send_mn_system_transaction",
		vec![serialize(&system_transaction).map_err(LedgerParametersError::SerializationError)?],
	);
	let send_system_tx_call_value = send_system_tx_call.clone().into_value();

	// Step 2: Wrap it in FederatedAuthority::motion_approve
	let fed_auth_call = dynamic::tx(
		"FederatedAuthority",
		"motion_approve",
		vec![send_system_tx_call_value.clone()],
	)
	.into_value();

	// Step 3: Council proposes to approve the federated motion
	println!("Council proposing federated motion approval...");

	// Compute the proposal hash ourselves (same way the collective pallet does)
	// We need to encode the full call data including pallet and call indices
	let fed_auth_tx = dynamic::tx(
		"FederatedAuthority",
		"motion_approve",
		vec![send_system_tx_call_value.clone()],
	);
	let fed_auth_call_data = fed_auth_tx.encode_call_data(&api.metadata()).map_err(|e| {
		LedgerParametersError::EncodingError(format!("Failed to encode call: {:?}", e))
	})?;
	let council_proposal_hash = sp_crypto_hashing::blake2_256(&fed_auth_call_data);
	let council_proposal_hash = H256(council_proposal_hash);

	let council_proposal = dynamic::tx(
		"Council",
		"propose",
		vec![Value::u128(2), fed_auth_call.clone(), Value::u128(10000)],
	);

	let council_propose_events = api
		.tx()
		.sign_and_submit_then_watch_default(&council_proposal, &dave)
		.await?
		.wait_for_finalized_success()
		.await?;

	// Extract proposal index from the Proposed event
	let council_proposal_index = extract_proposal_index(&council_propose_events, "Council")?;
	println!(
		"Council proposal created with hash: 0x{} and index: {}",
		hex::encode(council_proposal_hash.0),
		council_proposal_index
	);

	// Step 4: Council members vote (need 2 out of 3: Alice and Bob)
	println!("Council members voting...");
	vote_on_proposal(&api, &dave, "Council", council_proposal_hash, council_proposal_index, true)
		.await?;
	vote_on_proposal(&api, &eve, "Council", council_proposal_hash, council_proposal_index, true)
		.await?;
	// Charlie doesn't need to vote since we already have 2/3

	// Step 5: Close Council proposal
	println!("Closing Council proposal...");
	close_proposal(&api, &dave, "Council", council_proposal_hash, council_proposal_index).await?;

	// Step 6: Technical Committee proposes to approve the federated motion
	println!("Technical Committee proposing federated motion approval...");

	let tech_proposal_hash = council_proposal_hash;

	let tech_proposal = dynamic::tx(
		"TechnicalCommittee",
		"propose",
		vec![Value::u128(2), fed_auth_call, Value::u128(10000)],
	);

	let tech_propose_events = api
		.tx()
		.sign_and_submit_then_watch_default(&tech_proposal, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;

	let tech_proposal_index = extract_proposal_index(&tech_propose_events, "TechnicalCommittee")?;
	println!(
		"Technical Committee proposal created with hash: 0x{} and index: {}",
		hex::encode(tech_proposal_hash.0),
		tech_proposal_index
	);

	// Step 7: Technical Committee members vote (need 2 out of 3: Dave and Eve)
	println!("Technical Committee members voting...");
	vote_on_proposal(
		&api,
		&alice,
		"TechnicalCommittee",
		tech_proposal_hash,
		tech_proposal_index,
		true,
	)
	.await?;
	vote_on_proposal(
		&api,
		&bob,
		"TechnicalCommittee",
		tech_proposal_hash,
		tech_proposal_index,
		true,
	)
	.await?;
	// Ferdie doesn't need to vote since we already have 2/3

	// Step 8: Close Technical Committee proposal
	println!("Closing Technical Committee proposal...");
	close_proposal(&api, &alice, "TechnicalCommittee", tech_proposal_hash, tech_proposal_index)
		.await?;

	println!("Federated authority motion approved by both councils!");

	// Step 9: Compute the motion hash for the send_system_tx call
	// The motion hash is computed by hashing the call data
	let call_data = send_system_tx_call
		.encode_call_data(&api.metadata())
		.map_err(|e| LedgerParametersError::EncodingError(format!("{:?}", e)))?;

	let motion_hash = sp_crypto_hashing::blake2_256(&call_data);
	let motion_hash = H256(motion_hash);
	println!("Motion hash: 0x{}", hex::encode(motion_hash.0));

	// Step 10: Close the federated motion to execute send_system_tx with Root origin
	println!("Closing federated motion to execute send_system_tx...");
	let close_motion_call =
		dynamic::tx("FederatedAuthority", "motion_close", vec![Value::from_bytes(&motion_hash.0)]);

	let events = api
		.tx()
		.sign_and_submit_then_watch_default(&close_motion_call, &signer)
		.await?
		.wait_for_finalized_success()
		.await?;

	println!("Federated motion closed, send_system_tx executed with Root origin!");

	// Verify the parameres update was successful
	let mut success = false;
	for event in events.iter() {
		let event = event?;
		if event.pallet_name() == "MidnightSystem"
			&& event.variant_name() == "SystemTransactionApplied"
		{
			println!("MidnightSystem::SystemTransactionApplied");
			success = true;
			break;
		}
	}
	if !success {
		return Err(LedgerParametersError::ParametersUpdateFailed);
	}

	println!("Parameters got successfully updated!");
	Ok(())
}

async fn vote_on_proposal(
	api: &OnlineClient<SubstrateConfig>,
	signer: &Keypair,
	pallet: &str,
	proposal_hash: H256,
	proposal_index: u32,
	approve: bool,
) -> Result<(), LedgerParametersError> {
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
) -> Result<(), LedgerParametersError> {
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
) -> Result<u32, LedgerParametersError> {
	use parity_scale_codec::Decode;

	for event in events.iter() {
		let event = event?;
		if event.pallet_name() == pallet && event.variant_name() == "Proposed" {
			// Get the raw field bytes
			let field_bytes = event.field_bytes();

			// Parse the raw bytes manually
			// The Proposed event has: (account_id: 32 bytes, proposal_index: compact u32, ...)
			let mut cursor = field_bytes;

			// Skip account_id (32 bytes)
			if cursor.len() < 32 {
				continue;
			}
			cursor = &cursor[32..];

			// Read proposal_index (compact encoded u32)
			if let Ok(parity_scale_codec::Compact(index)) =
				parity_scale_codec::Compact::<u32>::decode(&mut cursor)
			{
				return Ok(index);
			}
		}
	}
	Err(LedgerParametersError::ProposalIndexNotFound)
}
