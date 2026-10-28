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

use super::*;
use boundary::types::Hash;

/// Version-agnostic validation error returned across the native API boundary.
#[derive(Debug)]
pub struct ValidationError {
	/// The LedgerApiError encoded as a u8 error code.
	pub error_code: u8,
	/// Human-readable Display of the LedgerApiError.
	pub reason: String,
	/// Debug representation of the original ledger error (before lossy conversion).
	pub details: String,
}

macro_rules! to_validation_error {
	($ver:ident) => {
		|e: $ver::types::DetailedTransactionError| ValidationError {
			reason: format!("{}", e.error),
			error_code: e.error.into(),
			details: e.details,
		}
	};
}

/// Validates `tx` against the ledger state at `state_key` without submitting it. Dispatches on the
/// `StateKey`'s `ledger-state[vNN]` tag and picks the DB instantiation by `unified`, as in
/// [`crate::ledger_stats`].
pub fn validate_transaction_verbose(
	unified: bool,
	state_key: &[u8],
	tx: &[u8],
	block_context: latest::BlockContext,
	runtime_version: u32,
	max_weight: u64,
) -> Result<Hash, ValidationError> {
	match ledger_state_tag_version(state_key) {
		Some(16..=18) => bridge_arena_call!(
			ledger_9,
			unified,
			validate_transaction_verbose(state_key, tx, block_context, runtime_version, max_weight)
		)
		.map_err(to_validation_error!(ledger_9)),
		Some(13) => {
			let block_context = ledger_8::BlockContext {
				tblock: block_context.tblock,
				tblock_err: block_context.tblock_err,
				parent_block_hash: block_context.parent_block_hash,
				last_block_time: block_context.last_block_time,
			};
			bridge_arena_call!(
				ledger_8,
				unified,
				validate_transaction_verbose(
					state_key,
					tx,
					block_context,
					runtime_version,
					max_weight
				)
			)
			.map_err(to_validation_error!(ledger_8))
		},
		other => Err(ValidationError {
			error_code: 151, // NoLedgerState
			reason: "Unsupported ledger version".into(),
			details: format!("unsupported ledger-state version {other:?} in StateKey"),
		}),
	}
}
