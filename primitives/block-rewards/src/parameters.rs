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

//! Reward parameters and the check that governance runs before storing them.

use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_arithmetic::helpers_128bit::gcd;

/// `n / d`. Encodes as two little-endian `u128`, the same bytes as a pair of `_n`, `_d` fields.
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
	Serialize,
	Deserialize,
)]
pub struct Ratio {
	pub n: u128,
	pub d: u128,
}

impl Ratio {
	pub const fn new(n: u128, d: u128) -> Self {
		Self { n, d }
	}

	pub const fn is_at_most_one(&self) -> bool {
		self.n <= self.d
	}

	/// Lowest terms. `d` must not be zero.
	fn reduced(self) -> Self {
		let g = gcd(self.n, self.d);
		Self::new(self.n / g, self.d / g)
	}

	/// Product in lowest terms of two ratios in lowest terms.
	fn checked_mul(self, other: Self) -> Option<Self> {
		let g1 = gcd(self.n, other.d);
		let g2 = gcd(other.n, self.d);
		Some(Self::new(
			(self.n / g1).checked_mul(other.n / g2)?,
			(self.d / g2).checked_mul(other.d / g1)?,
		))
	}
}

/// Why a parameter set is rejected.
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
pub enum ParameterError {
	#[codec(index = 0)]
	ZeroDenominator,
	#[codec(index = 1)]
	ZeroExpectedBlocksPerYear,
	#[codec(index = 2)]
	ZeroReserveAllocation,
	/// `B + T > 1`.
	#[codec(index = 3)]
	AllocationsAboveOne,
	/// `distribution_rate` is not `π (1 − B − T) / B / γ` in lowest terms.
	#[codec(index = 4)]
	RateNotDerived,
	/// A block would draw more than the reserve holds.
	#[codec(index = 5)]
	RateAboveOne,
	/// The fixed part would exceed the draw.
	#[codec(index = 6)]
	SubsidyAboveOne,
	/// An intermediate of the rate derivation does not fit in `u128`.
	#[codec(index = 7)]
	Overflow,
	/// `γ` blocks of the chain's slot duration do not make [`YEAR_MS`].
	#[codec(index = 8)]
	BlocksPerYearNotSlotDerived,
	#[codec(index = 9)]
	ZeroUpdatesPerBlock,
	/// `max_updates_per_block` is above [`MAX_UPDATES_PER_BLOCK`].
	#[codec(index = 10)]
	TooManyUpdatesPerBlock,
}

/// The most settlement entries a parameter set may let one block process, so a chunk's weight
/// stays well below a block's.
pub const MAX_UPDATES_PER_BLOCK: u32 = 1_000;

/// The year `γ` counts blocks in: 365 days, so `γ = 5 256 000` at six-second blocks.
pub const YEAR_MS: u64 = 365 * 24 * 60 * 60 * 1000;

/// Reward parameters, version 1.
#[derive(
	Clone,
	PartialEq,
	Eq,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Serialize,
	Deserialize,
)]
pub struct RewardParametersV1 {
	pub version: u32,
	/// `π`
	pub initial_expansion: Ratio,
	/// `B`, not to be confused with the outstanding reserve `B_o`.
	pub reserve_allocation: Ratio,
	/// `T`
	pub treasury_allocation: Ratio,
	/// `γ`: blocks in [`YEAR_MS`] at the chain's slot duration.
	pub expected_blocks_per_year: u64,
	/// `R = π (1 − B − T) / B / γ`: the share of `B_o` one rewarded block draws.
	pub distribution_rate: Ratio,
	/// Share of a block's draw `N_b` that its producer gets regardless of utilization.
	pub fixed_subsidy: Ratio,
	pub recipient_split_policy: [u8; 32],
	pub recipient_policy_hash: [u8; 32],
	/// The most settlement entries one block processes: payee rows read for a split and outputs
	/// paid.
	pub max_updates_per_block: u32,
	pub publication_hash: [u8; 32],
	pub governance_action_hash: [u8; 32],
}

/// A stored parameter set. A later shape adds a variant, so the sets that records name keep
/// decoding without a migration.
#[derive(
	Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub enum RewardParameters {
	#[codec(index = 1)]
	V1(RewardParametersV1),
}

impl RewardParameters {
	pub fn into_v1(self) -> RewardParametersV1 {
		match self {
			Self::V1(parameters) => parameters,
		}
	}
}

impl RewardParametersV1 {
	/// Whether `other` keeps `π`, `B`, `T` and the subsidy, compared by value. With
	/// [`Self::validate`] this also keeps `R·γ`, so only a slot duration change can move `R`.
	pub fn keeps_monetary_policy(&self, other: &Self) -> bool {
		let policy = |p: &Self| {
			[p.initial_expansion, p.reserve_allocation, p.treasury_allocation, p.fixed_subsidy]
				.map(Ratio::reduced)
		};
		policy(self) == policy(other)
	}

