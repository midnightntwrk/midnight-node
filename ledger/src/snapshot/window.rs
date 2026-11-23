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

//! The window: post-block ledger states held in memory instead of persisted, by `StateKey`.
//!
//! Holding a state's tracked handle keeps every node it reaches resolvable: the storage engine's
//! reference counts keep them in memory, or in the store once a snapshot wrote them, and free
//! unshared ones when the last handle is dropped. The handles hold no loaded ledger data (see
//! [`super::hold`]).
//!
//! Handles are dropped after the window lock is released, as dropping one takes arena locks.

use std::{
	any::Any,
	collections::{HashMap, HashSet},
	sync::Mutex,
};

type State = Box<dyn Any + Send>;

struct Entry {
	/// Held, never read: dropping it releases the state.
	_handle: State,
	/// The eviction pass during which it was last inserted.
	pass: u64,
	pins: u32,
}

#[derive(Default)]
struct Window {
	entries: HashMap<Vec<u8>, Entry>,
	pass: u64,
}

impl Window {
	/// Returns `state` back if `state_key` is already held.
	fn insert(&mut self, state_key: Vec<u8>, state: State) -> Option<State> {
		match self.entries.get_mut(&state_key) {
			Some(entry) => {
				entry.pass = self.pass;
				Some(state)
			},
			None => {
				self.entries
					.insert(state_key, Entry { _handle: state, pass: self.pass, pins: 0 });
				None
			},
		}
	}

	fn pin_all(&mut self) -> Vec<Vec<u8>> {
		self.entries
			.iter_mut()
			.map(|(key, entry)| {
				entry.pins += 1;
				key.clone()
			})
			.collect()
	}

	/// Pins those of `keys` it holds; returns them.
	fn pin(&mut self, mut keys: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
		keys.retain(|key| self.entries.get_mut(key).map(|entry| entry.pins += 1).is_some());
		keys
	}

	fn unpin(&mut self, keys: &[Vec<u8>]) {
		for key in keys {
			if let Some(entry) = self.entries.get_mut(key) {
				entry.pins -= 1;
			}
		}
	}

	/// A state inserted during the previous pass is kept: its block may not be in the chain yet.
	fn evict(&mut self, keep: &HashSet<Vec<u8>>) -> Vec<Entry> {
		self.pass += 1;
		let pass = self.pass;
		let doomed: Vec<Vec<u8>> = self
			.entries
			.iter()
			.filter(|(key, e)| !keep.contains(*key) && e.pins == 0 && e.pass + 2 <= pass)
			.map(|(key, _)| key.clone())
			.collect();
		doomed.iter().filter_map(|key| self.entries.remove(key)).collect()
	}
}

static WINDOW: Mutex<Option<Window>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Window) -> R) -> R {
	f(WINDOW
		.lock()
		.unwrap_or_else(|e| e.into_inner())
		.get_or_insert_with(Window::default))
}

/// Hold `state` under `state_key`. A key already held keeps its handle and counts as new.
pub(crate) fn insert(state_key: Vec<u8>, state: State) {
	let duplicate = with(|w| w.insert(state_key, state));
	drop(duplicate);
}

pub fn contains(state_key: &[u8]) -> bool {
	with(|w| w.entries.contains_key(state_key))
}

pub fn len() -> usize {
	with(|w| w.entries.len())
}

/// Held keys, pinned against [`evict`] until dropped.
pub(crate) struct Pinned(pub(crate) Vec<Vec<u8>>);

/// Every held key.
pub(crate) fn pin_all() -> Pinned {
	Pinned(with(Window::pin_all))
}

/// Those of `keys` the window holds.
pub(crate) fn pin(keys: Vec<Vec<u8>>) -> Pinned {
	Pinned(with(|w| w.pin(keys)))
}

impl Drop for Pinned {
	fn drop(&mut self) {
		with(|w| w.unpin(&self.0))
	}
}

/// Drop every unpinned state not in `keep` and not inserted since the previous call. Returns how
/// many were dropped.
pub fn evict(keep: &HashSet<Vec<u8>>) -> usize {
	let dropped = with(|w| w.evict(keep));
	dropped.len()
}
