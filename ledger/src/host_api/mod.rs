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

pub mod ledger_7;
pub mod ledger_8;
pub mod ledger_9;

/// Frozen-interface guard.
///
/// `ledger_7` and `ledger_8` are **frozen** host interfaces: runtimes already deployed on live
/// networks import their symbols (`ext_ledger_8_bridge_<fn>_version_N`) and will do so forever. A
/// node that drops or renames one of these methods can no longer instantiate those deployed
/// runtimes — which is exactly what happened in #1604 (it removed
/// `construct_distribute_treasury_system_tx` and silently made the ledger_9 build unable to boot any
/// existing ledger_8 network).
///
/// This golden test fails loudly if the frozen interface's method set changes, forcing a conscious,
/// reviewed decision. **New** host functions must be added to the newest (`ledger_9`) interface only;
/// never edit a frozen one.
#[cfg(test)]
mod frozen_interface_guard {
	use std::collections::BTreeSet;

	/// Extract the trait-method names from a `#[runtime_interface]` source file. Trait methods are
	/// declared at exactly one tab of indentation (`\tfn name(...)`); free functions (column 0) and
	/// nested fns (≥2 tabs) are ignored.
	fn trait_method_names(src: &str) -> BTreeSet<String> {
		src.lines()
			.filter_map(|line| line.strip_prefix("\tfn "))
			.filter_map(|rest| rest.split(|c: char| c == '(' || c == '<' || c == ' ').next())
			.filter(|name| !name.is_empty())
			.map(|name| name.to_string())
			.collect()
	}

	#[test]
	fn ledger_8_host_interface_is_frozen() {
		let got = trait_method_names(include_str!("ledger_8.rs"));
		let expected: BTreeSet<String> = [
			"apply_system_transaction",
			"apply_transaction",
			"construct_cnight_generates_dust_event",
			"construct_cnight_generates_dust_system_tx",
			"construct_distribute_night_cardano_bridge_system_tx",
			"construct_distribute_reserve_system_tx",
			"construct_distribute_treasury_system_tx",
			"ensure_storage_initialized",
			"flush_storage",
			"get_c_to_m_bridge_min_amount",
			"get_contract_state",
			"get_decoded_transaction",
			"get_ledger_parameters",
			"get_ledger_state_root",
			"get_transaction_cost",
			"get_unclaimed_amount",
			"get_version",
			"get_zswap_chain_state",
			"get_zswap_state_root",
			"is_governance_allowed_system_tx",
			"post_block_update",
			"set_default_storage",
			"validate_guaranteed_execution",
			"validate_transaction",
		]
		.iter()
		.map(|s| s.to_string())
		.collect();

		assert_eq!(
			got, expected,
			"\n`Ledger8Bridge` is a FROZEN host interface: deployed ledger_8 runtimes import these \
			 exact symbols and a node that lacks one cannot instantiate them (this breaks the \
			 ledger_8 -> ledger_9 upgrade). If you are ADDING \
			 a host function, add it to the newest (ledger_9) interface instead. If you genuinely \
			 must change the frozen set, update this golden list in the same reviewed commit."
		);
	}
}
