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

//! What one rewarded block draws from the reserve and how the draw divides.
//!
//! ```text
//! N_b = floor(B_o × R)                          base; the remainder stays in the reserve
//! N_f = floor(N_b × fixed_subsidy)
//! N_v = floor((N_b − N_f) × used / max)         used / max is the block's utilization
//! N_a = N_f + N_v                               producer
//! N_t = N_b − N_a                               treasury, including both floors' remainders
//! ```

use crate::Ratio;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_arithmetic::{Rounding, helpers_128bit::multiply_by_rational_with_rounding};

/// The share of the block's capacity that was used: `used / max`.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
	Default,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct Utilization {
	pub used: u128,
	pub max: u128,
}

/// One block's draw, in STAR. `producer + treasury == base`.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
	Default,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct BlockDraw {
	/// `N_b`: taken from the outstanding reserve `B_o`.
	pub base: u128,
	/// `N_a`: the producer's part, added to its pool's bucket.
	pub producer: u128,
	/// `N_t`: the rest, added to the epoch's Treasury amount.
	pub treasury: u128,
}

/// Parameters or a utilization the draw cannot use.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DrawError {
	ZeroDenominator,
	UtilizationAboveMax,
	/// `R > 1`.
	DrawAboveReserve,
	/// Subsidy `> 1`.
	SubsidyAboveDraw,
}

/// `floor(a × n / d)` for `n ≤ d`, with a 256-bit intermediate.
fn mul_floor(a: u128, ratio: Ratio) -> u128 {
	multiply_by_rational_with_rounding(a, ratio.n, ratio.d, Rounding::Down)
		.expect("d is non-zero and the quotient is at most a; checked by the caller")
}

/// The draw of one rewarded block, from the outstanding reserve before the block.
pub fn block_draw(
	reserve_outstanding: u128,
	rate: Ratio,
	subsidy: Ratio,
	utilization: Utilization,
) -> Result<BlockDraw, DrawError> {
	if rate.d == 0 || subsidy.d == 0 || utilization.max == 0 {
		return Err(DrawError::ZeroDenominator);
	}
	if !rate.is_at_most_one() {
		return Err(DrawError::DrawAboveReserve);
	}
	if !subsidy.is_at_most_one() {
		return Err(DrawError::SubsidyAboveDraw);
	}
	if utilization.used > utilization.max {
		return Err(DrawError::UtilizationAboveMax);
	}
	let base = mul_floor(reserve_outstanding, rate);
	let fixed = mul_floor(base, subsidy);
	let variable = mul_floor(base - fixed, Ratio::new(utilization.used, utilization.max));
	let producer = fixed + variable;
	Ok(BlockDraw { base, producer, treasury: base - producer })
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::parameters::tests::recommended;

	fn draw(reserve: u128, used: u128, max: u128) -> Result<BlockDraw, DrawError> {
		let params = recommended();
		block_draw(
			reserve,
			params.distribution_rate,
			params.fixed_subsidy,
			Utilization { used, max },
		)
	}

	const RESERVE: u128 = 6_000_000_000_873_988;

	#[test]
	fn first_block_after_activation_with_the_recommended_parameters() {
		// N_b = floor(6 000 000 000 873 988 × 7 / 438 000 000), N_f = floor(N_b × 95 / 100).
		assert_eq!(
			draw(RESERVE, 0, 1),
			Ok(BlockDraw { base: 95_890_410, producer: 91_095_889, treasury: 4_794_521 })
		);
		assert_eq!(
			draw(RESERVE, 1, 2),
			Ok(BlockDraw { base: 95_890_410, producer: 93_493_149, treasury: 2_397_261 })
		);
		assert_eq!(
			draw(RESERVE, 1, 1),
			Ok(BlockDraw { base: 95_890_410, producer: 95_890_410, treasury: 0 })
		);
	}

	#[test]
	fn the_draw_always_splits_exactly_and_never_exceeds_the_reserve() {
		for reserve in [0, 1, 62_571, 438_000_000, RESERVE, u128::MAX] {
			for (used, max) in
				[(0, 1), (1, 3), (2, 3), (999, 1000), (7, 7), (u128::MAX - 1, u128::MAX)]
			{
				let d = draw(reserve, used, max).unwrap();
				assert_eq!(d.producer + d.treasury, d.base);
				assert!(d.base <= reserve);
				assert!(d.producer >= mul_floor(d.base, Ratio::new(95, 100)));
			}
		}
	}

	#[test]
	fn a_rate_of_one_draws_the_whole_reserve_without_overflow() {
		let one = Ratio::new(1, 1);
		let full = Utilization { used: 1, max: 1 };
		assert_eq!(
			block_draw(u128::MAX, one, one, full),
			Ok(BlockDraw { base: u128::MAX, producer: u128::MAX, treasury: 0 })
		);
	}

	#[test]
	fn invalid_inputs_are_errors() {
		let params = recommended();
		let (rate, subsidy) = (params.distribution_rate, params.fixed_subsidy);
		let full = Utilization { used: 1, max: 1 };
		let cases = [
			(block_draw(RESERVE, Ratio::new(1, 0), subsidy, full), DrawError::ZeroDenominator),
			(block_draw(RESERVE, rate, Ratio::new(1, 0), full), DrawError::ZeroDenominator),
			(
				block_draw(RESERVE, rate, subsidy, Utilization { used: 0, max: 0 }),
				DrawError::ZeroDenominator,
			),
			(block_draw(RESERVE, Ratio::new(2, 1), subsidy, full), DrawError::DrawAboveReserve),
			(block_draw(RESERVE, rate, Ratio::new(101, 100), full), DrawError::SubsidyAboveDraw),
			(
				block_draw(RESERVE, rate, subsidy, Utilization { used: 2, max: 1 }),
				DrawError::UtilizationAboveMax,
			),
		];
		for (result, error) in cases {
			assert_eq!(result, Err(error));
		}
	}
}
