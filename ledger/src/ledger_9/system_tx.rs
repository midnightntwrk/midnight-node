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

use crate::ledger_9::{mn_ledger_local::structure::SystemTransaction, types::LedgerApiError};

pub fn distribute_reserve_system_tx(amount: u128) -> SystemTransaction {
	SystemTransaction::DistributeReserve { amount }
}

pub fn is_distribute_reserve_system_tx(tx: &SystemTransaction) -> bool {
	matches!(tx, SystemTransaction::DistributeReserve { .. })
}

pub fn unlock_to_treasury_system_tx(amount: u128) -> Result<SystemTransaction, LedgerApiError> {
	Ok(SystemTransaction::UnlockToTreasury { amount })
}

pub fn is_unlock_to_treasury_system_tx(tx: &SystemTransaction) -> bool {
	matches!(tx, SystemTransaction::UnlockToTreasury { .. })
}

/// Not applicable to ledger-9: the block-rewards-to-treasury system tx was
/// removed for v9 (only the ledger-8 bridge exposes this host fn, so the ledger-8
/// WASM can be executed across the 8->9 hardfork).
pub fn distribute_treasury_system_tx(_amount: u128) -> Result<SystemTransaction, LedgerApiError> {
	Err(LedgerApiError::HostApiError)
}
