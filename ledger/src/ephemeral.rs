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

//! Ledger storage for runtime calls that are neither block authoring nor block import.
//!
//! Ledger state is persisted into the process-wide `default_storage::<D>()` only during block
//! authoring and import. Other runtime calls run all their ledger work, reads included, against
//! `Storage<EphemeralDb<D>>`, which reads through the shared storage and keeps its own writes in
//! memory. The states they produce are held in [`LedgerEphemeralExt`] for the rest of the
//! execution and released with it. Reads go there too because they allocate, and a flush of the
//! shared storage would write those allocations to disk.
//!
//! A call counts as block execution when [`LedgerOnchainExt`] is registered, which the node's
//! executor does for `CallContext::Onchain` only.

use ledger_storage_ledger_8::{
	Storage,
	arena::{ArenaHash, Sp},
	backend::OnDiskObject,
	db::{DB, DummyArbitrary, Update},
	storage::{DEFAULT_CACHE_SIZE, default_storage, set_default_storage, try_get_default_storage},
};
use midnight_primitives_ledger::{LedgerEphemeralExt, LedgerOnchainExt, LedgerStorageExt};
use sp_externalities::{Externalities, ExternalitiesExt};
use std::{collections::HashMap, marker::PhantomData, sync::Arc};

const LOG_TARGET: &str = "midnight::ledger::ephemeral";

/// A [`DB`] that serves reads from the shared storage for `D` (its caches, then its database)
/// and discards every write.
#[derive(Debug, Default)]
pub struct EphemeralDb<D>(PhantomData<D>);

impl<D: DB> DummyArbitrary for EphemeralDb<D> {}

impl<D: DB> DB for EphemeralDb<D> {
	type Hasher = D::Hasher;
	type ScanResumeHandle = ();

	fn get_node(&self, key: &ArenaHash<Self::Hasher>) -> Option<OnDiskObject<Self::Hasher>> {
		default_storage::<D>().with_backend(|backend| backend.get(key).cloned())
	}

	fn insert_node(&mut self, _key: ArenaHash<Self::Hasher>, _object: OnDiskObject<Self::Hasher>) {}

	fn delete_node(&mut self, _key: &ArenaHash<Self::Hasher>) {}

	fn batch_update<I>(&mut self, _iter: I)
	where
		I: Iterator<Item = (ArenaHash<Self::Hasher>, Update<Self::Hasher>)>,
	{
	}

	fn batch_get_nodes<I>(
		&self,
		keys: I,
	) -> Vec<(ArenaHash<Self::Hasher>, Option<OnDiskObject<Self::Hasher>>)>
	where
		I: Iterator<Item = ArenaHash<Self::Hasher>>,
	{
		default_storage::<D>().with_backend(|backend| {
			keys.map(|key| {
				let node = backend.get(&key).cloned();
				(key, node)
			})
			.collect()
		})
	}

	fn get_root_count(&self, _key: &ArenaHash<Self::Hasher>) -> u32 {
		0
	}

	fn set_root_count(&mut self, _key: ArenaHash<Self::Hasher>, _count: u32) {}

	fn get_roots(&self) -> HashMap<ArenaHash<Self::Hasher>, u32> {
		HashMap::new()
	}

	fn size(&self) -> usize {
		0
	}

	fn scan(
		&self,
		_resume_from: Option<Self::ScanResumeHandle>,
		_batch_size: usize,
	) -> (Vec<(ArenaHash<Self::Hasher>, OnDiskObject<Self::Hasher>)>, Option<Self::ScanResumeHandle>)
	{
		(Vec::new(), None)
	}
}

/// Whether this call is block authoring or block import.
pub fn is_onchain(mut ext: &mut dyn Externalities) -> bool {
	ext.extension::<LedgerOnchainExt>().is_some()
}

/// Make sure `Storage<EphemeralDb<D>>` exists before a bridge instantiated over it runs.
pub fn ensure_storage<D: DB>(mut ext: &mut dyn Externalities) -> Arc<Storage<EphemeralDb<D>>> {
	if let Some(storage) = try_get_default_storage::<EphemeralDb<D>>() {
		return storage;
	}
	let cache_size = ext
		.extension::<LedgerStorageExt>()
		.map_or(DEFAULT_CACHE_SIZE, |storage| storage.cache_size);
	set_default_storage(|| Storage::new(cache_size, EphemeralDb::<D>::default()))
		.unwrap_or_else(|storage| storage)
}

/// Keep a newly produced ledger state: persist it during block execution, otherwise hold it for
/// the rest of the execution. `state_root` must be its serialized key, taken before this call.
pub fn retain<T, D>(mut ext: &mut dyn Externalities, state_root: &[u8], mut state: Sp<T, D>)
where
	T: ledger_storage_ledger_8::Storable<D>,
	D: DB,
	Sp<T, D>: Send,
{
	if is_onchain(ext) {
		state.persist();
		return;
	}
	if ext.extension::<LedgerEphemeralExt>().is_none()
		&& ext.register_extension(LedgerEphemeralExt::default()).is_err()
	{
		log::warn!(target: LOG_TARGET, "No extension support; ledger state dropped after this call");
		return;
	}
	if let Some(held) = ext.extension::<LedgerEphemeralExt>() {
		held.hold(state_root.to_vec(), Box::new(state));
	}
}
