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

//! Host-side batched read of the *pre-fork* (ledger-8) dust generation state.
//!
//! The ledger 8 -> 9 hardfork wipes dust state, so `pallet-cnight-observation`
//! has to re-apply the cNIGHT-generates-DUST entries it fed the ledger over the
//! chain's life (see `pallet_cnight_observation::migrations::v2`). Its own
//! storage records only which nonces are cnight's (`UtxoOwners`) — the night
//! `value` and the dust `owner` of each entry live in the about-to-be-wiped
//! `DustGenerationInfo`, which is what this module reads back out.
//!
//! It reads the *v8* state through the arena root the migration saved before
//! translation (`pallet_cnight_observation::PreForkStateKey`), which stays
//! resolvable because the arena retains historical ledger states. Reading a v8
//! state from the v9 bridge is the same trick `serve_pre_migration_v8_read!`
//! plays in [`crate::host_api::ledger_9`].

use crate::common::types::{DustGenerationEntry, DustGenerationValues};
use crate::ledger_8::api::Ledger as Ledger8;
use crate::ledger_9::types::{DeserializationError, LedgerApiError};

use base_crypto::{hash::HashOutput, time::Timestamp};
use ledger_storage_ledger_8 as storage;
use midnight_serialize::{Serializable, tagged_deserialize};
use mn_ledger_8::dust::InitialNonce;
use storage::{
	arena::{Sp, TypedArenaKey},
	db::DB,
	storage::default_storage,
};

const LOG_TARGET: &str = "midnight::ledger::dust_generation";

