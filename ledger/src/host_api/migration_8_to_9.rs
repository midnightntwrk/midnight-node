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

//! Host-side v8 -> v9 ledger state translation, driven by the ledger team's
//! [`v8_to_v9_state_translation::StateTranslationTable`].
//!
//! The on-chain pallet stores only the arena root of the ledger state
//! (`pallet_midnight::StateKey`, a `tagged_serialize`d `TypedArenaKey<Ledger>`);
//! the `LedgerState` itself lives in the process-global ledger arena (parity-db).
//! When the runtime upgrades from a ledger-8 runtime (spec < 2_000_000) to a
//! ledger-9 runtime (spec >= 2_000_000), the on-chain migration
//! (`pallet_midnight::migrations`) calls [`Ledger9Bridge::migrate_state_v8_to_v9`]
//! which lands here: it reads the v8 arena root, walks the v8 `LedgerState`
//! translating it into a v9 `LedgerState`, re-persists it, and returns the new v9
//! arena root for the pallet to store back into `StateKey`.
//!
//! v8 and v9 share one storage crate (`ledger-storage-ledger-8`,
//! midnight-storage 2.0.1) and hence one arena, so the translation reads and
//! writes the same parity-db instance the pre-fork ledger-8 blocks populated.

use crate::ledger_8::api::Ledger as Ledger8;
use crate::ledger_9::api::Ledger as Ledger9;
use crate::ledger_9::types::{DeserializationError, LedgerApiError, SerializationError};
use v8_to_v9_state_translation::StateTranslationTable;

use base_crypto::cost_model::CostDuration;
use ledger_storage_ledger_8 as storage;
use midnight_serialize::{tagged_deserialize, tagged_serialize};
use storage::{
	arena::{Sp, TypedArenaKey},
	db::DB,
	state_translation::TypedTranslationState,
	storage::default_storage,
};

type LedgerState8<D> = mn_ledger_8::structure::LedgerState<D>;
type LedgerState9<D> = mn_ledger_9::structure::LedgerState<D>;

const LOG_TARGET: &str = "midnight::ledger::migration_8_to_9";

/// Picoseconds granted to each `TypedTranslationState::run` step. The migration
/// is single-block (it must complete within one host call), so we loop over
/// `run` with a per-step budget until the translation reports a result.
///
/// This budget also doubles as the quantization granularity of the synthetic
/// cost reported back to the pallet (see `consumed_cost_ps` below): `run`
/// doesn't return unused budget, so every completed step is charged in full.
/// 10ms keeps that over-approximation small relative to real translation
/// costs while still comfortably draining dev/undeployed-sized state in a
/// handful of iterations; the loop cap is a runaway backstop only.
const RUN_BUDGET_PS: u64 = 10_000_000_000;
const MAX_STEPS: usize = 100_000;

