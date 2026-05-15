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

// In-tree replacement for the standalone `try-runtime-cli`, kept slim because
// the standalone tool only links `sp_io::SubstrateHostFunctions` and so cannot
// resolve the `ledger_*_bridge` host functions that `pallet-midnight`'s
// `on_runtime_upgrade` invokes.

use std::path::PathBuf;

use clap::Args;
use frame_remote_externalities::{Builder, Mode, OfflineConfig, SnapshotConfig};
use frame_support::weights::Weight;
use frame_try_runtime::UpgradeCheckSelect;
use parity_scale_codec::{Decode, Encode};
use sc_executor::{DEFAULT_HEAP_ALLOC_STRATEGY, WasmExecutor};
use sp_api::CallContext;
use sp_core::storage::well_known_keys;
use sp_core::traits::ReadRuntimeVersion;
use sp_externalities::Extensions;
use sp_runtime::traits::HashingFor;
use sp_state_machine::{OverlayedChanges, StateMachine, backend::TryPendingCode};
use sp_version::RuntimeVersion;

use crate::service::HostFunctions;
use midnight_node_runtime::Block;

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
}

impl TryRuntimeCmd {
	pub fn run(&self) -> sc_cli::Result<()> {
		let tokio = sc_cli::build_runtime()?;
		tokio.block_on(self.run_inner())
	}

	async fn run_inner(&self) -> sc_cli::Result<()> {
		let executor: WasmExecutor<HostFunctions> = WasmExecutor::builder()
			.with_onchain_heap_alloc_strategy(DEFAULT_HEAP_ALLOC_STRATEGY)
			.with_offchain_heap_alloc_strategy(DEFAULT_HEAP_ALLOC_STRATEGY)
			.build();

		let mut ext = Builder::<Block>::new()
			.mode(Mode::Offline(OfflineConfig {
				state_snapshot: SnapshotConfig::new(&self.snap),
			}))
			.build()
			.await?;

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

		log::info!(
			"🔬 Running TryRuntime_on_runtime_upgrade with checks: {:?}",
			self.checks
		);

		let runtime_code_backend = sp_state_machine::backend::BackendRuntimeCode::new(
			&ext.backend,
			TryPendingCode::No,
		);
		let runtime_code = runtime_code_backend.runtime_code()?;
		let mut changes = OverlayedChanges::<HashingFor<Block>>::default();
		let mut extensions = Extensions::default();

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
		.map_err(|e| format!("TryRuntime_on_runtime_upgrade failed: {e}"))?;

		let (consumed, max) = <(Weight, Weight)>::decode(&mut &encoded[..])
			.map_err(|e| format!("decoding migration weight result: {e:?}"))?;
		log::info!("Migration consumed {consumed:?} of max block weight {max:?}");

		Ok(())
	}
}

fn decode_version(
	executor: &WasmExecutor<HostFunctions>,
	code: &[u8],
	ext: &mut frame_remote_externalities::RemoteExternalities<Block>,
) -> sc_cli::Result<RuntimeVersion> {
	let encoded = executor
		.read_runtime_version(code, &mut ext.ext())
		.map_err(|e| format!("read_runtime_version failed: {e:?}"))?;
	RuntimeVersion::decode(&mut &*encoded).map_err(|e| format!("decode RuntimeVersion: {e:?}").into())
}
