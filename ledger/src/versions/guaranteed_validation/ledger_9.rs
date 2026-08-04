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

//! Ledger-9 dry-run validation of the guaranteed transaction segment.
//!
//! Uses ledger 9's split-phase `apply_guaranteed_only` API so validation does not
//! execute the fallible segment.

#![cfg(feature = "std")]

use super::{
	ledger_storage_local::db::DB,
	mn_ledger_local::{
		error::TransactionInvalid,
		semantics::TransactionContext,
		structure::{LedgerState, VerifiedTransaction},
	},
};

pub fn validate_guaranteed_execution<D: DB>(
	state: &LedgerState<D>,
	verified_tx: VerifiedTransaction<D>,
	ctx: &TransactionContext<D>,
) -> Result<(), TransactionInvalid<D>> {
	state.apply_guaranteed_only(verified_tx, ctx).map(|_| ())
}
