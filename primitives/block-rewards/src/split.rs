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

//! How one pool's producer total for a Cardano epoch divides between its operator and payees.
//!
//! ```text
//! fixed              = min(P_pool, fixed_cost)
//! operator_cut       = fixed + floor((P_pool − fixed) × m_n / m_d)
//! shared             = P_pool − operator_cut
//! share_i            = floor(shared × s_i / s)
//! share_operator     = floor(shared × s_operator / s)
//! rounding_remainder = shared − share_operator − Σ share_i
//! operator           = operator_cut + share_operator + rounding_remainder
//! ```
//!
//! `s = s_operator + Σ s_i`. The payees and their stakes come from the epoch's snapshot record:
//! delegator rows that name the pool, except the rows of the pool's owners, whose lovelace is
//! `s_operator`, and lock rows, whose stake is their cNIGHT weighted by the record's
//! `cnight_weight`.

use crate::Ratio;
use alloc::vec::Vec;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_arithmetic::{Rounding, helpers_128bit::multiply_by_rational_with_rounding};

/// What an operator declared for the epoch, as the snapshot record carries it.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct PoolTerms {
	/// STAR per Cardano epoch, taken before the margin.
	pub fixed_cost: u128,
	/// From the pool's Cardano registration certificate; at most one.
	pub margin: Ratio,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SplitError {
	ZeroDenominator,
	MarginAboveOne,
	/// The pool's total stake `s` is zero.
	ZeroStake,
	/// A stake is larger than the pool's total stake.
	StakeAboveTotal,
	/// The payees' shares add up to more than `shared`.
	SharesAboveShared,
	Overflow,
}

/// The operator's part of a pool's split, fixed before any payee is paid. Payees' shares come
/// from [`PoolSplit::share`], one at a time, so the payees can be read in pages.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PoolSplit {
	/// `operator_cut`: the fixed cost and the margin, before any stake share.
	pub operator_cut: u128,
	/// `shared = P_pool − operator_cut`, divided by stake.
	pub shared: u128,
	/// `share_operator = floor(shared × s_operator / s)`.
	pub share_operator: u128,
	/// `s`: the pool's total stake.
	pub total_stake: u128,
}

fn mul_floor(a: u128, n: u128, d: u128) -> Result<u128, SplitError> {
	multiply_by_rational_with_rounding(a, n, d, Rounding::Down).ok_or(SplitError::Overflow)
}

impl PoolSplit {
	pub fn new(
		pool_total: u128,
		terms: PoolTerms,
		operator_stake: u128,
		total_stake: u128,
	) -> Result<Self, SplitError> {
		if terms.margin.d == 0 {
			return Err(SplitError::ZeroDenominator);
		}
		if !terms.margin.is_at_most_one() {
			return Err(SplitError::MarginAboveOne);
		}
		if total_stake == 0 {
			return Err(SplitError::ZeroStake);
		}
		if operator_stake > total_stake {
			return Err(SplitError::StakeAboveTotal);
		}
		let fixed = pool_total.min(terms.fixed_cost);
		let operator_cut = fixed + mul_floor(pool_total - fixed, terms.margin.n, terms.margin.d)?;
		let shared = pool_total - operator_cut;
		let share_operator = mul_floor(shared, operator_stake, total_stake)?;
		Ok(Self { operator_cut, shared, share_operator, total_stake })
	}

	/// `share_i` of a payee with stake `s_i`.
	pub fn share(&self, stake: u128) -> Result<u128, SplitError> {
		if stake > self.total_stake {
			return Err(SplitError::StakeAboveTotal);
		}
		mul_floor(self.shared, stake, self.total_stake)
	}

	/// The operator's amount, given `Σ share_i` over every payee of the pool. The rounding
	/// remainder goes to the operator.
	pub fn operator(&self, payee_shares: u128) -> Result<u128, SplitError> {
		let remainder = self
			.shared
			.checked_sub(self.share_operator)
			.and_then(|rest| rest.checked_sub(payee_shares))
			.ok_or(SplitError::SharesAboveShared)?;
		Ok(self.operator_cut + self.share_operator + remainder)
	}
}

/// The whole split of one pool: the operator's amount and each payee's share, in the order of
/// `payee_stakes`. The amounts add up to `pool_total`.
pub fn split_pool(
	pool_total: u128,
	terms: PoolTerms,
	operator_stake: u128,
	payee_stakes: &[u128],
) -> Result<(u128, Vec<u128>), SplitError> {
	let total_stake = payee_stakes
		.iter()
		.try_fold(operator_stake, |sum, stake| sum.checked_add(*stake))
		.ok_or(SplitError::Overflow)?;
	let split = PoolSplit::new(pool_total, terms, operator_stake, total_stake)?;
	let shares = payee_stakes
		.iter()
		.map(|stake| split.share(*stake))
		.collect::<Result<Vec<_>, _>>()?;
	let operator = split.operator(shares.iter().sum())?;
	Ok((operator, shares))
}

