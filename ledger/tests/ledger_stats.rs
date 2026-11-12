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

//! Integration tests for `midnight_node_ledger::ledger_stats`, which backs the
//! `midnight_ledgerStats` RPC.
//!
//! These run as their own binary because `set_default_storage` is process-global
//! and set-once: sharing a process with the crate's unit tests would make the
//! arena setup order-dependent.

use ledger_storage_ledger_8::DefaultDB;
use midnight_node_res::networks::{MidnightNetwork, UndeployedNetwork};
use midnight_serialize::tagged_deserialize;
use mn_ledger_9::structure::LedgerState;

/// Seed a throwaway arena from the undeployed-network genesis and return its
/// `StateKey`. This is the same call the node makes on first boot.
fn seed_arena() -> Vec<u8> {
	let dir = std::env::temp_dir().join(format!("mn-ledger-stats-{}", std::process::id()));
	let _ = std::fs::remove_dir_all(&dir);
	midnight_node_ledger::init_ledger_storage_separate(&dir, UndeployedNetwork.genesis_state(), 0)
}

#[test]
fn stats_match_the_state_they_were_read_from() {
	let state_key = seed_arena();

	// Read the stats through the arena, the way the RPC will.
	let stats = midnight_node_ledger::ledger_stats(false, &state_key)
		.unwrap_or_else(|e| panic!("ledger_stats failed: {e}"));

	// Independently deserialize the same genesis and read the same fields
	// directly, so a mis-wired field cannot pass by agreeing with itself.
	let state: LedgerState<DefaultDB> = tagged_deserialize(UndeployedNetwork.genesis_state())
		.expect("genesis state must deserialize");

	let utxo_ann = state.utxo.utxos.ann();
	assert_eq!(stats.unshielded_utxo_count, utxo_ann.size, "unshielded UTXO count");
	assert_eq!(stats.unshielded_utxo_stars, utxo_ann.value, "NIGHT held in UTXOs");
	assert_eq!(stats.zswap_commitment_count, state.zswap.first_free, "zswap commitments");
	assert_eq!(
		stats.zswap_nullifier_count,
		state.zswap.nullifiers.size() as u64,
		"zswap nullifiers"
	);
	assert_eq!(
		stats.dust_commitment_count, state.dust.utxo.commitments_first_free,
		"dust commitments"
	);
	assert_eq!(
		stats.dust_nullifier_count,
		state.dust.utxo.nullifiers.size() as u64,
		"dust nullifiers"
	);
	assert_eq!(stats.contract_count, state.contract.ann().size, "contract count");

	// The annotation is only worth reading if it agrees with the collection it
	// annotates — an O(1) count that disagrees with an O(n) walk is worse than none.
	assert_eq!(
		stats.unshielded_utxo_count as usize,
		state.utxo.utxos.size(),
		"annotation count must equal the actual number of UTXOs"
	);
	assert_eq!(
		stats.unshielded_utxo_stars,
		state.utxo.utxos.iter().map(|kv| kv.0.value).sum::<u128>(),
		"annotation value must equal the summed UTXO values"
	);

	// A genesis that funds wallets is the only interesting fixture here; if this
	// ever becomes an empty state the assertions above stop proving anything.
	assert!(stats.unshielded_utxo_count > 0, "fixture must have UTXOs to be meaningful");
}

#[test]
fn unknown_state_key_version_is_rejected_not_panicked() {
	// No `ledger-state[vNN]` tag at all.
	let err = midnight_node_ledger::ledger_stats(false, b"not-a-state-key")
		.expect_err("untagged key must not resolve");
	assert!(err.contains("unsupported ledger-state version"), "unexpected error: {err}");

	// A tag this build has no reader for — the guard is the version range, not the
	// bytes that follow it.
	let future = b"midnight:storage-key(ledger-state[v99]):xxxx";
	let err = midnight_node_ledger::ledger_stats(false, future)
		.expect_err("future ledger version must not resolve");
	assert!(err.contains("unsupported ledger-state version"), "unexpected error: {err}");
}
