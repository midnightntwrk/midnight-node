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

//! The snapshot marker and the snapshot directory layout.
//!
//! ```text
//! <root>/MARKER             which snapshot is valid: its directory, block and StateKey
//! <root>/snap-<n>-<hash8>/  one ParityDb per snapshot
//! ```
//!
//! The marker is written last, by write-to-temp, fsync, rename, fsync of the directory. Every
//! snapshot directory other than the one it names is incomplete or superseded, and is removed by
//! [`clean`] at startup. A crash at any point therefore leaves either the previous marker and its
//! snapshot, or the new marker and the new snapshot.

use serde::{Deserialize, Serialize};
use std::{
	fs,
	io::Write,
	path::{Path, PathBuf},
};

const MARKER_FILE: &str = "MARKER";
const MARKER_TMP: &str = "MARKER.tmp";
const SNAPSHOT_PREFIX: &str = "snap-";

/// What the marker records about the valid snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
	/// Directory name of the snapshot, relative to the snapshot root.
	pub dir: String,
	pub block_number: u64,
	/// Hex block hash. `None` for the genesis snapshot, written before the genesis block exists.
	pub block_hash: Option<String>,
	/// Hex of the `StateKey` bytes the snapshot's state is stored under on chain.
	pub state_key: String,
}

impl Marker {
	pub(crate) fn new(
		dir: String,
		block_number: u64,
		block_hash: Option<&[u8]>,
		state_key: &[u8],
	) -> Self {
		Self {
			dir,
			block_number,
			block_hash: block_hash.map(hex::encode),
			state_key: hex::encode(state_key),
		}
	}

	pub fn state_key_bytes(&self) -> Result<Vec<u8>, String> {
		hex::decode(&self.state_key).map_err(|e| format!("bad state_key in marker: {e}"))
	}

	pub fn block_hash_bytes(&self) -> Result<Option<Vec<u8>>, String> {
		self.block_hash
			.as_ref()
			.map(|h| hex::decode(h).map_err(|e| format!("bad block_hash in marker: {e}")))
			.transpose()
	}

	pub(crate) fn path(&self, root: &Path) -> PathBuf {
		root.join(&self.dir)
	}
}

/// A new directory name for a snapshot of block `number`, unique in this process. A directory
/// an earlier process left is either the marked one, whose block no new snapshot repeats, or
/// removed at startup.
pub(crate) fn snapshot_dir_name(number: u64, hash: Option<&[u8]>) -> String {
	static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
	let hash = hash.map_or_else(|| "genesis".into(), |h| hex::encode(&h[..h.len().min(4)]));
	let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
	format!("{SNAPSHOT_PREFIX}{number:010}-{hash}-{seq}")
}

/// Read the marker under `root`, if any.
pub(crate) fn read(root: &Path) -> Result<Option<Marker>, String> {
	let path = root.join(MARKER_FILE);
	let bytes = match fs::read(&path) {
		Ok(bytes) => bytes,
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
		Err(e) => return Err(format!("failed to read {}: {e}", path.display())),
	};
	serde_json::from_slice(&bytes)
		.map(Some)
		.map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

pub(crate) fn fsync_dir(dir: &Path) -> Result<(), String> {
	fs::File::open(dir)
		.and_then(|d| d.sync_all())
		.map_err(|e| format!("fsync {}: {e}", dir.display()))
}

/// Atomically replace the marker under `root` with `marker`.
pub(crate) fn write(root: &Path, marker: &Marker) -> Result<(), String> {
	let io = |what: &str, e: std::io::Error| format!("{what}: {e}");
	let bytes = serde_json::to_vec_pretty(marker).map_err(|e| format!("serialize marker: {e}"))?;
	let tmp = root.join(MARKER_TMP);
	{
		let mut file = fs::File::create(&tmp).map_err(|e| io("create marker", e))?;
		file.write_all(&bytes).map_err(|e| io("write marker", e))?;
		file.sync_all().map_err(|e| io("fsync marker", e))?;
	}
	fs::rename(&tmp, root.join(MARKER_FILE)).map_err(|e| io("rename marker", e))?;
	fsync_dir(root)
}

/// Make `root` exist, durably.
pub(crate) fn ensure_root(root: &Path) -> Result<(), String> {
	fs::create_dir_all(root).map_err(|e| format!("create {}: {e}", root.display()))?;
	root.parent().map_or(Ok(()), fsync_dir)
}

/// Remove every snapshot directory the marker does not name.
pub(crate) fn clean(root: &Path, marker: Option<&Marker>) -> Result<(), String> {
	let mut removed = false;
	for entry in fs::read_dir(root).map_err(|e| format!("read {}: {e}", root.display()))? {
		let entry = entry.map_err(|e| format!("read {}: {e}", root.display()))?;
		let name = entry.file_name().to_string_lossy().into_owned();
		if name.starts_with(SNAPSHOT_PREFIX) && marker.is_none_or(|m| m.dir != name) {
			let path = entry.path();
			fs::remove_dir_all(&path).map_err(|e| format!("remove {}: {e}", path.display()))?;
			removed = true;
		}
	}
	if removed { fsync_dir(root) } else { Ok(()) }
}
