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

//! Host-side v8 -> v9 ledger state translation, driven by
//! [`crate::state_translation_v8_to_v9::StateTranslationTable`].
//!
//! The on-chain pallet stores only the arena root of the ledger state
//! (`pallet_midnight::StateKey`, a `tagged_serialize`d `TypedArenaKey<Ledger>`);
//! the `LedgerState` itself lives in the process-global ledger arena (parity-db).
//! When the runtime upgrades from a ledger-8 runtime (spec < 2_000_000) to a
//! ledger-9 runtime (spec >= 2_000_000), the on-chain migration
//! (`pallet_midnight::migrations::v2`) drives
//! [`Ledger9Bridge::migrate_state_v8_to_v9_step`] once per block: it reads the v8
//! arena root, walks the v8 `LedgerState` translating it into a v9 `LedgerState`,
//! re-persists it, and returns the new v9 arena root for the pallet to store back
//! into `StateKey`.
//!
//! The walk is unbounded in state size, so it is *stepped*: each call gets a
//! picosecond budget against the ledger's deterministic cost model, and returns
//! either [`TranslationStep::Done`] or a [`TranslationStep::InProgress`] cursor
//! to hand back next block. The cursor is an (untagged) serialized
//! `TypedArenaKey<TranslationCursor>` — the in-flight `TypedTranslationState` is
//! parked in the arena, so only ~40 bytes cross the host boundary and live in
//! on-chain storage.
//!
//! ## The budget has a per-state floor
//!
//! `TypedTranslationState::run` is **not** resumable at arbitrary granularity,
//! and this is a property of the engine, not of the cursor: every `run` call
//! round-trips its state through `InflightTranslationState`, and that conversion
//! keeps only the memo-cache entries whose *source* node has an `ArenaKey::Ref`
//! key — results memoised against an inlined (small) node live in a transient
//! map that is dropped (`TranslationCacheKey::persist` in midnight-storage's
//! `state_translation`). Work spent below the nearest `Ref` node is therefore
//! thrown away if the budget runs out mid-way.
//!
//! Consequently each state has a threshold budget below which `run` makes no net
//! progress — measured empirically it either sits on a fixed point or cycles
//! between a handful of states. The threshold tracks the state's *node
//! granularity*, not its total size (a 512-entry and a 2048-entry `u128` map both
//! need ~20ms, while the 2048-entry translation as a whole needs ~200ms), so it
//! stays small as state grows. The runtime's per-block budget is ~1.2s, comfortably
//! above it.
//!
//! For scale: the `dev` genesis state the `hardfork_e2e` test forks from
//! (61KB, `ledger-state[v13]`) translates in a single step at any budget from
//! ~140us up, i.e. ~4 orders of magnitude inside one block's share. It is
//! indivisible below that — a consequence of being all-small-nodes — so the
//! multi-block path cannot be exercised on it by shrinking the budget alone;
//! shrinking it past the threshold instead exercises the pallet's escalation.
//!
//! The pallet still guarantees termination rather than relying on that: it grows
//! the per-step budget the longer the migration runs (see
//! `pallet_midnight::migrations::v2`), so a state whose threshold does exceed one
//! block's budget eventually gets a step big enough to finish.
//!
//! v8 and v9 share one storage crate (`ledger-storage-ledger-8`,
//! midnight-storage 2.0.1) and hence one arena, so the translation reads and
//! writes the same parity-db instance the pre-fork ledger-8 blocks populated.

use crate::common::types::TranslationStep;
use crate::ledger_8::api::Ledger as Ledger8;
use crate::ledger_9::api::Ledger as Ledger9;
use crate::ledger_9::types::{DeserializationError, LedgerApiError, SerializationError};
use midnight_node_ledger_helpers::state_translation_v8_to_v9::StateTranslationTable;

use base_crypto::cost_model::CostDuration;
use ledger_storage_ledger_8 as storage;
use midnight_serialize::{
	Deserializable, Serializable, Tagged, tagged_deserialize, tagged_serialize,
};
use storage::{
	Storable,
	arena::{ArenaKey, Sp, TypedArenaKey},
	db::DB,
	state_translation::TypedTranslationState,
	storable::Loader,
	storage::default_storage,
};

type LedgerState8<D> = mn_ledger_8::structure::LedgerState<D>;
type LedgerState9<D> = mn_ledger_9::structure::LedgerState<D>;

/// The in-flight v8 -> v9 translation engine.
type Translation<D> =
	TypedTranslationState<LedgerState8<D>, LedgerState9<D>, StateTranslationTable, D>;

