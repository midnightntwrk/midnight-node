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

use crate::Runtime;
use frame_support::traits::OnRuntimeUpgrade;
// `Get` is brought in via the prelude where `DbWeight::get()` is resolved.
use frame_support::weights::Weight;
#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
#[cfg(feature = "try-runtime")]
use midnight_node_ledger::types::active_ledger_bridge as LedgerApi;
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
/// It reads the *live* ledger pools and moves only the **gap** to the targets (`target - current`,
/// saturating) out of the reserve pool, crediting the locked pool and the NIGHT treasury. The move
/// is performed as a direct, supply-preserving edit of the ledger state
/// (`pallet_midnight::seed_pools_to_target` → ledger host fn `seed_pools_to_target` →
/// `Ledger::seed_pools_from_reserve`) rather than via a privileged system transaction — the
/// **identical database effect** against the stock ledger, so the node carries **no
/// `midnight-ledger` change**. Properties (design §10.3):
///
/// * **S1** self-targeting — the gap is computed from live state; it **no-ops on networks already
///   at/above target** (healthy testnets) and moves the exact shortfall on an under-funded one
///   (Preview: locked 0 → 16.8e15, treasury 0 → 1.2e15).
/// * **S2** supply-preserving — only value *between* the three summed NIGHT pools is moved, so the
///   ledger's NIGHT invariant (whose total is unchanged) still holds; the reserve guard lives in
///   the ledger host fn (`to_locked + to_treasury <= reserve_pool`).
/// * **S3** a run-once storage gate (`MIGRATION_DONE_KEY`) so it cannot re-apply across later
///   upgrades.
/// * **S5** fail-safe: on any error (e.g. insufficient reserve) it logs and leaves the chain
///   healthy (never panics / bricks), and leaves the gate unset so a future upgrade can retry.
pub struct SeedPreviewLockedPool;

impl OnRuntimeUpgrade for SeedPreviewLockedPool {
	fn on_runtime_upgrade() -> Weight {
		let db_weight = <Runtime as frame_system::Config>::DbWeight::get();

		// S3 — run at most once per network.
		if frame_support::storage::unhashed::get_or_default::<bool>(MIGRATION_DONE_KEY) {
			log::info!(target: "runtime::migration", "SeedPreviewLockedPool: already applied, skipping");
			return db_weight.reads(1);
		}

		// S1 + S2 — move only the live reserve->locked/treasury gap as a direct, supply-preserving
		// state edit (reserve-guarded inside the ledger host fn). No system transaction is applied.
		match pallet_midnight::Pallet::<Runtime>::seed_pools_to_target(
			TARGET_LOCKED,
			TARGET_TREASURY,
		) {
			Ok(true) => {
				log::info!(
					target: "runtime::migration",
					"SeedPreviewLockedPool: moved reserve gap to target (locked->{TARGET_LOCKED}, treasury->{TARGET_TREASURY})"
				);
				frame_support::storage::unhashed::put(MIGRATION_DONE_KEY, &true);
				db_weight.reads_writes(8, 8)
			},
			Ok(false) => {
				// Pools already at/above target (healthy networks): nothing to move. Mark done.
				log::info!(
					target: "runtime::migration",
					"SeedPreviewLockedPool: pools already at/above target; no move needed"
				);
				frame_support::storage::unhashed::put(MIGRATION_DONE_KEY, &true);
				db_weight.reads_writes(2, 1)
			},
			Err(e) => {
				// S5 — fail safe: e.g. insufficient reserve on a network we did not intend to fix.
				// Do NOT mark done; surface the error and leave the chain healthy (no panic, no
				// brick), so a later upgrade can retry.
				log::error!(
					target: "runtime::migration",
					"SeedPreviewLockedPool: correction not applied, pools left unchanged (will retry): {e:?}"
				);
				db_weight.reads_writes(2, 0)
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

	/// S6 — assert the migration did *exactly* the right thing for this network's pre-state, without
	/// committing. The expected outcome is fully determined by the live pre-state, so we recompute it
	/// and assert the post-state matches — rather than the looser "unchanged-or-at-target", which
	/// would have let a Preview migration that silently failed to move (locked still 0) pass.
	///  * supply is conserved across the three pools (always);
	///  * already at/above target  → clean no-op, gate set;
	///  * under-funded, reserve covers the gap (Preview) → locked **and** treasury reach target,
	///    reserve drops by exactly the moved amount, gate set;
	///  * under-funded, reserve short → fail-safe no-op, gate left UNSET (retryable).
	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let (pre_locked, pre_reserve, pre_treasury): (u128, u128, u128) =
			Decode::decode(&mut &state[..])
				.map_err(|_| "SeedPreviewLockedPool::post_upgrade: decode pre-state failed")?;
		let state_key = pallet_midnight::StateKey::<Runtime>::get();
		let (post_locked, post_reserve, post_treasury) = LedgerApi::get_night_pools(&state_key)
			.map_err(|_| "SeedPreviewLockedPool::post_upgrade: night_pools read failed")?;

		// Supply is conserved regardless of which branch we took.
		frame_support::ensure!(
			pre_locked + pre_reserve + pre_treasury
				== post_locked + post_reserve + post_treasury,
			"SeedPreviewLockedPool: pool supply not conserved across the migration"
		);

		// Recompute the gap the migration would have moved, from the captured pre-state.
		let to_locked = TARGET_LOCKED.saturating_sub(pre_locked);
		let to_treasury = TARGET_TREASURY.saturating_sub(pre_treasury);
		let total = to_locked.saturating_add(to_treasury);
		let gate = frame_support::storage::unhashed::get_or_default::<bool>(MIGRATION_DONE_KEY);
		let unchanged = post_locked == pre_locked
			&& post_treasury == pre_treasury
			&& post_reserve == pre_reserve;

		if total == 0 {
			// Already at/above target (healthy networks): nothing should move, gate set.
			frame_support::ensure!(unchanged, "SeedPreviewLockedPool: at-target network must no-op");
			frame_support::ensure!(gate, "SeedPreviewLockedPool: gate must be set after no-op");
		} else if pre_reserve >= total {
			// Under-funded but reserve covers the gap (Preview): must reach targets exactly, gate set.
			frame_support::ensure!(
				post_locked == pre_locked + to_locked,
				"SeedPreviewLockedPool: locked did not reach target on a fixable network"
			);
			frame_support::ensure!(
				post_treasury == pre_treasury + to_treasury,
				"SeedPreviewLockedPool: treasury did not reach target on a fixable network"
			);
			frame_support::ensure!(
				post_reserve == pre_reserve - total,
				"SeedPreviewLockedPool: reserve did not drop by exactly the moved amount"
			);
			frame_support::ensure!(gate, "SeedPreviewLockedPool: gate must be set after a move");
		} else {
			// Under-funded and reserve can't cover: fail-safe no-op, gate left unset for retry.
			frame_support::ensure!(
				unchanged,
				"SeedPreviewLockedPool: reserve-short network must be left unchanged"
			);
			frame_support::ensure!(
				!gate,
				"SeedPreviewLockedPool: gate must remain unset when the move was rejected"
			);
		}
		Ok(())
	}
}
