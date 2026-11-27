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

//! Dry-runs a runtime upgrade against a `try-runtime create-snapshot` file.
//!
//! In-tree because the standalone `try-runtime-cli` lacks the `ledger_*_bridge` host functions.
//! The snapshot reader is hand-rolled because `frame-remote-externalities` re-resolves `bip39`
//! onto `rand_core 0.4`, which breaks the `pallas-wallet` build.

#![allow(clippy::result_large_err)]

use std::path::{Path, PathBuf};

use clap::Args;
use frame_support::weights::Weight;
use frame_try_runtime::UpgradeCheckSelect;
use parity_scale_codec::{Compact, Decode, Encode};
use sc_executor::{DEFAULT_HEAP_ALLOC_STRATEGY, WasmExecutor};
use sp_api::CallContext;
use sp_core::storage::well_known_keys;
use sp_core::traits::ReadRuntimeVersion;
use sp_externalities::Extensions;
use sp_runtime::StateVersion;
use sp_runtime::traits::{Block as BlockT, HashingFor};
use sp_state_machine::{
	OverlayedChanges, StateMachine, TestExternalities,
	backend::{BackendRuntimeCode, TryPendingCode},
};
use sp_version::RuntimeVersion;
use tempfile::TempDir;

use midnight_node_runtime::Block;
use midnight_primitives_ledger::{LedgerStorage, LedgerStorageExt};

use crate::service::HostFunctions;

type Externalities = TestExternalities<HashingFor<Block>>;

/// Mirrors upstream `frame_remote_externalities::Snapshot`.
#[derive(Decode)]
struct Snapshot {
	_version: Compact<u16>,
	state_version: StateVersion,
	#[allow(clippy::type_complexity)]
	raw_storage: Vec<(Vec<u8>, (Vec<u8>, i32))>,
	storage_root: <Block as BlockT>::Hash,
	_header: <Block as BlockT>::Header,
}

const SNAPSHOT_VERSION: u16 = 4;

/// Same as `storage_cache_size` in `res/cfg/default.toml`.
const LEDGER_CACHE_SIZE: usize = 100_000;

#[derive(Debug, Clone, Args)]
pub struct TryRuntimeCmd {
	/// Snapshot file from `try-runtime create-snapshot`.
	#[arg(long)]
	pub snap: PathBuf,

	/// Runtime WASM to upgrade to. Defaults to the snapshot's own runtime.
	#[arg(long)]
	pub runtime: Option<PathBuf>,

	/// One of: none, all, pre-and-post, try-state.
	#[arg(long, default_value = "all")]
	pub checks: UpgradeCheckSelect,

	/// Allow a `spec_version` that is not above the snapshot's.
	#[arg(long)]
	pub disable_spec_version_check: bool,

	/// Allow a `spec_name` that differs from the snapshot's.
	#[arg(long)]
	pub disable_spec_name_check: bool,

	/// A node's `<base-path>/ledger_storage`, for migrations that read ledger state, which a
	/// snapshot does not carry (see `docs/fork-testing.md`). Defaults to an empty ledger.
	#[arg(long)]
	pub ledger_db: Option<PathBuf>,
}

impl TryRuntimeCmd {
	pub fn run(&self) -> sc_cli::Result<()> {
		let executor = WasmExecutor::<HostFunctions>::builder()
			.with_onchain_heap_alloc_strategy(DEFAULT_HEAP_ALLOC_STRATEGY)
			.with_offchain_heap_alloc_strategy(DEFAULT_HEAP_ALLOC_STRATEGY)
			.build();

		let mut ext = load_snapshot(&self.snap)?;
		if let Some(new_runtime) = &self.runtime {
			self.set_runtime(&executor, &mut ext, new_runtime)?;
		}

		let state_version = runtime_version(&executor, &mut ext)?.state_version();
		let code_in_snapshot = BackendRuntimeCode::new(&ext.backend, TryPendingCode::No);
		let runtime_code = code_in_snapshot.runtime_code()?;
		let mut changes = OverlayedChanges::default();
		let mut extensions = Extensions::default();
		let (ledger_storage, _empty_ledger_dir) = self.ledger_storage()?;
		extensions.register(LedgerStorageExt::new(ledger_storage));

		let encoded_weights = StateMachine::new(
			&ext.backend,
			&mut changes,
			&executor,
			"TryRuntime_on_runtime_upgrade",
			&self.checks.encode(),
			&mut extensions,
			&runtime_code,
			CallContext::Offchain,
			state_version,
		)
		.execute()
		.map_err(|e| {
			let e = e.to_string();
			// Logged separately, as the trap backtrace in `e` buries it.
			if self.ledger_db.is_none() && e.contains("not in storage arena") {
				log::error!("A migration read ledger state: pass --ledger-db");
			}
			format!("TryRuntime_on_runtime_upgrade failed: {e}")
		})?;

		let (consumed, max) = <(Weight, Weight)>::decode(&mut &encoded_weights[..])
			.map_err(|e| format!("decoding the upgrade weight: {e}"))?;
		log::info!("Upgrade consumed {consumed:?} of max block weight {max:?}");
		Ok(())
	}

