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

//! Ledger-9 post-block-update prevalidation after each transaction.
//!
//! Uses ledger 9's `LedgerState::prevalidate_post_block_update` to reject
//! transactions that would fail end-of-block processing before they are applied.

#![cfg(feature = "std")]

use super::{
	base_crypto_local::cost_model::SyntheticCost,
	common::{LOG_TARGET, types::LedgerApiError},
	helpers_local::compute_overall_fullness,
	ledger_storage_local::db::DB,
	mn_ledger_local::{error::BlockLimitExceeded, structure::LedgerState},
};

pub fn prevalidate_post_block_update<D: DB>(
	state: &LedgerState<D>,
	block_fullness: &SyntheticCost,
	block_limits: &SyntheticCost,
	context: &str,
) -> Result<(), LedgerApiError> {
	let normalized_fullness = (*block_fullness).normalize(*block_limits).ok_or_else(|| {
		log::warn!(
			target: LOG_TARGET,
			"Ledger block limit exceeded in {context}: fullness={block_fullness:?}, limits={block_limits:?}"
		);
		LedgerApiError::BlockLimitExceededError
	})?;
	let overall_fullness = compute_overall_fullness(&normalized_fullness);
	state
		.prevalidate_post_block_update(normalized_fullness, overall_fullness)
		.map_err(|_err: BlockLimitExceeded| LedgerApiError::BlockLimitExceededError)
}
