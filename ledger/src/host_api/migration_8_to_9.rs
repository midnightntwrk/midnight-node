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
use midnight_node_ledger_helpers::state_translation_v8_to_v9::StateTranslationTable;

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
pub fn migrate_state_v8_to_v9<D: DB>(
	state_key_v8: &[u8],
) -> Result<(Vec<u8>, u64), LedgerApiError> {
	let t_total = std::time::Instant::now();

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
