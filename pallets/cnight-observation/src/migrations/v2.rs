// This file is part of midnight-node.
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
//!   `(value, owner)`, and applies one `CNightGeneratesDustUpdate` per batch.
//!
//! The wipe itself lives in the ledger team's translation table (the
//! `v8-to-v9-state-translation` crate), which replaces the v8 dust state with the
//! empty one.
extern crate alloc;

use alloc::vec::Vec;
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::*,
	traits::OnRuntimeUpgrade,
	weights::WeightMeter,
};
use midnight_node_ledger::{
	host_api::ledger_8::{DustGenerationValues, ledger_8_bridge as Ledger8Api},
	types::active_ledger_bridge as LedgerApi,
};
use midnight_primitives::{
	LedgerBlockContextProvider, LedgerStateProvider, MidnightSystemTransactionCNightExecutor,
};

use super::PALLET_MIGRATIONS_ID;
use crate::{
	Config, DustReapplyCtime, DustReapplyProgress, Event, Pallet, PreForkStateKey, UtxoActionType,
	UtxoOwners,
};

const LOG_TARGET: &str = "cnight-observation::migration";

/// Nonces restored per batch, and hence per host call and per system transaction.
///
/// This is the granularity at which [`MigrateV1ToV2::step`] packs the MBM weight
/// budget — it prices each batch and stops at the first one the budget left cannot
/// pay for — so a smaller batch packs the budget more tightly (7 x 25 = 175
/// `Create`s per block against 3 x 50 = 150) and keeps the blast radius of a failed
/// batch small.
pub const MAX_REAPPLY_BATCH: u32 = 25;

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
		// Every event this migration deposits is named in its log line: most of them
		// are invisible to explorers that group events under their extrinsic (see
		// `apply_batch`), so the log is where you go looking for them.
		log::info!(
			target: LOG_TARGET,
			"DustReapplyStarted: recorded pre-fork ledger state key for the dust generation replay"
		);

		weight
	}
}

/// Replays cnight's dust generation entries into the post-hardfork ledger state,
/// one `UtxoOwners` page per step.
pub struct MigrateV1ToV2<T: Config>(core::marker::PhantomData<T>);

