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

//! Ledger-9-only mappings for variants that don't exist in ledgers 7 and 8. Each
//! helper handles only the variants we know are ledger-9-specific and returns
//! `Err` for anything else, so the shared common conversion can fall back to
//! its `UnknownError + log` arm rather than misclassifying future additions.

#![cfg(feature = "std")]

use super::{
	common::types::{InvalidError, SystemTransactionError, ZswapInvalidErrorCode},
	ledger_storage_local::db::DB,
	mn_ledger_local::error::{
		SystemTransactionError as LedgerSystemTransactionError, TransactionInvalid,
	},
	zswap_local::error::TransactionInvalid as ZswapTransactionInvalid,
};

pub fn try_convert_extra_invalid<D: DB>(
	err: TransactionInvalid<D>,
) -> Result<InvalidError, TransactionInvalid<D>> {
	match err {
		TransactionInvalid::MerkleTreeError(_) => Ok(InvalidError::MerkleTreeError),
		other => Err(other),
	}
}

pub fn try_convert_extra_zswap_invalid(
	err: ZswapTransactionInvalid,
) -> Result<ZswapInvalidErrorCode, ZswapTransactionInvalid> {
	match err {
		ZswapTransactionInvalid::MerkleTreeError(_) => Ok(ZswapInvalidErrorCode::MerkleTreeError),
		other => Err(other),
	}
}

pub fn try_convert_extra_system_tx(
	err: LedgerSystemTransactionError,
) -> Result<SystemTransactionError, LedgerSystemTransactionError> {
	match err {
		LedgerSystemTransactionError::MerkleTreeError(_) => {
			Ok(SystemTransactionError::MerkleTreeError)
		},
		other => Err(other),
	}
}
