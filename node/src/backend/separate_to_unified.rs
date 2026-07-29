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

//! In-place migration of Midnight Ledger storage from `separate` to `unified`.
//!
//! With `storage_separation = "separate"` the ledger nodes live in their own
//! ParityDb at `<base-path>/ledger_storage/`, using columns
//! `0..NUM_COLUMNS_LEDGER`. With `"unified"` the very same nodes live in the
//! Substrate ParityDb, shifted up by [`NUM_COLUMNS_POLKADOT`]. Ledger nodes are
//! content-addressed, so the two layouts hold byte-identical keys and values —
//! switching modes needs no resync, only a copy of three columns from one
//! ParityDb into another.
//!
//! Two things stand in the way of just opening the database in the new mode:
//!
//! 1. `separate` mode reserves the trailing ledger columns in the Substrate
//!    database with the ledger's own column options (btree index, compression)
//!    even though it never writes to them, while `unified` mode leaves them at
//!    ParityDb's defaults. ParityDb compares the requested column options
//!    against the on-disk metadata and refuses to open on any difference.
//!    Because those columns are provably empty in a `separate` database,
//!    rewriting the metadata is safe.
//! 2. The ledger nodes themselves have to be copied across.
//!
//! Both happen here, before the node's long-lived ParityDb handle is created.
//!
//! ## Crash safety
//!
//! The migration is driven off two pieces of on-disk state: the source
//! directory, and a marker file written next to it for the duration of the
//! import. The marker is what makes an interrupted migration recoverable —
//! once the column metadata has been rewritten, the "database is still in
//! `separate` layout" signal is gone, so without the marker a half-copied
//! database would look finished. Copying is idempotent (every entry is a plain
//! `Set` of a content-addressed key), so a resumed migration simply starts
//! over.
//!
//! The source directory is only retired (renamed aside) once every write has
//! been flushed, which is why the import uses its own short-lived handle on the
//! Substrate database: dropping it joins ParityDb's background commit worker.

use std::path::{Path, PathBuf};

use midnight_storage_core::db::paritydb::NUM_COLUMNS as NUM_COLUMNS_LEDGER;

use super::custom_parity_db::{NUM_COLUMNS, NUM_COLUMNS_POLKADOT};

/// Marker recording an import that has started but not finished. Written as a
/// sibling of the source directory, e.g. `<base-path>/ledger_storage.importing`.
const MARKER_SUFFIX: &str = "importing";

/// Where the source directory is moved once the import is complete, e.g.
/// `<base-path>/ledger_storage.migrated`.
const RETIRED_SUFFIX: &str = "migrated";

