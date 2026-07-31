// This file is part of midnight-node.
// Copyright (C) 2025-2026 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Storage migration from v1 to v2: ledger-v8 -> ledger-v9 state translation.
//!
//! This is the on-chain half of the ledger 8 -> 9 hardfork. The pallet stores
//! only the ledger state's arena root in [`crate::StateKey`]; the `LedgerState`
//! itself lives in the ledger arena (parity-db). This migration hands the v8
//! root to the
//! [`migrate_state_v8_to_v9_step`](midnight_node_ledger::host_api::migration_8_to_9)
//! host function, which walks the v8 state, translates it into the v9 shape
//! (see [`midnight_node_ledger::state_translation_v8_to_v9`]), re-persists it,
//! and returns the new v9 root — which we write back into `StateKey`.
//!
//! It is a [`SteppedMigration`] driven by `pallet-migrations`. The translation
//! walks the whole state DAG, so its cost is unbounded in state size: for
//! mainnet-sized state it completes in well under a second (i.e. in the upgrade
//! block itself), but a single-block migration that overran would make the
//! upgrade block take longer than a slot to execute on *every* importing node.
//! Stepping it turns that failure mode into graceful multi-block progress. Each
//! step gets a share of the MBM service weight as its cost-model budget (see
//! [`step_budget_ps`]) and parks the in-flight translation in the ledger arena,
//! handing back an arena key as its cursor.
//!
//! Unlike a `VersionedMigration`, an MBM gets no automatic storage-version gate,
//! so [`MigrateV1ToV2::step`] reproduces it: a chain whose genesis is already
//! ledger-9 starts at storage version 2 and short-circuits on the first step.
//!
//! Storage version 1 does not, however, imply the *ledger* state is still v8:
//! the 2.0.0 runtime already ran ledger-9 yet shipped pallet-midnight at storage
//! version 1 (it had no v1->v2 migration), so a network upgrading 2.0.0 -> this
//! runtime triggers this migration over an already-v9 state. The host function
//! detects that case and no-ops (returns the `StateKey` unchanged), so the only
//! effect on that path is the storage-version bump to 2.
//!
//! ## The ledger is paused while this runs
//!
//! MBM steps are serviced in `frame_executive::inherents_applied()`, i.e. *after*
//! the block's inherents, whereas `OnRuntimeUpgrade` ran *before* them. So from
//! the first instruction of the upgrade block until this migration finishes,
//! `StateKey` still references a v8 state under the v9 ledger API. Everything
//! that would read or write the ledger from an inherent or from `on_finalize` is
//! gated on [`crate::Pallet::ledger_migration_pending`] — see that function for
//! the list. `frame_executive` additionally restricts these blocks to
//! `ExtrinsicInclusionMode::OnlyInherents`, so no user transactions are lost
//! either.

#[cfg(feature = "try-runtime")]
extern crate alloc;

use crate::{Pallet, StateKey, pallet::Config};
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::*,
	weights::WeightMeter,
};
use midnight_node_ledger::types::{TranslationStep, active_ledger_bridge as LedgerApi};
use sp_runtime::Perbill;

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;

use super::PALLET_MIGRATIONS_ID;

/// Upper bound on the serialized in-flight translation cursor (an untagged
/// `TypedArenaKey`, ~40 bytes in practice). Deliberately not
/// [`crate::pallet::StateKeyLength`]: that bound is sized for the ledger state
/// key and leaves no headroom for the cursor's own tag.
pub type TranslationCursorLength = ConstU32<2048>;

/// Share of the per-block MBM service weight handed to the ledger's cost model as
/// one step's translation budget. The remainder absorbs the pallet's own
/// reads/writes plus `pallet-migrations`' bookkeeping, so a step is always
/// affordable out of a full [`WeightMeter`] — which matters because
/// `pallet_migrations::exec_migration` treats [`SteppedMigrationError::InsufficientWeight`]
/// from the *first* migration serviced in a block as a fatal `upgrade_failed`.
const STEP_BUDGET_SHARE: Perbill = Perbill::from_percent(75);