/// Translate the ledger state referenced by a v8 arena root (`state_key_v8`,
/// the pallet's `StateKey` bytes) into a v9 ledger state, persist it, and
/// return the new v9 arena root (to store back into `StateKey`) together with
/// the synthetic cost, in picoseconds, the translation consumed against the
/// ledger's deterministic cost model — for the pallet to charge as this
/// migration's weight.
///
/// If `state_key_v8` already references a ledger-9 state this is a no-op: it
/// returns the key unchanged with zero cost (see the idempotency guard below).
pub fn migrate_state_v8_to_v9<D: DB>(
	state_key_v8: &[u8],
) -> Result<(Vec<u8>, u64), LedgerApiError> {
	let t_total = std::time::Instant::now();

	// 0. Idempotency guard: no-op if the state is already ledger-9.
	//
	// The pallet gates this behind `VersionedMigration<1, 2>`, but the on-chain
	// pallet-midnight storage version is not a faithful proxy for the ledger
	// version. The 2.0.0 runtime (spec 2_000_000) already runs ledger-9 yet
	// shipped pallet-midnight at storage version 1 (it had no v1->v2 migration),
	// so a network upgrading 2.0.0 -> this runtime still triggers this migration
	// even though its `StateKey` already points at a v9 arena root. Feeding that
	// v9 root to the v8 decode below would fail on the tag mismatch and abort the
	// upgrade (the pallet `expect`s success), bricking the chain. Detect it by
	// the root's serialized tag — `TypedArenaKey`'s tag embeds the `LedgerState`
	// version (`storage-key(midnight:ledger-state[vN]:...)`), so a successful
	// tagged decode as a v9 key is a reliable, arena-free discriminator — and
	// return the key unchanged with zero synthetic cost.
	if tagged_deserialize::<TypedArenaKey<Ledger9<D>, D::Hasher>>(&mut &state_key_v8[..]).is_ok() {
		log::info!(
			target: LOG_TARGET,
			"StateKey already references a ledger-9 state; skipping v8->v9 translation (no-op)"
		);
		return Ok((state_key_v8.to_vec(), 0));
	}

	// 1. Decode the v8 arena root and load the v8 ledger wrapper from the arena.
	let t_load = std::time::Instant::now();
	let key8: TypedArenaKey<Ledger8<D>, D::Hasher> = tagged_deserialize(&mut &state_key_v8[..])
		.map_err(|e| {
			log::error!(target: LOG_TARGET, "failed to deserialize v8 state key: {e:?}");
			LedgerApiError::Deserialization(DeserializationError::TypedArenaKey)
		})?;
	let ledger8: Sp<Ledger8<D>, D> = default_storage::<D>().arena.get_lazy(&key8).map_err(|e| {
		log::error!(target: LOG_TARGET, "failed to load v8 ledger from arena: {e:?}");
		LedgerApiError::NoLedgerState
	})?;
	log::debug!(target: LOG_TARGET, "[perf] migrate_state_v8_to_v9 load took {:?}", t_load.elapsed());

	// 2. Run the state translation table over the inner v8 `LedgerState`.
	let t_translate = std::time::Instant::now();
	let input: Sp<LedgerState8<D>, D> = Sp::new(ledger8.state.clone());
	let mut tl =
		TypedTranslationState::<LedgerState8<D>, LedgerState9<D>, StateTranslationTable, D>::start(
			input,
		)
		.map_err(|e| {
			log::error!(target: LOG_TARGET, "failed to start v8->v9 translation: {e:?}");
			LedgerApiError::HostApiError
		})?;

	let run_budget = CostDuration::from_picoseconds(RUN_BUDGET_PS);
	let mut steps = 0usize;
	let state9: Sp<LedgerState9<D>, D> = loop {
		steps += 1;
		if steps > MAX_STEPS {
			log::error!(target: LOG_TARGET, "v8->v9 translation did not converge in {MAX_STEPS} steps");
			return Err(LedgerApiError::HostApiError);
		}
		tl = tl.run(run_budget).map_err(|e| {
			log::error!(target: LOG_TARGET, "v8->v9 translation step failed: {e:?}");
			LedgerApiError::HostApiError
		})?;
		if let Some(result) = tl.result().map_err(|e| {
			log::error!(target: LOG_TARGET, "v8->v9 translation result failed: {e:?}");
			LedgerApiError::HostApiError
		})? {
			break result;
		}
	};
	// Every completed `run` call before the last one must have exhausted its
	// full `RUN_BUDGET_PS` — otherwise `tl.result()` would already have been
	// `Some` and the loop would have stopped there. Only the final call may
	// have used less. `steps * RUN_BUDGET_PS` is therefore a deterministic
	// upper bound on the synthetic cost actually consumed, accurate to one
	// `RUN_BUDGET_PS` quantum.
	let consumed_cost_ps = steps as u64 * RUN_BUDGET_PS;
	log::info!(
		target: LOG_TARGET,
		"v8->v9 ledger state translation complete in {steps} step(s), {:?}, {consumed_cost_ps}ps synthetic cost",
		t_translate.elapsed()
	);

	// 3. Wrap the translated state in the v9 ledger wrapper, persist it, and
	//    flush the arena so the new root is durable before the pallet stores it.
	let t_persist = std::time::Instant::now();
	let ledger9 = Ledger9::new((*state9).clone());
	let mut sp9: Sp<Ledger9<D>, D> = default_storage::<D>().arena.alloc(ledger9);
	sp9.persist();
	default_storage::<D>().with_backend(|backend| backend.flush_all_changes_to_db());
	log::debug!(
		target: LOG_TARGET,
		"[perf] migrate_state_v8_to_v9 persist+flush took {:?}",
		t_persist.elapsed()
	);

	// 4. Serialize the new v9 arena root for the pallet to store in `StateKey`.
	let mut bytes = Vec::new();
	tagged_serialize(&sp9.as_typed_key(), &mut bytes).map_err(|e| {
		log::error!(target: LOG_TARGET, "failed to serialize v9 state key: {e:?}");
		LedgerApiError::Serialization(SerializationError::TypedArenaKey)
	})?;

	log::debug!(target: LOG_TARGET, "[perf] migrate_state_v8_to_v9 took {:?}", t_total.elapsed());
	Ok((bytes, consumed_cost_ps))
}

#[cfg(test)]
mod tests {
	use super::*;
	use ledger_storage_ledger_8::db::InMemoryDB;
	use midnight_serialize::Tagged;
	use std::borrow::Cow;
	use std::ops::Deref;
	use storage::state_translation::TranslationTable;