const LOG_TARGET: &str = "midnight::ledger::migration_8_to_9";

/// Storage tag for [`TranslationCursor`]. Versioned explicitly: the cursor is
/// on-chain state for the duration of the hardfork, so its shape must not drift
/// silently.
const TRANSLATION_CURSOR_TAG: &str = "midnight-node:ledger-v8-to-v9-translation-cursor[v1]";

/// Cost the translation engine charges per translated node (a flat 20us
/// heuristic, `midnight-storage`'s `state_translation`). Floor for a step's
/// budget, so a zero budget still translates one node.
const NODE_COST_PS: u64 = 20_000_000;

/// Single-child [`Storable`] wrapper that makes a partially complete
/// [`Translation`] addressable by an `ArenaKey`, so it can be parked in the arena
/// between blocks and resumed from the on-chain cursor.
///
/// The wrapper is needed because [`TypedTranslationState`] carries no `#[tag]`
/// and hence is not [`Tagged`]: `Arena::get_lazy` requires `Storable + Tagged`,
/// the untagged `get_lazy_unversioned` is crate-private, and `Arena::get` is
/// eager with unbounded recursion (it would force the entire work queue and memo
/// cache every block).
///
/// Both `Storable` and `Tagged` are hand-written: the `Storable` derive's
/// generated `tag_unique_factor` emits `<FieldTy>::tag()`, which would
/// re-require `Tagged` on the field we are wrapping precisely because it has
/// none.
struct TranslationCursor<D: DB> {
	state: Sp<Translation<D>, D>,
}

// Hand-written: `#[derive(Clone)]` would add a spurious `D: Clone` bound.
impl<D: DB> Clone for TranslationCursor<D> {
	fn clone(&self) -> Self {
		Self { state: self.state.clone() }
	}
}

impl<D: DB> Tagged for TranslationCursor<D> {
	fn tag() -> std::borrow::Cow<'static, str> {
		std::borrow::Cow::Borrowed(TRANSLATION_CURSOR_TAG)
	}

	fn tag_unique_factor() -> String {
		TRANSLATION_CURSOR_TAG.into()
	}
}

impl<D: DB> Storable<D> for TranslationCursor<D> {
	fn children(&self) -> Vec<ArenaKey<D::Hasher>> {
		vec![Sp::as_child(&self.state)]
	}

	fn to_binary_repr<W: std::io::Write>(&self, _writer: &mut W) -> Result<(), std::io::Error> {
		Ok(())
	}

	fn from_binary_repr<R: std::io::Read>(
		_reader: &mut R,
		child_nodes: &mut impl Iterator<Item = ArenaKey<D::Hasher>>,
		loader: &impl Loader<D>,
	) -> Result<Self, std::io::Error> {
		Ok(Self { state: loader.get_next(child_nodes)? })
	}
}

