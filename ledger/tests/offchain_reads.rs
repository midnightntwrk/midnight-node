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

//! Off-chain ledger reads against flushes of the shared ledger storage.
//!
//! A separate binary because `set_default_storage` is process-global and set-once, and because
//! the assertions inspect the shared write cache, which other tests would also fill.

use ledger_storage_ledger_8::{db::ParityDb, storage::default_storage};
use midnight_node_ledger::{
	host_api::ledger_9::ledger_9_bridge as host, ledger_9::storage::init_storage_paritydb_separate,
};
use midnight_node_res::networks::{MidnightNetwork, UndeployedNetwork};
use sp_state_machine::BasicExternalities;
use std::{
	sync::atomic::{AtomicBool, Ordering},
	thread,
};

/// Host calls without a `LedgerStorageExt` use the separate-db storage.
type Shared = ParityDb;

const READS: usize = 2_000;

/// Reads at a committed state allocate. Outside block execution none of that may reach the shared
/// write cache, where a flush, including one running during the reads, would write it to disk.
#[test]
fn offchain_reads_leave_nothing_for_a_flush() {
	let dir = tempfile::tempdir().unwrap();
	let genesis =
		init_storage_paritydb_separate(dir.path(), UndeployedNetwork.genesis_state(), 1024);
	default_storage::<Shared>().with_backend(|backend| backend.flush_all_changes_to_db());

	let done = AtomicBool::new(false);
	let flushed = thread::scope(|scope| {
		let flusher = scope.spawn(|| {
			let mut flushed = 0;
			while !done.load(Ordering::Relaxed) {
				flushed += default_storage::<Shared>().with_backend(|backend| {
					let pending = backend.get_write_cache_len();
					backend.flush_all_changes_to_db();
					pending
				});
			}
			flushed
		});

		let mut offchain = BasicExternalities::default();
		sp_externalities::set_and_run_with_externalities(&mut offchain, || {
			for _ in 0..READS {
				host::get_ledger_state_root(&genesis).unwrap();
				host::get_zswap_state_root(&genesis).unwrap();
			}
		});
		done.store(true, Ordering::Relaxed);
		flusher.join().unwrap()
	});

	let pending = default_storage::<Shared>().with_backend(|backend| backend.get_write_cache_len());
	assert_eq!(flushed + pending, 0, "off-chain reads left writes for a flush");
}
