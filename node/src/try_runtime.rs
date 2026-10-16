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

//! `midnight-node try-runtime`: dry-run a runtime upgrade against a state snapshot.
//!
//! In-tree rather than the standalone `try-runtime-cli` because that links only
//! `sp_io::SubstrateHostFunctions` and cannot resolve the `ledger_*_bridge` host
//! functions the upgrade calls, so it traps before any check runs. Snapshot
//! *creation* still uses `try-runtime-cli`, which needs no host functions.
//!
//! The snapshot reader is hand-rolled because adding `frame-remote-externalities`
//! re-resolves `bip39` onto `rand_core 0.4`, which then fails to build
//! `pallas-wallet`. The layout below mirrors the upstream `Snapshot<B>`.

#![allow(clippy::result_large_err)]

use std::path::PathBuf;

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
	OverlayedChanges, StateMachine, TestExternalities, backend::TryPendingCode,
};
use sp_version::RuntimeVersion;

use midnight_primitives_ledger::{LedgerStorage, LedgerStorageExt};

use crate::service::HostFunctions;
use midnight_node_runtime::Block;

/// A snapshot file as written by `try-runtime create-snapshot`, field for field
/// the upstream `frame_remote_externalities::Snapshot<B>`. `raw_storage` is
/// `(hashed_key, (trie_node_payload, ref_count))`.
#[derive(Decode)]
struct Snapshot {
	#[allow(dead_code)]
	snapshot_version: Compact<u16>,
	state_version: StateVersion,
	#[allow(clippy::type_complexity)]
	raw_storage: Vec<(Vec<u8>, (Vec<u8>, i32))>,
	storage_root: <Block as BlockT>::Hash,
	#[allow(dead_code)]
	header: <Block as BlockT>::Header,
}

const EXPECTED_SNAPSHOT_VERSION: Compact<u16> = Compact(4);

#[derive(Debug, Clone, Args)]
pub struct TryRuntimeCmd {
	/// Path to a runtime snapshot file produced by `try-runtime create-snapshot`.
	#[arg(long, short = 'p')]
	pub snap: PathBuf,

	/// Path to a new runtime wasm to test against the snapshot. If omitted, the
	/// migration runs against the runtime that is already embedded in the snapshot.
	#[arg(long)]
	pub runtime: Option<PathBuf>,

	/// Which `try-runtime` checks to run. One of: none, all, pre-and-post, try-state.
	#[arg(long, default_value = "all")]
	pub checks: UpgradeCheckSelect,

	/// Skip enforcing that the new runtime's `spec_version` is greater than the
	/// on-chain one. Use only when intentionally re-running the same version.
	#[arg(long, default_value_t = false)]
	pub disable_spec_version_check: bool,

	/// Skip enforcing that the new runtime's `spec_name` matches the on-chain one.
	#[arg(long, default_value_t = false)]
	pub disable_spec_name_check: bool,

	/// Path to a node's ledger storage (`<base-path>/ledger_storage`), e.g. from the
	/// data directory the fork-testing flow restores — see `docs/fork-testing.md`.
	///
	/// A snapshot carries substrate storage only, so migrations that read ledger
	/// state need this. Without it the run gets an empty arena, which is fine for
	/// upgrades that leave the ledger alone and aborts on any that do not.
	#[arg(long)]
	pub ledger_db: Option<PathBuf>,

	/// Arena cache size, in entries. Matches `storage_cache_size` in
	/// `res/cfg/default.toml`.
	#[arg(long, default_value_t = 100_000)]
	pub ledger_cache_size: usize,
}

