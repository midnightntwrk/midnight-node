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

//! Implementation derived from polkadot-sdk:
//! substrate/client/db/src/lib.rs
//! substrate/client/db/src/utils.rs

use std::sync::Arc;

use midnight_primitives_ledger::LedgerStorageDb;
use midnight_storage_core::db::paritydb::OwnedDb;
use sc_service::{DatabaseSource, config::Database};

use crate::{backend::custom_parity_db::DbAdapter, service::StorageInit};

pub mod custom_parity_db;

pub fn open_paritydb(
	path: &std::path::Path,
	storage_config: &StorageInit,
) -> Result<(OwnedDb, LedgerStorageDb, bool), sp_blockchain::Error> {
	// Flag the db for initialisation if it doesn't already exist
	let require_create_flag =
		std::fs::read_dir(path).map(|dir| dir.into_iter().count() == 0).unwrap_or(true);

	let (db, storage) =
		match custom_parity_db::open::<sp_core::H256>(path, false, storage_config) {
			Ok(db) => Ok(db),
			Err(parity_db::Error::InvalidConfiguration(_)) => {
				log::warn!("Invalid parity db configuration, attempting database metadata update.");
				// Try to update the database with the new config
				custom_parity_db::open::<sp_core::H256>(path, true, storage_config)
			},
			Err(e @ parity_db::Error::IncompatibleColumnConfig { .. }) => {
				return Err(sp_blockchain::Error::Backend(format!(
					"Failed to open parity-db: {e}. This typically means the \
					 `storage_separation` config option was changed between runs. \
					 Switching between `separate` and `unified` is not supported on an \
					 existing database — to change `storage_separation`, delete the chain \
					 data directory and resync.",
				)));
			},
			Err(e) => Err(e),
		}
		.map_err(|e| sp_blockchain::Error::Backend(e.to_string()))?;

	Ok((db, storage, require_create_flag))
}

pub fn create_database_source(
	db: OwnedDb,
	require_create_flag: bool,
) -> Result<DatabaseSource, sp_blockchain::Error> {
	let db = DbAdapter(db.0);
	Ok(DatabaseSource::Custom {
		db: Arc::new(db) as Arc<dyn Database<sp_core::H256>>,
		require_create_flag,
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cfg::midnight_cfg::StorageSeparation;
	use midnight_node_res::networks::{MidnightNetwork, UndeployedNetwork};
	use std::path::PathBuf;
	use tempfile::TempDir;

	fn separate_storage_init(db_path: PathBuf) -> StorageInit {
		StorageInit {
			separation: StorageSeparation::Separate,
			db_path,
			genesis_state: UndeployedNetwork.genesis_state().to_vec(),
			cache_size: 10_000,
		}
	}

	#[test]
	fn create_database_source_forwards_require_create_flag() {
		let tmp = TempDir::new().unwrap();
		let db = parity_db::Db::open_or_create(&parity_db::Options::with_columns(tmp.path(), 1))
			.unwrap();
		let owned = OwnedDb(Arc::new(db));

		let source = create_database_source(OwnedDb(owned.0.clone()), true).unwrap();
		let DatabaseSource::Custom { require_create_flag, .. } = source else {
			panic!("expected DatabaseSource::Custom");
		};
		assert!(require_create_flag);

		let source = create_database_source(owned, false).unwrap();
		let DatabaseSource::Custom { require_create_flag, .. } = source else {
			panic!("expected DatabaseSource::Custom");
		};
		assert!(!require_create_flag);
	}

	#[test]
	fn open_paritydb_separate_mode_flags_fresh_dir_and_clears_on_reopen() {
		let base = TempDir::new().unwrap();
		let db_path = base.path().join("paritydb");
		let ledger_path = base.path().join("ledger_storage");
		let cfg = separate_storage_init(ledger_path.clone());

		// Non-existent dir is treated as fresh.
		let (db, storage, require_create) = open_paritydb(&db_path, &cfg).unwrap();
		assert!(require_create, "fresh path should require create");
		let LedgerStorageDb::SeparateDb(returned_path) = &storage else {
			panic!("expected SeparateDb in Separate mode");
		};
		assert_eq!(returned_path, &ledger_path);

		// Release substrate parity-db file locks before reopening.
		drop(db);
		drop(storage);

		let (_db, storage, require_create) = open_paritydb(&db_path, &cfg).unwrap();
		assert!(!require_create, "existing populated path should not require create");
		assert!(matches!(storage, LedgerStorageDb::SeparateDb(_)));
	}

	#[test]
	fn column_count_constants_are_consistent() {
		use midnight_storage_core::db::paritydb::NUM_COLUMNS as NUM_COLUMNS_LEDGER;

		assert_eq!(
			midnight_primitives_ledger::LedgerStorageExt::COLUMN_OFFSET,
			midnight_primitives_ledger::NUM_COLUMNS_POLKADOT,
			"ledger column offset must match polkadot column count"
		);
		assert_eq!(
			custom_parity_db::NUM_COLUMNS_POLKADOT,
			midnight_primitives_ledger::NUM_COLUMNS_POLKADOT,
		);
		let total = midnight_primitives_ledger::NUM_COLUMNS_POLKADOT + NUM_COLUMNS_LEDGER;
		assert_eq!(custom_parity_db::NUM_COLUMNS, total);
	}
}
