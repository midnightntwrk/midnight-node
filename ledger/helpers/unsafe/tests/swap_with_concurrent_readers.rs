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

//! Threads building ledger objects while the on-disk arena is installed: the swap
//! either fails or no object lands in the arena it replaced.
//! Its own test binary, so no other test has touched the arena first.

use midnight_ledger_unsafe_helpers::{
	DefaultDB, Sp,
	arena_db::{InstallError, install_persistent_storage, try_default_storage},
};
use std::sync::{
	Arc, Barrier,
	atomic::{AtomicBool, Ordering},
};

/// One past the small-object limit, so the arena tracks it.
type Tracked = Sp<[u32; 256], DefaultDB>;

#[test]
fn no_object_is_lost_to_the_replaced_arena() {
	const READERS: usize = 4;
	let done = Arc::new(AtomicBool::new(false));
	let start = Arc::new(Barrier::new(READERS + 1));
	let readers: Vec<_> = (0..READERS)
		.map(|_| {
			let (done, start) = (done.clone(), start.clone());
			std::thread::spawn(move || {
				let mut used = Vec::new();
				start.wait();
				while !done.load(Ordering::SeqCst) {
					// `try_default_storage` returns `None` inside the swap instead of panicking.
					if let Some(storage) = try_default_storage() {
						let sp: Tracked = storage.arena.alloc(std::array::from_fn(|i| i as u32));
						used.push((storage, sp));
					}
				}
				used
			})
		})
		.collect();

	let dir = tempfile::tempdir().unwrap();
	start.wait();
	let result = install_persistent_storage(dir.path());
	done.store(true, Ordering::SeqCst);
	let used: Vec<_> = readers.into_iter().flat_map(|r| r.join().unwrap()).collect();

	match result {
		Ok(installed) => assert!(
			used.iter().all(|(storage, _)| Arc::ptr_eq(storage, &installed)),
			"an object was built in the replaced arena"
		),
		Err(InstallError::InUse | InstallError::AlreadyInstalled { .. }) => {},
		Err(e) => panic!("unexpected error: {e}"),
	}
}
