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

//! Ledger-7 post-block-update prevalidation.
//!
//! Not supported on ledger 7; validation runs only at end-of-block `post_block_update`.

#![cfg(feature = "std")]

use super::{
	base_crypto_local::{
		cost_model::{FixedPoint, NormalizedCost},
		time::Timestamp,
	},
	common::types::LedgerApiError,
	ledger_storage_local::db::DB,
	mn_ledger_local::structure::LedgerState,
};

pub fn prevalidate_post_block_update<D: DB>(
	_state: &LedgerState<D>,
	_block_fullness: &super::base_crypto_local::cost_model::SyntheticCost,
	_block_limits: &super::base_crypto_local::cost_model::SyntheticCost,
	_context: &str,
) -> Result<(), LedgerApiError> {
	Ok(())
}

/// Applies the end-of-block ledger update. Ledger 7 exposes only the fallible combined
/// `post_block_update`; the caller has already clamped fullness to the block limits, so its
/// internal limit check cannot fail here.
pub fn apply_post_block_update<D: DB>(
	state: &LedgerState<D>,
	tblock: Timestamp,
	detailed_block_fullness: NormalizedCost,
	overall_block_fullness: FixedPoint,
) -> LedgerState<D> {
	state
		.post_block_update(tblock, detailed_block_fullness, overall_block_fullness)
		.unwrap_or_else(|_| {
			panic!("apply_post_block_update: post_block_update failed despite clamped fullness")
		})
}
