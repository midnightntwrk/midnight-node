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

use sc_client_db::Database;
use sp_database::{Change, ColumnId, Transaction, error::DatabaseError};

struct DbAdapter(parity_db::Db);

fn handle_err<T>(result: parity_db::Result<T>) -> T {
	match result {
		Ok(r) => r,
		Err(e) => {
			panic!("Critical database error: {:?}", e);
		},
	}
}

const NUM_COLUMNS_POLKADOT: u32 = 13;
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

// TODO: use midnight_storage::parity_db::NUM_COLUMNS as NUM_COLUMNS_LEDGER;
const NUM_COLUMNS_LEDGER: u32 = 3;

const NUM_COLUMNS: u32 = NUM_COLUMNS_POLKADOT + NUM_COLUMNS_LEDGER;

/// Wrap parity-db database into a trait object that implements `sp_database::Database`
pub fn open<H: Clone + AsRef<[u8]>>(
	path: &std::path::Path,
	upgrade: bool,
	_midnight_storage_cache_size: usize,
) -> parity_db::Result<std::sync::Arc<dyn Database<H>>> {
	let mut config = parity_db::Options::with_columns(path, NUM_COLUMNS as u8);

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

	// TODO: Add call to:
	// midnight_storage::ParityDb::<COLUMN_OFFSET = NUM_COLUMNS_POLKADOT>::set_init_options(&mut config)

	if upgrade {
		log::info!("Upgrading database metadata.");
		if let Some(meta) = parity_db::Options::load_metadata(path)? {
			config.write_metadata_with_version(path, &meta.salt, Some(meta.version))?;
		}
	}

	let db = parity_db::Db::open_or_create(&config)?;

	// TODO: Add call to:
	// let res = set_default_storage(|| {
	// 	let db = ParityDb::<sha2::Sha256>::from_existing_db(db.clone());
	// 	Storage::new(midnight_storage_cache_size, db)
	// });

	Ok(std::sync::Arc::new(DbAdapter(db)))
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
