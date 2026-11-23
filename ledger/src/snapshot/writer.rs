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

//! Copying ledger trees node by node into a store, through the storage engine's public API:
//! nodes are `OnDiskObject`s, `ArenaKey::refs` yields the stored children, `DB::batch_update`
//! writes them.

use super::{Hash, Key, Node, store::Store};
use midnight_storage_core::db::Update;
use std::collections::HashSet;

const WRITE_BATCH: usize = 8192;
const LOOKUP_BATCH: usize = 1024;

/// Copy every node reachable from `roots` into `target`. Each node is looked up with `stored`
/// first and, for those it misses, with `in_memory` in batches. A node `target` already holds is
/// taken to come with everything below it, as every copy writes whole trees.
pub fn copy(
	roots: &[Key],
	mut stored: impl FnMut(&Hash) -> Option<Node>,
	mut in_memory: impl FnMut(&[Hash]) -> Vec<Option<Node>>,
	target: &Store,
) -> Result<(), String> {
	let mut visited = HashSet::new();
	let mut stack: Vec<Hash> = roots.iter().flat_map(|r| r.refs()).cloned().collect();
	let mut writes = Vec::with_capacity(WRITE_BATCH);
	let mut misses = Vec::new();
	let visit = |key: Hash, node: Node, stack: &mut Vec<Hash>, writes: &mut Vec<_>| {
		stack.extend(node.children.iter().flat_map(|c| c.refs()).cloned());
		writes.push((key, Update::InsertNode(node)));
	};
	loop {
		while let Some(key) = stack.pop() {
			if !visited.insert(key.clone()) || target.contains(&key) {
				continue;
			}
			match stored(&key) {
				Some(node) => visit(key, node, &mut stack, &mut writes),
				None => misses.push(key),
			}
			if misses.len() >= LOOKUP_BATCH {
				break;
			}
			if writes.len() >= WRITE_BATCH {
				target.batch_update(writes.drain(..));
			}
		}
		if misses.is_empty() {
			break;
		}
		let found = in_memory(&misses);
		for (key, node) in std::mem::take(&mut misses).into_iter().zip(found) {
			let node =
				node.ok_or_else(|| format!("ledger node {} not found", hex::encode(key.0)))?;
			visit(key, node, &mut stack, &mut writes);
		}
	}
	target.batch_update(writes.into_iter());
	Ok(())
}
