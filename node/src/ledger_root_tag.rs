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
//! flush leaves the raw pin on disk; on restart only `best` (and genesis, which
//! is imported before this task exists) can have been missed — later imports
//! go through `every_import_notification_stream`, including initial sync.
//! Each imported block's `on_finalize` flushes the previous swap, so at most
//! the current tip is unflushed.

use futures::StreamExt;
use midnight_node_ledger::types::LedgerStateKey;
use midnight_node_runtime::{Runtime, opaque::Block};
use pallet_midnight::StateKey;
use parity_scale_codec::Decode;
use sc_client_api::{Backend, BlockchainEvents, StorageProvider};
use sp_blockchain::HeaderBackend;
use sp_core::storage::StorageKey;
use sp_runtime::traits::Block as BlockT;
use std::sync::Arc;

const LOG_TARGET: &str = "midnight::ledger-root-tag";

pub async fn watch<C, B>(client: Arc<C>)
where
	C: BlockchainEvents<Block>
		+ HeaderBackend<Block>
		+ StorageProvider<Block, B>
		+ Send
		+ Sync
		+ 'static,
	B: Backend<Block>,
{
	// Subscribe before tagging genesis/best so sync imports that race this
	// task are queued rather than dropped.
	let mut notifications = client.every_import_notification_stream();

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
	let storage_key = StorageKey(StateKey::<Runtime>::hashed_key().to_vec());
	let Some(bytes) = client.storage(hash, &storage_key).ok().flatten() else {
		return false;
	};
	let Ok(state_key) = LedgerStateKey::decode(&mut &bytes.0[..]) else {
		return false;
	};
	if !matches!(state_key, LedgerStateKey::Anchored(_)) {
		log::debug!(
			target: LOG_TARGET,
			"Skipping hash-tag at {hash:?}: StateKey is not Anchored"
		);
		return false;
	}
	midnight_node_ledger::tag_anchored_tip(state_key.bytes(), hash.as_ref()) == Some(true)
}
