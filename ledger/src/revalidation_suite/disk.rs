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

//! Two worlds, each with its own database, that run the same calls with a warm and a cold cache
//! and flush after each block, for P9a.

use super::ledger::{
	Bridge, LedgerState, TransactionSignature, api,
	ledger_storage_local::{
		DefaultDB,
		arena::ArenaHash,
		backend::OnDiskObject,
		db::{DB, DummyArbitrary, Update},
		storage::{
			DEFAULT_CACHE_SIZE, Storage, default_storage, set_default_storage,
			unsafe_drop_default_storage,
		},
	},
	midnight_serialize_local::{tagged_deserialize, tagged_serialize},
};
use super::{
	Call, Exts, Step, Storage as CallStorage, TX_VALIDATION_CACHE_CAPACITY, TxValidationCache,
	TxValidationCacheOutcome, cache_key, model, peek,
	proofs::UnverifiedProofs,
	properties, run, shim, smoke_trees, step, storage, target,
	tree::{EdgeRecord, RUNTIME_VERSION, Tree, TreeBridge},
};
use crate::ephemeral::EphemeralDb;
use proptest::prelude::*;
use sp_externalities::Externalities;
use std::{
	collections::{BTreeMap, BTreeSet, HashMap},
	sync::{Mutex, MutexGuard, OnceLock, PoisonError},
};

type Hash = ArenaHash<<DefaultDB as DB>::Hasher>;
type Object = OnDiskObject<<DefaultDB as DB>::Hasher>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Write {
	Node(Hash),
	RootCount(Hash, u32),
	Delete(Hash),
}

/// What reached a world's database.
pub type Writes = BTreeSet<Write>;

static WRITES: [Mutex<Writes>; 2] = [const { Mutex::new(BTreeSet::new()) }; 2];

/// An in-memory database that records every write reaching it. Each `WARM` value is its own
/// `DB` type, so it gets its own process-wide storage.
#[derive(Debug, Default)]
struct RecordingDb<const WARM: bool>(DefaultDB);

impl<const WARM: bool> From<DefaultDB> for RecordingDb<WARM> {
	fn from(db: DefaultDB) -> Self {
		Self(db)
	}
}

type WarmDb = RecordingDb<true>;
type ColdDb = RecordingDb<false>;

impl<const WARM: bool> RecordingDb<WARM> {
	fn record(write: Write) {
		WRITES[WARM as usize].lock().unwrap().insert(write);
	}

	/// What reached the database since the last call.
	fn take() -> Writes {
		std::mem::take(&mut WRITES[WARM as usize].lock().unwrap())
	}
}

impl<const WARM: bool> DummyArbitrary for RecordingDb<WARM> {}

impl<const WARM: bool> DB for RecordingDb<WARM> {
	type Hasher = <DefaultDB as DB>::Hasher;
	type ScanResumeHandle = <DefaultDB as DB>::ScanResumeHandle;

	fn get_node(&self, key: &Hash) -> Option<Object> {
		self.0.get_node(key)
	}

	fn insert_node(&mut self, key: Hash, object: Object) {
		Self::record(Write::Node(key.clone()));
		self.0.insert_node(key, object)
	}

	fn delete_node(&mut self, key: &Hash) {
		Self::record(Write::Delete(key.clone()));
		self.0.delete_node(key)
	}

