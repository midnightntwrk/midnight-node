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

use super::ledger_helpers_local::{self, DefaultDB};
use crate::commands::show_night_pools::NightPools;

/// Read the NIGHT pools (Reserved / Locked / Unlocked + the rest of the supply
/// breakdown) off the full `LedgerState` held by a replayed `LedgerContext`.
pub fn night_pools(
	context: &ledger_helpers_local::context::LedgerContext<DefaultDB>,
) -> Result<NightPools, Box<dyn std::error::Error + Send + Sync>> {
	use ledger_helpers_local::{NIGHT, TokenType};

	let guard = context
		.ledger_state
		.lock()
		.map_err(|e| format!("ledger_state mutex poisoned: {e:?}"))?;
	let st = &**guard;

	Ok(NightPools {
		reserve: st.reserve_pool,
		locked: st.locked_pool,
		treasury: st.treasury.get(&TokenType::Unshielded(NIGHT)).copied().unwrap_or(0),
		block_reward: st.block_reward_pool,
		unclaimed: st.unclaimed_block_rewards.iter().map(|(_, amount)| *amount).sum(),
		utxo: st.utxo.utxos.ann().value,
		contract: st.contract.ann().value,
	})
}
