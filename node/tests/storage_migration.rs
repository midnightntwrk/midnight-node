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
//!
//! In-process coverage omits `Unified -> Unified` restart: the first `Unified` call
//! keeps an `Arc<parity_db::Db>` alive via the global storage, so reopening the same
//! path hits parity-db's "Database file is in use" file-lock error. In production the
//! old process has exited and released the lock, so this is a test-only artifact.
//! That scenario belongs in the e2e/local-env harness instead.

use midnight_node::backend::open_paritydb;
use midnight_node::cfg::midnight_cfg::StorageSeparation;
use midnight_node::service::StorageInit;
use midnight_node_res::networks::{MidnightNetwork, UndeployedNetwork};
use midnight_primitives_ledger::LedgerStorageDb;
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

#[test]
fn storage_migration_scenarios() {
	// 1. Unified mode opens cleanly on a fresh dir. Smoke test for the new code path;
	//    we cannot follow it with a reopen in-process (see module-level comment).
	{
		let base = TempDir::new().unwrap();
		let cfg = storage_init(base.path(), StorageSeparation::Unified);

		let (_db, storage, require_create) = open_paritydb(&paritydb_path(base.path()), &cfg)
			.unwrap_or_else(|e| panic!("fresh Unified open failed: {e}"));

		assert!(require_create, "fresh paritydb should be flagged for create");
		assert!(
			matches!(storage, LedgerStorageDb::UnifiedDb(_)),
			"Unified mode must return UnifiedDb",
		);
	}

	// 2. Separate -> Unified on the same data dir must be rejected at the parity-db
	//    layer. A silent success would leave the old `ledger_storage/` parity-db
	//    orphaned and reinitialise ledger state from genesis.
	{
		let base = TempDir::new().unwrap();
		let path = paritydb_path(base.path());
		let sep_cfg = storage_init(base.path(), StorageSeparation::Separate);
		let uni_cfg = storage_init(base.path(), StorageSeparation::Unified);

		let (db, storage, _) = open_paritydb(&path, &sep_cfg)
			.unwrap_or_else(|e| panic!("fresh Separate open failed: {e}"));
		drop((db, storage));

		let msg = match open_paritydb(&path, &uni_cfg) {
			Ok(_) => panic!("cross-mode swap must error"),
			Err(e) => e.to_string(),
		};
		assert!(
			msg.contains("storage_separation"),
			"expected storage_separation hint in error, got: {msg}",
		);
	}

	// 3. Unified -> Separate on the same data dir: same hazard in the opposite
	//    direction. Parity-db again catches the config mismatch.
	{
		let base = TempDir::new().unwrap();
		let path = paritydb_path(base.path());
		let uni_cfg = storage_init(base.path(), StorageSeparation::Unified);
		let sep_cfg = storage_init(base.path(), StorageSeparation::Separate);

		let (db, storage, _) = open_paritydb(&path, &uni_cfg)
			.unwrap_or_else(|e| panic!("fresh Unified open failed: {e}"));
		drop((db, storage));

		let msg = match open_paritydb(&path, &sep_cfg) {
			Ok(_) => panic!("cross-mode swap must error"),
			Err(e) => e.to_string(),
		};
		assert!(
			msg.contains("storage_separation"),
			"expected storage_separation hint in error, got: {msg}",
		);
	}
}