/// Steps taken at one budget level before it is doubled. See
/// [`step_budget_ps`].
const ESCALATE_EVERY_STEPS: u32 = 16;

/// Cap on the number of budget doublings, i.e. 2^20 times the base budget. At
/// the runtime's ~1.2s base that is over a fortnight of cost-model time, reached
/// only after [`ESCALATE_EVERY_STEPS`] * 20 blocks.
const MAX_BUDGET_DOUBLINGS: u32 = 20;

/// Cost-model budget to grant the `steps`-th step, given the MBM service weight
/// limit for this block.
///
/// `Weight::ref_time` is denominated in picoseconds
/// (`WEIGHT_REF_TIME_PER_SECOND == 1e12`), so ledger cost-model picoseconds map
/// 1:1 onto `ref_time` — the same mapping [`crate::Pallet::get_tx_weight`] uses
/// for ordinary transactions.
///
/// The budget *grows* the longer the migration runs, because the ledger's
/// translation engine has a per-state threshold budget below which a step makes
/// no net progress (see
/// [`migration_8_to_9`](midnight_node_ledger::host_api::migration_8_to_9)'s
/// module docs). A state whose threshold exceeds one block's share would
/// otherwise leave the migration — and with it the ledger — stuck forever.
/// Doubling every [`ESCALATE_EVERY_STEPS`] blocks guarantees a step eventually
/// large enough to finish, while a normally-progressing migration completes long
/// before the first doubling.
///
/// Escalation deliberately does *not* raise what the step charges the
/// [`WeightMeter`] (that stays at the base share, which is all the meter can
/// afford). An escalated block therefore does more ledger work than it accounts
/// for — the same over-run the single-block migration this replaced always risked,
/// and only on a state that would otherwise be unmigratable.
fn step_budget_ps(limit: Weight, steps: u32) -> u64 {
	let base = (STEP_BUDGET_SHARE * limit).ref_time();
	let doublings = (steps / ESCALATE_EVERY_STEPS).min(MAX_BUDGET_DOUBLINGS);
	base.saturating_mul(1u64 << doublings)
}

/// Translates the ledger state from v8 to v9 and bumps pallet-midnight storage
/// version 1 -> 2. Wired into `pallet_migrations::Config::Migrations`.
pub struct MigrateV1ToV2<T: Config>(core::marker::PhantomData<T>);

/// Migration cursor: the number of steps taken so far (which drives
/// [`step_budget_ps`]) and the ledger's opaque in-flight translation cursor.
pub type TranslationCursor = (u32, BoundedVec<u8, TranslationCursorLength>);

