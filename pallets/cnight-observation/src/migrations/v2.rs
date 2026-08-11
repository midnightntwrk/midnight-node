// Copyright (C) Midnight Foundation
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

//! Storage migration v1 → v2: re-apply cNIGHT dust generation after the
//! ledger 8 → 9 hardfork wipes dust state.
//!
//! Every cNIGHT UTXO this pallet observed fed a `Create` event into the ledger's
//! dust generating set. The hardfork wipes that state, so without this migration
//! every cNIGHT holder would silently stop generating DUST. Two parts:
//!
//! * [`RecordPreForkState`] — single-block, and **must run before**
//!   `pallet_midnight::migrations::v2` in the runtime's `Migrations` tuple: it
//!   saves the still-untranslated ledger-8 arena root, which is the only place
//!   the wiped entries' night `value` and dust `owner` survive.
//! * [`MigrateV1ToV2`] — the multi-block replay. It pages through `UtxoOwners`
//!   (read-only: the provenance-and-liveness filter for which nonces are
//!   cnight's and still live), asks the host for each nonce's pre-wipe
//!   `(value, owner)`, and applies one `CNightGeneratesDustUpdate` per step.
//!   `process_tokens` is gated off for the duration by the storage version.
//!
//! The restored generation entries are field-for-field identical to the wiped
//! ones. Only the accrual clock moves: the original `ctime` is not in ledger
//! state (it lives on the dust *UTXO*, which the wipe takes), so the replay
//! stamps `fork block time - dust.time_to_cap()`. DUST accrues linearly from
//! `ctime` to a cap of `night_value * night_dust_ratio` reached after
//! `time_to_cap` (~1 week), so backdating by exactly that much puts every holder
//! at their cap the moment the replay lands — the pre-fork steady state, since
//! anyone holding cNIGHT for a week was already capped.
//!
//! Stamping the fork block itself instead would be an equally arbitrary clock
//! that starts everyone at zero and refills over a week, in proportion to
//! holdings: large holders recover in minutes, small ones are locked out of
//! paying fees for days. The real per-UTXO `ctime` is only available from
//! db-sync, and would restore holders to the same cap anyway for all but the
//! youngest UTXOs — while making the hardfork depend on a new
//! consensus-critical mainchain query. The chosen offset over-credits only
//! cNIGHT locked in the last week, bounded by a cap it would reach regardless.
//!
//! Only cnight's slice of the generating set is restored. Native NIGHT registers
//! generation entries too, and nothing in this repo records which of those the
//! wipe took.
//!
//! The wipe itself lives in the translation table
//! (`midnight_node_ledger_helpers::state_translation_v8_to_v9`), which replaces
//! the v8 dust state with the empty one. Should that ever stop being true, this
//! migration self-cancels rather than corrupting state: the first replayed
//! `Create` collides with `GenerationInfoAlreadyPresent` (see
//! [`MigrateV1ToV2::step`]).

extern crate alloc;

use alloc::vec::Vec;
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::*,
	traits::OnRuntimeUpgrade,
	weights::WeightMeter,
};
use midnight_node_ledger::types::active_ledger_bridge as LedgerApi;
use midnight_primitives::{
	LedgerBlockContextProvider, LedgerStateProvider, MidnightSystemTransactionExecutor,
};

use super::PALLET_MIGRATIONS_ID;
use crate::{
	Config, DustReapplyCtime, DustReapplyProgress, Event, Pallet, PreForkStateKey, UtxoActionType,
	UtxoOwners,
};

const LOG_TARGET: &str = "cnight-observation::migration";

/// Nonces restored per step, and hence per host call and per system transaction.
///
/// Matches `DEFAULT_CARDANO_TX_CAPACITY_PER_BLOCK`: a batch this size is one
/// `CNightGeneratesDustUpdate` that `process_tokens` already applies in a single
/// block in production, so it is known to fit. It also bounds the blast radius
/// of a failed batch.
pub const MAX_REAPPLY_BATCH: u32 = 200;

