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

//! Rescaling a block's ledger capacity by `tx_weight_factor_permille`.
//!
//! `system-parameters-config.json` carries one factor that decides how many ledger transactions
//! a block should hold relative to the ledger's own limits, and it is applied in two places at
//! genesis-build time:
//!
//! * here, dividing `LedgerParameters::limits::block_limits` — which is what the *ledger* checks
//!   accrued block fullness against, and also what the runtime normalises a transaction's cost
//!   against when it computes that transaction's weight. Widening the limits therefore widens
//!   ledger capacity and narrows the ledger-derived part of the weight by the same ratio, which
//!   is what keeps FRAME's view of a full block and the ledger's view in step;
//! * in `pallet_midnight`, on the flat per-transaction weight, which is the one term that does
//!   not follow the block limits.
//!
//! `generate-genesis` applies this when it builds the genesis state, and genesis verification
//! applies it to the config file before comparing against that state — same function, so the two
//! cannot disagree.

use crate::mn_ledger::structure::{LedgerParameters, TransactionLimits};
use midnight_primitives_system_parameters::TX_WEIGHT_FACTOR_ONE;

/// Error returned when the factor cannot be applied.
#[derive(Debug, thiserror::Error)]
pub enum BlockCapacityError {
	#[error("tx_weight_factor_permille must be greater than 0")]
	ZeroFactor,
}

/// Divides `params.limits.block_limits` by `tx_weight_factor_permille / 1000`.
///
/// A factor of `500` ("half the weight per transaction") therefore doubles every block-limit
/// dimension, giving the block roughly twice the ledger capacity. [`TX_WEIGHT_FACTOR_ONE`]
/// returns `params` untouched.
pub fn scale_block_limits(
	params: LedgerParameters,
	tx_weight_factor_permille: u32,
) -> Result<LedgerParameters, BlockCapacityError> {
	if tx_weight_factor_permille == TX_WEIGHT_FACTOR_ONE {
		return Ok(params);
	}
	if tx_weight_factor_permille == 0 {
		return Err(BlockCapacityError::ZeroFactor);
	}

	let scale = f64::from(TX_WEIGHT_FACTOR_ONE) / f64::from(tx_weight_factor_permille);
	let block_limits = params.limits.block_limits * scale;

	Ok(LedgerParameters { limits: TransactionLimits { block_limits, ..params.limits }, ..params })
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mn_ledger::structure::INITIAL_PARAMETERS;

	fn limits(permille: u32) -> crate::SyntheticCost {
		scale_block_limits(INITIAL_PARAMETERS, permille).unwrap().limits.block_limits
	}

	#[test]
	fn one_is_a_no_op() {
		assert_eq!(limits(TX_WEIGHT_FACTOR_ONE), INITIAL_PARAMETERS.limits.block_limits);
	}

	#[test]
	fn half_the_weight_doubles_every_dimension() {
		let base = INITIAL_PARAMETERS.limits.block_limits;
		let doubled = limits(500);

		assert_eq!(doubled.compute_time, base.compute_time * 2.0);
		assert_eq!(doubled.read_time, base.read_time * 2.0);
		assert_eq!(doubled.block_usage, base.block_usage * 2);
		assert_eq!(doubled.bytes_written, base.bytes_written * 2);
		assert_eq!(doubled.bytes_churned, base.bytes_churned * 2);
	}

	#[test]
	fn double_the_weight_halves_every_dimension() {
		let base = INITIAL_PARAMETERS.limits.block_limits;
		let halved = limits(2000);

		assert_eq!(halved.compute_time, base.compute_time * 0.5);
		assert_eq!(halved.block_usage, base.block_usage / 2);
	}

	#[test]
	fn zero_is_rejected() {
		assert!(matches!(
			scale_block_limits(INITIAL_PARAMETERS, 0),
			Err(BlockCapacityError::ZeroFactor)
		));
	}
}
