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

pub mod raw_block_data;

#[cfg(feature = "can-panic")]
use crate::fork::raw_block_data::LedgerVersion;

#[cfg(feature = "can-panic")]
pub mod fork_7_to_8;
#[cfg(feature = "can-panic")]
pub mod fork_8_to_9;
#[cfg(feature = "can-panic")]
pub mod fork_aware_context;

#[cfg(feature = "can-panic")]
pub fn network_id_and_ledger_version_from_tx_bytes(
	tx_bytes: &[u8],
) -> Result<(String, LedgerVersion), std::io::Error> {
	let res9 = crate::ledger_9::network_id_from_transaction_bytes(tx_bytes);
	if let Ok(ref network_id) = res9 {
		return Ok((network_id.to_string(), LedgerVersion::Ledger9));
	}

	let res8 = crate::ledger_8::network_id_from_transaction_bytes(tx_bytes);
	if let Ok(ref network_id) = res8 {
		return Ok((network_id.to_string(), LedgerVersion::Ledger8));
	}

	let network_id = crate::ledger_7::network_id_from_transaction_bytes(tx_bytes)?;
	Ok((network_id.to_string(), LedgerVersion::Ledger7))
}
