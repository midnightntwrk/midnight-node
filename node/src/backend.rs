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

use std::{path::Path, sync::Arc};

use midnight_primitives_ledger::LedgerStorage;
use midnight_storage_core::db::paritydb::OwnedDb;
use sc_service::{DatabaseSource, config::Database};

use crate::{backend::custom_parity_db::DbAdapter, service::StorageInit};

pub mod custom_parity_db;

/// Operator guidance appended to every refusal of a data directory that still carries the
/// removed `separate` ledger storage layout.
fn resync_guidance(db_path: &Path, storage_config: &StorageInit) -> String {
	format!(
		"This node only supports the `unified` storage layout, in which the ledger lives in \
		 extra columns of the single Substrate parity-db, and it cannot migrate `separate` data \
		 in place. Stop the node, remove the chain data directory ({}) and the separate ledger \
		 database ({}) if present, and resync from genesis or from a unified-layout snapshot.",
		db_path.display(),
		storage_config.legacy_ledger_db_path.display(),
	)
}

/// Refuse a data directory created with the removed `separate` layout before anything opens or
/// rewrites it. Starting on such a directory would seed an empty ledger arena next to a chain
/// that already has state, and the node would crash on its first block.
///
/// Both checks are plain filesystem reads: the separate layout always created its own ledger
/// parity-db under `legacy_ledger_db_path`, and a Substrate parity-db with fewer than
/// [`custom_parity_db::NUM_COLUMNS`] columns predates the ledger columns altogether.
fn refuse_separate_layout(
	db_path: &Path,
	storage_config: &StorageInit,
) -> Result<(), sp_blockchain::Error> {
	if storage_config.legacy_ledger_db_path.is_dir() {
		return Err(sp_blockchain::Error::Backend(format!(
			"The chain data was created with the `separate` ledger storage layout: its ledger \
			 database exists at {}. {}",
			storage_config.legacy_ledger_db_path.display(),
			resync_guidance(db_path, storage_config),
		)));
	}

	let columns = parity_db::Options::load_metadata(db_path)
		.map_err(|e| sp_blockchain::Error::Backend(e.to_string()))?
		.map(|meta| meta.columns.len());
	if let Some(columns) = columns
		&& columns < custom_parity_db::NUM_COLUMNS as usize
	{
		return Err(sp_blockchain::Error::Backend(format!(
			"The chain data was created with the `separate` ledger storage layout: the Substrate \
			 parity-db at {} has {columns} columns and no ledger columns. {}",
			db_path.display(),
			resync_guidance(db_path, storage_config),
		)));
	}

	Ok(())
}

pub fn open_paritydb(
	path: &std::path::Path,
	storage_config: &StorageInit,
) -> Result<(OwnedDb, LedgerStorage, bool), sp_blockchain::Error> {
	refuse_separate_layout(path, storage_config)?;

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
					"Failed to open parity-db: {e}. Either the chain data was created with the \
					 removed `separate` ledger storage layout, a previous first start was \
					 interrupted before the storage mode was recorded, or the column \
					 configuration is not one this node creates; the fix is the same in every \
					 case. {}",
					resync_guidance(path, storage_config),
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
	use midnight_node_res::networks::{MidnightNetwork, UndeployedNetwork};
	use tempfile::TempDir;

	fn storage_init(base: &Path) -> StorageInit {
		StorageInit {
			legacy_ledger_db_path: base.join("ledger_storage"),
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
	fn fresh_and_absent_data_dirs_pass_the_separate_layout_guard() {
		let base = TempDir::new().unwrap();
		let cfg = storage_init(base.path());
		let db_path = base.path().join("paritydb");

		assert!(refuse_separate_layout(&db_path, &cfg).is_ok(), "absent dir must pass");
		std::fs::create_dir_all(&db_path).unwrap();
		assert!(refuse_separate_layout(&db_path, &cfg).is_ok(), "empty dir must pass");
	}

	#[test]
	fn separate_layout_ledger_dir_is_refused_with_resync_guidance() {
		let base = TempDir::new().unwrap();
		let cfg = storage_init(base.path());
		std::fs::create_dir_all(&cfg.legacy_ledger_db_path).unwrap();

		let err = refuse_separate_layout(&base.path().join("paritydb"), &cfg)
			.expect_err("a data dir with ledger_storage/ must be refused");
		let msg = err.to_string();
		assert!(msg.contains("`separate`"), "{msg}");
		assert!(msg.contains("ledger_storage"), "{msg}");
		assert!(msg.contains("resync"), "{msg}");
	}

	#[test]
	fn pre_unified_substrate_db_is_refused_with_resync_guidance() {
		let base = TempDir::new().unwrap();
		let cfg = storage_init(base.path());
		let db_path = base.path().join("paritydb");

		// The layout from before the ledger columns existed (pre-2.0.0-alpha.1 separate mode).
		let options =
			parity_db::Options::with_columns(&db_path, custom_parity_db::NUM_COLUMNS_POLKADOT);
		drop(parity_db::Db::open_or_create(&options).unwrap());

		let err = refuse_separate_layout(&db_path, &cfg)
			.expect_err("a parity-db without ledger columns must be refused");
		let msg = err.to_string();
		assert!(msg.contains("`separate`"), "{msg}");
		assert!(msg.contains("13 columns"), "{msg}");
		assert!(msg.contains("resync"), "{msg}");
	}

	#[test]
	fn unified_substrate_db_passes_the_separate_layout_guard() {
		let base = TempDir::new().unwrap();
		let cfg = storage_init(base.path());
		let db_path = base.path().join("paritydb");

		let options = parity_db::Options::with_columns(&db_path, custom_parity_db::NUM_COLUMNS);
		drop(parity_db::Db::open_or_create(&options).unwrap());

		assert!(refuse_separate_layout(&db_path, &cfg).is_ok());
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
