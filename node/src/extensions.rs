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

use midnight_primitives_ledger::{
	LedgerEphemeralExt, LedgerMetrics, LedgerMetricsExt, LedgerOnchainExt, LedgerStorage,
	LedgerStorageExt,
};
use sc_client_api::execution_extensions::ExtensionsFactory as ExtensionsFactoryT;
use sc_executor::RuntimeVersionOf;
use sp_core::traits::{CallContext, CodeExecutor, ReadRuntimeVersion, RuntimeCode};
use sp_externalities::{Extensions, Externalities};
use sp_runtime::traits::{Block as BlockT, NumberFor};
use sp_version::RuntimeVersion;
use std::{
	marker::PhantomData,
	sync::{Arc, Mutex},
};

/// Extensions factory
pub struct ExtensionsFactory<Block> {
	ledger_metrics: Arc<Mutex<Option<LedgerMetrics>>>,
	ledger_storage: LedgerStorage,
	_marker: PhantomData<Block>,
}

impl<Block> ExtensionsFactory<Block> {
	pub fn new(
		ledger_metrics: Arc<Mutex<Option<LedgerMetrics>>>,
		ledger_storage: LedgerStorage,
	) -> Self {
		Self { ledger_metrics, ledger_storage, _marker: Default::default() }
	}
}

impl<Block> ExtensionsFactoryT<Block> for ExtensionsFactory<Block>
where
	Block: BlockT,
{
	fn extensions_for(
		&self,
		_block_hash: Block::Hash,
		_block_number: NumberFor<Block>,
	) -> Extensions {
		let mut exts = Extensions::new();

		exts.register(LedgerMetricsExt::new(self.ledger_metrics.clone()));
		exts.register(LedgerStorageExt::new(self.ledger_storage.clone()));
		exts.register(LedgerEphemeralExt::default());

		exts
	}
}

/// A [`CodeExecutor`] that tells the ledger host functions whether a call is block authoring or
/// block import (`CallContext::Onchain`), by registering [`LedgerOnchainExt`] for that call only.
///
/// Ledger state is persisted only for those calls; other runtime calls use an ephemeral storage.
#[derive(Clone)]
pub struct LedgerContextExecutor<E>(pub E);

impl<E: CodeExecutor> CodeExecutor for LedgerContextExecutor<E>
where
	E::Error: From<String>,
{
	type Error = E::Error;

	fn call(
		&self,
		ext: &mut dyn Externalities,
		runtime_code: &RuntimeCode,
		method: &str,
		data: &[u8],
		context: CallContext,
	) -> (Result<Vec<u8>, Self::Error>, bool) {
		let marker = LedgerOnchainExt::type_id();
		let registered = match context {
			CallContext::Onchain { .. } => {
				match ext
					.register_extension_with_type_id(marker, Box::new(LedgerOnchainExt::default()))
				{
					Ok(()) => true,
					Err(sp_externalities::Error::ExtensionAlreadyRegistered) => false,
					// Without the marker the block's ledger state would not be persisted, so
					// refuse to execute it rather than continue.
					Err(e) => {
						let error = format!("Failed to mark `{method}` as block execution: {e:?}");
						log::error!(target: "ledger::executor", "{error}");
						return (Err(error.into()), false);
					},
				}
			},
			CallContext::Offchain => {
				let _ = ext.deregister_extension_by_type_id(marker);
				false
			},
		};

		let result = self.0.call(ext, runtime_code, method, data, context);

		if registered {
			let _ = ext.deregister_extension_by_type_id(marker);
		}
		result
	}
}

impl<E: RuntimeVersionOf> RuntimeVersionOf for LedgerContextExecutor<E> {
	fn runtime_version(
		&self,
		ext: &mut dyn Externalities,
		runtime_code: &RuntimeCode,
	) -> sc_executor::error::Result<RuntimeVersion> {
		self.0.runtime_version(ext, runtime_code)
	}
}

impl<E: ReadRuntimeVersion> ReadRuntimeVersion for LedgerContextExecutor<E> {
	fn read_runtime_version(
		&self,
		wasm_code: &[u8],
		ext: &mut dyn Externalities,
	) -> Result<Vec<u8>, String> {
		self.0.read_runtime_version(wasm_code, ext)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_externalities::ExternalitiesExt;
	use sp_state_machine::BasicExternalities;

	/// Reports whether the call ran with the marker registered.
	#[derive(Clone)]
	struct Probe;

	impl ReadRuntimeVersion for Probe {
		fn read_runtime_version(
			&self,
			_wasm_code: &[u8],
			_ext: &mut dyn Externalities,
		) -> Result<Vec<u8>, String> {
			Ok(vec![])
		}
	}

	impl CodeExecutor for Probe {
		type Error = String;

		fn call(
			&self,
			mut ext: &mut dyn Externalities,
			_runtime_code: &RuntimeCode,
			_method: &str,
			_data: &[u8],
			_context: CallContext,
		) -> (Result<Vec<u8>, Self::Error>, bool) {
			(Ok(vec![ext.extension::<LedgerOnchainExt>().is_some() as u8]), false)
		}
	}

	fn has_marker(ext: &mut BasicExternalities) -> bool {
		let mut ext: &mut dyn Externalities = ext;
		ext.extension::<LedgerOnchainExt>().is_some()
	}

	fn call(ext: &mut BasicExternalities, context: CallContext) -> bool {
		let code = sp_core::traits::WrappedRuntimeCode(Default::default());
		let runtime_code = RuntimeCode { code_fetcher: &code, heap_pages: None, hash: vec![] };
		let (result, _) =
			LedgerContextExecutor(Probe).call(ext, &runtime_code, "Core_version", &[], context);
		result.unwrap() == vec![1]
	}

	#[test]
	fn marks_block_authoring_and_import_only() {
		let mut ext = BasicExternalities::default();
		assert!(call(&mut ext, CallContext::Onchain { import: false }));
		assert!(call(&mut ext, CallContext::Onchain { import: true }));
		assert!(!call(&mut ext, CallContext::Offchain));
	}

	#[test]
	fn marker_does_not_outlive_the_call() {
		let mut ext = BasicExternalities::default();
		assert!(call(&mut ext, CallContext::Onchain { import: true }));
		assert!(!has_marker(&mut ext));
		assert!(!call(&mut ext, CallContext::Offchain));
	}

	#[test]
	fn offchain_calls_never_see_a_marker() {
		let mut ext = BasicExternalities::default();
		ext.register_extension(LedgerOnchainExt::default());
		assert!(!call(&mut ext, CallContext::Offchain));
	}
}
