use clap::Args;
use subxt::{
	OnlineClient, SubstrateConfig,
	dynamic::{self, Value},
	tx::Payload,
	utils::H256,
};
use thiserror::Error;

use midnight_node_ledger_helpers::{
	Duration, FeePrices, FixedPoint, Keypair, deserialize,
	mn_ledger::structure::{LedgerParameters, SystemTransaction},
	serialize,
};
use midnight_node_metadata::midnight_metadata_latest as mn_meta;
use midnight_node_toolkit::cli_parsers::{self as cli};

#[derive(Error, Debug)]
pub enum LedgerParametersError {
	#[error("Subxt error: {0}")]
	SubxtError(#[from] subxt::Error),
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
	/// The new serialized ledger parameters. If not provided, the default parameters will be fetched from the server.
	#[arg(long, env)]
	parameters: Option<String>,

	/// Seed for applying the authorized update (can be any authority member).
	#[arg(short, long, env, default_value = "//Alice", value_parser = cli::keypair_from_str)]
	signer_key: Keypair,

	/// RPC URL for sending the update.
	#[arg(short, long, default_value = "ws://localhost:9944", env)]
	rpc_url: String,

	/// Technical committee members.
	#[arg(short, long, env, required = true, value_parser = cli::keypair_from_str)]
	technical_committee_members: Vec<Keypair>,

	/// Council members.
	#[arg(short, long, env, required = true, value_parser = cli::keypair_from_str)]
	council_members: Vec<Keypair>,

	/// Ledger's `read_price_a` parameter, used in FixedPoint::from_u64_div(read_price_a, read_price_b).
	#[arg(long)]
	read_price_a: Option<u64>,

	/// Ledger's `read_price_b` parameter, used in FixedPoint::from_u64_div(read_price_a, read_price_b).
	#[arg(long)]
	read_price_b: Option<u64>,

	/// Ledger's `compute_price_a` parameter, used in FixedPoint::from_u64_div(compute_price_a, compute_price_b).
	#[arg(long)]
	compute_price_a: Option<u64>,

	/// Ledger's `compute_price_b` parameter, used in FixedPoint::from_u64_div(compute_price_a, compute_price_b).
	#[arg(long)]
	compute_price_b: Option<u64>,

	/// Ledger's `block_usage_price_a` parameter, used in FixedPoint::from_u64_div(block_usage_price_a, block_usage_price_b).
	#[arg(long)]
	block_usage_price_a: Option<u64>,

	/// Ledger's `block_usage_price_b` parameter, used in FixedPoint::from_u64_div(block_usage_price_a, block_usage_price_b).
	#[arg(long)]
	block_usage_price_b: Option<u64>,

	/// Ledger's `write_price_a` parameter, used in FixedPoint::from_u64_div(write_price_a, write_price_b).
	#[arg(long)]
	write_price_a: Option<u64>,

	/// Ledger's `write_price_b` parameter, used as FixedPoint::from_u64_div(write_price_a, write_price_b).
	#[arg(long)]
	write_price_b: Option<u64>,

	/// Ledger's `global_ttl` parameter.
	#[arg(long)]
	global_ttl: Option<i128>,

	/// Ledger's `cardano_to_midnight_bridge_fee_basis_points` parameter.
	#[arg(long)]
	cardano_to_midnight_bridge_fee_basis_points: Option<u32>,

	/// Ledger's `cost_dimension_min_ratio_a` parameter, used as FixedPoint::from_u64_div(cost_dimension_min_ratio_a, cost_dimension_min_ratio_b).
	#[arg(long)]
	cost_dimension_min_ratio_a: Option<u64>,

	/// Ledger's `cost_dimension_min_ratio_b` parameter, used as FixedPoint::from_u64_div(cost_dimension_min_ratio_a, cost_dimension_min_ratio_b).
	#[arg(long)]
	cost_dimension_min_ratio_b: Option<u64>,

	/// Ledger's `price_adjustment_a_parameter_a` parameter, used as FixedPoint::from_u64_div(price_adjustment_a_parameter_a, price_adjustment_a_parameter_b).
	#[arg(long)]
	price_adjustment_a_parameter_a: Option<u64>,

	/// Ledger's `price_adjustment_a_parameter_b` parameter, used as FixedPoint::from_u64_div(price_adjustment_a_parameter_a, price_adjustment_a_parameter_b).
	#[arg(long)]
	price_adjustment_a_parameter_b: Option<u64>,

	/// Ledger's `c_to_m_bridge_min_amount` parameter.
	#[arg(long)]
	c_to_m_bridge_min_amount: Option<u128>,
}

pub async fn execute(args: UpdateLedgerParametersArgs) -> Result<(), LedgerParametersError> {
	// Create a new API client
	let api = OnlineClient::<SubstrateConfig>::from_insecure_url(args.rpc_url).await?;

	let signer = args.signer_key;
	let bytes = match args.parameters {
		Some(parameters) => hex::decode(&parameters.replace("0x", ""))
			.map_err(|e| LedgerParametersError::DecodeLedgerParameters(e.into()))?,
		None => {
			let call = mn_meta::apis().midnight_runtime_api().get_ledger_parameters();
			api.runtime_api()
				.at_latest()
				.await?
				.call(call)
				.await?
				.expect("not possible to retrieve ledger parameters from RPC server")
		},
	};

	let base: LedgerParameters = deserialize(&mut &bytes[..])
		.map_err(|e| LedgerParametersError::DeserializeLedgerParameters(e.into()))?;

	let parameters = LedgerParameters {
		fee_prices: FeePrices {
			read_price: match (args.read_price_a, args.read_price_b) {
				(Some(read_price_a), Some(read_price_b)) => {
					FixedPoint::from_u64_div(read_price_a, read_price_b)
				},
				_ => base.fee_prices.read_price,
			},
			compute_price: match (args.compute_price_a, args.compute_price_b) {
				(Some(compute_price_a), Some(compute_price_b)) => {
					FixedPoint::from_u64_div(compute_price_a, compute_price_b)
				},
				_ => base.fee_prices.compute_price,
			},
			block_usage_price: match (args.block_usage_price_a, args.block_usage_price_b) {
				(Some(block_usage_price_a), Some(block_usage_price_b)) => {
					FixedPoint::from_u64_div(block_usage_price_a, block_usage_price_b)
				},
				_ => base.fee_prices.block_usage_price,
			},
			write_price: match (args.write_price_a, args.write_price_b) {
				(Some(write_price_a), Some(write_price_b)) => {
					FixedPoint::from_u64_div(write_price_a, write_price_b)
				},
				_ => base.fee_prices.write_price,
			},
		},
		global_ttl: args
			.global_ttl
			.map(|global_ttl| Duration::from_secs(global_ttl))
			.unwrap_or(base.global_ttl),
		cardano_to_midnight_bridge_fee_basis_points: args
			.cardano_to_midnight_bridge_fee_basis_points
			.unwrap_or(base.cardano_to_midnight_bridge_fee_basis_points),
		cost_dimension_min_ratio: match (
			args.cost_dimension_min_ratio_a,
			args.cost_dimension_min_ratio_b,
		) {
			(Some(cost_dimension_min_ratio_a), Some(cost_dimension_min_ratio_b)) => {
				FixedPoint::from_u64_div(cost_dimension_min_ratio_a, cost_dimension_min_ratio_b)
			},
			_ => base.cost_dimension_min_ratio,
		},
		price_adjustment_a_parameter: match (
			args.price_adjustment_a_parameter_a,
			args.price_adjustment_a_parameter_b,
		) {
			(Some(price_adjustment_a_parameter_a), Some(price_adjustment_a_parameter_b)) => {
				FixedPoint::from_u64_div(
					price_adjustment_a_parameter_a,
					price_adjustment_a_parameter_b,
				)
			},
			_ => base.price_adjustment_a_parameter,
		},
		c_to_m_bridge_min_amount: args
			.c_to_m_bridge_min_amount
			.unwrap_or(base.c_to_m_bridge_min_amount),
		..base
	};

	println!("Ledger params loaded: {:#?}", parameters);

	println!("Executing ledger parameters update via federated authority.");

	// Authority member keypairs
	let tc_member_1 = args.technical_committee_members[0].clone();
	let tc_other_members = args.technical_committee_members[1..].to_vec();
	// Council members
	let council_member_1 = args.council_members[0].clone();
	let council_other_members = args.council_members[1..].to_vec();

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
		.sign_and_submit_then_watch_default(&council_proposal, &council_member_1.0)
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

	// Step 4: Council members vote
	println!("Council members voting...");
	vote_on_proposal(
		&api,
		&council_member_1,
		"Council",
		council_proposal_hash,
		council_proposal_index,
		true,
	)
	.await?;
	for council_member in council_other_members {
		vote_on_proposal(
			&api,
			&council_member,
			"Council",
			council_proposal_hash,
			council_proposal_index,
			true,
		)
		.await?;
	}

	// Step 5: Close Council proposal
	println!("Closing Council proposal...");
	close_proposal(
		&api,
		&council_member_1,
		"Council",
		council_proposal_hash,
		council_proposal_index,
	)
	.await?;

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
		.sign_and_submit_then_watch_default(&tech_proposal, &tc_member_1.0)
		.await?
		.wait_for_finalized_success()
		.await?;

	let tech_proposal_index = extract_proposal_index(&tech_propose_events, "TechnicalCommittee")?;
	println!(
		"Technical Committee proposal created with hash: 0x{} and index: {}",
		hex::encode(tech_proposal_hash.0),
		tech_proposal_index
	);

	// Step 7: Technical Committee members vote
	println!("Technical Committee members voting...");
	vote_on_proposal(
		&api,
		&tc_member_1,
		"TechnicalCommittee",
		tech_proposal_hash,
		tech_proposal_index,
		true,
	)
	.await?;
	for tc_member in tc_other_members {
		vote_on_proposal(
			&api,
			&tc_member,
			"TechnicalCommittee",
			tech_proposal_hash,
			tech_proposal_index,
			true,
		)
		.await?;
	}

	// Step 8: Close Technical Committee proposal
	println!("Closing Technical Committee proposal...");
	close_proposal(
		&api,
		&tc_member_1,
		"TechnicalCommittee",
		tech_proposal_hash,
		tech_proposal_index,
	)
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
		.sign_and_submit_then_watch_default(&close_motion_call, &signer.0)
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
		.sign_and_submit_then_watch_default(&vote_call, &signer.0)
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
		.sign_and_submit_then_watch_default(&close_call, &signer.0)
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