/// Saves the pre-hardfork ledger-8 arena root for [`MigrateV1ToV2`] to read the
/// wiped dust entries' values and owners from.
///
/// Single-block and O(1). Must sit *before* `pallet_midnight::migrations::v2` in
/// the runtime `Migrations` tuple — that migration replaces
/// `pallet_midnight::StateKey` with the translated v9 root.
pub struct RecordPreForkState<T: Config>(core::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for RecordPreForkState<T> {
	fn on_runtime_upgrade() -> Weight {
		let weight = T::DbWeight::get().reads_writes(2, 1);

		if Pallet::<T>::on_chain_storage_version() >= 2 {
			return weight;
		}
		if PreForkStateKey::<T>::exists() {
			// Should be impossible: `pallet_migrations` blocks `set_code` while
			// an MBM is in flight, so the replay cannot still be holding a key.
			log::error!(
				target: LOG_TARGET,
				"pre-fork ledger state key is already set; leaving it alone rather than overwriting"
			);
			return weight;
		}

		PreForkStateKey::<T>::put(T::LedgerStateProvider::get_ledger_state_key());
		Pallet::<T>::deposit_event(Event::<T>::DustReapplyStarted);
		log::info!(target: LOG_TARGET, "recorded pre-fork ledger state key for the dust generation replay");

		weight
	}
}

/// Replays cnight's dust generation entries into the post-hardfork ledger state,
/// one `UtxoOwners` page per step.
pub struct MigrateV1ToV2<T: Config>(core::marker::PhantomData<T>);

impl<T: Config> SteppedMigration for MigrateV1ToV2<T> {
	/// The last `UtxoOwners` nonce processed.
	type Cursor = T::Hash;
	type Identifier = MigrationId<25>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 1, version_to: 2 }
	}

	fn step(
		cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		// One batch per step, and — by charging half a block — one step per block.
		//
		// The weight model cannot pace this: `process_tokens`' benchmark observes
		// *registration* UTXOs, which never reach the ledger, so its ~15ms for 200
		// UTXOs says nothing about 200 dust `Create`s. Against
		// `MbmServiceWeight` (80% of the block) that would service ~100 batches —
		// 20k ledger dust creates — in a single block. Half a block is over the
		// service budget for a second step and under it for the first, so exactly
		// one batch lands per block, and never the fatal
		// `required > MaxServiceWeight`.
		//
		// The cost is latency, and it is small: mainnet's live set was ~4.9k
		// nonces on 2026-08-06 (preview ~1.5k, preprod ~85), i.e. ~25 batches,
		// so ~25 blocks (~2.5 min) of gated observation. The observer re-delivers
		// everything afterwards.
		let required = Weight::from_parts(T::BlockWeights::get().max_block.ref_time() / 2, 0);
		if meter.remaining().any_lt(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}
		let _ = meter.try_consume(required);

		// Never return `Err` below this point: steps run under
		// `FreezeChainOnFailedMigration`, so any error freezes the chain. Every
		// failure path instead winds the replay up and lets the observer resume.
		let Some(pre_fork_key) = PreForkStateKey::<T>::get() else {
			log::info!(
				target: LOG_TARGET,
				"no pre-fork ledger state key recorded; nothing to replay"
			);
			return Ok(cancel::<T>());
		};

		// Read-only paging: `UtxoOwners` is not drained, it stays the live set.
		let mut iter = match cursor {
			Some(last) => UtxoOwners::<T>::iter_from(UtxoOwners::<T>::hashed_key_for(last)),
			None => UtxoOwners::<T>::iter(),
		};
		let nonces: Vec<T::Hash> =
			iter.by_ref().take(MAX_REAPPLY_BATCH as usize).map(|(nonce, _)| nonce).collect();

		let Some(last) = nonces.last().copied() else {
			return Ok(complete::<T>());
		};

		let raw_nonces: Vec<[u8; 32]> = nonces.iter().map(|nonce| nonce.0).collect();
		let (time_to_cap, values) =
			match LedgerApi::dust_generation_values_v8(&pre_fork_key, raw_nonces) {
				Ok(values) => values,
				Err(e) => {
					// The pre-fork arena root has been reaped, or (defensively) is
					// not a ledger-8 root at all. Nothing to restore from.
					log::error!(
						target: LOG_TARGET,
						"pre-fork dust generation state is unreadable ({e:?}); abandoning the replay"
					);
					return Ok(cancel::<T>());
				},
			};

		// Stamped once, on the first step that has something to restore, and
		// reused by every later batch so the whole set shares one clock. Steps
		// run in `inherents_applied()`, i.e. after the timestamp inherent, so
		// `tblock` is the current block's own time; backdating it by
		// `time_to_cap` puts every restored entry straight at its DUST cap.
		let ctime = match DustReapplyCtime::<T>::get() {
			Some(ctime) => ctime,
			None => {
				let tblock = T::LedgerBlockContextProvider::get_block_context().tblock;
				let ctime = tblock.saturating_sub(time_to_cap);
				DustReapplyCtime::<T>::put(ctime);
				ctime
			},
		};

		let mut skipped = 0u32;
		let mut events = Vec::with_capacity(nonces.len());
		for (nonce, value) in nonces.iter().zip(values) {
			// `None`: the nonce is untracked in the v8 dust state, or was
			// already destroyed there (both logged host-side).
			let Some((night_value, owner)) = value else {
				skipped = skipped.saturating_add(1);
				continue;
			};
			match LedgerApi::construct_cnight_generates_dust_event(
				night_value,
				&owner,
				ctime,
				UtxoActionType::Create as u8,
				nonce.0,
			) {
				Ok(event) => events.push(event),
				Err(e) => {
					log::error!(target: LOG_TARGET, "failed to construct replay event: {e:?}");
					skipped = skipped.saturating_add(1);
				},
			}
		}

		let (restored_so_far, _) = DustReapplyProgress::<T>::get();
		let mut applied = events.len() as u32;
		if !events.is_empty() && !apply_batch::<T>(events) {
			if restored_so_far == 0 {
				// Nothing has been restored yet, so the likely reason is that the
				// hardfork did not wipe dust after all: re-applying a surviving
				// `Create` fails with `GenerationInfoAlreadyPresent`. This is the
				// self-cancel that keeps the migration inert against a
				// translation that carries dust across. (Keyed on "nothing
				// restored" rather than "first
				// batch" because a leading page can legitimately resolve to no
				// events at all, and then never apply anything.)
				log::warn!(
					target: LOG_TARGET,
					"replay batch failed with nothing restored yet; assuming dust state survived the hardfork and cancelling the replay"
				);
				return Ok(cancel::<T>());
			}
			// A failed batch left the ledger state untouched (the ledger
			// propagates the first event's error out of the whole system
			// transaction, and `mut_ledger_state` only writes on success), so
			// carrying on with the next page is safe.
			Pallet::<T>::deposit_event(Event::<T>::DustReapplyBatchFailed { nonces });
			skipped = skipped.saturating_add(applied);
			applied = 0;
		}

		DustReapplyProgress::<T>::mutate(|(total_applied, total_skipped)| {
			*total_applied = total_applied.saturating_add(applied);
			*total_skipped = total_skipped.saturating_add(skipped);
		});

		Ok(Some(last))
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		// Count only: `UtxoOwners` is chain-scale, never snapshot it.
		Ok((UtxoOwners::<T>::iter_keys().count() as u64).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use frame_support::ensure;

		let live: u64 =
			Decode::decode(&mut state.as_slice()).expect("pre_upgrade count must decode");

		ensure!(
			Pallet::<T>::on_chain_storage_version() == 2,
			"storage version must be 2 after the dust replay"
		);
		ensure!(
			UtxoOwners::<T>::iter_keys().count() as u64 == live,
			"the dust replay must not change the live UtxoOwners set"
		);
		ensure!(
			PreForkStateKey::<T>::get().is_none(),
			"pre-fork ledger state key must be cleared after the dust replay"
		);
		ensure!(
			DustReapplyCtime::<T>::get().is_none(),
			"replay ctime must be cleared after the dust replay"
		);
		ensure!(
			DustReapplyProgress::<T>::get() == (0, 0),
			"replay progress must be cleared after the dust replay"
		);

		Ok(())
	}
}