	fn set_runtime(
		&self,
		executor: &WasmExecutor<HostFunctions>,
		ext: &mut Externalities,
		new_runtime: &Path,
	) -> sc_cli::Result<()> {
		let old = runtime_version(executor, ext)?;
		let code = std::fs::read(new_runtime)
			.map_err(|e| format!("reading {}: {e}", new_runtime.display()))?;
		ext.insert(well_known_keys::CODE.to_vec(), code);
		let new = runtime_version(executor, ext)?;
		log::info!(
			"Upgrading {} {} to {} {}",
			old.spec_name,
			old.spec_version,
			new.spec_name,
			new.spec_version
		);

		if !self.disable_spec_name_check && new.spec_name != old.spec_name {
			return Err("spec_name differs from the snapshot's (--disable-spec-name-check)".into());
		}
		if !self.disable_spec_version_check && new.spec_version <= old.spec_version {
			return Err(
				"spec_version is not above the snapshot's (--disable-spec-version-check)".into()
			);
		}
		Ok(())
	}

	/// The `TempDir` backs the empty-ledger default and must outlive the run.
	fn ledger_storage(&self) -> sc_cli::Result<(LedgerStorage, Option<TempDir>)> {
		let (path, temp_dir) = match &self.ledger_db {
			Some(path) => (path.clone(), None),
			None => {
				let dir = tempfile::Builder::new()
					.prefix("midnight-try-runtime-ledger-")
					.tempdir()
					.map_err(|e| format!("creating a temporary ledger dir: {e}"))?;
				log::warn!("No --ledger-db; using an empty ledger at {}", dir.path().display());
				(dir.path().to_path_buf(), Some(dir))
			},
		};
		Ok((LedgerStorage::new_separate(path, LEDGER_CACHE_SIZE), temp_dir))
	}
}

fn load_snapshot(path: &Path) -> sc_cli::Result<Externalities> {
	let bytes = std::fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))?;

	// Checked first: another version's layout fails to decode with a misleading error.
	let Compact(version) = Compact::<u16>::decode(&mut &bytes[..])
		.map_err(|e| format!("decoding the snapshot version: {e}"))?;
	if version != SNAPSHOT_VERSION {
		return Err(format!(
			"snapshot version {version}, expected {SNAPSHOT_VERSION}: recreate it with a \
			 matching try-runtime-cli"
		)
		.into());
	}

	let snapshot =
		Snapshot::decode(&mut &bytes[..]).map_err(|e| format!("decoding the snapshot: {e}"))?;
	Ok(TestExternalities::from_raw_snapshot(
		snapshot.raw_storage,
		snapshot.storage_root,
		snapshot.state_version,
	))
}

fn runtime_version(
	executor: &WasmExecutor<HostFunctions>,
	ext: &mut Externalities,
) -> sc_cli::Result<RuntimeVersion> {
	let code = ext
		.execute_with(|| sp_io::storage::get(well_known_keys::CODE))
		.ok_or("the snapshot has no :code")?;
	let encoded = executor
		.read_runtime_version(&code, &mut ext.ext())
		.map_err(|e| format!("reading the runtime version: {e}"))?;
	RuntimeVersion::decode(&mut &encoded[..])
		.map_err(|e| format!("decoding the runtime version: {e}").into())
}
