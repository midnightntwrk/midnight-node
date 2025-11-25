use clap::Args;
use midnight_node_ledger_helpers::base_crypto::time::Duration;
use midnight_node_ledger_helpers::mn_ledger::dust::INITIAL_DUST_PARAMETERS;
use midnight_node_ledger_helpers::mn_ledger::structure::{
	INITIAL_LIMITS, INITIAL_TRANSACTION_COST_MODEL,
};
use midnight_node_ledger_helpers::{FeePrices, FixedPoint, LedgerParameters, serialize};

// TODO: add support for the serialized base params
#[derive(Args, Clone, Debug)]
pub struct ShowLedgerParamsArgs {
	#[arg(long, default_value_t = 10)]
	read_price_a: u64,
	#[arg(long, default_value_t = 1)]
	read_price_b: u64,
	#[arg(long, default_value_t = 10)]
	compute_price_a: u64,
	#[arg(long, default_value_t = 1)]
	compute_price_b: u64,
	#[arg(long, default_value_t = 10)]
	block_usage_price_a: u64,
	#[arg(long, default_value_t = 1)]
	block_usage_price_b: u64,
	#[arg(long, default_value_t = 10)]
	write_price_a: u64,
	#[arg(long, default_value_t = 1)]
	write_price_b: u64,
	#[arg(long, default_value_t = 3600)]
	global_ttl: i128,
	#[arg(long, default_value_t = 500)]
	cardano_to_midnight_bridge_fee_basis_points: u32,
	#[arg(long, default_value_t = 1)]
	cost_dimension_min_ratio_a: u64,
	#[arg(long, default_value_t = 4)]
	cost_dimension_min_ratio_b: u64,
	#[arg(long, default_value_t = 100)]
	price_adjustment_a_parameter_a: u64,
	#[arg(long, default_value_t = 1)]
	price_adjustment_a_parameter_b: u64,
	#[arg(long, default_value_t = 1000)]
	c_to_m_bridge_min_amount: u128,
}

#[derive(Debug)]
pub struct LedgerParametersResult {
	parameters: LedgerParameters,
	serialized: String,
}

pub fn execute(args: ShowLedgerParamsArgs) -> LedgerParametersResult {
	let params = LedgerParameters {
		cost_model: INITIAL_TRANSACTION_COST_MODEL,
		limits: INITIAL_LIMITS,
		dust: INITIAL_DUST_PARAMETERS,
		fee_prices: FeePrices {
			read_price: FixedPoint::from_u64_div(args.read_price_a, args.read_price_b),
			compute_price: FixedPoint::from_u64_div(args.compute_price_a, args.compute_price_b),
			block_usage_price: FixedPoint::from_u64_div(
				args.block_usage_price_a,
				args.block_usage_price_b,
			),
			write_price: FixedPoint::from_u64_div(args.write_price_a, args.write_price_b),
		},
		global_ttl: Duration::from_secs(args.global_ttl),
		cardano_to_midnight_bridge_fee_basis_points: args
			.cardano_to_midnight_bridge_fee_basis_points,
		cost_dimension_min_ratio: FixedPoint::from_u64_div(
			args.cost_dimension_min_ratio_a,
			args.cost_dimension_min_ratio_b,
		),
		price_adjustment_a_parameter: FixedPoint::from_u64_div(
			args.price_adjustment_a_parameter_a,
			args.price_adjustment_a_parameter_b,
		),
		c_to_m_bridge_min_amount: args.c_to_m_bridge_min_amount,
	};
	let serialized =
		hex::encode(serialize(&params).expect("failed to serialize ledger parameters"));
	LedgerParametersResult { parameters: params, serialized }
}

#[cfg(test)]
mod test {
	use super::*;
	use midnight_node_ledger_helpers::mn_ledger::structure::INITIAL_PARAMETERS;

	#[test]
	fn test_ledger_params() {
		let default_params = ShowLedgerParamsArgs {
			read_price_a: 10,
			read_price_b: 1,
			compute_price_a: 10,
			compute_price_b: 1,
			block_usage_price_a: 10,
			block_usage_price_b: 1,
			write_price_a: 10,
			write_price_b: 1,
			global_ttl: 3600,
			cardano_to_midnight_bridge_fee_basis_points: 500,
			cost_dimension_min_ratio_a: 1,
			cost_dimension_min_ratio_b: 4,
			price_adjustment_a_parameter_a: 100,
			price_adjustment_a_parameter_b: 1,
			c_to_m_bridge_min_amount: 1000,
		};
		let result_default_params = execute(default_params.clone());

		let initial_params = INITIAL_PARAMETERS;
		let serialized =
			hex::encode(serialize(&initial_params).expect("failed to serialize ledger parameters"));

		assert_eq!(result_default_params.parameters, initial_params);
		assert_eq!(result_default_params.serialized, serialized);

		let new_params = ShowLedgerParamsArgs { c_to_m_bridge_min_amount: 2000, ..default_params };
		let result_new_params = execute(new_params);
		assert_ne!(result_new_params.parameters, result_default_params.parameters);
		assert_ne!(result_new_params.serialized, serialized);
	}
}
