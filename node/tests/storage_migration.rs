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

//! Storage-open scenarios for the unified-only parity-db layout. A fresh data directory opens
//! with storage-core's ledger column options and records `unified` plus the ledger GC marker in
//! the META column; every shape a data directory created with the removed `separate` layout can
//! take (ledger directory, 13 columns, separate-era column options, explicit `separate` marker)
//! is refused with resync guidance, before anything on disk is touched where possible.
//!
//! Scenarios intentionally run sequentially inside a single `#[test]` because
//! `midnight_node_ledger::init_ledger_storage` installs a process-wide
//! `default_storage` singleton; parallel sub-tests would race on that global.
//! Each scenario that opens a database calls `drop_all_default_storage()` afterwards
//! to release the registered `Arc<parity_db::Db>` so the next open sees a clean slate
//! and parity-db's background commit thread is joined before the `TempDir` removes
//! the data directory.

use midnight_node::backend::open_paritydb;
use midnight_node::service::StorageInit;
use midnight_node_ledger::drop_all_default_storage;
use midnight_node_res::networks::{MidnightNetwork, UndeployedNetwork};
use midnight_primitives_ledger::NUM_COLUMNS_POLKADOT;
use midnight_storage_core::db::paritydb::{
	GC_ROOT_COLUMN, NODE_COLUMN, NUM_COLUMNS as NUM_COLUMNS_LEDGER,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;

const MARKER_KEY: &[u8] = b"midnight:storage_separation";

const NUM_COLUMNS: u8 = NUM_COLUMNS_POLKADOT + NUM_COLUMNS_LEDGER;

fn storage_init(base: &Path) -> StorageInit {
	StorageInit {
		legacy_ledger_db_path: base.join("ledger_storage"),
		genesis_state: UndeployedNetwork.genesis_state().to_vec(),
		cache_size: 10_000,
	}
}

/// Index of a storage-core ledger column inside the shared database.
fn ledger_column(col: u8) -> usize {
	(NUM_COLUMNS_POLKADOT + col) as usize
}

fn paritydb_path(base: &Path) -> PathBuf {
	base.join("paritydb")
}

/// The Substrate column options `custom_parity_db::open` sets, so a database written here
/// differs from what the node opens only in the ledger columns under test.
fn substrate_options(path: &Path, num_columns: u8) -> parity_db::Options {
	const STATE: usize = 1;
	const HEADER: usize = 4;
	const BODY: usize = 5;
	const JUSTIFICATIONS: usize = 6;
	const TRANSACTION: usize = 11;
	const BODY_INDEX: usize = 12;

	let mut options = parity_db::Options::with_columns(path, num_columns);
	for i in [STATE, HEADER, BODY, BODY_INDEX, TRANSACTION, JUSTIFICATIONS] {
		options.columns[i].compression = parity_db::CompressionType::Lz4;
	}
	for i in [STATE, TRANSACTION] {
		let column = &mut options.columns[i];
		column.ref_counted = true;
		column.preimage = true;
		column.uniform = true;
	}
	options
}

fn create_and_close(options: &parity_db::Options) {
	drop(parity_db::Db::open_or_create(options).expect("create fixture parity-db"));
}

fn column_count(path: &Path) -> usize {
	parity_db::Options::load_metadata(path)
		.expect("read parity-db metadata")
		.expect("parity-db metadata present")
		.columns
		.len()
}

fn assert_refused_with_resync_guidance(
	result: Result<impl Sized, sp_blockchain::Error>,
	what: &str,
) {
	let msg = match result {
		Ok(_) => panic!("{what} must be refused"),
		Err(e) => e.to_string(),
	};
	assert!(msg.contains("`separate`"), "{what}: expected the separate layout to be named: {msg}");
	assert!(msg.contains("resync"), "{what}: expected resync guidance: {msg}");
	assert!(msg.contains("ledger_storage"), "{what}: expected the legacy path to be named: {msg}");
}

#[test]
fn storage_layout_scenarios() {
	// 1. A fresh data directory opens on the unified layout and hands the host functions the
	//    very parity-db instance Substrate writes to.
	{
		let base = TempDir::new().unwrap();
		let cfg = storage_init(base.path());
		let path = paritydb_path(base.path());

		let (db, storage, require_create) =
			open_paritydb(&path, &cfg).unwrap_or_else(|e| panic!("fresh open failed: {e}"));

		assert!(require_create, "fresh paritydb should be flagged for create");
		assert!(Arc::ptr_eq(&db.0, &storage.db), "ledger must share the Substrate parity-db");
		assert_eq!(column_count(&path), NUM_COLUMNS as usize);

		let meta = parity_db::Options::load_metadata(&path).unwrap().expect("metadata");
		let node = &meta.columns[ledger_column(NODE_COLUMN)];
		assert!(node.btree_index, "ledger node column must be indexed");
		assert!(
			matches!(node.compression, parity_db::CompressionType::Lz4),
			"ledger node column must be LZ4-compressed",
		);
		assert!(meta.columns[ledger_column(GC_ROOT_COLUMN)].btree_index);
		assert_eq!(db.0.get(0, MARKER_KEY).unwrap(), Some(b"unified".to_vec()));
		let format = midnight_node::backend::custom_parity_db::storage_format(&db.0).unwrap();
		assert_eq!(format.version, 2);
		assert_eq!(
			format.created_by,
			Some(midnight_node::backend::custom_parity_db::NODE_VERSION.to_string())
		);
		assert_eq!(format.status(), "current");
		assert!(db.0.get(0, b"midnight:ledger_gc_tagged_roots").unwrap().is_none());
		drop((db, storage));
		drop_all_default_storage();
	}

	// 2. An existing unified database reopens as before: no create flag, same column layout, and
	//    a value written into a ledger column in the first session is still there afterwards.
	{
		let base = TempDir::new().unwrap();
		let cfg = storage_init(base.path());
		let path = paritydb_path(base.path());

		let (db, storage, _) =
			open_paritydb(&path, &cfg).unwrap_or_else(|e| panic!("fresh open failed: {e}"));
		db.0.commit([(NUM_COLUMNS_POLKADOT, b"probe".to_vec(), Some(b"survives-reopen".to_vec()))])
			.expect("write probe into a ledger column");
		drop((db, storage));
		drop_all_default_storage();

		let (db, storage, require_create) =
			open_paritydb(&path, &cfg).unwrap_or_else(|e| panic!("reopen failed: {e}"));
		assert!(!require_create, "existing populated path should not require create");
		assert_eq!(column_count(&path), NUM_COLUMNS as usize);
		assert_eq!(
			db.0.get(NUM_COLUMNS_POLKADOT, b"probe").unwrap(),
			Some(b"survives-reopen".to_vec()),
			"ledger column data must persist across reopen",
		);
		drop((db, storage));
		drop_all_default_storage();
	}

	// 3. A data directory with the separate layout's own ledger database is refused before the
	//    Substrate parity-db is even created.
	{
		let base = TempDir::new().unwrap();
		let cfg = storage_init(base.path());
		let path = paritydb_path(base.path());
		std::fs::create_dir_all(&cfg.legacy_ledger_db_path).unwrap();

		assert_refused_with_resync_guidance(open_paritydb(&path, &cfg), "ledger_storage/ dir");
		assert!(!path.exists(), "refusal must not create the Substrate parity-db");
	}

	// 4. A Substrate parity-db without ledger columns (a node from before the ledger columns
	//    existed, after an operator deleted `ledger_storage/` by hand) is refused, and its
	//    metadata is left untouched rather than upgraded to the unified column count over an
	//    empty arena.
	{
		let base = TempDir::new().unwrap();
		let cfg = storage_init(base.path());
		let path = paritydb_path(base.path());
		create_and_close(&substrate_options(&path, NUM_COLUMNS_POLKADOT));

		assert_refused_with_resync_guidance(open_paritydb(&path, &cfg), "13-column parity-db");
		assert_eq!(
			column_count(&path),
			NUM_COLUMNS_POLKADOT as usize,
			"metadata must be untouched"
		);
	}

	// 5. What 2.x separate mode wrote (16 columns, ledger columns indexed and compressed) once
	//    `ledger_storage/` has been deleted by hand is refused: the stored options are reused, so
	//    the unmarked indexed node column is what identifies it. Refusal writes no marker, so it
	//    is idempotent.
	{
		let base = TempDir::new().unwrap();
		let cfg = storage_init(base.path());
		let path = paritydb_path(base.path());
		let mut options = substrate_options(&path, NUM_COLUMNS);
		midnight_storage_core::db::paritydb::set_init_options(
			&mut options,
			NUM_COLUMNS_POLKADOT,
			true,
		);
		create_and_close(&options);

		for attempt in ["first open", "second open"] {
			assert_refused_with_resync_guidance(
				open_paritydb(&path, &cfg),
				&format!("separate-mode ledger column options ({attempt})"),
			);
			drop_all_default_storage();
		}
	}

	// 6. A unified database created before the ledger columns got options (plain parity-db
	//    defaults, no marker) still opens, keeps the options it was created with (parity-db cannot
	//    change them), and gets the `unified` marker without the GC marker.
	{
		let base = TempDir::new().unwrap();
		let cfg = storage_init(base.path());
		let path = paritydb_path(base.path());
		create_and_close(&substrate_options(&path, NUM_COLUMNS));

		for _ in 0..2 {
			let (db, storage, _) = open_paritydb(&path, &cfg)
				.unwrap_or_else(|e| panic!("opening a pre-options unified db failed: {e}"));
			assert_eq!(db.0.get(0, MARKER_KEY).unwrap(), Some(b"unified".to_vec()));
			let format = midnight_node::backend::custom_parity_db::storage_format(&db.0).unwrap();
			assert_eq!(format.version, 1);
			assert_eq!(format.created_by, None);
			assert_eq!(format.status(), "legacy");
			assert!(
				db.0.get(0, b"midnight:storage_format").unwrap().is_none(),
				"an existing database must not be stamped with the storage format",
			);
			drop((db, storage));
			drop_all_default_storage();

			let meta = parity_db::Options::load_metadata(&path).unwrap().expect("metadata");
			let node = &meta.columns[ledger_column(NODE_COLUMN)];
			assert!(!node.btree_index, "existing db must keep its stored options");
			assert!(matches!(node.compression, parity_db::CompressionType::NoCompression));
		}
	}

	// 7. A database explicitly marked `separate` is refused whatever its column options are.
	{
		let base = TempDir::new().unwrap();
		let cfg = storage_init(base.path());
		let path = paritydb_path(base.path());
		let options = substrate_options(&path, NUM_COLUMNS);
		let db = parity_db::Db::open_or_create(&options).expect("create fixture parity-db");
		db.commit([(0u8, MARKER_KEY.to_vec(), Some(b"separate".to_vec()))]).unwrap();
		drop(db);

		assert_refused_with_resync_guidance(open_paritydb(&path, &cfg), "`separate` marker");
		drop_all_default_storage();
	}
}