/// Run one bounded step of the v8 -> v9 ledger state translation.
///
/// * `state_key_v8` — the pallet's `StateKey` bytes (a v8 arena root). Only read
///   when `cursor` is empty, i.e. when starting a fresh translation.
/// * `cursor` — empty to start, otherwise the [`TranslationStep::InProgress`]
///   cursor returned by the previous step, verbatim.
/// * `budget_ps` — picoseconds this step may spend against the ledger's
///   deterministic cost model. Must clear the state's threshold budget for the
///   step to make net progress; see the module docs. A step that makes no
///   progress returns a cursor equal to the one it was given, which is not an
///   error — the caller is expected to retry with a larger budget.
///
/// Returns [`TranslationStep::Done`] with the new v9 arena root (to store back
/// into `StateKey`) once the walk completes, or [`TranslationStep::InProgress`]
/// with the cursor to feed the next step.
///
/// If `state_key_v8` already references a ledger-9 state this is a no-op: it
/// returns `Done` with the key unchanged (see the idempotency guard below).
///
/// Deliberately does *not* flush the arena — `pallet_midnight::on_finalize`
/// flushes once per block, which is the right granularity given blocks are
/// atomic (a restart re-executes the whole block).
pub fn migrate_state_v8_to_v9_step<D: DB>(
	state_key_v8: &[u8],
	cursor: &[u8],
	budget_ps: u64,
) -> Result<TranslationStep, LedgerApiError> {
	let t_total = std::time::Instant::now();
	let arena = &default_storage::<D>().arena;

	// 1. Resume the parked translation, or start a fresh one from the v8 root.
	//
	// `resumed` is the cursor node we loaded, kept so its GC root marking can be
	// dropped once its successor is persisted below.
	let (tl, resumed) = if cursor.is_empty() {
		// Idempotency guard: no-op if the state is already ledger-9.
		//
		// The pallet gates this behind an `on_chain_storage_version() >= 2`
		// check, but the on-chain pallet-midnight storage version is not a
		// faithful proxy for the ledger version. The 2.0.0 runtime (spec
		// 2_000_000) already runs ledger-9 yet shipped pallet-midnight at
		// storage version 1 (it had no v1->v2 migration), so a network upgrading
		// 2.0.0 -> this runtime still triggers this migration even though its
		// `StateKey` already points at a v9 arena root. Feeding that v9 root to
		// the v8 decode below would fail on the tag mismatch and fail the
		// upgrade. Detect it by the root's serialized tag — `TypedArenaKey`'s tag
		// embeds the `LedgerState` version (`storage-key(midnight:ledger-state[vN]:...)`),
		// so a successful tagged decode as a v9 key is a reliable, arena-free
		// discriminator — and return the key unchanged.
		if tagged_deserialize::<TypedArenaKey<Ledger9<D>, D::Hasher>>(&mut &state_key_v8[..])
			.is_ok()
		{
			log::info!(
				target: LOG_TARGET,
				"StateKey already references a ledger-9 state; skipping v8->v9 translation (no-op)"
			);
			return Ok(TranslationStep::Done { state_key: state_key_v8.to_vec() });
		}

		let t_load = std::time::Instant::now();
		let key8: TypedArenaKey<Ledger8<D>, D::Hasher> = tagged_deserialize(&mut &state_key_v8[..])
			.map_err(|e| {
				log::error!(target: LOG_TARGET, "failed to deserialize v8 state key: {e:?}");
				LedgerApiError::Deserialization(DeserializationError::TypedArenaKey)
			})?;
		let ledger8: Sp<Ledger8<D>, D> = arena.get_lazy(&key8).map_err(|e| {
			log::error!(target: LOG_TARGET, "failed to load v8 ledger from arena: {e:?}");
			LedgerApiError::NoLedgerState
		})?;
		log::debug!(target: LOG_TARGET, "[perf] v8->v9 load took {:?}", t_load.elapsed());

		let input: Sp<LedgerState8<D>, D> = Sp::new(ledger8.state.clone());
		let tl = Translation::<D>::start(input).map_err(|e| {
			log::error!(target: LOG_TARGET, "failed to start v8->v9 translation: {e:?}");
			LedgerApiError::HostApiError
		})?;
		(tl, None)
	} else {
		let cursor_key: TypedArenaKey<TranslationCursor<D>, D::Hasher> =
			Deserializable::deserialize(&mut &cursor[..], 0).map_err(|e| {
				log::error!(target: LOG_TARGET, "failed to deserialize translation cursor: {e:?}");
				LedgerApiError::Deserialization(DeserializationError::TypedArenaKey)
			})?;
		let parked: Sp<TranslationCursor<D>, D> = arena.get_lazy(&cursor_key).map_err(|e| {
			log::error!(target: LOG_TARGET, "failed to load translation cursor from arena: {e:?}");
			LedgerApiError::NoLedgerState
		})?;
		let tl = (*parked.state).clone();
		(tl, Some(parked))
	};

	// 2. Advance the translation by one bounded `run`.
	let t_run = std::time::Instant::now();
	let tl = tl
		.run(CostDuration::from_picoseconds(budget_ps.max(NODE_COST_PS)))
		.map_err(|e| {
			log::error!(target: LOG_TARGET, "v8->v9 translation step failed: {e:?}");
			LedgerApiError::HostApiError
		})?;
	let result = tl.result().map_err(|e| {
		log::error!(target: LOG_TARGET, "v8->v9 translation result failed: {e:?}");
		LedgerApiError::HostApiError
	})?;

	// 3. Park the outcome in the arena: either the finished v9 ledger (whose root
	//    the pallet stores in `StateKey`) or the in-flight translation (whose root
	//    becomes the next step's cursor).
	let step = match result {
		Some(state9) => {
			let ledger9 = Ledger9::new((*state9).clone());
			let mut sp9: Sp<Ledger9<D>, D> = arena.alloc(ledger9);
			sp9.persist();
			let mut bytes = Vec::new();
			tagged_serialize(&sp9.as_typed_key(), &mut bytes).map_err(|e| {
				log::error!(target: LOG_TARGET, "failed to serialize v9 state key: {e:?}");
				LedgerApiError::Serialization(SerializationError::TypedArenaKey)
			})?;
			log::info!(
				target: LOG_TARGET,
				"v8->v9 ledger state translation complete (final step took {:?})",
				t_run.elapsed()
			);
			TranslationStep::Done { state_key: bytes }
		},
		None => {
			let mut parked: Sp<TranslationCursor<D>, D> =
				arena.alloc(TranslationCursor { state: arena.alloc(tl) });
			parked.persist();
			// Untagged: the cursor is this migration's private format, and
			// `TranslationCursor`'s tag is long enough to matter against the
			// pallet's bounded cursor.
			let mut bytes = Vec::new();
			Serializable::serialize(&parked.as_typed_key(), &mut bytes).map_err(|e| {
				log::error!(target: LOG_TARGET, "failed to serialize translation cursor: {e:?}");
				LedgerApiError::Serialization(SerializationError::TypedArenaKey)
			})?;
			log::debug!(
				target: LOG_TARGET,
				"v8->v9 translation step incomplete after {:?}; parked cursor ({} bytes)",
				t_run.elapsed(),
				bytes.len()
			);
			TranslationStep::InProgress { cursor: bytes }
		},
	};

	// 4. Drop the GC-root marking on the cursor we resumed from, now that its
	//    successor is persisted, so intermediate roots don't accumulate.
	if let Some(prev) = resumed {
		prev.unpersist();
	}

	log::debug!(target: LOG_TARGET, "[perf] migrate_state_v8_to_v9_step took {:?}", t_total.elapsed());
	Ok(step)
}

