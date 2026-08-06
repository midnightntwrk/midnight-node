// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Unpersist Anchored tip roots (one `unpersist` per tip).

use std::time::Duration;

use crate::ledger_9::api::{self, Ledger};
use ledger_storage_ledger_8::{
	DefaultDB,
	arena::{ArenaKey, TypedArenaKey},
	db::{DB, ParityDb, paritydb::OwnedDb},
	storage::{default_storage, try_get_default_storage},
};
use midnight_primitives_ledger::LedgerStorageExt;

type DbSeparate = ParityDb;
type DbUnified = ParityDb<sha2::Sha256, OwnedDb, { LedgerStorageExt::COLUMN_OFFSET }>;

/// Unpersist each tip once.
///
/// **Not idempotent**: each call blindly decrements the tip's arena root
/// count by one — calling twice for the same persist event drives the count
/// negative (storage-core treats negative root counts as a fatal invariant
/// violation) or steals a count from another tip sharing the root. Callers
/// must guarantee at-most-once delivery per tip: durably retire the tip
/// *before* calling this, and only re-submit a tip whose retirement was
/// rolled back (see the node GC worker's remove-then-unpersist ordering).
pub fn unpersist_tips(tips: &[Vec<u8>]) -> Result<(), String> {
	if tips.is_empty() {
		return Ok(());
	}
	if unpersist_in::<DbSeparate>(tips)?.is_some() {
		return Ok(());
	}
	if unpersist_in::<DbUnified>(tips)?.is_some() {
		return Ok(());
	}
	if unpersist_in::<DefaultDB>(tips)?.is_some() {
		return Ok(());
	}
	Err("ledger storage is not initialized".into())
}

/// Run incremental arena GC for up to `budget`. Returns culled node count.
pub fn collect_garbage(budget: Duration) -> Result<usize, String> {
	if let Some(n) = gc_in::<DbSeparate>(budget)? {
		return Ok(n);
	}
	if let Some(n) = gc_in::<DbUnified>(budget)? {
		return Ok(n);
	}
	if let Some(n) = gc_in::<DefaultDB>(budget)? {
		return Ok(n);
	}
	Err("ledger storage is not initialized".into())
}

fn unpersist_in<D: DB + 'static>(tips: &[Vec<u8>]) -> Result<Option<()>, String> {
	if try_get_default_storage::<D>().is_none() {
		return Ok(None);
	}
	let api = api::new();
	let mut roots = Vec::with_capacity(tips.len());
	for tip in tips {
		let typed: TypedArenaKey<Ledger<D>, D::Hasher> = api
			.tagged_deserialize(tip)
			.map_err(|e| format!("decode tip state key: {e:?}"))?;
		let key: ArenaKey<D::Hasher> = typed.into();
		roots.push(key.hash().clone());
	}
	default_storage::<D>().with_backend(|backend| {
		for root in &roots {
			backend.unpersist(root);
		}
	});
	Ok(Some(()))
}

fn gc_in<D: DB + 'static>(budget: Duration) -> Result<Option<usize>, String> {
	if try_get_default_storage::<D>().is_none() {
		return Ok(None);
	}
	let culled = default_storage::<D>().with_backend(|backend| backend.gc(budget));
	Ok(Some(culled))
}