	fn batch_update<I>(&mut self, iter: I)
	where
		I: Iterator<Item = (Hash, Update<Self::Hasher>)>,
	{
		self.0.batch_update(iter.inspect(|(key, update)| {
			Self::record(match update {
				Update::InsertNode(_) => Write::Node(key.clone()),
				Update::DeleteNode => Write::Delete(key.clone()),
				Update::SetRootCount(count) => Write::RootCount(key.clone(), *count),
			})
		}))
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
		Self::record(Write::RootCount(key.clone(), count));
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

type B<D> = Bridge<TransactionSignature, D, UnverifiedProofs>;

/// Rebuilds the tree's states in `D`'s storage from its genesis and edges, against a cold cache,
/// and checks they are the tree's.
fn replay<D: DB>(tree: &Tree, ext: &mut dyn Externalities)
where
	D::Hasher: storage_sha2::digest::OutputSizeUser<OutputSize = storage_sha2::digest::consts::U32>,
{
	let api = api::new();
	let genesis = TreeBridge::load_ledger(&api, &tree.nodes[0].state_key).unwrap();
	let mut bytes = vec![];
	tagged_serialize(&genesis.state, &mut bytes).unwrap();
	let state: LedgerState<D> = tagged_deserialize(&bytes[..]).unwrap();
	let mut genesis = default_storage::<D>().arena.alloc(api::Ledger::new(state));
	genesis.persist();
	assert_eq!(api.tagged_serialize(&genesis.as_typed_key()).unwrap(), tree.nodes[0].state_key);

	let cold = TxValidationCache::new(1);
	for node in &tree.nodes[1..] {
		let parent = &tree.nodes[node.parent.unwrap()];
		let (at, bc) = (&parent.state_key[..], parent.block_context.clone());
		let state_key = match node.edge.as_ref().unwrap() {
			EdgeRecord::Tx(tx) => {
				let bytes = &tree.txs[*tx].bytes;
				shim::apply_transaction::<D, UnverifiedProofs>(
					ext,
					at,
					bytes,
					bc,
					true,
					true,
					RUNTIME_VERSION,
					false,
					&cold,
				)
				.unwrap()
				.state_root
			},
			EdgeRecord::EndBlock => B::<D>::post_block_update(ext, at, bc).unwrap(),
			EdgeRecord::System(tx) => {
				B::<D>::apply_system_transaction(ext, at, tx, bc).unwrap().state_root
			},
		};
		assert_eq!(state_key, node.state_key, "a replayed state is the tree's");
		// As `build_tree` does.
		B::<D>::load_ledger(&api, &state_key).unwrap().persist();
	}
}

/// Flushes `D`'s storage the way a block does, and returns what reached its database.
fn flushed<const WARM: bool>(exts: &mut Exts) -> Writes {
	B::<RecordingDb<WARM>>::flush_storage(&mut exts.onchain.ext());
	RecordingDb::<WARM>::take()
}

/// The smoke trees' states as a database holds them once replayed and flushed.
pub struct Disk {
	nodes: Vec<(Hash, Object)>,
	roots: HashMap<Hash, u32>,
}

impl Disk {
	fn replayed() -> Self {
		let db = DefaultDB::default();
		let storage = || Storage::new(DEFAULT_CACHE_SIZE, WarmDb::from(db.clone()));
		assert!(set_default_storage(storage).is_ok(), "nothing else uses WarmDb");
		let mut exts = Exts::new();
		for tree in smoke_trees() {
			replay::<WarmDb>(tree, &mut exts.onchain.ext());
		}
		flushed::<true>(&mut exts);
		Self { nodes: db.scan(None, usize::MAX).0, roots: db.get_roots() }
	}

	/// Gives `RecordingDb<WARM>` a fresh storage over a copy of this disk, and drops the
	/// off-chain storage reading through it: what that caches decides what off-chain calls load
	/// into the on-chain storage's memory, and so what a later flush writes.
	fn restore<const WARM: bool>(&self) {
		let mut db = DefaultDB::default();
		for (key, object) in &self.nodes {
			db.insert_node(key.clone(), object.clone());
		}
		for (key, count) in &self.roots {
			db.set_root_count(key.clone(), *count);
		}
		unsafe_drop_default_storage::<RecordingDb<WARM>>();
		unsafe_drop_default_storage::<EphemeralDb<RecordingDb<WARM>>>();
		let storage = || Storage::new(DEFAULT_CACHE_SIZE, RecordingDb::<WARM>::from(db));
		assert!(set_default_storage(storage).is_ok(), "the lock keeps other cases out");
		RecordingDb::<WARM>::take();
	}
}

/// Both worlds, reset to the replayed smoke trees, locked for one history at a time.
///
/// Reset per case, not carried over, so a case's verdict depends on its own history only and a
/// failure shrinks cleanly.
pub fn worlds() -> MutexGuard<'static, Disk> {
	static DISK: OnceLock<Mutex<Disk>> = OnceLock::new();
	let disk = DISK.get_or_init(|| Mutex::new(Disk::replayed())).lock();
	let disk = disk.unwrap_or_else(PoisonError::into_inner);
	disk.restore::<true>();
	disk.restore::<false>();
	disk
}

/// What a warm call met that makes P9a non-trivial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Seen {
	/// An on-chain call that hit a receipt for another state, so the warm world loaded it.
	OnChainHitFromOtherState,
	OnChainRejectedOnHit,
	AppliedOnHit,
}

/// A call, and whether a block ends after it.
type BlockStep = (Step, bool);

/// Runs each step in both worlds: the warm one keeps its cache, the cold one starts each call
/// with an empty one. Checks [`properties::same_result`] per call, and at each block end and
/// after the last step, flushes both and checks [`properties::flushes_match`].
///
/// Flushing after every call would hide a receipt that holds an allocation: the flush after the
/// call that made it writes it in both worlds. Across several calls, the cold node holds only
/// the last call's receipt.
pub fn check_disk_parity(
	tree: &Tree,
	steps: &[BlockStep],
	mut seen: Option<&mut BTreeMap<Seen, usize>>,
) -> Result<(), TestCaseError> {
	let mut exts = Exts::new();
	let warm = TxValidationCache::new(TX_VALIDATION_CACHE_CAPACITY);
	let mut cold;
	for (i, (step, block_end)) in steps.iter().enumerate() {
		let hit = seen.as_ref().map(|_| {
			let (tx, ..) = target(tree, step);
			let call = model::ModelCall::new(tree, step);
			let before = peek(&warm, &cache_key(tree, tx, step.runtime_version));
			let (row, outcome) = model::outcome(before.as_ref(), true, &call);
			let hit = call.reaches_cache && outcome != TxValidationCacheOutcome::CacheMiss;
			(hit, hit && row == model::OutcomeRow::OtherStateLoadable)
		});

		let warm_result = run::<WarmDb>(tree, step, &mut exts, &warm);
		cold = TxValidationCache::new(TX_VALIDATION_CACHE_CAPACITY);
		let cold_result = run::<ColdDb>(tree, step, &mut exts, &cold);
		properties::same_result(step, &warm_result, &cold_result)?;

		if let (Some(seen), Some((hit, other_state))) = (seen.as_deref_mut(), hit) {
			let on_chain = storage(&step.call) == CallStorage::OnChain;
			let mut see = |s| *seen.entry(s).or_default() += 1;
			if on_chain && other_state {
				see(Seen::OnChainHitFromOtherState);
			}
			if on_chain && hit && warm_result.is_err() {
				see(Seen::OnChainRejectedOnHit);
			}
			if hit && warm_result.is_ok() && matches!(step.call, Call::Apply { .. }) {
				see(Seen::AppliedOnHit);
			}
		}

		if *block_end || i + 1 == steps.len() {
			let (warm_writes, cold_writes) =
				(flushed::<true>(&mut exts), flushed::<false>(&mut exts));
			properties::flushes_match(step, &warm_writes, &cold_writes)?;
		}
	}
	Ok(())
}

pub fn steps() -> impl Strategy<Value = Vec<BlockStep>> {
	prop::collection::vec((step(), prop::bool::weighted(0.25)), 1..40)
}
