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
//!   `process_tokens` is gated off for the duration by the storage version. Each
//!   batch is priced from the ledger's own cost model, and a step applies as many
//!   as the MBM weight budget affords.
//!
//! The restored generation entries are field-for-field identical to the wiped
//! ones. Only the accrual clock moves: the original `ctime` is not publicly
//! visible in ledger state (it is stored as a commitment only), so the replay
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
use midnight_node_ledger::types::{DustGenerationValues, active_ledger_bridge as LedgerApi};
use midnight_primitives::{
	LedgerBlockContextProvider, LedgerStateProvider, MidnightSystemTransactionExecutor,
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
/// budget — it keeps applying batches until the ledger's price for the next one no
/// longer fits — so a smaller batch packs the budget more tightly (7 x 25 = 175
/// `Create`s per block against 3 x 50 = 150) and keeps the blast radius of a failed
/// batch small. The cost is one extra `dust_generation_values_v8` call and one extra
/// system transaction per batch, both negligible against the ledger work a batch
/// does.
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
	/// The last `UtxoOwners` nonce processed.
	type Cursor = T::Hash;
	type Identifier = MigrationId<25>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 1, version_to: 2 }
	}

	fn step(
		mut cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		// `pallet_migrations` runs exactly one `step` per migration per block
		// ("A migration cannot progress more than one step per block, we therefore
		// break", `substrate/frame/migrations/src/lib.rs`), so spending the block's
		// MBM budget means looping here rather than charging a whole block per batch.
		//
		// The first batch of every step runs unconditionally and the loop stops by
		// returning a cursor: `step` must never return
		// `SteppedMigrationError::InsufficientWeight`, because on the first step that
		// routes to `upgrade_failed` and the runtime's `FreezeChainOnFailedMigration`
		// freezes the chain. The same reason no path below returns `Err` — every
		// failure winds the replay up instead and lets the observer resume.
		//
		// Because the ledger normalizes each batch's cost against its own
		// `block_limits`, keeping the summed batch weight inside the meter's limit
		// also keeps the block inside the ledger's own per-block fullness accounting.
		// Summing per-batch maxima over-approximates true per-dimension accumulation,
		// so that side errs conservative and needs no separate cap.
		loop {
			match replay_batch::<T>(cursor) {
				Batch::Done => return Ok(None),
				// A batch that failed to apply ends the step: inside a loop "the next
				// page" is the same block, where a fullness-driven rejection would
				// repeat for every remaining page and tally them all as skipped. The
				// next block retries with a fresh ledger fullness budget.
				Batch::Failed(last) => return Ok(Some(last)),
				Batch::Applied(last, cost) => {
					if meter.try_consume(cost).is_err() {
						// The batch is already applied, so bring `consumed` exactly to
						// the limit rather than past it — `WeightMeter::consume`
						// carries a `debug_assert!(consumed <= limit)`.
						meter.consume(meter.remaining());
						return Ok(Some(last));
					}
					cursor = Some(last);
					// Every batch is the same shape, so the one just measured is the
					// estimate for the next.
					if !meter.can_consume(cost) {
						return Ok(cursor);
					}
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
	/// A page landed, ending at cursor `C`, at the given weight.
	Applied(C, Weight),
	/// A page failed to apply and was tallied. `C` is past it, but the step ends.
	Failed(C),
	/// The replay is wound up: `complete`/`cancel` have already deposited their
	/// event, cleared the transient storage and bumped the storage version.
	Done,
}

/// One page of `UtxoOwners` restored into the post-hardfork ledger state.
fn replay_batch<T: Config>(cursor: Option<T::Hash>) -> Batch<T::Hash> {
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
		match LedgerApi::dust_generation_values_v8(&pre_fork_key, raw_nonces) {
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

	let (restored_so_far, _) = DustReapplyProgress::<T>::get();
	let mut applied = events.len() as u32;
	let mut failed = false;
	// A page that resolved to no events at all applies nothing and so measures
	// nothing; charge the pessimistic figure rather than hand the loop a near-zero
	// estimate it would then overrun.
	let mut gas = fallback_gas::<T>();

	if !events.is_empty() {
		match apply_batch::<T>(events) {
			Ok(measured) => gas = measured,
			Err(()) => {
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
					cancel::<T>();
					return Batch::Done;
				}
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
		return Batch::Failed(last);
	}
	Batch::Applied(last, Weight::from_parts(gas, 0) + batch_db_weight::<T>(batch_size))
}

/// Substrate storage the step touches per batch, on top of the ledger's own cost:
/// `PreForkStateKey` (1R), `DustReapplyCtime` (1R, +1W on the first productive
/// batch), `DustReapplyProgress` (1R for the tally + 1R/1W for the mutate),
/// pallet-midnight's `StateKey` (1R/1W inside `execute_system_transaction`), and one
/// `UtxoOwners` read per nonce.
///
/// Three orders of magnitude below a batch's ledger gas (~4e8 ps against ~2.2e11 at
/// 25 nonces), but it is exactly the part the ledger's cost model does not see.
fn batch_db_weight<T: Config>(nonces: u32) -> Weight {
	T::DbWeight::get().reads_writes(5u64.saturating_add(nonces.into()), 3)
}

/// The charge for a batch the ledger could not price: half a block, which is what
/// this migration charged unconditionally before it asked the ledger. Pessimistic,
/// and already known to be accepted by the meter — `MbmServiceWeight` is 80% of the
/// block — so it costs latency, never a stall.
fn fallback_gas<T: Config>() -> u64 {
	T::BlockWeights::get().max_block.ref_time() / 2
}

/// Applies one batch as a single `CNightGeneratesDustUpdate`, the same pair of
/// calls `process_tokens` makes. Returns the ledger's own price for the batch,
/// scaled to a full [`MAX_REAPPLY_BATCH`], or (having logged) `Err` on failure.
///
/// `execute_system_transaction` deposits `pallet_midnight_system`'s own
/// `SystemTransactionApplied` event carrying the serialized transaction, which
/// is the indexer's hook — this pallet's variant is deliberately not emitted,
/// its `CmstHeader` being a Cardano position that has no meaning here.
///
/// That event is **block-scoped, not extrinsic-scoped**: steps run in
/// `inherents_applied()`, after the block's inherents, so the event's phase is an
/// `ApplyExtrinsic` index one past the last extrinsic and no extrinsic claims it.
/// Consumers must key on the event, not on a matching extrinsic — the indexer
/// already does; the toolkit fetcher did not, and was fixed alongside this
/// migration (`util/toolkit/src/fetcher/compute_task.rs`).
///
/// The same goes for everything else this migration deposits — `DustReapply*`
/// here, and `pallet_migrations`' own `MigrationAdvanced`/`MigrationCompleted`.
/// A block explorer that renders events grouped under their extrinsic shows none
/// of them, which makes the replay look like it never ran past
/// `DustReapplyStarted` (that one is emitted from `on_runtime_upgrade`, phase
/// `Initialization`, so it is visible). They are all in `System::Events`.
fn apply_batch<T: Config>(events: Vec<Vec<u8>>) -> Result<u64, ()> {
	let count = events.len() as u64;
	let tx = match LedgerApi::construct_cnight_generates_dust_system_tx(events) {
		Ok(tx) => tx,
		Err(e) => {
			log::error!(target: LOG_TARGET, "failed to construct replay system tx: {e:?}");
			return Err(());
		},
	};

	// What the ledger says this batch costs, from its own cost model: the
	// `CNightGeneratesDustUpdate` arm of `SystemTransaction::cost`, normalized
	// against `parameters.limits.block_limits` and scaled to the block's max
	// weight. Ledger picoseconds map 1:1 onto `ref_time` with `proof_size` 0, the
	// same convention `pallet_midnight::get_tx_weight` and
	// `pallet_midnight::migrations::v2` use — this chain doesn't build a PoV.
	//
	// Reading the live (post-hardfork) state key is safe here: the Executive
	// `Migrations` tuple runs `pallet_midnight::migrations::v2` in
	// `on_runtime_upgrade`, before the `inherents_applied()` where MBM steps run, so
	// by the first step it already points at the translated v9 root.
	//
	// Divided out per nonce and scaled back up to a full batch, so a short final
	// page cannot understate what the next full one will cost.
	let gas = match LedgerApi::get_transaction_cost(
		&T::LedgerStateProvider::get_ledger_state_key(),
		&tx,
		T::LedgerBlockContextProvider::get_block_context(),
		T::BlockWeights::get().max_block.ref_time(),
	) {
		Ok(gas) => (gas / count.max(1)).saturating_mul(MAX_REAPPLY_BATCH.into()),
		Err(e) => {
			log::warn!(
				target: LOG_TARGET,
				"could not price the replay batch ({e:?}); charging the pessimistic fallback"
			);
			fallback_gas::<T>()
		},
	};

	match T::MidnightSystemTransactionExecutor::execute_system_transaction(tx) {
		Ok(_) => Ok(gas),
		Err(e) => {
			log::error!(target: LOG_TARGET, "replay batch failed to apply: {e:?}");
			Err(())
		},
	}
}

/// Wind the replay up without restoring anything, and let the observer resume.
///
/// The caller has already logged *why*; this logs the event that goes with it.
fn cancel<T: Config>() {
	clear_transient::<T>();
	Pallet::<T>::deposit_event(Event::<T>::DustReapplySkipped);
	log::warn!(target: LOG_TARGET, "DustReapplySkipped: dust generation replay abandoned, nothing restored");
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
