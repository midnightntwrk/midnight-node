// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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
use {
	ledger_storage::db::DB,
	midnight_serialize::{self, Tagged},
	mn_ledger::structure::{ProofMarker, SignatureKind, Transaction},
	transient_crypto::commitment::PureGeneratorPedersen,
};

pub fn get_root(state: &[u8]) -> Vec<u8> {
	// Get empty state key
	use super::latest::api::Ledger;
	use ledger_storage::{DefaultDB, storage::default_storage};

	let state: mn_ledger::structure::LedgerState<DefaultDB> =
		midnight_serialize::tagged_deserialize(state).expect("Failed to deserialize initial state");
	let state = Ledger::new(state);
	let state = default_storage::<DefaultDB>().arena.alloc(state);
	let mut bytes = vec![];
	midnight_serialize::tagged_serialize(&state.hash(), &mut bytes).unwrap();
	bytes
}

#[cfg(feature = "std")]
fn alloc_with_initial_state<S: SignatureKind<D>, D: DB>(initial_state: &[u8]) -> Vec<u8>
where
	Transaction<S, ProofMarker, PureGeneratorPedersen, D>: Tagged,
{
	use super::latest::api::Ledger;
	use ledger_storage::storage::default_storage;

	let state: mn_ledger::structure::LedgerState<D> =
		midnight_serialize::tagged_deserialize(&mut &initial_state[..])
			.expect("failed to deserialize ledger genesis state");
	let state = Ledger::new(state);

	let state = default_storage::<D>().arena.alloc(state);
	state.persist();
	default_storage::<D>().with_backend(|backend| backend.flush_all_changes_to_db());
	let mut bytes = vec![];
	midnight_serialize::tagged_serialize(&state.hash(), &mut bytes).unwrap();
	bytes
}

#[cfg(feature = "std")]
pub fn init_storage_paritydb(
	dir: &std::path::Path,
	genesis_state: &[u8],
	cache_size: usize,
) -> Vec<u8> {
	use base_crypto::signatures::Signature;
	use ledger_storage::{Storage, db::ParityDb, storage::set_default_storage};

	let res = set_default_storage(|| {
		std::fs::create_dir_all(dir)
			.unwrap_or_else(|_| panic!("Failed to create dir {}", dir.display()));

		let db = ParityDb::<sha2::Sha256>::open(dir);
		Storage::new(cache_size, db)
	});
	if res.is_err() {
		log::warn!("Warning: Failed to set default storage: {res:?}");
	}

	alloc_with_initial_state::<Signature, ParityDb>(genesis_state)
}

#[cfg(test)]
mod tests {
	use frame_support::assert_ok;
	use ledger_storage::{
		Storage,
		db::ParityDb,
		storage::{set_default_storage, try_get_default_storage, unsafe_drop_default_storage},
	};
	use ledger_storage_hf::{
		Storage as StorageHF, db::ParityDb as ParityDbHF,
		storage::set_default_storage as set_default_storage_hf,
	};
	use std::path::PathBuf;

	#[test]
	fn set_and_drop_default_storage() {
		let mut db_path: PathBuf = std::env::temp_dir();
		db_path.push("node/chain");

		{
			// Set default storage
			let res = set_default_storage(|| {
				std::fs::create_dir_all(&db_path).unwrap_or_else(|err| {
					panic!("Failed to create dir {}, err {}", db_path.display(), err)
				});

				let db = ParityDb::<sha2::Sha256>::open(&db_path);

				Storage::new(0, db)
			});

			assert_ok!(res);
		}

		// Drop default storage
		unsafe_drop_default_storage::<ParityDb>();
		assert!(try_get_default_storage::<ParityDb>().is_none());

		// Reset default storage reusing the same `db_path`
		let res = set_default_storage_hf(|| {
			let db = ParityDbHF::<sha2::Sha256>::open(&db_path);
			StorageHF::new(0, db)
		});

		assert_ok!(res);
	}
}
