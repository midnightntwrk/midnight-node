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

//! Ledger-7-only mappings for variants that don't exist in the shared common
//! conversion. Ledger 7 has no extra variants beyond the shared set, so each
//! helper passes through unchanged.

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
	Err(err)
}

pub fn try_convert_extra_zswap_invalid(
	err: ZswapTransactionInvalid,
) -> Result<ZswapInvalidErrorCode, ZswapTransactionInvalid> {
	Err(err)
}

pub fn try_convert_extra_system_tx(
	err: LedgerSystemTransactionError,
) -> Result<SystemTransactionError, LedgerSystemTransactionError> {
	Err(err)
}