/// The still-generating entry of each requested initial nonce in the ledger-8
/// dust state referenced by `state_key`, plus that state's dust `time_to_cap`.
///
/// `time_to_cap` is how far the caller backdates the replayed `ctime` so every
/// restored entry lands at its DUST cap, i.e. at the balance it held before the
/// wipe. It comes from the v8 state because that is the one already loaded here,
/// and the 8 -> 9 translation recasts `parameters.dust` unchanged.
///
/// Errors with `NoLedgerState` when `state_key` is not a ledger-8 state. This
/// must *not* be an all-`None` success: the caller's key is only v8 because its
/// `RecordPreForkState` migration runs before the pallet-midnight translation,
/// so a future reorder of the runtime `Migrations` tuple would otherwise
/// silently restore nothing, chain-wide, detectable only by absent DUST.
pub fn dust_generation_values_v8<D: DB>(
	state_key: &[u8],
	nonces: &[[u8; 32]],
) -> Result<DustGenerationValues, LedgerApiError> {
	if !crate::is_ledger_8_state_key(state_key) {
		log::error!(
			target: LOG_TARGET,
			"pre-fork state key is not a ledger-8 arena root; refusing to serve dust generation values"
		);
		return Err(LedgerApiError::NoLedgerState);
	}

	let key8: TypedArenaKey<Ledger8<D>, D::Hasher> = tagged_deserialize(&mut &state_key[..])
		.map_err(|e| {
			log::error!(target: LOG_TARGET, "failed to deserialize v8 state key: {e:?}");
			LedgerApiError::Deserialization(DeserializationError::TypedArenaKey)
		})?;
	// One arena load, amortised over the whole batch.
	let ledger8: Sp<Ledger8<D>, D> = default_storage::<D>().arena.get_lazy(&key8).map_err(|e| {
		log::error!(target: LOG_TARGET, "failed to load v8 ledger from arena: {e:?}");
		LedgerApiError::NoLedgerState
	})?;
	let generation = &ledger8.state.dust.generation;
	// Non-negative by construction (`night_dust_ratio / generation_decay_rate`).
	let time_to_cap = ledger8.state.parameters.dust.time_to_cap().as_seconds().max(0) as u64;

	let entries = nonces
		.iter()
		.map(|nonce| {
			// Same lookup path the ledger's own `Destroy` handler takes:
			// nonce -> leaf index -> generating tree leaf.
			let Some(index) = generation.night_indices.get(&InitialNonce(HashOutput(*nonce))) else {
				// The caller only asks about nonces it believes are live, so an
				// untracked one is a pallet/ledger divergence, same as a destroyed
				// one below.
				log::warn!(target: LOG_TARGET, "nonce {} is not tracked in the v8 dust state", hex::encode(nonce));
				return None;
			};
			let Some((_, info)) = generation.generating_tree.index(*index) else {
				log::error!(
					target: LOG_TARGET,
					"invariant violated: `night_indices` entry for {} not backed in `generating_tree`",
					hex::encode(nonce),
				);
				return None;
			};
			// A destroyed entry keeps its leaf forever, with `dtime` rewritten.
			// The caller's `UtxoOwners` is meant to be exactly the live set, so
			// this is a pallet/ledger divergence worth surfacing.
			if info.dtime != Timestamp::MAX {
				log::warn!(
					target: LOG_TARGET,
					"nonce {} is already destroyed in the v8 dust state (dtime {:?}); not restoring",
					hex::encode(nonce),
					info.dtime,
				);
				return None;
			}
			let mut owner = Vec::new();
			if let Err(e) = Serializable::serialize(&info.owner, &mut owner) {
				log::error!(target: LOG_TARGET, "failed to serialize dust owner: {e:?}");
				return None;
			}
			Some(DustGenerationEntry { value: info.value, owner })
		})
		.collect();

	Ok(DustGenerationValues { time_to_cap, entries })
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ledger_9::api::Ledger as Ledger9;
	use ledger_storage_ledger_8::db::InMemoryDB;
	use midnight_serialize::tagged_serialize;
	use mn_ledger_8::{
		dust::DustPublicKey,
		structure::{
			CNightGeneratesDustActionType, CNightGeneratesDustEvent, LedgerState, SystemTransaction,
		},
	};
	use transient_crypto::curve::Fr;

	const NONCE_LIVE: [u8; 32] = [1; 32];
	const NONCE_DESTROYED: [u8; 32] = [2; 32];
	const NONCE_UNKNOWN: [u8; 32] = [3; 32];

	fn event(
		nonce: [u8; 32],
		value: u128,
		owner: DustPublicKey,
		action: CNightGeneratesDustActionType,
		time_secs: u64,
	) -> CNightGeneratesDustEvent {
		CNightGeneratesDustEvent {
			value,
			owner,
			time: Timestamp::from_secs(time_secs),
			action,
			nonce: InitialNonce(HashOutput(nonce)),
		}
	}

	/// Persist `ledger` into the default in-memory arena and return its root, in
	/// the same `StateKey` shape the pallet stores.
	fn root_of<T: storage::Storable<InMemoryDB> + midnight_serialize::Tagged>(value: T) -> Vec<u8> {
		let mut sp = default_storage::<InMemoryDB>().arena.alloc(value);
		sp.persist();
		let mut bytes = Vec::new();
		tagged_serialize(&sp.as_typed_key(), &mut bytes).expect("serialize root");
		bytes
	}

	/// A v8 dust state holding a live and a destroyed cnight entry.
	fn v8_root(owner: DustPublicKey) -> Vec<u8> {
		use CNightGeneratesDustActionType::*;
		let state = LedgerState::<InMemoryDB>::new("local-test");
		let (state, _) = state
			.apply_system_tx(
				&SystemTransaction::CNightGeneratesDustUpdate {
					events: vec![
						event(NONCE_LIVE, 100, owner, Create, 1_000),
						event(NONCE_DESTROYED, 200, owner, Create, 1_000),
						event(NONCE_DESTROYED, 200, owner, Destroy, 2_000),
					],
				},
				Timestamp::from_secs(2_000),
			)
			.expect("apply v8 cnight dust update");
		root_of(Ledger8::new(state))
	}

	#[test]
	fn serves_live_entries_and_skips_destroyed_and_unknown() {
		let owner = DustPublicKey(Fr::from(7u64));
		let root = v8_root(owner);

		let DustGenerationValues { time_to_cap, entries } =
			dust_generation_values_v8::<InMemoryDB>(
				&root,
				&[NONCE_LIVE, NONCE_DESTROYED, NONCE_UNKNOWN],
			)
			.expect("v8 root must resolve");

		let mut expected_owner = Vec::new();
		Serializable::serialize(&owner, &mut expected_owner).unwrap();

		assert_eq!(
			entries,
			vec![Some(DustGenerationEntry { value: 100, owner: expected_owner }), None, None]
		);
		assert_eq!(
			time_to_cap,
			mn_ledger_8::structure::INITIAL_PARAMETERS.dust.time_to_cap().as_seconds() as u64,
			"the served cap offset must be the state's own dust parameter",
		);
	}

	/// The migration only ever holds a v8 key by construction, so a v9 key means
	/// the migration order changed underneath us — that must fail loudly rather
	/// than restore nothing.
	#[test]
	fn v9_state_key_is_rejected() {
		let root = root_of(Ledger9::new(mn_ledger_9::structure::LedgerState::<InMemoryDB>::new(
			"local-test",
		)));

		assert!(matches!(
			dust_generation_values_v8::<InMemoryDB>(&root, &[NONCE_LIVE]),
			Err(LedgerApiError::NoLedgerState),
		));
	}
}