#[cfg(test)]
mod tests {
	use super::*;

	fn terms(fixed_cost: u128, n: u128, d: u128) -> PoolTerms {
		PoolTerms { fixed_cost, margin: Ratio::new(n, d) }
	}

	#[test]
	fn fixed_cost_and_margin_come_first_and_the_rest_follows_stake() {
		// fixed 100 000; cut = 100 000 + 900 000 / 10; shared 810 000 over s = 1 000.
		let (operator, shares) =
			split_pool(1_000_000, terms(100_000, 1, 10), 100, &[300, 600]).unwrap();
		assert_eq!(shares, [243_000, 486_000]);
		assert_eq!(operator, 190_000 + 81_000);
	}

	#[test]
	fn the_rounding_remainder_goes_to_the_operator() {
		let (operator, shares) = split_pool(10, terms(0, 0, 1), 0, &[1, 1, 1]).unwrap();
		assert_eq!(shares, [3, 3, 3]);
		assert_eq!(operator, 1);
	}

	#[test]
	fn a_total_at_or_below_the_fixed_cost_goes_to_the_operator() {
		for pool_total in [0, 1, 99, 100] {
			let (operator, shares) = split_pool(pool_total, terms(100, 0, 1), 5, &[5, 90]).unwrap();
			assert_eq!((operator, shares), (pool_total, vec![0, 0]));
		}
	}

	#[test]
	fn a_full_margin_leaves_nothing_to_share() {
		let (operator, shares) = split_pool(1_000, terms(0, 1, 1), 1, &[1_000]).unwrap();
		assert_eq!((operator, shares), (1_000, vec![0]));
	}

	#[test]
	fn a_payee_with_no_stake_gets_nothing() {
		let (operator, shares) = split_pool(1_000, terms(0, 0, 1), 0, &[0, 4]).unwrap();
		assert_eq!((operator, shares), (0, vec![0, 1_000]));
	}

	#[test]
	fn the_amounts_always_add_up_to_the_pool_total() {
		let stakes: [&[u128]; 5] = [
			&[1],
			&[1, 2, 3, 4, 5, 6, 7],
			&[999_999_999_999, 1, 7_777_777],
			&[u128::MAX / 4, u128::MAX / 4, 3],
			&[0, 0, 1],
		];
		for pool_total in [0, 1, 7, 91_095_889, 10u128.pow(30), u128::MAX] {
			for (fixed_cost, n, d) in [(0, 0, 1), (340, 1, 100), (5_000_000, 3, 7), (0, 1, 1)] {
				for payees in stakes {
					for operator_stake in [0, 1, u128::MAX / 4] {
						let (operator, shares) =
							split_pool(pool_total, terms(fixed_cost, n, d), operator_stake, payees)
								.unwrap();
						assert_eq!(operator + shares.iter().sum::<u128>(), pool_total);
						assert!(operator >= pool_total.min(fixed_cost));
					}
				}
			}
		}
	}

	#[test]
	fn shares_computed_page_by_page_match_the_whole_split() {
		let payees = [11, 22, 33, 44, 55];
		let (operator, shares) = split_pool(1_234_567, terms(1_000, 5, 100), 7, &payees).unwrap();

		let split = PoolSplit::new(1_234_567, terms(1_000, 5, 100), 7, 7 + 165).unwrap();
		let mut paid = 0;
		for page in payees.chunks(2) {
			for stake in page {
				paid += split.share(*stake).unwrap();
			}
		}
		assert_eq!(paid, shares.iter().sum::<u128>());
		assert_eq!(split.operator(paid), Ok(operator));
	}

	#[test]
	fn invalid_inputs_are_errors() {
		assert_eq!(split_pool(1, terms(0, 1, 0), 0, &[1]), Err(SplitError::ZeroDenominator));
		assert_eq!(split_pool(1, terms(0, 2, 1), 0, &[1]), Err(SplitError::MarginAboveOne));
		assert_eq!(split_pool(1, terms(0, 0, 1), 0, &[0]), Err(SplitError::ZeroStake));
		assert_eq!(split_pool(1, terms(0, 0, 1), 1, &[u128::MAX]), Err(SplitError::Overflow));

		let split = PoolSplit::new(100, terms(0, 0, 1), 0, 10).unwrap();
		assert_eq!(split.share(11), Err(SplitError::StakeAboveTotal));
		assert_eq!(split.operator(101), Err(SplitError::SharesAboveShared));
		assert_eq!(PoolSplit::new(100, terms(0, 0, 1), 11, 10), Err(SplitError::StakeAboveTotal));
	}

	#[test]
	fn terms_encoding_is_pinned() {
		let encoded = terms(1, 3, 100).encode();
		let expected = [1u128, 3, 100].map(u128::to_le_bytes).concat();
		assert_eq!(encoded, expected);
		assert_eq!(PoolTerms::decode(&mut &encoded[..]), Ok(terms(1, 3, 100)));
	}
}