#[cfg(test)]
mod tests {
	use super::*;
	use ledger_storage_ledger_8::db::InMemoryDB;

	/// Seed a v8 `LedgerState` into the arena the way a pre-fork ledger-8 node
	/// would, and return the `StateKey` bytes the pallet would hold.
	fn seed_v8_state(state: LedgerState8<InMemoryDB>) -> Vec<u8> {
		let ledger = Ledger8::<InMemoryDB>::new(state);
		let mut sp = default_storage::<InMemoryDB>().arena.alloc(ledger);
		sp.persist();
		let mut bytes = Vec::new();
		tagged_serialize(&sp.as_typed_key(), &mut bytes).expect("serialize v8 state key");
		bytes
	}

	/// A v8 state big enough to need several translation steps: `entries`
	/// `bridge_receiving` rows build a `u128` MPT deep enough that its interior
	/// nodes are separate arena nodes, which is the granularity the translation
	/// engine can actually suspend at.
	fn v8_state_with_bridge_entries(network_id: &str, entries: u32) -> LedgerState8<InMemoryDB> {
		use base_crypto::hash::HashOutput;
		use coin_structure::coin::UserAddress;

		let mut state = LedgerState8::<InMemoryDB>::new(network_id);
		for i in 0..entries {
			let mut bytes = [0u8; 32];
			bytes[..4].copy_from_slice(&i.to_be_bytes());
			state.bridge_receiving =
				state.bridge_receiving.insert(UserAddress(HashOutput(bytes)), i as u128);
		}
		state
	}

	/// Comfortably above the fixtures' threshold budget (see the module docs), so
	/// stepping actually converges, while still far below what the whole walk
	/// needs. Empirically the 2048-entry fixture needs ~20ms per step to progress
	/// and ~200ms in total.
	const STEP_BUDGET_PS: u64 = 25 * 1_000_000_000;

	/// Enough to translate either fixture in a single step.
	const UNBOUNDED_BUDGET_PS: u64 = 60 * 1_000_000_000_000;

	/// Drive `migrate_state_v8_to_v9_step` to completion with `budget_ps` per
	/// step, returning the final v9 `StateKey` bytes and the number of steps.
	fn translate_stepped(state_key_v8: &[u8], budget_ps: u64) -> (Vec<u8>, usize) {
		let mut cursor = Vec::new();
		for steps in 1..=200 {
			match migrate_state_v8_to_v9_step::<InMemoryDB>(state_key_v8, &cursor, budget_ps)
				.expect("translation step")
			{
				TranslationStep::Done { state_key } => return (state_key, steps),
				TranslationStep::InProgress { cursor: next } => {
					assert!(!next.is_empty(), "in-progress cursor must be non-empty");
					assert_ne!(
						next, cursor,
						"step {steps} made no progress: {budget_ps}ps is below this state's \
						 threshold budget",
					);
					cursor = next;
				},
			}
		}
		panic!("translation did not converge in 200 steps at {budget_ps}ps per step")
	}