impl<T: Config> SteppedMigration for MigrateV1ToV2<T> {
	/// The last `UtxoOwners` nonce processed, or `None` for "no page done yet" —
	/// which is a cursor [`Self::step`] can hand back, and so cannot be left to
	/// `Option<Self::Cursor>` (there `None` already means "the migration is done").
	type Cursor = Option<T::Hash>;
	type Identifier = MigrationId<25>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 1, version_to: 2 }
	}

	fn step(
		cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		// `pallet_migrations` runs exactly one `step` per migration per block
		// ("A migration cannot progress more than one step per block, we therefore
		// break", `substrate/frame/migrations/src/lib.rs`), so spending the block's
		// MBM budget means looping here rather than charging a whole block per batch.
		let mut last = cursor.flatten();
		loop {
			match replay_batch::<T>(last, meter) {
				Batch::Done => return Ok(None),
				// The block is full, so end the step *without* moving the cursor.
				Batch::Deferred(cost) => {
					meter.consume(cost);
					return Ok(Some(last));
				},
				// A batch the ledger rejected on its merits was tallied and skipped, and
				// ends the step for the same reason. This may happen if ledger work is applied
				// eariler in the block via another pallet's inherent.
				Batch::Failed(page, cost) => {
					meter.consume(cost);
					return Ok(Some(Some(page)));
				},
				// Priced above what is left of this block: nothing was applied, so the
				// same page is retried next block against a fresh budget.
				Batch::OutOfBudget => return Ok(Some(last)),
				Batch::Applied(page, cost) => {
					meter.consume(cost);
					last = Some(page);
				},
			}
		}
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

/// The outcome of one [`replay_batch`] call.
enum Batch<C> {
	/// A page landed, ending at cursor `C`, having cost `Weight` — priced before it
	/// was applied, so the meter can take the figure as it stands.
	Applied(C, Weight),
	/// The ledger turned a page away because the block is already full. Nothing was
	/// tallied and the cursor has not moved, so the page is re-read next block; the
	/// `Weight` is what the ledger charged for turning it away.
	Deferred(Weight),
	/// A page failed to apply on its merits and was tallied, at the cost the ledger
	/// charged for rejecting it. `C` is past it, but the step ends.
	Failed(C, Weight),
	/// The page prices above what is left of this step's budget. Nothing was applied
	/// and the cursor has not moved; the same page is retried next block.
	OutOfBudget,
	/// The replay is wound up: `complete`/`cancel` have already deposited their
	/// event, cleared the transient storage and bumped the storage version.
	Done,
}

/// One page of `UtxoOwners` restored into the post-hardfork ledger state.
///
/// `meter` is read-only here: it says whether the page's price fits, and
/// [`MigrateV1ToV2::step`] does the consuming.
fn replay_batch<T: Config>(cursor: Option<T::Hash>, meter: &WeightMeter) -> Batch<T::Hash> {
	let Some(pre_fork_key) = PreForkStateKey::<T>::get() else {
		log::info!(
			target: LOG_TARGET,
			"no pre-fork ledger state key recorded; nothing to replay"
		);
		cancel::<T>();
		return Batch::Done;
	};

	// Read-only paging: `UtxoOwners` is not drained, it stays the live set.
	let mut iter = match cursor {
		Some(last) => UtxoOwners::<T>::iter_from(UtxoOwners::<T>::hashed_key_for(last)),
		None => UtxoOwners::<T>::iter(),
	};
	let nonces: Vec<T::Hash> =
		iter.by_ref().take(MAX_REAPPLY_BATCH as usize).map(|(nonce, _)| nonce).collect();

	let Some(last) = nonces.last().copied() else {
		complete::<T>();
		return Batch::Done;
	};

	let raw_nonces: Vec<[u8; 32]> = nonces.iter().map(|nonce| nonce.0).collect();
	let DustGenerationValues { time_to_cap, entries } =
		match Ledger8Api::dust_generation_values(&pre_fork_key, raw_nonces) {
			Ok(values) => values,
			Err(e) => {
				// The pre-fork arena root has been reaped, or (defensively) is
				// not a ledger-8 root at all. Nothing to restore from.
				log::error!(
					target: LOG_TARGET,
					"pre-fork dust generation state is unreadable ({e:?}); abandoning the replay"
				);
				cancel::<T>();
				return Batch::Done;
			},
		};

	// Stamped once, on the first batch that has something to restore, and
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

	let batch_size = nonces.len() as u32;
	let mut skipped = 0u32;
	let mut events = Vec::with_capacity(nonces.len());
	for (nonce, entry) in nonces.iter().zip(entries) {
		// `None`: the nonce is untracked in the v8 dust state, or was
		// already destroyed there (both logged host-side).
		let Some(entry) = entry else {
			skipped = skipped.saturating_add(1);
			continue;
		};
		match LedgerApi::construct_cnight_generates_dust_event(
			entry.value,
			&entry.owner,
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

	let mut applied = events.len() as u32;
	let mut failed = false;
	// The storage this page touched is chargeable whatever becomes of it, so check it
	// before doing anything else. A page that resolved to no events — every nonce
	// untracked in the v8 state, or unconstructible — applies nothing, so this is also
	// all it costs.
	let mut cost = batch_db_weight::<T>(batch_size);
	if !meter.can_consume(cost) {
		return Batch::OutOfBudget;
	}

	if !events.is_empty() {
		match apply_batch::<T>(events, cost, meter) {
			Ok(charged) => cost = charged,
			Err(BatchFailure::OutOfBudget) => return Batch::OutOfBudget,
			Err(BatchFailure::Hopeless(price)) => {
				// Give up rather than bisect: a page is only 25 nonces (~11% of a
				// block), so a batch that outprices a whole fresh budget will never fit
				// one, and the observer is better off resuming than waiting.
				log::error!(
					target: LOG_TARGET,
					"replay batch prices at {} ref_time, above the {} this migration may spend in a whole block; abandoning the replay",
					price.ref_time(),
					meter.limit().ref_time(),
				);
				cancel::<T>();
				return Batch::Done;
			},
			Err(BatchFailure::Deferred(charged)) => return Batch::Deferred(charged),
			Err(BatchFailure::Rejected(charged)) => {
				cost = charged;
				// A failed batch left the ledger state untouched (the ledger
				// propagates the first event's error out of the whole system
				// transaction, and `mut_ledger_state` only writes on success).
				log::warn!(
					target: LOG_TARGET,
					"DustReapplyBatchFailed: {applied} nonces in this batch were not restored; retrying from the next page in the next block"
				);
				Pallet::<T>::deposit_event(Event::<T>::DustReapplyBatchFailed { nonces });
				skipped = skipped.saturating_add(applied);
				applied = 0;
				failed = true;
			},
		}
	}

	DustReapplyProgress::<T>::mutate(|(total_applied, total_skipped)| {
		*total_applied = total_applied.saturating_add(applied);
		*total_skipped = total_skipped.saturating_add(skipped);
	});

	if failed {
		return Batch::Failed(last, cost);
	}
	Batch::Applied(last, cost)
}

/// Substrate storage the step touches per batch, on top of the ledger's own cost:
/// `PreForkStateKey` (1R), `DustReapplyCtime` (1R, +1W on the first productive
/// batch), `DustReapplyProgress` (1R for the tally + 1R/1W for the mutate),
/// pallet-midnight's `StateKey` (1R/1W inside `execute_system_transaction`), and one
/// `UtxoOwners` read per nonce.
fn batch_db_weight<T: Config>(nonces: u32) -> Weight {
	T::DbWeight::get().reads_writes(5u64.saturating_add(nonces.into()), 3)
}

/// Why a batch did not apply, from [`apply_batch`].
enum BatchFailure {
	/// The ledger refused it because the block is already full, not because of
	/// anything about the batch. The `Weight` is what it cost anyway.
	Deferred(Weight),
	/// The ledger rejected it on its merits, or it could not even be constructed. The
	/// `Weight` is what it cost anyway.
	Rejected(Weight),
	/// It prices above what is left of this step's budget, so it was not applied.
	OutOfBudget,
	/// It prices above what this migration may spend in a whole block, so no block
	/// will ever afford it and retrying is pointless.
	Hopeless(Weight),
}

/// Prices one batch, checks the price against the step's budget, and only then
/// applies it as a single `CNightGeneratesDustUpdate` — the same pair of calls
/// `process_tokens` makes. Returns what it cost, `db` included.
fn apply_batch<T: Config>(
	events: Vec<Vec<u8>>,
	db: Weight,
	meter: &WeightMeter,
) -> Result<Weight, BatchFailure> {
	let tx = match LedgerApi::construct_cnight_generates_dust_system_tx(events) {
		Ok(tx) => tx,
		Err(e) => {
			log::error!(target: LOG_TARGET, "failed to construct replay system tx: {e:?}");
			return Err(BatchFailure::Rejected(db));
		},
	};

	// Calculate batch cost via Ledger's cost model.
	let gas = match LedgerApi::get_transaction_cost(
		&T::LedgerStateProvider::get_ledger_state_key(),
		&tx,
		T::LedgerBlockContextProvider::get_block_context(),
		T::BlockWeights::get().max_block.ref_time(),
	) {
		Ok(gas) => gas,
		Err(e) => {
			log::warn!(
				target: LOG_TARGET,
				"could not price the replay batch ({e:?}); rejecting."
			);
			return Err(BatchFailure::Rejected(Weight::zero()));
		},
	};
	let cost = db.saturating_add(Weight::from_parts(gas, 0));

	// 90% of the limit, not all of it: the meter is block-wide and `pallet_migrations`
	// charges its own bookkeeping against it before `step` runs, so a batch priced in
	// the top sliver of the limit would never fit a block and would spin on the
	// out-of-budget retry forever.
	if cost.ref_time() > meter.limit().ref_time() / 100 * 90 {
		return Err(BatchFailure::Hopeless(cost));
	}
	if !meter.can_consume(cost) {
		return Err(BatchFailure::OutOfBudget);
	}

	match T::MidnightSystemTransactionExecutor::execute_system_transaction(tx) {
		Ok(_) => Ok(cost),
		Err(e) if T::MidnightSystemTransactionExecutor::is_block_limit_exceeded(&e) => {
			log::warn!(
				target: LOG_TARGET,
				"replay batch did not fit the rest of this block ({e:?}); deferring the same page to the next block"
			);
			Err(BatchFailure::Deferred(cost))
		},
		Err(e) => {
			log::error!(target: LOG_TARGET, "replay batch failed to apply: {e:?}");
			Err(BatchFailure::Rejected(cost))
		},
	}
}

/// Wind the replay up short of the last page, and let the observer resume.
///
/// Reports the tallies like [`complete`] does: every caller but the missing-key one
/// can fire on any page, so whatever was restored up to that page stays restored and
/// is the operator's only record of how far the replay got.
///
/// The caller has already logged *why*; this logs the event that goes with it.
fn cancel<T: Config>() {
	let (applied, skipped) = DustReapplyProgress::<T>::get();
	clear_transient::<T>();
	Pallet::<T>::deposit_event(Event::<T>::DustReapplySkipped { applied, skipped });
	log::warn!(
		target: LOG_TARGET,
		"DustReapplySkipped: dust generation replay abandoned, {applied} applied, {skipped} skipped"
	);
	finish::<T>()
}

/// Wind the replay up after the last page, reporting the tallies.
fn complete<T: Config>() {
	let (applied, skipped) = DustReapplyProgress::<T>::get();
	clear_transient::<T>();
	Pallet::<T>::deposit_event(Event::<T>::DustReapplyCompleted { applied, skipped });
	log::info!(
		target: LOG_TARGET,
		"DustReapplyCompleted: dust generation replay complete, {applied} applied, {skipped} skipped"
	);
	finish::<T>()
}

fn clear_transient<T: Config>() {
	PreForkStateKey::<T>::kill();
	DustReapplyCtime::<T>::kill();
	DustReapplyProgress::<T>::kill();
}

/// MBMs don't bump the pallet's `StorageVersion`; do it ourselves so
/// `process_tokens` starts accepting observations again.
fn finish<T: Config>() {
	StorageVersion::new(2).put::<Pallet<T>>();
}