	/// Checks a parameter set before it is stored on a chain with the given slot duration.
	/// `R ≤ 1` and subsidy `≤ 1` are what [`crate::block_draw`] relies on.
	pub fn validate(&self, slot_duration_ms: u64) -> Result<(), ParameterError> {
		let ratios = [
			self.initial_expansion,
			self.reserve_allocation,
			self.treasury_allocation,
			self.distribution_rate,
			self.fixed_subsidy,
		];
		if ratios.iter().any(|ratio| ratio.d == 0) {
			return Err(ParameterError::ZeroDenominator);
		}
		if self.expected_blocks_per_year == 0 {
			return Err(ParameterError::ZeroExpectedBlocksPerYear);
		}
		if self.reserve_allocation.n == 0 {
			return Err(ParameterError::ZeroReserveAllocation);
		}
		if self.max_updates_per_block == 0 {
			return Err(ParameterError::ZeroUpdatesPerBlock);
		}
		if self.max_updates_per_block > MAX_UPDATES_PER_BLOCK {
			return Err(ParameterError::TooManyUpdatesPerBlock);
		}
		if !self.fixed_subsidy.is_at_most_one() {
			return Err(ParameterError::SubsidyAboveOne);
		}
		if self.derived_rate()? != self.distribution_rate {
			return Err(ParameterError::RateNotDerived);
		}
		if !self.distribution_rate.is_at_most_one() {
			return Err(ParameterError::RateAboveOne);
		}
		if u128::from(self.expected_blocks_per_year) * u128::from(slot_duration_ms)
			!= u128::from(YEAR_MS)
		{
			return Err(ParameterError::BlocksPerYearNotSlotDerived);
		}
		Ok(())
	}

	/// `π (1 − B − T) / B / γ` in lowest terms.
	fn derived_rate(&self) -> Result<Ratio, ParameterError> {
		let overflow = ParameterError::Overflow;
		let b = self.reserve_allocation.reduced();
		let t = self.treasury_allocation.reduced();
		let whole = b.d.checked_mul(t.d).ok_or(overflow)?;
		let allocated =
			b.n.checked_mul(t.d)
				.and_then(|bt| t.n.checked_mul(b.d).and_then(|tb| bt.checked_add(tb)))
				.ok_or(overflow)?;
		let circulating =
			whole.checked_sub(allocated).ok_or(ParameterError::AllocationsAboveOne)?;
		Ratio::new(circulating, whole)
			.reduced()
			.checked_mul(self.initial_expansion.reduced())
			.and_then(|r| r.checked_mul(Ratio::new(b.d, b.n)))
			.and_then(|r| r.checked_mul(Ratio::new(1, self.expected_blocks_per_year.into())))
			.ok_or(overflow)
	}
}

#[cfg(test)]
pub(crate) mod tests {
	use super::*;

	pub(crate) const SLOT_MS: u64 = 6_000;

	/// Recommended values: `π = 3/100`, `B = 1/4`, `T = 1/20`, six-second blocks.
	pub(crate) fn recommended() -> RewardParametersV1 {
		RewardParametersV1 {
			version: 1,
			initial_expansion: Ratio::new(3, 100),
			reserve_allocation: Ratio::new(1, 4),
			treasury_allocation: Ratio::new(1, 20),
			expected_blocks_per_year: 5_256_000,
			distribution_rate: Ratio::new(7, 438_000_000),
			fixed_subsidy: Ratio::new(95, 100),
			recipient_split_policy: [0; 32],
			recipient_policy_hash: [0; 32],
			publication_hash: [0; 32],
			governance_action_hash: [0; 32],
			max_updates_per_block: 100,
		}
	}

	type Change = fn(&mut RewardParametersV1);

	#[test]
	fn these_sets_are_valid() {
		let cases: [(&str, Change); 4] = [
			("the recommended values derive the published rate", |_| {}),
			("a block settles at most the cap", |p| {
				p.max_updates_per_block = MAX_UPDATES_PER_BLOCK
			}),
			("inputs need not be in lowest terms", |p| {
				p.reserve_allocation = Ratio::new(25, 100);
				p.treasury_allocation = Ratio::new(5, 100);
			}),
			("allocations summing to one derive a zero rate", |p| {
				p.treasury_allocation = Ratio::new(3, 4);
				p.distribution_rate = Ratio::new(0, 1);
			}),
		];
		for (case, change) in cases {
			let mut params = recommended();
			change(&mut params);
			assert_eq!(params.validate(SLOT_MS), Ok(()), "{case}");
		}
	}