impl<T: Config> SteppedMigration for MigrateV1ToV2<T> {
	type Cursor = TranslationCursor;
	type Identifier = MigrationId<19>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 1, version_to: 2 }
	}

	/// No cap. The framework compares `max_steps` against the number of *blocks*
	/// elapsed since the migration started, and this translation is unbounded in
	/// state size by design — any cap we picked could trip
	/// `FreezeChainOnFailedMigration` on a large state instead of just taking
	/// another block.
	fn max_steps() -> Option<u32> {
		None
	}

	fn step(
		cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		// Stand-in for the `VersionedMigration<1, 2, ..>` gate MBMs don't provide:
		// a chain whose genesis is already ledger-9 starts at storage version 2
		// and must not translate anything. Only checked on the first step — once
		// we have a cursor the translation is in flight and the version is still 1
		// by construction.
		if cursor.is_none() && !Pallet::<T>::ledger_migration_pending() {
			log::info!(
				target: "midnight::migration",
				"pallet-midnight already at storage version {:?}; skipping ledger v8->v9 migration",
				Pallet::<T>::on_chain_storage_version(),
			);
			return Ok(None);
		}

		let steps = cursor.as_ref().map(|(steps, _)| *steps).unwrap_or(0);
		let budget_ps = step_budget_ps(meter.limit(), steps);

		// Charge only the *base* share, which is what the meter can afford. The
		// meter's limit is the `MaxServiceWeight` the runtime grants MBMs per
		// block; deriving from it (rather than hardcoding a constant) is what makes
		// a step provably affordable out of a full meter. `proof_size` is 0: this
		// chain doesn't build a PoV. The reads/writes are `StateKey` plus the
		// storage version.
		let required = Weight::from_parts(step_budget_ps(meter.limit(), 0), 0)
			.saturating_add(T::DbWeight::get().reads_writes(2, 2));
		if meter.try_consume(required).is_err() {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		// One host call per step: `pallet-migrations` services a migration at most
		// once per block, so an inner loop would only ever run once.
		let state_key = StateKey::<T>::get();
		let cursor_bytes = cursor.as_ref().map(|(_, c)| c.as_slice()).unwrap_or(&[]);
		let step = LedgerApi::migrate_state_v8_to_v9_step(&state_key, cursor_bytes, budget_ps)
			.map_err(|e| {
				log::error!(
					target: "midnight::migration",
					"FATAL: ledger v8->v9 state migration step failed: {e:?}"
				);
				SteppedMigrationError::Failed
			})?;

		match step {
			TranslationStep::Done { state_key } => {
				StateKey::<T>::put(state_key);
				// MBMs don't bump the pallet's `StorageVersion`; do it ourselves so
				// `on_chain_storage_version()` reflects the post-migration ledger
				// version. This is also what un-freezes the ledger (see
				// `Pallet::ledger_migration_pending`), so it must happen only once
				// the state really is v9.
				StorageVersion::new(2).put::<Pallet<T>>();
				log::info!(
					target: "midnight::migration",
					"ledger v8->v9 state migration complete after {} step(s); StateKey re-pointed to v9 root",
					steps.saturating_add(1),
				);
				Ok(None)
			},
			TranslationStep::InProgress { cursor } => {
				let bounded = cursor.try_into().map_err(|_| {
					log::error!(
						target: "midnight::migration",
						"FATAL: ledger v8->v9 translation cursor exceeds {} bytes",
						<TranslationCursorLength as Get<u32>>::get(),
					);
					SteppedMigrationError::Failed
				})?;
				log::info!(
					target: "midnight::migration",
					"ledger v8->v9 state migration in progress after {} step(s) at {budget_ps}ps per step; ledger paused",
					steps.saturating_add(1),
				);
				Ok(Some((steps.saturating_add(1), bounded)))
			},
		}
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		frame_support::ensure!(
			!StateKey::<T>::get().is_empty(),
			"ledger StateKey must be populated before v8->v9 migration"
		);
		Ok(Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		frame_support::ensure!(
			!StateKey::<T>::get().is_empty(),
			"ledger StateKey must remain populated after v8->v9 migration"
		);
		frame_support::ensure!(
			Pallet::<T>::on_chain_storage_version() == 2,
			"pallet-midnight storage version must be 2 after v8->v9 migration"
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// The runtime's MBM service weight: 80% of a 2s `max_block`.
	fn limit() -> Weight {
		Perbill::from_percent(80) * Weight::from_parts(2 * 1_000_000_000_000, u64::MAX)
	}

	#[test]
	fn base_budget_is_the_configured_share_of_the_service_weight() {
		// 75% of 80% of 2s == 1.2s of ledger cost-model time.
		assert_eq!(step_budget_ps(limit(), 0), 1_200_000_000_000);
	}

	#[test]
	fn budget_holds_steady_then_doubles() {
		let base = step_budget_ps(limit(), 0);
		for steps in 0..ESCALATE_EVERY_STEPS {
			assert_eq!(step_budget_ps(limit(), steps), base, "step {steps} must stay at base");
		}
		assert_eq!(step_budget_ps(limit(), ESCALATE_EVERY_STEPS), base * 2);
		assert_eq!(step_budget_ps(limit(), 2 * ESCALATE_EVERY_STEPS), base * 4);
	}

	#[test]
	fn budget_escalation_is_capped() {
		let base = step_budget_ps(limit(), 0);
		let capped = base.saturating_mul(1 << MAX_BUDGET_DOUBLINGS);
		assert_eq!(step_budget_ps(limit(), MAX_BUDGET_DOUBLINGS * ESCALATE_EVERY_STEPS), capped);
		assert_eq!(step_budget_ps(limit(), u32::MAX), capped);
	}
}
