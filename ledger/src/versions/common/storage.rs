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

#[cfg(feature = "std")]
use std::path::Path;

use super::LOG_TARGET;
use super::ledger_storage_local::{
	db::ParityDb,
	storage::{try_get_default_storage, unsafe_drop_default_storage},
};

pub fn drop_default_storage_if_exists() {
	if try_get_default_storage::<ParityDb>().is_some() {
		unsafe_drop_default_storage::<ParityDb>();
		log::info!(
			target: LOG_TARGET,
			"Dropped HF storage after rollback"
		);
	}
}

#[cfg(feature = "std")]
use {
	super::ledger_storage_local::db::DB,
	super::midnight_serialize_local::Tagged,
	super::mn_ledger_local::structure::{ProofMarker, SignatureKind, Transaction},
	super::transient_crypto_local::commitment::PureGeneratorPedersen,
};

pub fn get_root(state: &[u8]) -> Vec<u8> {
	// Get empty state key
	use super::api::Ledger;
	use super::ledger_storage_local::{DefaultDB, storage::default_storage};

	let state: super::mn_ledger_local::structure::LedgerState<DefaultDB> =
		super::midnight_serialize_local::tagged_deserialize(state)
			.expect("Failed to deserialize initial state");
	let state = Ledger::new(state);
	let state = default_storage::<DefaultDB>().arena.alloc(state);
	let mut bytes = vec![];
	super::midnight_serialize_local::tagged_serialize(&state.as_typed_key(), &mut bytes).unwrap();
	bytes
}

#[cfg(feature = "std")]
fn alloc_with_initial_state<S: SignatureKind<D>, D: DB>(initial_state: &[u8]) -> Vec<u8>
where
	Transaction<S, ProofMarker, PureGeneratorPedersen, D>: Tagged,
{
	use super::api::Ledger;
	use super::ledger_storage_local::storage::default_storage;

	let state: super::mn_ledger_local::structure::LedgerState<D> =
		super::midnight_serialize_local::tagged_deserialize(&mut &initial_state[..])
			.expect("failed to deserialize ledger genesis state");
	let state = Ledger::new(state);

	let mut state = default_storage::<D>().arena.alloc(state);
	state.persist();
	default_storage::<D>().with_backend(|backend| backend.flush_all_changes_to_db());
	let mut bytes = vec![];
	super::midnight_serialize_local::tagged_serialize(&state.as_typed_key(), &mut bytes).unwrap();
	bytes
}

#[cfg(feature = "std")]
pub fn init_storage_paritydb<P: AsRef<Path>>(
	dir: P,
	genesis_state: &[u8],
	cache_size: usize,
) -> Vec<u8> {
	use super::base_crypto_local::signatures::Signature;
	use super::ledger_storage_local::{Storage, db::ParityDb, storage::set_default_storage};

	let res = set_default_storage(|| {
		std::fs::create_dir_all(dir.as_ref())
			.unwrap_or_else(|_| panic!("Failed to create dir {}", dir.as_ref().display()));

		let db = ParityDb::<sha2::Sha256>::open(dir.as_ref());
		Storage::new(cache_size, db)
	});
	if res.is_err() {
		log::warn!("Warning: Failed to set default storage: {res:?}");
	}

	alloc_with_initial_state::<Signature, ParityDb>(genesis_state)
}
