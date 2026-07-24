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

use midnight_primitives_ledger::LedgerStorageExt;

use super::LOG_TARGET;
use super::ledger_storage_local::{
	db::{ParityDb, paritydb::OwnedDb},
	storage::{try_get_default_storage, unsafe_drop_default_storage},
};

// Storage may be registered under either of two `ParityDb` instantiations
// depending on the operator's `storage_separation` config: the default
// (column offset 0) for `Separate`, or column offset = NUM_COLUMNS_POLKADOT
// for `Unified`. Drop whichever exists.
type DbSeparate = ParityDb;
type DbUnified = ParityDb<sha2::Sha256, OwnedDb, { LedgerStorageExt::COLUMN_OFFSET }>;

pub fn drop_default_storage_if_exists() {
	if try_get_default_storage::<DbSeparate>().is_some() {
		unsafe_drop_default_storage::<DbSeparate>();
		log::info!(
			target: LOG_TARGET,
			"Dropped HF storage after rollback (separate)"
		);
	}
	if try_get_default_storage::<DbUnified>().is_some() {
		unsafe_drop_default_storage::<DbUnified>();
		log::info!(
			target: LOG_TARGET,
			"Dropped HF storage after rollback (unified)"
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

#[derive(Debug)]
pub enum GetRootError {
	DeserializationFailure(std::io::Error),
	NetworkIdMismatch,
	SerializationFailure(std::io::Error),
}

impl core::fmt::Display for GetRootError {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		match self {
			GetRootError::DeserializationFailure(e) => {
				write!(f, "Failed to deserialize genesis state: {e}")
			},
			GetRootError::NetworkIdMismatch => {
				write!(f, "genesis state network id != configured chainspec network id")
			},
			GetRootError::SerializationFailure(e) => {
				write!(f, "Failed to serialize genesis state: {e}")
			},
		}
	}
}

pub fn get_root(state: &[u8], network_id: Option<&str>) -> Result<Vec<u8>, GetRootError> {
	// Get empty state key
	use super::api::Ledger;
	use super::ledger_storage_local::{DefaultDB, storage::default_storage};

	let state: super::mn_ledger_local::structure::LedgerState<DefaultDB> =
		super::midnight_serialize_local::tagged_deserialize(state)
			.map_err(GetRootError::DeserializationFailure)?;
	let state = Ledger::new(state);

	if network_id.is_some_and(|n| state.state.network_id != n) {
		return Err(GetRootError::NetworkIdMismatch);
	}

	let state = default_storage::<DefaultDB>().arena.alloc(state);
	let mut bytes = vec![];
	super::midnight_serialize_local::tagged_serialize(&state.as_typed_key(), &mut bytes)
		.map_err(GetRootError::SerializationFailure)?;
	Ok(bytes)
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
pub fn init_storage_paritydb_separate<P: AsRef<std::path::Path>>(
	dir: P,
	genesis_state: &[u8],
	cache_size: usize,
) -> Vec<u8> {
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

	alloc_with_initial_state::<super::TransactionSignature, ParityDb>(genesis_state)
}

#[cfg(feature = "std")]
pub fn set_init_options_paritydb(
	options: &mut parity_db::Options,
	column_offset: u8,
	use_compression: bool,
) {
	midnight_storage_core::db::paritydb::set_init_options(options, column_offset, use_compression);
}

#[cfg(feature = "std")]
pub fn init_storage_paritydb_unified<
	D: std::ops::Deref<Target = parity_db::Db> + Default + Send + Sync + 'static,
	const COLUMN_OFFSET: u8,
>(
	db_instance: D,
	genesis_state: &[u8],
	cache_size: usize,
) -> Vec<u8> {
	use super::ledger_storage_local::{Storage, db::ParityDb, storage::set_default_storage};

	let res = set_default_storage(|| {
		let db = ParityDb::<sha2::Sha256, D, COLUMN_OFFSET>::from_existing_db(db_instance);
		Storage::new(cache_size, db)
	});
	if res.is_err() {
		log::warn!("Warning: Failed to set default storage: {res:?}");
	}

	alloc_with_initial_state::<super::TransactionSignature, ParityDb<sha2::Sha256, D, COLUMN_OFFSET>>(
		genesis_state,
	)
}