	#[test]
	fn stored_parameters_carry_their_shape() {
		let stored = RewardParameters::V1(recommended());
		assert_eq!(stored.encode()[0], 1);
		assert_eq!(RewardParameters::decode(&mut &stored.encode()[..]), Ok(stored.clone()));
		assert_eq!(stored.into_v1(), recommended());
	}

	#[test]
	fn each_bound_has_its_own_error() {
		use ParameterError::*;
		let cases: [(&str, Change, ParameterError); 11] = [
			("a zero denominator", |p| p.fixed_subsidy = Ratio::new(95, 0), ZeroDenominator),
			("zero blocks per year", |p| p.expected_blocks_per_year = 0, ZeroExpectedBlocksPerYear),
			(
				"a zero reserve allocation",
				|p| p.reserve_allocation = Ratio::new(0, 4),
				ZeroReserveAllocation,
			),
			(
				"allocations above one",
				|p| p.treasury_allocation = Ratio::new(4, 5),
				AllocationsAboveOne,
			),
			("a subsidy above one", |p| p.fixed_subsidy = Ratio::new(101, 100), SubsidyAboveOne),
			(
				"an overflow",
				|p| {
					p.initial_expansion = Ratio::new(u128::MAX, 1);
					p.reserve_allocation = Ratio::new(1, u128::MAX);
				},
				Overflow,
			),
			(
				"a block must settle at least one entry",
				|p| p.max_updates_per_block = 0,
				ZeroUpdatesPerBlock,
			),
			(
				"a block settles at most the cap",
				|p| p.max_updates_per_block = MAX_UPDATES_PER_BLOCK + 1,
				TooManyUpdatesPerBlock,
			),
			(
				"the stored rate must be in lowest terms",
				|p| p.distribution_rate = Ratio::new(14, 876_000_000),
				RateNotDerived,
			),
			(
				"a rate that does not follow from the inputs",
				|p| p.distribution_rate = Ratio::new(8, 438_000_000),
				RateNotDerived,
			),
			(
				// π = 1 000 000, B = 1/2, T = 0, γ = 1: R = 1 000 000.
				"a rate above one, even when derived",
				|p| {
					p.initial_expansion = Ratio::new(1_000_000, 1);
					p.reserve_allocation = Ratio::new(1, 2);
					p.treasury_allocation = Ratio::new(0, 1);
					p.expected_blocks_per_year = 1;
					p.distribution_rate = Ratio::new(1_000_000, 1);
				},
				RateAboveOne,
			),
		];
		for (case, change, error) in cases {
			let mut params = recommended();
			change(&mut params);
			assert_eq!(params.validate(SLOT_MS), Err(error), "{case}");
		}
	}

	#[test]
	fn blocks_per_year_must_match_the_slot_duration() {
		assert_eq!(recommended().validate(6_000), Ok(()));
		assert_eq!(recommended().validate(3_000), Err(ParameterError::BlocksPerYearNotSlotDerived));
		// Halving the slot doubles γ and halves R in the same version.
		let rescaled = RewardParametersV1 {
			expected_blocks_per_year: 2 * 5_256_000,
			distribution_rate: Ratio::new(7, 876_000_000),
			..recommended()
		};
		assert_eq!(rescaled.validate(3_000), Ok(()));
		assert_eq!(recommended().validate(0), Err(ParameterError::BlocksPerYearNotSlotDerived));
	}

	#[test]
	fn the_monetary_policy_is_compared_by_value() {
		let same = RewardParametersV1 { initial_expansion: Ratio::new(6, 200), ..recommended() };
		assert!(recommended().keeps_monetary_policy(&same));
		let rescaled = RewardParametersV1 {
			expected_blocks_per_year: 2 * 5_256_000,
			distribution_rate: Ratio::new(7, 876_000_000),
			..recommended()
		};
		assert!(recommended().keeps_monetary_policy(&rescaled));
		let changes: [Change; 4] = [
			|p| p.initial_expansion = Ratio::new(4, 100),
			|p| p.reserve_allocation = Ratio::new(1, 5),
			|p| p.treasury_allocation = Ratio::new(1, 10),
			|p| p.fixed_subsidy = Ratio::new(9, 10),
		];
		for change in changes {
			let mut changed = recommended();
			change(&mut changed);
			assert!(!recommended().keeps_monetary_policy(&changed));
		}
	}

	#[test]
	fn a_ratio_encodes_as_an_n_d_pair() {
		assert_eq!(
			Ratio::new(7, 438_000_000).encode(),
			[7u128.to_le_bytes(), 438_000_000u128.to_le_bytes()].concat()
		);
	}
}