/// Applies one batch as a single `CNightGeneratesDustUpdate`, the same pair of
/// calls `process_tokens` makes. Returns false (having logged) on failure.
///
/// `execute_system_transaction` deposits `pallet_midnight_system`'s own
/// `SystemTransactionApplied` event carrying the serialized transaction, which
/// is the indexer's hook — this pallet's variant is deliberately not emitted,
/// its `CmstHeader` being a Cardano position that has no meaning here.
fn apply_batch<T: Config>(events: Vec<Vec<u8>>) -> bool {
	let tx = match LedgerApi::construct_cnight_generates_dust_system_tx(events) {
		Ok(tx) => tx,
		Err(e) => {
			log::error!(target: LOG_TARGET, "failed to construct replay system tx: {e:?}");
			return false;
		},
	};

	match T::MidnightSystemTransactionExecutor::execute_system_transaction(tx) {
		Ok(_) => true,
		Err(e) => {
			log::error!(target: LOG_TARGET, "replay batch failed to apply: {e:?}");
			false
		},
	}
}

/// Wind the replay up without restoring anything, and let the observer resume.
fn cancel<T: Config>() -> Option<T::Hash> {
	clear_transient::<T>();
	Pallet::<T>::deposit_event(Event::<T>::DustReapplySkipped);
	finish::<T>()
}

/// Wind the replay up after the last page, reporting the tallies.
fn complete<T: Config>() -> Option<T::Hash> {
	let (applied, skipped) = DustReapplyProgress::<T>::get();
	clear_transient::<T>();
	Pallet::<T>::deposit_event(Event::<T>::DustReapplyCompleted { applied, skipped });
	log::info!(target: LOG_TARGET, "dust generation replay complete: {applied} applied, {skipped} skipped");
	finish::<T>()
}

fn clear_transient<T: Config>() {
	PreForkStateKey::<T>::kill();
	DustReapplyCtime::<T>::kill();
	DustReapplyProgress::<T>::kill();
}

/// MBMs don't bump the pallet's `StorageVersion`; do it ourselves so
/// `process_tokens` starts accepting observations again.
fn finish<T: Config>() -> Option<T::Hash> {
	StorageVersion::new(2).put::<Pallet<T>>();
	None
}
