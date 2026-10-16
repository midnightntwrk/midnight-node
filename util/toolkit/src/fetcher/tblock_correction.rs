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

//! Resolving whether a block's runtime applies the `tblock` correction.
//!
//! The node verifies a block's first ledger transaction against
//! `parent_block_time + 12s` when the runtime on-chain at that height imports version 1 of
//! `Ledger8Bridge::apply_transaction`, and against the block's own `tblock` when it imports
//! version 2 (see `ledger/src/host_api/ledger_8.rs` and
//! <https://github.com/midnightntwrk/midnight-node/issues/1924>). Which one runs is decided
//! purely by which symbol the runtime wasm imports — the node never consults a version number.
//!
//! The toolkit replays blocks without a wasm executor, so it reads that same import directly
//! out of the runtime blob at the block. That keeps the replay a function of the artifact the
//! chain actually ran.

use std::collections::HashMap;

use tokio::sync::Mutex;
use wasmparser::{Parser, Payload};

use crate::fetcher::MidnightClientAtBlock;

const CODE_KEY: &[u8] = b":code";

/// `#[runtime_interface]` emits `ext_<trait_snake_case>_<fn>_version_<n>`; for
/// `pub trait Ledger8Bridge` that is `ext_ledger_8_bridge_…`.
const APPLY_TX_PREFIX: &str = "ext_ledger_8_bridge_apply_transaction_version_";

/// Runtime blobs are a few MB; this bounds decompression of a malformed or hostile blob.
const CODE_BLOB_BOMB_LIMIT: usize = 64 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum TblockCorrectionError {
	#[error("could not read `:code` at block {block_number}: {reason}")]
	CodeUnavailable { block_number: u64, reason: String },
	#[error("could not decompress the runtime blob at block {block_number}: {reason}")]
	Decompress { block_number: u64, reason: String },
	#[error("could not parse the runtime wasm at block {block_number}: {reason}")]
	ParseWasm { block_number: u64, reason: String },
}

/// Per-runtime memo of the decision.
///
/// `spec_version` is a cache key only; the answer comes from the blob's imports. Only a
/// runtime upgrade can change it, and that bumps `spec_version`, so one lookup per distinct
/// runtime covers the whole chain.
#[derive(Default)]
pub struct TblockCorrectionPolicy {
	by_spec_version: Mutex<HashMap<u32, bool>>,
}

impl TblockCorrectionPolicy {
	/// Whether the runtime at `block` applies the correction to the block's first ledger tx.
	pub async fn skews_tblock(
		&self,
		block: &MidnightClientAtBlock,
		spec_version: u32,
	) -> Result<bool, TblockCorrectionError> {
		let mut cache = self.by_spec_version.lock().await;
		if let Some(&skews) = cache.get(&spec_version) {
			return Ok(skews);
		}

		let skews = read_skews_tblock_from_runtime(block).await?;
		log::info!(
			"runtime spec_version {spec_version} imports {}; tblock correction {}",
			if skews {
				"apply_transaction v1"
			} else {
				"apply_transaction v2 (or no Ledger8Bridge)"
			},
			if skews { "applies" } else { "does not apply" },
		);
		cache.insert(spec_version, skews);
		Ok(skews)
	}
}

/// Reads `:code` at `block` and reports whether it imports `apply_transaction` version 1.
async fn read_skews_tblock_from_runtime(
	block: &MidnightClientAtBlock,
) -> Result<bool, TblockCorrectionError> {
	let block_number = block.block_number();

	let code = block.storage().fetch_raw(CODE_KEY.to_vec()).await.map_err(|e| {
		TblockCorrectionError::CodeUnavailable { block_number, reason: e.to_string() }
	})?;

	let wasm = sp_maybe_compressed_blob::decompress(&code, CODE_BLOB_BOMB_LIMIT)
		.map_err(|e| TblockCorrectionError::Decompress { block_number, reason: e.to_string() })?;

	imports_apply_transaction_v1(&wasm)
		.map_err(|reason| TblockCorrectionError::ParseWasm { block_number, reason })
}

/// Whether `wasm` imports `ext_ledger_8_bridge_apply_transaction_version_1`.
///
/// A runtime imports exactly one version of a given host function, so finding v1 is the whole
/// decision. Anything else — v2, or a runtime with no `Ledger8Bridge` import at all — does not
/// skew. Ledger-9 runtimes fall in the latter group, matching `Ledger9Bridge`, which never
/// skews.
fn imports_apply_transaction_v1(wasm: &[u8]) -> Result<bool, String> {
	for payload in Parser::new(0).parse_all(wasm) {
		let payload = payload.map_err(|e| e.to_string())?;
		let Payload::ImportSection(section) = payload else { continue };
		// `into_imports` flattens the compact-import encodings into plain `Import`s.
		for import in section.into_imports() {
			let import = import.map_err(|e| e.to_string())?;
			if let Some(version) = import.name.strip_prefix(APPLY_TX_PREFIX) {
				return Ok(version == "1");
			}
		}
	}
	Ok(false)
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Minimal wasm module importing a single function from `env` under `name`.
	fn wasm_importing(name: &str) -> Vec<u8> {
		let mut module = wasm_encoder::Module::new();
		let mut types = wasm_encoder::TypeSection::new();
		types.ty().function(vec![], vec![]);
		module.section(&types);
		let mut imports = wasm_encoder::ImportSection::new();
		imports.import("env", name, wasm_encoder::EntityType::Function(0));
		module.section(&imports);
		module.finish()
	}

	#[test]
	fn v1_import_skews() {
		let wasm = wasm_importing("ext_ledger_8_bridge_apply_transaction_version_1");
		assert!(imports_apply_transaction_v1(&wasm).unwrap());
	}

	/// #2002 closed the loophole by adding v2; those runtimes verify against the block's own
	/// timestamp.
	#[test]
	fn v2_import_does_not_skew() {
		let wasm = wasm_importing("ext_ledger_8_bridge_apply_transaction_version_2");
		assert!(!imports_apply_transaction_v1(&wasm).unwrap());
	}

	/// A ledger-9 runtime imports `Ledger9Bridge`, which never skews.
	#[test]
	fn unrelated_import_does_not_skew() {
		let wasm = wasm_importing("ext_ledger_9_bridge_apply_transaction_version_1");
		assert!(!imports_apply_transaction_v1(&wasm).unwrap());
	}

	#[test]
	fn malformed_wasm_is_an_error_not_a_silent_false() {
		assert!(imports_apply_transaction_v1(b"not wasm at all").is_err());
	}
}
