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

//! Derived implementation from polkadot-sdk: substrate/client/db/src/parity_db.rs
//! Adds custom column definitions for midnight-ledger

use midnight_primitives_ledger::LedgerStorage;
use midnight_storage_core::db::paritydb::{NUM_COLUMNS as NUM_COLUMNS_LEDGER, OwnedDb};
use sc_client_db::Database;
use sp_database::{Change, ColumnId, Transaction, error::DatabaseError};
use std::sync::Arc;

use crate::service::StorageInit;

pub struct DbAdapter(pub Arc<parity_db::Db>);

fn handle_err<T>(result: parity_db::Result<T>) -> T {
	match result {
		Ok(r) => r,
		Err(e) => {
			panic!("Critical database error: {:?}", e);
		},
	}
}

pub(crate) const NUM_COLUMNS_POLKADOT: u8 = midnight_primitives_ledger::NUM_COLUMNS_POLKADOT;
/// Length of a [`DbHash`].
const DB_HASH_LEN: usize = 32;

#[allow(dead_code)]
pub(crate) mod columns_polkadot {
	pub const META: u32 = 0;
	pub const STATE: u32 = 1;
	pub const STATE_META: u32 = 2;
	/// maps hashes to lookup keys and numbers to canon hashes.
	pub const KEY_LOOKUP: u32 = 3;
	pub const HEADER: u32 = 4;
	pub const BODY: u32 = 5;
	pub const JUSTIFICATIONS: u32 = 6;
	pub const AUX: u32 = 8;
	/// Offchain workers local storage
	pub const OFFCHAIN: u32 = 9;
	/// Transactions
	pub const TRANSACTION: u32 = 11;
	pub const BODY_INDEX: u32 = 12;
}

pub(crate) const NUM_COLUMNS: u8 = NUM_COLUMNS_POLKADOT + NUM_COLUMNS_LEDGER;

/// META-column key recording which `storage_separation` mode a database was created in.
const STORAGE_SEPARATION_KEY: &[u8] = b"midnight:storage_separation";

/// META-column key written when the ledger storage is created by a node that persists every
/// ledger root through the tagged-root wrapper. Its absence means the ledger storage was created
/// by an older node and holds roots the ledger GC can never release, so the GC does not run
/// (`ledger_gc::try_spawn`). One point read; no root is ever iterated to decide this.
const LEDGER_GC_TAGGED_KEY: &[u8] = b"midnight:ledger_gc_tagged_roots";

/// Whether this database was created with tagged ledger roots from genesis (see
/// [`LEDGER_GC_TAGGED_KEY`]).
pub fn ledger_gc_tagged_from_genesis(db: &parity_db::Db) -> parity_db::Result<bool> {
	Ok(db.get(columns_polkadot::META as u8, LEDGER_GC_TAGGED_KEY)?.is_some())
}

/// Refuse a database created with the removed `separate` storage layout, and record `unified` in
/// the META column of one that has no marker yet.
///
/// Before the marker existed only separate mode set storage-core's options on the main DB's
/// ledger columns (unified left them default), so an unmarked indexed DB was created in separate
/// mode even if `ledger_storage/` was deleted by hand. A first start killed before the marker
/// commit became durable leaves the same shape, so the refusal names that cause too.
fn check_storage_separation(
	db: &parity_db::Db,
	existing_columns: Option<&[parity_db::ColumnOptions]>,
	storage_config: &StorageInit,
) -> parity_db::Result<()> {
	let recorded = db.get(columns_polkadot::META as u8, STORAGE_SEPARATION_KEY)?;
	let refusal = match &recorded {
		Some(v) if v.as_slice() == b"separate" => {
			Some("the META column records the removed `separate` storage layout")
		},
		Some(_) => None,
		None => existing_columns
			.is_some_and(|columns| {
				let node_column = (NUM_COLUMNS_POLKADOT
					+ midnight_storage_core::db::paritydb::NODE_COLUMN) as usize;
				storage_config.legacy_ledger_db_path.is_dir()
					|| columns.get(node_column).is_some_and(|c| c.btree_index)
			})
			.then_some(
				"no storage mode is recorded and the data directory looks like it was created \
				 with the removed `separate` storage layout, or a previous first start was \
				 interrupted before the storage mode was recorded",
			),
	};
	if let Some(reason) = refusal {
		return Err(parity_db::Error::IncompatibleColumnConfig {
			id: NUM_COLUMNS_POLKADOT,
			reason: reason.into(),
		});
	}
	if recorded.is_none() {
		db.commit([(
			columns_polkadot::META as u8,
			STORAGE_SEPARATION_KEY,
			Some(b"unified".to_vec()),
		)])?;
	}
	Ok(())
}

