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

//! Invariants of the ledger state as the specification states them, checked by reading an
//! observed `LedgerState`'s data structures, never by calling a ledger verifier. The table of
//! invariants is in the crate README.
//!
//! The root invariants hold at block boundaries only: the ledger rehashes its commitment trees
//! in the end-of-block update, so mid-block a tree may have no root yet. [`check`] verifies them
//! whenever a root exists and [`check_at_block_boundary`] requires one.

use midnight_ledger_unsafe_helpers::{
	DefaultDB, LedgerState, NIGHT, TokenType, mn_ledger::structure::MAX_SUPPLY,
};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
	pub invariant: &'static str,
	pub detail: String,
}

/// Every violated invariant, in table order. Empty means the state satisfies all of them.
/// Root invariants are checked when the tree has a root; see [`check_at_block_boundary`].
pub fn check(state: &LedgerState<DefaultDB>) -> Vec<Violation> {
	check_with(state, false)
}

/// [`check`] plus the requirement that both commitment trees are rehashed and their roots
/// published, which the end-of-block update guarantees.
pub fn check_at_block_boundary(state: &LedgerState<DefaultDB>) -> Vec<Violation> {
	check_with(state, true)
}

fn check_with(state: &LedgerState<DefaultDB>, at_block_boundary: bool) -> Vec<Violation> {
	let mut violations = Vec::new();
	let mut violated = |invariant: &'static str, detail: String| {
		violations.push(Violation { invariant, detail });
	};

	let zswap = &state.zswap;
	let commitments = zswap.coin_coms_set.size() as u64;
	if zswap.first_free != commitments {
		violated(
			"zswap_tree_dense",
			format!("first_free {} but {commitments} commitments", zswap.first_free),
		);
	}
	if let Some(detail) =
		root_violation(zswap.coin_coms.root(), |r| zswap.past_roots.contains(r), at_block_boundary)
	{
		violated("zswap_root_known", detail);
	}
	let nullifiers = zswap.nullifiers.size() as u64;
	if nullifiers > commitments {
		violated(
			"zswap_nullifiers_bounded",
			format!("{nullifiers} nullifiers but only {commitments} commitments"),
		);
	}

	let mut ids = HashSet::new();
	for entry in state.utxo.utxos.iter() {
		let utxo = &entry.0;
		if !ids.insert((utxo.intent_hash, utxo.output_no)) {
			violated(
				"utxo_ids_unique",
				format!("two live UTXOs with id {:?}#{}", utxo.intent_hash, utxo.output_no),
			);
		}
		if utxo.value == 0 {
			violated(
				"utxo_values_positive",
				format!("zero-valued UTXO {:?}#{}", utxo.intent_hash, utxo.output_no),
			);
		}
	}

	let dust = &state.dust.utxo;
	if let Some(detail) = root_violation(
		dust.commitments.root(),
		|r| dust.root_history.contains(r),
		at_block_boundary,
	) {
		violated("dust_root_known", detail);
	}
	let dust_nullifiers = dust.nullifiers.size() as u64;
	if dust_nullifiers > dust.commitments_first_free {
		violated(
			"dust_nullifiers_bounded",
			format!(
				"{dust_nullifiers} DUST nullifiers but only {} commitments",
				dust.commitments_first_free
			),
		);
	}

	let visible = visible_night(state);
	if visible > MAX_SUPPLY {
		violated(
			"night_within_supply",
			format!("{visible} visible NIGHT > MAX_SUPPLY {MAX_SUPPLY}"),
		);
	}

	violations
}

fn root_violation<R: std::fmt::Debug>(
	root: Option<R>,
	is_known: impl Fn(&R) -> bool,
	at_block_boundary: bool,
) -> Option<String> {
	match root {
		Some(root) if !is_known(&root) => {
			Some(format!("current root {root:?} not in root history"))
		},
		None if at_block_boundary => Some("commitment tree not rehashed at block boundary".into()),
		_ => None,
	}
}

/// Panics with every violation listed; for use at each block boundary of a simulation.
pub fn assert_holds_at_block_boundary(state: &LedgerState<DefaultDB>, at: &str) {
	let violations = check_at_block_boundary(state);
	assert!(violations.is_empty(), "ledger invariants violated {at}: {violations:#?}");
}

/// NIGHT accounted for on the unshielded side: the reserve, locked and block-reward pools,
/// unclaimed rewards, NIGHT awaiting bridge claims, the treasury's NIGHT and every live
/// unshielded NIGHT UTXO. Shielded NIGHT is hidden behind commitments and is the only other
/// place NIGHT can be.
pub fn visible_night(state: &LedgerState<DefaultDB>) -> u128 {
	let pools = state.reserve_pool + state.locked_pool + state.block_reward_pool;
	let unclaimed: u128 = state.unclaimed_block_rewards.iter().map(|e| *e.1).sum();
	let bridge: u128 = state.bridge_receiving.iter().map(|e| *e.1).sum();
	let is_night = |t: &TokenType| matches!(t, TokenType::Unshielded(t) if *t == NIGHT);
	let treasury: u128 = state.treasury.iter().filter(|e| is_night(&e.0)).map(|e| *e.1).sum();
	let utxos: u128 =
		state.utxo.utxos.iter().filter(|e| e.0.type_ == NIGHT).map(|e| e.0.value).sum();
	pools + unclaimed + bridge + treasury + utxos
}

/// Conservation across a state transition: the visible NIGHT changes by exactly the net amount
/// the shielded offers moved to the unshielded side (positive when unshielding). With no
/// shielded NIGHT traffic, `shielded_to_unshielded` is 0 and the visible supply is constant.
pub fn night_conserved(
	before: &LedgerState<DefaultDB>,
	after: &LedgerState<DefaultDB>,
	shielded_to_unshielded: i128,
) -> Result<(), Violation> {
	let (b, a) = (visible_night(before) as i128, visible_night(after) as i128);
	if a - b == shielded_to_unshielded {
		Ok(())
	} else {
		Err(Violation {
			invariant: "night_conserved",
			detail: format!(
				"visible NIGHT moved by {} but shielded offers account for {shielded_to_unshielded}",
				a - b
			),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use midnight_ledger_unsafe_helpers::{Sp, deserialize, zswap};
	use midnight_node_res::networks::{MidnightNetwork, UndeployedNetwork};

	fn genesis() -> LedgerState<DefaultDB> {
		deserialize(&mut &UndeployedNetwork.genesis_state()[..]).expect("genesis ledger state")
	}

	#[test]
	fn genesis_state_satisfies_every_invariant_at_a_block_boundary() {
		let state = genesis();
		assert_eq!(check_at_block_boundary(&state), vec![]);
		assert!(visible_night(&state) > 0);
		assert_eq!(night_conserved(&state, &state, 0), Ok(()));
	}

	#[test]
	fn a_gap_in_the_commitment_tree_is_reported() {
		let mut state = genesis();
		state.zswap = Sp::new(zswap::ledger::State {
			first_free: state.zswap.first_free + 1,
			..(*state.zswap).clone()
		});
		let violated: Vec<_> = check(&state).into_iter().map(|v| v.invariant).collect();
		assert_eq!(violated, vec!["zswap_tree_dense"]);
	}

	#[test]
	fn a_supply_change_without_shielded_traffic_is_reported() {
		let before = genesis();
		let mut after = genesis();
		after.reserve_pool -= 1;
		assert_eq!(
			night_conserved(&before, &after, 0).map_err(|v| v.invariant),
			Err("night_conserved")
		);
		assert_eq!(night_conserved(&before, &after, -1), Ok(()));
	}
}