	/// The load-bearing determinism guarantee: the cursor is on-chain state, so
	/// every node importing the migration blocks must reproduce the same bytes.
	/// A translation split over many bounded steps must therefore land on
	/// exactly the same v9 arena root as one unbounded translation.
	#[test]
	fn stepped_translation_matches_single_shot() {
		let state_key_v8 = seed_v8_state(v8_state_with_bridge_entries("stepped-vs-one-shot", 2048));

		// A budget well below what the whole walk needs, so it is spread over
		// several steps, i.e. several blocks.
		let (stepped, steps) = translate_stepped(&state_key_v8, STEP_BUDGET_PS);
		assert!(steps > 1, "a small budget must force a multi-step translation (took {steps})");

		// Enough budget for the whole walk in one step.
		let (single_shot, one) = translate_stepped(&state_key_v8, UNBOUNDED_BUDGET_PS);
		assert_eq!(one, 1, "an unbounded budget must finish in one step");

		assert_eq!(
			hex::encode(&stepped),
			hex::encode(&single_shot),
			"stepped translation must produce the same v9 root as a single-shot one",
		);
	}

	/// The parked cursor must survive the full serialize -> `get_lazy` -> `run`
	/// round trip, which is what carries the translation across a block boundary.
	#[test]
	fn cursor_round_trips_through_the_arena() {
		let state_key_v8 = seed_v8_state(v8_state_with_bridge_entries("cursor-round-trip", 2048));

		let TranslationStep::InProgress { cursor } =
			migrate_state_v8_to_v9_step::<InMemoryDB>(&state_key_v8, &[], STEP_BUDGET_PS)
				.expect("first step")
		else {
			panic!("a partial budget must leave the translation in progress");
		};

		// The cursor decodes as a typed arena key that resolves in the arena.
		let key: TypedArenaKey<TranslationCursor<InMemoryDB>, sha2::Sha256> =
			Deserializable::deserialize(&mut &cursor[..], 0).expect("cursor decodes");
		default_storage::<InMemoryDB>()
			.arena
			.get_lazy(&key)
			.expect("cursor resolves in the arena");

		// `&[]` for the state key proves the resume path never touches it.
		let resumed = migrate_state_v8_to_v9_step::<InMemoryDB>(&[], &cursor, STEP_BUDGET_PS)
			.expect("resumed step");
		assert_ne!(
			resumed,
			TranslationStep::InProgress { cursor: cursor.clone() },
			"resuming must advance the translation",
		);
	}

	/// The 2.0.0 -> this-runtime path: pallet-midnight is at storage version 1
	/// but `StateKey` already references a ledger-9 root. The step must no-op
	/// rather than fail the v8 decode.
	#[test]
	fn already_v9_state_key_short_circuits() {
		let state = mn_ledger_9::structure::LedgerState::<InMemoryDB>::new("already-v9");
		let ledger = Ledger9::<InMemoryDB>::new(state);
		let mut sp = default_storage::<InMemoryDB>().arena.alloc(ledger);
		sp.persist();
		let mut state_key_v9 = Vec::new();
		tagged_serialize(&sp.as_typed_key(), &mut state_key_v9).expect("serialize v9 state key");

		let step = migrate_state_v8_to_v9_step::<InMemoryDB>(&state_key_v9, &[], STEP_BUDGET_PS)
			.expect("v9 short-circuit");
		assert_eq!(step, TranslationStep::Done { state_key: state_key_v9 });
	}

	/// Dev-only: verify that seeding a v8 genesis blob with *this* crate's
	/// ledger_8 `Ledger` wrapper reproduces the exact arena root a ledger-8 node
	/// (e.g. release 1.0.1) stored in the chain-spec as `genesisStateKey`. If the
	/// wrapper serialization drifted, the seeded root wouldn't match and block
	/// execution after boot would fail to find the state. Runs only when the two
	/// blob paths are provided via env (extracted from a fork-from chain-spec).
	#[test]
	fn v8_genesis_seed_root_matches_chainspec_key() {
		let (Ok(gs_path), Ok(key_path)) =
			(std::env::var("HF_GENESIS_STATE"), std::env::var("HF_GENESIS_KEY"))
		else {
			eprintln!("skipping: set HF_GENESIS_STATE and HF_GENESIS_KEY to run");
			return;
		};
		let genesis = std::fs::read(gs_path).expect("read genesis_state");
		let expected = std::fs::read(key_path).expect("read genesisStateKey");

		let state: LedgerState8<InMemoryDB> =
			midnight_serialize::tagged_deserialize(&mut &genesis[..])
				.expect("deserialize v8 state");
		let got = seed_v8_state(state);

		assert_eq!(
			got,
			expected,
			"seeded v8 root must match the chain-spec genesisStateKey \n got={} \n exp={}",
			hex::encode(&got),
			hex::encode(&expected),
		);
	}
}