/// Wrap parity-db database into a trait object that implements `sp_database::Database`
pub fn open<H: Clone + AsRef<[u8]>>(
	path: &std::path::Path,
	upgrade: bool,
	storage_config: &StorageInit,
) -> parity_db::Result<(OwnedDb, LedgerStorage)> {
	let mut config = parity_db::Options::with_columns(path, NUM_COLUMNS);

	let compressed = [
		columns_polkadot::STATE,
		columns_polkadot::HEADER,
		columns_polkadot::BODY,
		columns_polkadot::BODY_INDEX,
		columns_polkadot::TRANSACTION,
		columns_polkadot::JUSTIFICATIONS,
	];

	for i in compressed {
		let column = &mut config.columns[i as usize];
		column.compression = parity_db::CompressionType::Lz4;
	}

	let state_col = &mut config.columns[columns_polkadot::STATE as usize];
	state_col.ref_counted = true;
	state_col.preimage = true;
	state_col.uniform = true;

	let tx_col = &mut config.columns[columns_polkadot::TRANSACTION as usize];
	tx_col.ref_counted = true;
	tx_col.preimage = true;
	tx_col.uniform = true;

	// Without `btree_index` on the root and node columns storage-core cannot iterate them
	// (`get_roots`, `size`, the GC scan), and without the options the node column is stored
	// uncompressed (issue #2294).
	// Column options are fixed when a database is created, and parity-db refuses to open an
	// existing one with different options (`IncompatibleColumnConfig`), so an existing DB
	// keeps the ledger column options it was created with; only new databases get these.
	// The node then starts normally and the ledger GC fails closed on the unindexed columns,
	// asking for a resync (`ledger_gc::try_spawn`).
	midnight_storage_core::db::paritydb::set_init_options(&mut config, NUM_COLUMNS_POLKADOT, true);
	let existing_meta = parity_db::Options::load_metadata(path)?;
	if let Some(meta) = &existing_meta {
		let ledger_cols = NUM_COLUMNS_POLKADOT as usize..NUM_COLUMNS as usize;
		if let Some(stored) = meta.columns.get(ledger_cols.clone()) {
			config.columns[ledger_cols].clone_from_slice(stored);
		}
	}

	if upgrade {
		log::info!("Upgrading database metadata.");
		if let Some(meta) = parity_db::Options::load_metadata(path)? {
			config.write_metadata_with_version(path, &meta.salt, Some(meta.version))?;
		}
	}

	// The ledger is fresh exactly when the main database is new; a fresh ledger only ever
	// receives tagged roots.
	let ledger_fresh = existing_meta.is_none();

	let db = Arc::new(parity_db::Db::open_or_create(&config)?);
	check_storage_separation(
		&db,
		existing_meta.as_ref().map(|m| m.columns.as_slice()),
		storage_config,
	)?;
	if ledger_fresh {
		db.commit([(columns_polkadot::META as u8, LEDGER_GC_TAGGED_KEY, Some(b"1".to_vec()))])?;
	}

	// Version-aware: a ledger-9 node may boot on a ledger-8 genesis during the 8->9 hardfork; the
	// arena is seeded with the deserializer matching the genesis `ledger-state[vN]` tag.
	midnight_node_ledger::init_ledger_storage::<_, NUM_COLUMNS_POLKADOT>(
		OwnedDb(db.clone()),
		&storage_config.genesis_state,
		storage_config.cache_size,
	);
	Ok((OwnedDb(db.clone()), LedgerStorage::new(db, storage_config.cache_size)))
}

fn ref_counted_column(col: u32) -> bool {
	col == columns_polkadot::TRANSACTION || col == columns_polkadot::STATE
}

impl<H: Clone + AsRef<[u8]>> Database<H> for DbAdapter {
	fn commit(&self, transaction: Transaction<H>) -> Result<(), DatabaseError> {
		let mut not_ref_counted_column = Vec::new();
		let result = self.0.commit(transaction.0.into_iter().filter_map(|change| {
			Some(match change {
				Change::Set(col, key, value) => (col as u8, key, Some(value)),
				Change::Remove(col, key) => (col as u8, key, None),
				Change::Store(col, key, value) => {
					if ref_counted_column(col) {
						(col as u8, key.as_ref().to_vec(), Some(value))
					} else {
						if !not_ref_counted_column.contains(&col) {
							not_ref_counted_column.push(col);
						}
						return None;
					}
				},
				Change::Reference(col, key) => {
					if ref_counted_column(col) {
						// FIXME accessing value is not strictly needed, optimize this in parity-db.
						let value = <Self as Database<H>>::get(self, col, key.as_ref());
						(col as u8, key.as_ref().to_vec(), value)
					} else {
						if !not_ref_counted_column.contains(&col) {
							not_ref_counted_column.push(col);
						}
						return None;
					}
				},
				Change::Release(col, key) => {
					if ref_counted_column(col) {
						(col as u8, key.as_ref().to_vec(), None)
					} else {
						if !not_ref_counted_column.contains(&col) {
							not_ref_counted_column.push(col);
						}
						return None;
					}
				},
			})
		}));

		if !not_ref_counted_column.is_empty() {
			return Err(DatabaseError(Box::new(parity_db::Error::InvalidInput(format!(
				"Ref counted operation on non ref counted columns {:?}",
				not_ref_counted_column
			)))));
		}

		result.map_err(|e| DatabaseError(Box::new(e)))
	}

	fn get(&self, col: ColumnId, key: &[u8]) -> Option<Vec<u8>> {
		handle_err(self.0.get(col as u8, key))
	}

	fn contains(&self, col: ColumnId, key: &[u8]) -> bool {
		handle_err(self.0.get_size(col as u8, key)).is_some()
	}

	fn value_size(&self, col: ColumnId, key: &[u8]) -> Option<usize> {
		handle_err(self.0.get_size(col as u8, key)).map(|s| s as usize)
	}

	fn supports_ref_counting(&self) -> bool {
		true
	}

	fn sanitize_key(&self, key: &mut Vec<u8>) {
		let _prefix = key.drain(0..key.len() - DB_HASH_LEN);
	}
}
