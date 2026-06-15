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

//! Runtime migrations

use crate::{MidnightSystem, Runtime};
use frame_support::traits::OnRuntimeUpgrade;
// `Get` is brought in via the prelude where `DbWeight::get()` is resolved.
use frame_support::weights::Weight;
use midnight_node_ledger::types::active_ledger_bridge as LedgerApi;
use midnight_primitives::MidnightSystemTransactionExecutor;
#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
#[cfg(feature = "try-runtime")]
use parity_scale_codec::{Decode, Encode};

/// Healthy-testnet pool targets (qanet/preprod/mainnet all carry exactly these figures). The
/// migration brings an under-funded network's locked pool + NIGHT treasury *up to* these targets;
/// it never overshoots and never claws back. See `docs/design/locked-pool-rebalance-no-reset.md`.
const TARGET_LOCKED: u128 = 16_799_999_999_126_012;
const TARGET_TREASURY: u128 = 1_200_000_000_000_000;

/// Run-once gate (design §10.3 S3). A `:`-prefixed well-known-style key written via
/// `unhashed` storage; ASCII so it cannot collide with hash-derived pallet storage keys.
const MIGRATION_DONE_KEY: &[u8] = b":mn:seed_preview_locked_pool:v1:applied";

/// One-shot, supply-preserving correction of an empty Locked pool (Preview's #1674 condition)
/// without a chain reset.
///
/// It reads the *live* ledger pools and applies a privileged `SystemTransaction::SeedPoolsFromReserve`
/// that moves only the **gap** to the targets (`target - current`, saturating). This makes it
/// genuinely self-targeting (design §10.3 S1): it **no-ops on networks already at/above target**
/// (the healthy testnets) and moves the exact shortfall on an under-funded one (Preview:
/// locked 0 → 16.8e15, treasury 0 → 1.2e15). Properties (design §10.3):
///
/// * **S1** self-targeting — the gap is computed from live state by `construct_seed_pools_to_target_tx`.
/// * **S2** applied *through* `apply_system_tx`, so the ledger enforces the supply invariant and the
///   reserve guard; the runtime never hand-edits a pool field.
/// * **S3** a run-once storage gate (`MIGRATION_DONE_KEY`) so it cannot re-apply across later upgrades.
/// * **S5** fail-safe: on any error it logs and leaves the chain healthy (never panics / bricks),
///   and leaves the gate unset so a future upgrade can retry.
pub struct SeedPreviewLockedPool;

impl OnRuntimeUpgrade for SeedPreviewLockedPool {
	fn on_runtime_upgrade() -> Weight {
		let db_weight = <Runtime as frame_system::Config>::DbWeight::get();

		// S3 — run at most once per network.
		if frame_support::storage::unhashed::get_or_default::<bool>(MIGRATION_DONE_KEY) {
			log::info!(target: "runtime::migration", "SeedPreviewLockedPool: already applied, skipping");
			return db_weight.reads(1);
		}

		// S1 — compute the reserve->locked/treasury gap from the *live* pools and build the tx.
		let state_key = pallet_midnight::StateKey::<Runtime>::get();
		let tx_bytes = match LedgerApi::construct_seed_pools_to_target_tx(
			&state_key,
			TARGET_LOCKED,
			TARGET_TREASURY,
		) {
			Ok(bytes) => bytes,
			Err(e) => {
				// S5 — fail safe: leave the gate unset so a later upgrade can retry.
				log::error!(
					target: "runtime::migration",
					"SeedPreviewLockedPool: could not construct correction, skipping (will retry): {e:?}"
				);
				return db_weight.reads_writes(2, 0);
			},
		};

		if tx_bytes.is_empty() {
			// Pools already at/above target (healthy networks): nothing to move. Mark done.
			log::info!(
				target: "runtime::migration",
				"SeedPreviewLockedPool: pools already at/above target; no move needed"
			);
			frame_support::storage::unhashed::put(MIGRATION_DONE_KEY, &true);
			return db_weight.reads_writes(2, 1);
		}

		// S2 — apply *through* the ledger so the supply invariant + reserve guard are enforced.
		match <MidnightSystem as MidnightSystemTransactionExecutor>::execute_system_transaction(
			tx_bytes,
		) {
			Ok(hash) => {
				log::info!(
					target: "runtime::migration",
					"SeedPreviewLockedPool: applied SeedPoolsFromReserve gap to target (tx {hash:?})"
				);
				frame_support::storage::unhashed::put(MIGRATION_DONE_KEY, &true);
				db_weight.reads_writes(8, 8)
			},
			Err(e) => {
				// S5 — e.g. insufficient reserve on a network we did not intend to fix. Do NOT mark
				// done; surface the error and leave the chain healthy (no panic, no brick).
				log::error!(
					target: "runtime::migration",
					"SeedPreviewLockedPool: apply rejected, pools left unchanged (will retry): {e:?}"
				);
				db_weight.reads_writes(8, 0)
			},
		}
	}

	/// S6 — capture the live pools before the upgrade so `post_upgrade` can prove conservation.
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		let state_key = pallet_midnight::StateKey::<Runtime>::get();
		let pools = LedgerApi::get_night_pools(&state_key)
			.map_err(|_| "SeedPreviewLockedPool::pre_upgrade: night_pools read failed")?;
		// (locked, reserve, treasury_night)
		Ok(pools.encode())
	}

	/// S6 — assert the migration's invariants against real state, without committing:
	///  1. supply is conserved across the three pools (the core correctness guarantee);
	///  2. locked is either untouched (no-op) or exactly at target — never partial, never overshoot;
	///  3. movement is monotonic — reserve is the only source, so locked can't fall / reserve can't rise.
	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let (pre_locked, pre_reserve, pre_treasury): (u128, u128, u128) =
			Decode::decode(&mut &state[..])
				.map_err(|_| "SeedPreviewLockedPool::post_upgrade: decode pre-state failed")?;
		let state_key = pallet_midnight::StateKey::<Runtime>::get();
		let (post_locked, post_reserve, post_treasury) = LedgerApi::get_night_pools(&state_key)
			.map_err(|_| "SeedPreviewLockedPool::post_upgrade: night_pools read failed")?;

		frame_support::ensure!(
			pre_locked + pre_reserve + pre_treasury
				== post_locked + post_reserve + post_treasury,
			"SeedPreviewLockedPool: pool supply not conserved across the migration"
		);
		frame_support::ensure!(
			post_locked == pre_locked || post_locked == TARGET_LOCKED,
			"SeedPreviewLockedPool: locked_pool neither unchanged nor exactly at target"
		);
		frame_support::ensure!(
			post_locked >= pre_locked && post_reserve <= pre_reserve,
			"SeedPreviewLockedPool: non-monotonic pool movement"
		);
		Ok(())
	}
}
