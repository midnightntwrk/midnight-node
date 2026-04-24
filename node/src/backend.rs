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