/// Ledger databases run to hundreds of gigabytes, so the copy is streamed. Cap
/// each destination commit by entry count and by payload size — node sizes vary
/// by orders of magnitude, and either bound alone lets the other run away.
const COMMIT_BATCH_ENTRIES: usize = 20_000;
const COMMIT_BATCH_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("parity-db error while migrating ledger storage: {0}")]
	Db(#[from] parity_db::Error),
	#[error("io error while migrating ledger storage: {0}")]
	Io(#[from] std::io::Error),
	#[error("{0}")]
	Unsupported(String),
}

/// Copy a `separate`-mode ledger database into the unified Substrate ParityDb at
/// `paritydb_path`, if one is waiting to be imported.
///
/// A no-op unless `ledger_db_path` holds a ParityDb *and* either the Substrate
/// database still carries the `separate` column layout or a previous import was
/// interrupted. In particular a natively-unified database sitting next to a
/// stale `ledger_storage/` is left alone: its ledger columns are already
/// authoritative and re-importing would overwrite them with older data.
///
/// Must be called before the Substrate ParityDb is opened for the node — the
/// import needs exclusive access to it.
pub fn import_if_pending(
	paritydb_path: &Path,
	unified_config: &parity_db::Options,
	ledger_db_path: &Path,
) -> Result<(), Error> {
	let marker = sibling(ledger_db_path, MARKER_SUFFIX)?;
	let interrupted = marker.is_file();

	if !is_paritydb(ledger_db_path) {
		if interrupted {
			// The source was removed while a migration was in flight. Whatever
			// made it across is all there is; say so rather than starting the
			// node against a silently truncated ledger.
			return Err(Error::Unsupported(format!(
				"an interrupted ledger storage migration is recorded at {}, but its source \
				 database {} is gone. The unified ledger columns may be incomplete — restore \
				 the source directory, or delete the chain data directory and resync.",
				marker.display(),
				ledger_db_path.display(),
			)));
		}
		return Ok(());
	}

	let stale_layout = separate_layout_metadata(paritydb_path, unified_config)?;
	if stale_layout.is_none() && !interrupted {
		log::debug!(
			"Ignoring {} — the unified database's ledger columns are already authoritative.",
			ledger_db_path.display(),
		);
		return Ok(());
	}

	log::info!(
		"Migrating ledger storage into the unified database: {} -> {} (columns {}..{})",
		ledger_db_path.display(),
		paritydb_path.display(),
		NUM_COLUMNS_POLKADOT,
		NUM_COLUMNS,
	);
	if interrupted {
		log::warn!("Resuming an interrupted ledger storage migration.");
	}

	// Marker before metadata: after the rewrite the layout no longer records
	// which mode the database was built in.
	std::fs::write(
		&marker,
		format!(
			"Importing {} into {} (columns {NUM_COLUMNS_POLKADOT}..{NUM_COLUMNS}).\n\
			 The unified ledger columns are incomplete while this file exists; the node \
			 restarts the import on the next start and removes this file when done.\n",
			ledger_db_path.display(),
			paritydb_path.display(),
		),
	)?;

	if let Some((salt, version)) = stale_layout {
		log::info!("Rewriting parity-db column metadata: separate -> unified ledger layout.");
		unified_config.write_metadata_with_version(paritydb_path, &salt, Some(version))?;
	}

	// Scoped: dropping both handles joins ParityDb's background commit worker,
	// so everything is on disk before the marker is cleared below.
	{
		let source = parity_db::Db::open(&source_config(ledger_db_path))?;
		let destination = parity_db::Db::open(unified_config)?;

		for column in 0..NUM_COLUMNS_LEDGER {
			let target = NUM_COLUMNS_POLKADOT + column;
			let copied = copy_column(&source, &destination, column, target)?;
			log::info!("Copied {copied} ledger entries from column {column} to column {target}.");
		}
	}

	std::fs::remove_file(&marker)?;

	let retired = sibling(ledger_db_path, RETIRED_SUFFIX)?;
	match std::fs::rename(ledger_db_path, &retired) {
		Ok(()) => log::info!(
			"Ledger storage migration complete. {} is no longer used and can be deleted.",
			retired.display(),
		),
		// Not fatal: the import itself is durable, and the next start skips it
		// because the layout is now unified. Only the cleanup is left undone.
		Err(e) => log::warn!(
			"Ledger storage migration complete, but {} could not be renamed to {}: {e}. \
			 It is no longer used and can be deleted.",
			ledger_db_path.display(),
			retired.display(),
		),
	}

	Ok(())
}

/// Inspects the on-disk metadata and, if the ledger columns still carry the
/// `separate` layout, returns the `(salt, version)` needed to rewrite it.
///
/// Returns `None` for a database that has no metadata yet (a fresh one), whose
/// column count differs (handled by the metadata-upgrade path in
/// [`super::open_paritydb`]), or whose ledger columns already match the unified
/// layout.
fn separate_layout_metadata(
	paritydb_path: &Path,
	unified_config: &parity_db::Options,
) -> Result<Option<([u8; 32], u32)>, Error> {
	let Some(metadata) = parity_db::Options::load_metadata(paritydb_path)? else {
		return Ok(None);
	};
	if metadata.columns.len() != unified_config.columns.len() {
		return Ok(None);
	}

	if (NUM_COLUMNS_POLKADOT as usize..NUM_COLUMNS as usize)
		.all(|c| metadata.columns[c] == unified_config.columns[c])
	{
		return Ok(None);
	}

	// Rewriting metadata is only defensible for the ledger columns, which a
	// `separate` database provably never wrote to. A mismatch anywhere else is
	// something this migration does not understand.
	if let Some(c) = (0..NUM_COLUMNS_POLKADOT as usize)
		.find(|&c| metadata.columns[c] != unified_config.columns[c])
	{
		return Err(Error::Unsupported(format!(
			"parity-db at {} has an unexpected configuration for Substrate column {c}; \
			 refusing to migrate ledger storage.",
			paritydb_path.display(),
		)));
	}

	Ok(Some((metadata.salt, metadata.version)))
}

/// Column options the `separate`-mode ledger database was created with. Must
/// stay in step with `midnight_storage_core::db::paritydb::OwnedDb::new`.
fn source_config(path: &Path) -> parity_db::Options {
	let mut options = parity_db::Options::with_columns(path, NUM_COLUMNS_LEDGER);
	midnight_node_ledger::ledger_9::storage::set_init_options_paritydb(&mut options, 0, false);
	options
}

/// Streams every entry of `source_column` into `destination_column`.
fn copy_column(
	source: &parity_db::Db,
	destination: &parity_db::Db,
	source_column: u8,
	destination_column: u8,
) -> Result<u64, Error> {
	let mut entries = source.iter(source_column)?;
	let mut batch = Vec::new();
	let mut batch_bytes = 0usize;
	let mut copied = 0u64;

	while let Some((key, value)) = entries.next()? {
		batch_bytes += key.len() + value.len();
		batch.push((destination_column, parity_db::Operation::Set(key, value)));
		copied += 1;

		if batch.len() >= COMMIT_BATCH_ENTRIES || batch_bytes >= COMMIT_BATCH_BYTES {
			destination.commit_changes(std::mem::take(&mut batch))?;
			batch_bytes = 0;
		}
	}
	if !batch.is_empty() {
		destination.commit_changes(batch)?;
	}

	Ok(copied)
}

/// A directory ParityDb has already written its metadata to.
fn is_paritydb(path: &Path) -> bool {
	path.join("metadata").is_file()
}

/// `<parent>/<name>.<suffix>` for the given path.
fn sibling(path: &Path, suffix: &str) -> Result<PathBuf, Error> {
	let name = path.file_name().ok_or_else(|| {
		Error::Unsupported(format!("ledger storage path {} has no file name", path.display()))
	})?;
	let mut name = name.to_os_string();
	name.push(format!(".{suffix}"));
	Ok(path.with_file_name(name))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cfg::midnight_cfg::StorageSeparation;
	use tempfile::TempDir;

	/// Populates `path` the way `separate` mode would: a three-column ledger
	/// ParityDb whose node column holds `count` entries.
	fn seed_source(path: &Path, count: u32) {
		let db = parity_db::Db::open_or_create(&source_config(path)).unwrap();
		let mut ops = Vec::new();
		for i in 0..count {
			ops.push((0u8, parity_db::Operation::Set(node_key(i), vec![i as u8; 48])));
			ops.push((1u8, parity_db::Operation::Set(node_key(i), 1u32.to_le_bytes().to_vec())));
		}
		db.commit_changes(ops).unwrap();
	}

	fn node_key(i: u32) -> Vec<u8> {
		let mut key = vec![0u8; 32];
		key[..4].copy_from_slice(&i.to_be_bytes());
		key
	}

	fn config(path: &Path, separation: StorageSeparation) -> parity_db::Options {
		super::super::custom_parity_db::column_options(path, separation)
	}

	/// Creates a Substrate ParityDb in `separate` layout holding one recognisable
	/// entry, so the test can prove the migration leaves Substrate data alone.
	fn seed_substrate_db(path: &Path) {
		let db = parity_db::Db::open_or_create(&config(path, StorageSeparation::Separate)).unwrap();
		db.commit_changes(vec![(
			0u8,
			parity_db::Operation::Set(b"substrate-meta".to_vec(), b"intact".to_vec()),
		)])
		.unwrap();
	}

	#[test]
	fn imports_ledger_columns_and_retires_the_source() {
		let base = TempDir::new().unwrap();
		let paritydb = base.path().join("paritydb");
		let ledger = base.path().join("ledger_storage");
		seed_substrate_db(&paritydb);
		seed_source(&ledger, 300);

		let unified = config(&paritydb, StorageSeparation::Unified);
		// The whole point: this open fails before the migration runs.
		assert!(parity_db::Db::open(&unified).is_err());

		import_if_pending(&paritydb, &unified, &ledger).unwrap();

		assert!(!ledger.exists(), "source should be retired");
		assert!(base.path().join("ledger_storage.migrated").is_dir());
		assert!(!base.path().join("ledger_storage.importing").exists());

		let db = parity_db::Db::open(&unified).unwrap();
		assert_eq!(db.get(0, b"substrate-meta").unwrap().as_deref(), Some(&b"intact"[..]));
		for i in [0u32, 42, 299] {
			assert_eq!(
				db.get(NUM_COLUMNS_POLKADOT, &node_key(i)).unwrap(),
				Some(vec![i as u8; 48]),
				"ledger node {i} should have been copied",
			);
			assert_eq!(
				db.get(NUM_COLUMNS_POLKADOT + 1, &node_key(i)).unwrap().as_deref(),
				Some(&1u32.to_le_bytes()[..]),
				"gc root count for {i} should have been copied",
			);
		}
	}

	#[test]
	fn is_a_no_op_without_a_source() {
		let base = TempDir::new().unwrap();
		let paritydb = base.path().join("paritydb");
		let ledger = base.path().join("ledger_storage");
		seed_substrate_db(&paritydb);

		let unified = config(&paritydb, StorageSeparation::Unified);
		import_if_pending(&paritydb, &unified, &ledger).unwrap();

		// Untouched, so the caller still surfaces parity-db's mode-change error.
		assert!(parity_db::Db::open(&unified).is_err());
	}

	#[test]
	fn leaves_a_native_unified_database_alone() {
		let base = TempDir::new().unwrap();
		let paritydb = base.path().join("paritydb");
		let ledger = base.path().join("ledger_storage");
		let unified = config(&paritydb, StorageSeparation::Unified);

		// Already unified, with ledger data of its own.
		let db = parity_db::Db::open_or_create(&unified).unwrap();
		db.commit_changes(vec![(
			NUM_COLUMNS_POLKADOT,
			parity_db::Operation::Set(node_key(7), b"current".to_vec()),
		)])
		.unwrap();
		drop(db);

		// A stale source left over from an earlier `separate` run.
		seed_source(&ledger, 10);

		import_if_pending(&paritydb, &unified, &ledger).unwrap();

		assert!(ledger.is_dir(), "stale source should be left in place");
		let db = parity_db::Db::open(&unified).unwrap();
		assert_eq!(
			db.get(NUM_COLUMNS_POLKADOT, &node_key(7)).unwrap().as_deref(),
			Some(&b"current"[..]),
			"existing unified ledger data must not be overwritten",
		);
	}

	#[test]
	fn resumes_an_interrupted_import() {
		let base = TempDir::new().unwrap();
		let paritydb = base.path().join("paritydb");
		let ledger = base.path().join("ledger_storage");
		seed_substrate_db(&paritydb);
		seed_source(&ledger, 50);
		let unified = config(&paritydb, StorageSeparation::Unified);

		// Simulate dying right after the metadata rewrite: unified layout on
		// disk, marker still present, nothing copied.
		let metadata = parity_db::Options::load_metadata(&paritydb).unwrap().unwrap();
		unified
			.write_metadata_with_version(&paritydb, &metadata.salt, Some(metadata.version))
			.unwrap();
		std::fs::write(base.path().join("ledger_storage.importing"), "interrupted").unwrap();

		import_if_pending(&paritydb, &unified, &ledger).unwrap();

		assert!(!base.path().join("ledger_storage.importing").exists());
		let db = parity_db::Db::open(&unified).unwrap();
		assert_eq!(db.get(NUM_COLUMNS_POLKADOT, &node_key(49)).unwrap(), Some(vec![49u8; 48]));
	}

	#[test]
	fn refuses_to_start_when_an_interrupted_import_lost_its_source() {
		let base = TempDir::new().unwrap();
		let paritydb = base.path().join("paritydb");
		let ledger = base.path().join("ledger_storage");
		seed_substrate_db(&paritydb);
		std::fs::write(base.path().join("ledger_storage.importing"), "interrupted").unwrap();

		let unified = config(&paritydb, StorageSeparation::Unified);
		let err = import_if_pending(&paritydb, &unified, &ledger).unwrap_err();
		assert!(matches!(err, Error::Unsupported(_)), "got {err:?}");
	}
}
