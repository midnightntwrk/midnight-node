use clap::Args;
use midnight_node_ledger_helpers::base_crypto::time::Duration;
use midnight_node_ledger_helpers::mn_ledger::structure::INITIAL_PARAMETERS;
use midnight_node_ledger_helpers::{
	FeePrices, FixedPoint, LedgerParameters, deserialize, serialize,
};
use midnight_node_metadata::midnight_metadata_latest as mn_meta;
use subxt::{OnlineClient, SubstrateConfig};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LedgerParametersError {
	#[error("Subxt error: {0}")]
	SubxtError(#[from] subxt::Error),
	#[error("failed to decode ledger parameters: {0}")]
	DecodeLedgerParameters(Box<dyn std::error::Error + Send + Sync>),
	#[error("failed to deserialize ledger parameters: {0}")]
	DeserializeLedgerParameters(Box<dyn std::error::Error + Send + Sync>),
	#[error("failed to serialize ledger parameters: {0}")]
	SerializeLedgerParameters(Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Args, Clone, Debug, Default)]
pub struct ShowLedgerParametersArgs {
	/// Base serialized ledger parameters, otherwise the default will be used.
	#[arg(short, long)]
	base_parameters: Option<String>,
	/// Set to true to return the serialized parameters only, otherwise the whole structure will be printed.
	#[arg(long, default_value_t = false)]
	pub serialize: bool,
	/// Optional RPC URL for reading the parameters from.
	#[arg(short, long, env)]
	read_from_rpc_url: Option<String>,
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

#[derive(Debug)]
pub struct LedgerParametersResult {
	#[allow(dead_code)]
	pub parameters: LedgerParameters,
	#[allow(dead_code)]
	pub serialized: String,
}

pub async fn execute(
	args: ShowLedgerParametersArgs,
) -> Result<LedgerParametersResult, LedgerParametersError> {
	let base = if let Some(rpc_url) = args.read_from_rpc_url {
		let api = OnlineClient::<SubstrateConfig>::from_insecure_url(rpc_url).await?;
		let call = mn_meta::apis().midnight_runtime_api().get_ledger_parameters();
		let response = api.runtime_api().at_latest().await?.call(call).await?;
		let bytes = response.expect("not possible to retrieve ledger parameters from RPC server");
		let parameters: LedgerParameters = deserialize(&mut &bytes[..])
			.map_err(|e| LedgerParametersError::DeserializeLedgerParameters(e.into()))?;
		parameters
	} else {
		match args.base_parameters {
			Some(serialized_parameters) => {
				let bytes = hex::decode(&serialized_parameters.replace("0x", ""))
					.map_err(|e| LedgerParametersError::DecodeLedgerParameters(e.into()))?;
				let parameters: LedgerParameters = deserialize(&mut &bytes[..])
					.map_err(|e| LedgerParametersError::DeserializeLedgerParameters(e.into()))?;
				parameters
			},
			_ => INITIAL_PARAMETERS,
		}
	};

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
	let serialized = hex::encode(
		serialize(&parameters)
			.map_err(|e| LedgerParametersError::SerializeLedgerParameters(e.into()))?,
	);

	Ok(LedgerParametersResult { parameters, serialized })
}

#[cfg(test)]
mod test {
	use super::*;

	#[tokio::test]
	async fn test_ledger_default_params() {
		let default_params = ShowLedgerParametersArgs::default();
		let result = execute(default_params.clone()).await.expect("failed to execute command");

		let initial_params = INITIAL_PARAMETERS;
		let serialized =
			hex::encode(serialize(&initial_params).expect("failed to serialize ledger parameters"));

		assert_eq!(result.parameters, initial_params);
		assert_eq!(result.serialized, serialized);
	}

	#[tokio::test]
	async fn test_ledger_params_override() {
		let initial_params = INITIAL_PARAMETERS;
		let initial_params_serialized =
			hex::encode(serialize(&initial_params).expect("failed to serialize ledger parameters"));

		let new_params = ShowLedgerParametersArgs {
			c_to_m_bridge_min_amount: Some(2000),
			..ShowLedgerParametersArgs::default()
		};
		let result_new_params = execute(new_params).await.expect("failed to execute command");
		assert_eq!(result_new_params.parameters.c_to_m_bridge_min_amount, 2000);
		assert_ne!(result_new_params.parameters, initial_params);
		assert_ne!(result_new_params.serialized, initial_params_serialized);
	}

	#[tokio::test]
	async fn test_base_ledger_params() {
		let params = LedgerParameters { c_to_m_bridge_min_amount: 2000, ..INITIAL_PARAMETERS };
		let base_parameters =
			hex::encode(serialize(&params).expect("failed to serialize ledger parameters"));

		let new_params = ShowLedgerParametersArgs {
			cardano_to_midnight_bridge_fee_basis_points: Some(600),
			base_parameters: Some(base_parameters),
			..ShowLedgerParametersArgs::default()
		};
		let result_new_params = execute(new_params).await.expect("failed to execute command");
		assert_eq!(result_new_params.parameters.cardano_to_midnight_bridge_fee_basis_points, 600);
		assert_eq!(result_new_params.parameters.c_to_m_bridge_min_amount, 2000);
	}
}
