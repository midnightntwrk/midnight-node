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

//! Ledger version specific code related to SystemTransaction

#![cfg(feature = "std")]

use super::{
	common::types::{LedgerApiError, SystemTransactionError, TransactionError},
	mn_ledger_local::structure::SystemTransaction,
};

pub fn distribute_reserve_system_tx(amount: u128) -> SystemTransaction {
	SystemTransaction::DistributeReserve(amount)
}

pub fn is_distribute_reserve_system_tx(tx: &SystemTransaction) -> bool {
	matches!(tx, SystemTransaction::DistributeReserve(_))
}

pub fn unlock_to_treasury_system_tx(_amount: u128) -> Result<SystemTransaction, LedgerApiError> {
	Err(LedgerApiError::Transaction(TransactionError::SystemTransaction(
		SystemTransactionError::UnknownError,
	)))
}

pub fn is_unlock_to_treasury_system_tx(_tx: &SystemTransaction) -> bool {
	false
}