	/// Drive the imported translation table to completion over an in-memory arena.
	fn translate_to_completion(v8: LedgerState8<InMemoryDB>) -> LedgerState9<InMemoryDB> {
		let tl = TypedTranslationState::<
			LedgerState8<InMemoryDB>,
			LedgerState9<InMemoryDB>,
			StateTranslationTable,
			InMemoryDB,
		>::start(Sp::new(v8))
		.expect("failed to start translation");

		let finished = tl
			.run(CostDuration::from_picoseconds(1_000_000_000_000))
			.expect("translation failed");

		finished
			.result()
			.expect("failed to get result")
			.expect("translation did not complete")
			.deref()
			.clone()
	}

	/// Every `TranslationId` a table entry requires must itself be in the table,
	/// or translation errors at runtime the first time the entry is needed.
	#[test]
	fn table_is_closed() {
		<StateTranslationTable as TranslationTable<InMemoryDB>>::assert_closure();
	}

	/// The imported `TABLE` hardcodes tag string literals, and upstream validates them
	/// against *its* dependency pins — not this workspace's `[patch.crates-io]` rc pins.
	/// If a tag on either the v8 or v9 side drifts (e.g. an rc bump changes a `#[tag]`),
	/// the literal no longer matches what `T::tag()` produces here and the migration
	/// silently mis-dispatches. Rebuild every expected ID from the node's actual crate
	/// types and compare.
	#[test]
	fn table_tags_match_types() {
		use storage::merkle_patricia_trie::{MerklePatriciaTrie, Node};
		use storage::storable::SizeAnn;

		type V8Ann = mn_ledger_8::annotation::NightAnn;
		type V9Ann = mn_ledger_9::annotation::NightAnn;
		type V8Contract = onchain_state_ledger_8::state::ContractState<InMemoryDB>;
		type V9Contract = onchain_state_ledger_9::state::ContractState<InMemoryDB>;

		let expected: Vec<(Cow<'static, str>, Cow<'static, str>)> = vec![
			(LedgerState8::<InMemoryDB>::tag(), LedgerState9::<InMemoryDB>::tag()),
			(
				mn_ledger_8::structure::LedgerParameters::tag(),
				mn_ledger_9::structure::LedgerParameters::tag(),
			),
			(V8Contract::tag(), V9Contract::tag()),
			(
				MerklePatriciaTrie::<V8Contract, InMemoryDB, V8Ann>::tag(),
				MerklePatriciaTrie::<V9Contract, InMemoryDB, V9Ann>::tag(),
			),
			(
				Node::<V8Contract, InMemoryDB, V8Ann>::tag(),
				Node::<V9Contract, InMemoryDB, V9Ann>::tag(),
			),
			(u128::tag(), u128::tag()),
			(
				MerklePatriciaTrie::<u128, InMemoryDB, SizeAnn>::tag(),
				MerklePatriciaTrie::<u128, InMemoryDB, V9Ann>::tag(),
			),
			(Node::<u128, InMemoryDB, SizeAnn>::tag(), Node::<u128, InMemoryDB, V9Ann>::tag()),
		];

		let actual: Vec<_> = <StateTranslationTable as TranslationTable<InMemoryDB>>::TABLE
			.iter()
			.map(|(id, _)| (id.0.clone(), id.1.clone()))
			.collect();

		assert_eq!(actual, expected);
	}

	/// The translation wipes dust: whatever generation/utxo state v8 held, the v9 side
	/// comes out as the empty state genesis starts from. `pallet_cnight_observation`'s
	/// dust re-apply migration is built entirely on this being true, and upstream has no
	/// test for it.
	#[test]
	fn dust_state_is_wiped() {
		let mut v8 = LedgerState8::<InMemoryDB>::new("test-network");
		let mut dust = (*v8.dust).clone();
		dust.generation.generating_tree_first_free = 7;
		dust.utxo.commitments_first_free = 3;
		v8.dust = Sp::new(dust);

		let v9 = translate_to_completion(v8);

		assert_eq!(*v9.dust, mn_ledger_9::dust::DustState::default());
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
		let ledger = Ledger8::<InMemoryDB>::new(state);
		let mut sp = default_storage::<InMemoryDB>().arena.alloc(ledger);
		sp.persist();
		let mut got = Vec::new();
		tagged_serialize(&sp.as_typed_key(), &mut got).expect("serialize key");

		assert_eq!(
			got,
			expected,
			"seeded v8 root must match the chain-spec genesisStateKey \n got={} \n exp={}",
			hex::encode(&got),
			hex::encode(&expected),
		);
	}
}
