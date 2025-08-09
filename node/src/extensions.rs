// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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
	LedgerMetrics, LedgerMetricsExt, LedgerStorage, LedgerStorageExt,
};
use sc_client_api::execution_extensions::ExtensionsFactory as ExtensionsFactoryT;
use sp_externalities::Extensions;
use sp_runtime::traits::{Block as BlockT, NumberFor};
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

		// Place to add more extensions if necessary

		exts
	}
}
