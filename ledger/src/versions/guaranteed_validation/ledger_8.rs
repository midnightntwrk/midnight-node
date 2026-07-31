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

//! Ledger-8 dry-run validation of the guaranteed transaction segment.
//!
//! Ledger 8 has no `apply_guaranteed_only`; fall back to a full `apply()` dry-run
//! and treat `PartialSuccess` as acceptable (guaranteed segment succeeded).

#![cfg(feature = "std")]

use super::{
	ledger_storage_local::db::DB,
	mn_ledger_local::{
		error::TransactionInvalid,
		semantics::{TransactionContext, TransactionResult},
		structure::{LedgerState, VerifiedTransaction},
	},
};

pub fn validate_guaranteed_execution<D: DB>(
	state: &LedgerState<D>,
	verified_tx: VerifiedTransaction<D>,
	ctx: &TransactionContext<D>,
) -> Result<(), TransactionInvalid<D>> {
	let (_next_state, result) = state.apply(&verified_tx, ctx);
	match result {
		TransactionResult::Success(_) | TransactionResult::PartialSuccess(_, _) => Ok(()),
		TransactionResult::Failure(reason) => Err(reason),
	}
}