impl TryRuntimeCmd {
	pub fn run(&self) -> sc_cli::Result<()> {
		let executor: WasmExecutor<HostFunctions> = WasmExecutor::builder()
			.with_onchain_heap_alloc_strategy(DEFAULT_HEAP_ALLOC_STRATEGY)
			.with_offchain_heap_alloc_strategy(DEFAULT_HEAP_ALLOC_STRATEGY)
			.build();

		log::info!("Loading snapshot from {:?}", self.snap);
		let mut ext = load_snapshot(&self.snap)?;

		let original_code = ext
			.execute_with(|| sp_io::storage::get(well_known_keys::CODE))
			.ok_or("snapshot does not contain :code")?;
		let old_version = decode_version(&executor, &original_code, &mut ext)?;
		log::info!(
			"Original runtime [Name: {:?}] [Version: {}]",
			old_version.spec_name,
			old_version.spec_version,
		);

		if let Some(new_wasm_path) = &self.runtime {
			let new_code = std::fs::read(new_wasm_path)
				.map_err(|e| format!("reading {new_wasm_path:?}: {e}"))?;
			ext.insert(well_known_keys::CODE.to_vec(), new_code.clone());
			let new_version = decode_version(&executor, &new_code, &mut ext)?;
			log::info!(
				"New runtime      [Name: {:?}] [Version: {}]",
				new_version.spec_name,
				new_version.spec_version,
			);

			if !self.disable_spec_name_check && new_version.spec_name != old_version.spec_name {
				return Err(format!(
					"spec_name mismatch: on-chain={:?}, new={:?} (use --disable-spec-name-check to override)",
					old_version.spec_name, new_version.spec_name,
				)
				.into());
			}
			if !self.disable_spec_version_check
				&& new_version.spec_version <= old_version.spec_version
			{
				return Err(format!(
					"new spec_version {} is not greater than on-chain {} (use --disable-spec-version-check to override)",
					new_version.spec_version, old_version.spec_version,
				)
				.into());
			}
		}

		log::info!("🔬 Running TryRuntime_on_runtime_upgrade with checks: {:?}", self.checks);

		let runtime_code_backend =
			sp_state_machine::backend::BackendRuntimeCode::new(&ext.backend, TryPendingCode::No);
		let runtime_code = runtime_code_backend.runtime_code()?;
		let mut changes = OverlayedChanges::<HashingFor<Block>>::default();
		let mut extensions = Extensions::default();
		extensions.register(LedgerStorageExt::new(LedgerStorage::new_separate(
			self.ledger_db_path()?,
			self.ledger_cache_size,
		)));

		let encoded = StateMachine::new(
			&ext.backend,
			&mut changes,
			&executor,
			"TryRuntime_on_runtime_upgrade",
			self.checks.encode().as_ref(),
			&mut extensions,
			&runtime_code,
			CallContext::Offchain,
		)
		.execute()
		.map_err(|e| {
			let e = e.to_string();
			// Say this up front; the trap backtrace below buries the cause.
			if self.ledger_db.is_none() && e.contains("not in storage arena") {
				log::error!(
					"A migration read ledger state the empty scratch arena does not have. \
					 Pass --ledger-db pointing at the ledger_storage of a node holding \
					 this chain's state (docs/fork-testing.md)."
				);
			}
			format!("TryRuntime_on_runtime_upgrade failed: {e}")
		})?;

		let (consumed, max) = <(Weight, Weight)>::decode(&mut &encoded[..])
			.map_err(|e| format!("decoding migration weight result: {e:?}"))?;
		log::info!("Migration consumed {consumed:?} of max block weight {max:?}");

		Ok(())
	}

	/// Falls back to a fresh directory so the ledger host functions always have an
	/// arena to open; without one every ledger call fails.
	fn ledger_db_path(&self) -> sc_cli::Result<PathBuf> {
		if let Some(path) = &self.ledger_db {
			log::info!("Using ledger storage at {path:?}");
			return Ok(path.clone());
		}
		let path = std::env::temp_dir()
			.join(format!("midnight-try-runtime-ledger-{}", std::process::id()));
		std::fs::create_dir_all(&path)
			.map_err(|e| format!("creating scratch ledger storage {path:?}: {e}"))?;
		log::warn!("No --ledger-db given; using an empty scratch arena at {path:?}");
		Ok(path)
	}
}

fn load_snapshot(path: &PathBuf) -> sc_cli::Result<TestExternalities<HashingFor<Block>>> {
	let bytes = std::fs::read(path).map_err(|e| format!("reading snapshot {path:?}: {e}"))?;

	// Decode the version prefix first, so a mismatch reports itself instead of
	// surfacing as a confusing struct-decode failure.
	let version = Compact::<u16>::decode(&mut &*bytes)
		.map_err(|e| format!("decoding snapshot version: {e:?}"))?;
	if version != EXPECTED_SNAPSHOT_VERSION {
		return Err(format!(
			"unsupported snapshot version {}: expected {}; recreate with a matching try-runtime-cli",
			version.0, EXPECTED_SNAPSHOT_VERSION.0,
		)
		.into());
	}

	let snapshot =
		Snapshot::decode(&mut &*bytes).map_err(|e| format!("decoding snapshot body: {e:?}"))?;

	Ok(TestExternalities::from_raw_snapshot(
		snapshot.raw_storage,
		snapshot.storage_root,
		snapshot.state_version,
	))
}

fn decode_version(
	executor: &WasmExecutor<HostFunctions>,
	code: &[u8],
	ext: &mut TestExternalities<HashingFor<Block>>,
) -> sc_cli::Result<RuntimeVersion> {
	let encoded = executor
		.read_runtime_version(code, &mut ext.ext())
		.map_err(|e| format!("read_runtime_version failed: {e:?}"))?;
	RuntimeVersion::decode(&mut &*encoded)
		.map_err(|e| format!("decode RuntimeVersion: {e:?}").into())
}
