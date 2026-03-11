use clap::Args;
use subxt::{OnlineClient, SubstrateConfig, dynamic, tx::Payload};
use thiserror::Error;

use crate::commands::root_call::{self, RootCallArgs};
use midnight_node_ledger_helpers::{
	CostDuration, Duration, DustParameters, FeePrices, FixedPoint, SyntheticCost, deserialize,
	mn_ledger::structure::{LedgerParameters, SystemTransaction, TransactionLimits},
	serialize,
};
use midnight_node_metadata::midnight_metadata_latest as mn_meta;

#[derive(Error, Debug)]
pub enum LedgerParametersError {
	#[error("Subxt error: {0}")]
	SubxtError(#[from] subxt::Error),
	#[error("Subxt core error: {0}")]
	SubxtCoreError(#[from] subxt::ext::subxt_core::Error),
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
	#[error("error executing root call: {0}")]
	RootCallError(Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Args, Clone)]
pub struct UpdateableParams {
	/// Block limit: The time spent in IO reads (picoseconds)
	#[arg(long)]
	block_limit_read_time: Option<u64>,

	/// Block limit: The time spent in single-threaded compute (picoseconds)
	#[arg(long)]
	block_limit_compute_time: Option<u64>,

	/// Block limit: The bytes used of block size capacity
	#[arg(long)]
	block_limit_block_usage: Option<u64>,

	/// Block limit: The bytes written persistently to disk.
	/// Unlike in [`RunningCost`], this represents net bytes written, defined for `r: RunningCost`
	/// as `max(0, r.bytes_written - r.bytes_deleted)`
	#[arg(long)]
	block_limit_bytes_written: Option<u64>,

	/// Block limit: The bytes written temporarily or overwritten
	#[arg(long)]
	block_limit_bytes_churned: Option<u64>,

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

	/// Ledger's `dust.dust_grace_period` parameter (in seconds).
	#[arg(long)]
	dust_grace_period: Option<u64>,
}

#[derive(Args, Clone)]
pub struct UpdateLedgerParametersArgs {
	/// The new serialized ledger parameters. If not provided, the default parameters will be fetched from the server.
	#[arg(long, env)]
	parameters: Option<String>,

	/// Council member private keys as hex strings (32-byte sr25519 seeds)
	#[arg(short, long, required = true)]
	council_members: Vec<String>,

	/// Technical Committee member private keys as hex strings (32-byte sr25519 seeds)
	#[arg(short, long, required = true)]
	technical_committee_members: Vec<String>,

	/// RPC URL for sending the update.
	#[arg(short, long, default_value = "ws://localhost:9944", env)]
	rpc_url: String,

	#[command(flatten)]
	params: UpdateableParams,
}

pub async fn execute(args: UpdateLedgerParametersArgs) -> Result<(), LedgerParametersError> {
	// Create a new API client
	let api = OnlineClient::<SubstrateConfig>::from_insecure_url(&args.rpc_url).await?;

	let bytes = match args.parameters {
		Some(parameters) => {
			let hex_str = parameters.strip_prefix("0x").unwrap_or(&parameters);
			hex::decode(hex_str)
				.map_err(|e| LedgerParametersError::DecodeLedgerParameters(Box::new(e)))?
		},
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

	let params = &args.params;

	let parameters = LedgerParameters {
		limits: TransactionLimits {
			block_limits: SyntheticCost {
				read_time: params
					.block_limit_read_time
					.map(|t| CostDuration::from_picoseconds(t))
					.unwrap_or(base.limits.block_limits.read_time),
				compute_time: params
					.block_limit_read_time
					.map(|t| CostDuration::from_picoseconds(t))
					.unwrap_or(base.limits.block_limits.compute_time),
				block_usage: params
					.block_limit_read_time
					.unwrap_or(base.limits.block_limits.block_usage),
				bytes_written: params
					.block_limit_read_time
					.unwrap_or(base.limits.block_limits.bytes_written),
				bytes_churned: params
					.block_limit_read_time
					.unwrap_or(base.limits.block_limits.bytes_churned),
			},
			..base.limits
		},
		fee_prices: FeePrices {
			overall_price: base.fee_prices.overall_price,
			read_factor: match (params.read_price_a, params.read_price_b) {
				(Some(read_price_a), Some(read_price_b)) => {
					FixedPoint::from_u64_div(read_price_a, read_price_b)
				},
				_ => base.fee_prices.read_factor,
			},
			compute_factor: match (params.compute_price_a, params.compute_price_b) {
				(Some(compute_price_a), Some(compute_price_b)) => {
					FixedPoint::from_u64_div(compute_price_a, compute_price_b)
				},
				_ => base.fee_prices.compute_factor,
			},
			block_usage_factor: match (params.block_usage_price_a, params.block_usage_price_b) {
				(Some(block_usage_price_a), Some(block_usage_price_b)) => {
					FixedPoint::from_u64_div(block_usage_price_a, block_usage_price_b)
				},
				_ => base.fee_prices.block_usage_factor,
			},
			write_factor: match (params.write_price_a, params.write_price_b) {
				(Some(write_price_a), Some(write_price_b)) => {
					FixedPoint::from_u64_div(write_price_a, write_price_b)
				},
				_ => base.fee_prices.write_factor,
			},
		},
		global_ttl: params
			.global_ttl
			.map(|global_ttl| Duration::from_secs(global_ttl))
			.unwrap_or(base.global_ttl),
		cardano_to_midnight_bridge_fee_basis_points: params
			.cardano_to_midnight_bridge_fee_basis_points
			.unwrap_or(base.cardano_to_midnight_bridge_fee_basis_points),
		cost_dimension_min_ratio: match (
			params.cost_dimension_min_ratio_a,
			params.cost_dimension_min_ratio_b,
		) {
			(Some(cost_dimension_min_ratio_a), Some(cost_dimension_min_ratio_b)) => {
				FixedPoint::from_u64_div(cost_dimension_min_ratio_a, cost_dimension_min_ratio_b)
			},
			_ => base.cost_dimension_min_ratio,
		},
		price_adjustment_a_parameter: match (
			params.price_adjustment_a_parameter_a,
			params.price_adjustment_a_parameter_b,
		) {
			(Some(price_adjustment_a_parameter_a), Some(price_adjustment_a_parameter_b)) => {
				FixedPoint::from_u64_div(
					price_adjustment_a_parameter_a,
					price_adjustment_a_parameter_b,
				)
			},
			_ => base.price_adjustment_a_parameter,
		},
		c_to_m_bridge_min_amount: params
			.c_to_m_bridge_min_amount
			.unwrap_or(base.c_to_m_bridge_min_amount),
		dust: DustParameters {
			dust_grace_period: params
				.dust_grace_period
				.map(|d| Duration::from_secs(d as i128))
				.unwrap_or(base.dust.dust_grace_period),
			..base.dust
		},
		..base
	};

	log::info!("Ledger params loaded: {:#?}", parameters);

	log::info!("Executing ledger parameters update via federated authority.");

	// Step 1: Create the send system transaction call
	let system_transaction = SystemTransaction::OverwriteParameters(parameters.clone());
	let send_system_tx_call = dynamic::tx(
		"MidnightSystem",
		"send_mn_system_transaction",
		vec![serialize(&system_transaction).map_err(LedgerParametersError::SerializationError)?],
	);
	let send_system_tx_call_value =
		send_system_tx_call.clone().encode_call_data(&api.metadata())?;

	root_call::execute(RootCallArgs {
		rpc_url: args.rpc_url,
		council_keys: args.council_members,
		tc_keys: args.technical_committee_members,
		encoded_call: Some(send_system_tx_call_value),
		encoded_call_file: None,
	})
	.await
	.map_err(|e| LedgerParametersError::RootCallError(e))
}
