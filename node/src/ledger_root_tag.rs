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

//! Swap raw Anchored ledger persists for wrappers tagged with the block hash.
//!
//! `on_finalize` persists the post-block tip as a raw GC root because the block
//! hash is not known yet (it includes that state root). Once the block is
//! imported, this task reads `Midnight::StateKey` at that hash and stages
//! `persist_tagged(hash, inner)` plus `unpersist` of the raw pin. Distinct
//! hashes are distinct wrappers, so sibling forks release independently.
//!
//! The swap is not flushed here. Durability is the next block's `on_finalize`
//! `flush_storage`, which runs after that tip is rooted. A crash before that
//! flush leaves the raw pin on disk; on restart the stream is subscribed in
//! `new_full` before this task is spawned (imports cannot miss the queue),
//! then genesis and best are tagged once — they were imported before the
//! stream existed. Later imports go through `every_import_notification_stream`,
//! including initial sync. Warp ledger-sync tags the recovered arena itself
//! (`persist_tagged` in the snapshot import flush) because that target's
//! import notification already fired against an empty arena.
//!
//! `StateKey` is read via the warp ledger-sync `read_state_key` helper so the
//! on-chain storage version picks the layout — the same authority
//! `Pallet::state_key` uses. Byte-sniffing a pre-v3 raw `Vec<u8>` whose length
//! is a multiple of 256 would misdecode it as `Transient` and skip the tip.
//! Pre-v3 *block tips* therefore get tagged during a historical full sync.
//! Pre-v3 *intra-block* intermediates still leak: v1 host functions persist
//! every successor without unpersisting, and that is not recoverable from the
//! native node.

use futures::StreamExt;
use midnight_node_runtime::opaque::Block;
use sc_client_api::{Backend, StorageProvider, client::ImportNotifications};
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use std::sync::Arc;

use crate::warp_ledger_sync::read_state_key;

const LOG_TARGET: &str = "midnight::ledger-root-tag";

pub async fn watch<C, B>(client: Arc<C>, mut notifications: ImportNotifications<Block>)
where
	C: HeaderBackend<Block> + StorageProvider<Block, B> + Send + Sync + 'static,
	B: Backend<Block>,
{
	let info = client.info();
	for hash in [info.genesis_hash, info.best_hash] {
		if tag_hash(&*client, hash) {
			log::debug!(
				target: LOG_TARGET,
				"Staged hash-tag for pre-stream tip {hash:?}"
			);
		}
	}

	while let Some(notification) = notifications.next().await {
		if tag_hash(&*client, notification.hash) {
			log::debug!(
				target: LOG_TARGET,
				"Staged hash-tag for anchored ledger tip {:?}",
				notification.hash
			);
		}
	}
}

/// `true` if a new wrapper was staged.
fn tag_hash<C, B>(client: &C, hash: <Block as BlockT>::Hash) -> bool
where
	C: StorageProvider<Block, B>,
	B: Backend<Block>,
{
	let state_key = match read_state_key::<Block, _, B>(client, hash) {
		Ok(Some(bytes)) => bytes,
		Ok(None) => return false,
		Err(e) => {
			log::debug!(target: LOG_TARGET, "Skipping hash-tag at {hash:?}: {e}");
			return false;
		},
	};
	midnight_node_ledger::tag_anchored_tip(&state_key, hash.as_ref()) == Some(true)
}
