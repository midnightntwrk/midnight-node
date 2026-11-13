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

//! Backwards-compatibility / migration scenarios for the `storage_separation` config option.
//!
//! Scenarios intentionally run sequentially inside a single `#[test]` because
//! `midnight_node_ledger::...::init_storage_paritydb_*` installs a process-wide
//! `default_storage` singleton; parallel sub-tests would race on that global.
//! Each scenario calls `drop_all_default_storage()` between opens (and at end
//! of scope) to release the registered `Arc<parity_db::Db>` so the next open
//! sees a clean slate and parity-db's background commit thread is joined
//! before the `TempDir` removes the data directory.

use midnight_node::backend::{custom_parity_db::column_options, open_paritydb};
use midnight_node::cfg::midnight_cfg::StorageSeparation;
use midnight_node::service::StorageInit;
use midnight_node_ledger::drop_all_default_storage;
use midnight_node_res::networks::{MidnightNetwork, UndeployedNetwork};
use midnight_primitives_ledger::{LedgerStorageDb, NUM_COLUMNS_POLKADOT};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn storage_init(base: &Path, separation: StorageSeparation) -> StorageInit {
	StorageInit {
		separation,
		db_path: base.join("ledger_storage"),
		genesis_state: UndeployedNetwork.genesis_state().to_vec(),
		cache_size: 10_000,
	}
}

fn paritydb_path(base: &Path) -> PathBuf {
	base.join("paritydb")
}

/// Column layout `separate` mode's standalone ledger database is created with,
/// mirroring `midnight_storage_core::db::paritydb::OwnedDb::new`.
fn ledger_db_options(path: &Path) -> parity_db::Options {
	let mut options =
		parity_db::Options::with_columns(path, midnight_storage_core::db::paritydb::NUM_COLUMNS);
	midnight_node_ledger::ledger_9::storage::set_init_options_paritydb(&mut options, 0, false);
	options
}

/// Every node key in a `separate`-mode ledger database, read out of its btree
/// index. Only the node column is indexed in a way we can walk cheaply, and it
/// is the column that matters: those are the ledger's content-addressed nodes.
fn ledger_node_keys(ledger_db_path: &Path) -> Vec<Vec<u8>> {
	let db = parity_db::Db::open_read_only(&ledger_db_options(ledger_db_path)).unwrap();
	let mut keys = Vec::new();
	let mut entries = db.iter(midnight_storage_core::db::paritydb::NODE_COLUMN).unwrap();
	while let Some((key, _)) = entries.next().unwrap() {
		keys.push(key);
	}
	keys
}

#[test]
fn storage_migration_scenarios() {
	// 1. Unified mode opens cleanly on a fresh dir. Smoke test for the new code path.
	{
		let base = TempDir::new().unwrap();
		let cfg = storage_init(base.path(), StorageSeparation::Unified);

		let (db, storage, require_create) = open_paritydb(&paritydb_path(base.path()), &cfg)
			.unwrap_or_else(|e| panic!("fresh Unified open failed: {e}"));

		assert!(require_create, "fresh paritydb should be flagged for create");
		assert!(
			matches!(storage, LedgerStorageDb::UnifiedDb(_)),
			"Unified mode must return UnifiedDb",
		);
		drop((db, storage));
		drop_all_default_storage();
	}

	// 2. Separate -> Unified on the same data dir folds the standalone ledger
	//    database into the shared one. Every ledger node the operator already
	//    had on disk must survive: a silent miss would reinitialise ledger
	//    state from genesis behind an intact block history.
	{
		let base = TempDir::new().unwrap();
		let path = paritydb_path(base.path());
		let sep_cfg = storage_init(base.path(), StorageSeparation::Separate);
		let uni_cfg = storage_init(base.path(), StorageSeparation::Unified);

		let (db, storage, _) = open_paritydb(&path, &sep_cfg)
			.unwrap_or_else(|e| panic!("fresh Separate open failed: {e}"));
		drop((db, storage));
		drop_all_default_storage();

		let node_keys = ledger_node_keys(&sep_cfg.db_path);
		assert!(!node_keys.is_empty(), "Separate mode should have persisted genesis ledger state");

		let (db, storage, _) = open_paritydb(&path, &uni_cfg)
			.unwrap_or_else(|e| panic!("Separate -> Unified migration failed: {e}"));
		assert!(
			matches!(storage, LedgerStorageDb::UnifiedDb(_)),
			"after migration the ledger must be served from the shared db",
		);
		drop((db, storage));
		drop_all_default_storage();

		assert!(!sep_cfg.db_path.exists(), "the migrated-from database should be retired");
		let retired = base.path().join("ledger_storage.migrated");
		assert!(retired.is_dir(), "expected the source to be renamed to {}", retired.display());

		let db = parity_db::Db::open(&column_options(&path, StorageSeparation::Unified)).unwrap();
		for key in &node_keys {
			assert!(
				db.get(NUM_COLUMNS_POLKADOT, key).unwrap().is_some(),
				"ledger node {} did not survive the migration",
				hex::encode(key),
			);
		}
	}

	// 3. Separate -> Unified once the ledger database is gone. Nothing to fold
	//    in, so parity-db's config mismatch has to keep standing: opening
	//    anyway would strand the block history against empty ledger state.
	{
		let base = TempDir::new().unwrap();
		let path = paritydb_path(base.path());
		let sep_cfg = storage_init(base.path(), StorageSeparation::Separate);
		let uni_cfg = storage_init(base.path(), StorageSeparation::Unified);

		let (db, storage, _) = open_paritydb(&path, &sep_cfg)
			.unwrap_or_else(|e| panic!("fresh Separate open failed: {e}"));
		drop((db, storage));
		drop_all_default_storage();
		std::fs::remove_dir_all(&sep_cfg.db_path).unwrap();

		let msg = match open_paritydb(&path, &uni_cfg) {
			Ok(_) => panic!("cross-mode swap without a source database must error"),
			Err(e) => e.to_string(),
		};
		assert!(
			msg.contains("storage_separation"),
			"expected storage_separation hint in error, got: {msg}",
		);
		drop_all_default_storage();
	}

	// 4. Unified -> Separate on the same data dir. There is no migration in
	//    this direction; parity-db catches the config mismatch.
	{
		let base = TempDir::new().unwrap();
		let path = paritydb_path(base.path());
		let uni_cfg = storage_init(base.path(), StorageSeparation::Unified);
		let sep_cfg = storage_init(base.path(), StorageSeparation::Separate);

		let (db, storage, _) = open_paritydb(&path, &uni_cfg)
			.unwrap_or_else(|e| panic!("fresh Unified open failed: {e}"));
		drop((db, storage));
		drop_all_default_storage();

		let msg = match open_paritydb(&path, &sep_cfg) {
			Ok(_) => panic!("cross-mode swap must error"),
			Err(e) => e.to_string(),
		};
		assert!(
			msg.contains("storage_separation"),
			"expected storage_separation hint in error, got: {msg}",
		);
		drop_all_default_storage();
	}
}
