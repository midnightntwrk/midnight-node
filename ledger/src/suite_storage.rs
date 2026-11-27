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

//! A ledger storage that tests choose per execution, so two worlds can run side by side in one
//! process. Host functions use it in place of the node's ParityDb when [`SuiteStorageExt`] is
//! registered.

use crate::ledger_9::ledger_storage_local::{
	DefaultDB,
	arena::ArenaHash,
	backend::OnDiskObject,
	db::{DB, DummyArbitrary, Update},
};
use std::collections::HashMap;

type Hash = ArenaHash<<DefaultDB as DB>::Hasher>;
type Object = OnDiskObject<<DefaultDB as DB>::Hasher>;

sp_externalities::decl_extension! {
	/// Makes the ledger host functions use `SuiteDb<W>`'s storage.
	pub struct SuiteStorageExt(u8);
}

/// An in-memory database. Each `W` is its own `DB` type, so it gets its own process-wide
/// storage. `DefaultDB`'s clones share their contents, so a clone of the inner database reads
/// what the storage wrote.
#[derive(Debug, Default)]
pub struct SuiteDb<const W: u8>(pub DefaultDB);

impl<const W: u8> DummyArbitrary for SuiteDb<W> {}

impl<const W: u8> DB for SuiteDb<W> {
	type Hasher = <DefaultDB as DB>::Hasher;
	type ScanResumeHandle = <DefaultDB as DB>::ScanResumeHandle;

	fn get_node(&self, key: &Hash) -> Option<Object> {
		self.0.get_node(key)
	}

	fn insert_node(&mut self, key: Hash, object: Object) {
		self.0.insert_node(key, object)
	}

	fn delete_node(&mut self, key: &Hash) {
		self.0.delete_node(key)
	}

	fn batch_update<I>(&mut self, iter: I)
	where
		I: Iterator<Item = (Hash, Update<Self::Hasher>)>,
	{
		self.0.batch_update(iter)
	}

	fn batch_get_nodes<I>(&self, keys: I) -> Vec<(Hash, Option<Object>)>
	where
		I: Iterator<Item = Hash>,
	{
		self.0.batch_get_nodes(keys)
	}

	fn get_root_count(&self, key: &Hash) -> u32 {
		self.0.get_root_count(key)
	}

	fn set_root_count(&mut self, key: Hash, count: u32) {
		self.0.set_root_count(key, count)
	}

	fn get_roots(&self) -> HashMap<Hash, u32> {
		self.0.get_roots()
	}

	fn size(&self) -> usize {
		self.0.size()
	}

	fn scan(
		&self,
		resume_from: Option<Self::ScanResumeHandle>,
		batch_size: usize,
	) -> (Vec<(Hash, Object)>, Option<Self::ScanResumeHandle>) {
		self.0.scan(resume_from, batch_size)
	}
}
