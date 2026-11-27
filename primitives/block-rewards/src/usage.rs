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

//! What a block used of the ledger's block limit, and the utilization its producer is paid for.

use crate::Utilization;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// One dimension of the ledger's block limit.
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
pub struct UsageDimension {
	pub used: u64,
	pub limit: u64,
}

/// What a block has used of each dimension of the ledger's block limit, in the ledger's order:
/// read time, compute time, block usage, bytes written, bytes churned.
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
pub struct BlockUsage(pub [UsageDimension; 5]);

impl BlockUsage {
	/// The largest share of any dimension of the block's limit that the block used besides
	/// `settlement`: the same share the ledger's DUST price update sees, since it leaves
	/// `settlement` out too. The block's own reward payments are thereby not counted as the
	/// producer's work. A dimension used beyond its limit is not capped: the draw rejects it.
	pub fn utilization_after(&self, settlement: &BlockUsage) -> Utilization {
		self.0
			.iter()
			.zip(settlement.0.iter())
			.filter_map(|(block, settlement)| {
				let used = block.used.saturating_sub(settlement.used);
				(block.limit > 0)
					.then(|| Utilization { used: used.into(), max: block.limit.into() })
			})
			// u64 × u64 fits in u128, so the cross products compare the shares exactly.
			.max_by(|a, b| (a.used * b.max).cmp(&(b.used * a.max)))
			.unwrap_or(Utilization { used: 0, max: 1 })
	}

	/// What the block used after `earlier`, with the block's limits.
	pub fn since(&self, earlier: &BlockUsage) -> BlockUsage {
		let mut usage = *self;
		for (dimension, earlier) in usage.0.iter_mut().zip(earlier.0.iter()) {
			dimension.used = dimension.used.saturating_sub(earlier.used);
		}
		usage
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn usage(dimensions: [(u64, u64); 5]) -> BlockUsage {
		BlockUsage(dimensions.map(|(used, limit)| UsageDimension { used, limit }))
	}

	const NONE: BlockUsage = BlockUsage([UsageDimension { used: 0, limit: 0 }; 5]);

	#[test]
	fn utilization_is_the_largest_share_of_any_dimension() {
		let block = usage([(1, 10), (3, 10), (20, 100), (7, 1_000), (999, 10_000)]);
		assert_eq!(block.utilization_after(&NONE), Utilization { used: 3, max: 10 });
	}

	#[test]
	fn a_full_dimension_makes_a_full_block() {
		let block = usage([(0, 10), (0, 10), (100, 100), (0, 1_000), (0, 10_000)]);
		assert_eq!(block.utilization_after(&NONE), Utilization { used: 100, max: 100 });
	}

	#[test]
	fn a_dimension_over_its_limit_is_not_capped() {
		let block = usage([(0, 10), (0, 10), (120, 100), (0, 1_000), (0, 10_000)]);
		assert_eq!(block.utilization_after(&NONE), Utilization { used: 120, max: 100 });
	}

	#[test]
	fn an_empty_block_has_zero_utilization() {
		let block = usage([(0, 10), (0, 10), (0, 100), (0, 1_000), (0, 10_000)]);
		assert_eq!(block.utilization_after(&NONE).used, 0);
	}

	#[test]
	fn no_limits_mean_zero_utilization() {
		assert_eq!(NONE.utilization_after(&NONE), Utilization { used: 0, max: 1 });
	}

	#[test]
	fn the_comparison_does_not_overflow_at_u64_max() {
		let block = usage([(u64::MAX, u64::MAX), (u64::MAX - 1, u64::MAX), (0, 1), (0, 1), (0, 1)]);
		assert_eq!(
			block.utilization_after(&NONE),
			Utilization { used: u64::MAX.into(), max: u64::MAX.into() }
		);
	}

	#[test]
	fn what_settlement_used_is_not_the_producers_utilization() {
		let settlement = usage([(0, 100), (0, 100), (40, 100), (0, 100), (0, 100)]);
		let block = usage([(10, 100), (0, 100), (40, 100), (0, 100), (0, 100)]);
		assert_eq!(block.utilization_after(&settlement), Utilization { used: 10, max: 100 });
		let full = usage([(10, 100), (0, 100), (100, 100), (0, 100), (0, 100)]);
		// Measured against the whole limit, as the DUST price is.
		assert_eq!(full.utilization_after(&settlement), Utilization { used: 60, max: 100 });
	}

	#[test]
	fn usage_differences() {
		let before = usage([(10, 100), (0, 100), (5, 50), (0, 100), (0, 100)]);
		let after = usage([(40, 100), (7, 100), (5, 50), (0, 100), (0, 100)]);
		let delta = after.since(&before);
		assert_eq!(delta, usage([(30, 100), (7, 100), (0, 50), (0, 100), (0, 100)]));
	}
}
