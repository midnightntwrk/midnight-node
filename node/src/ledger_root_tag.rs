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
//! The swap is not flushed here. Flushing the shared write cache from this
//! task would empty it while another block may be mid-execution (authoring
//! N+1 while tagging N), letting arena GC sweep unrooted in-flight nodes.
//! Durability is the next block's `on_finalize` `flush_storage`, which runs
//! after that tip is rooted. A crash before that flush leaves the raw pin on
//! disk; catch-up re-swaps idempotently.
//!
//! Tagging must run for every executed import, including initial/gap/file
//! sync. The filtered import stream is silent for those origins; this task
//! therefore uses `every_import_notification_stream`. Catch-up only covers a
//! restart window where state is still readable — it cannot retag a completed
//! sync of pruned history.

use futures::StreamExt;
use midnight_node_ledger::types::LedgerStateKey;
use midnight_node_runtime::{Runtime, opaque::Block};
use pallet_midnight::StateKey;
use parity_scale_codec::Decode;
use sc_client_api::{Backend, BlockchainEvents, StorageProvider};
use sp_blockchain::HeaderBackend;
use sp_core::storage::StorageKey;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use std::sync::Arc;

const LOG_TARGET: &str = "midnight::ledger-root-tag";

/// Import notifications are not replayed on restart; walk this many canonical
/// parents from best to cover a crash between swap and the next `on_finalize`
/// flush. The raw pin is still on disk, so re-swap is idempotent. Not a
/// substitute for tagging during sync: pruned `StateKey`s cannot be retagged.
const CATCH_UP_MAX: u32 = 256;

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
	// Subscribe first so imports that land between catch-up and the loop are
	// queued. Duplicates with catch-up are idempotent (`swap_raw_pin_for_tagged`).
	let mut notifications = client.every_import_notification_stream();
	catch_up(&*client);

	while let Some(notification) = notifications.next().await {
		if tag_hash(&*client, notification.hash) == Some(true) {
			log::debug!(
				target: LOG_TARGET,
				"Staged hash-tag for anchored ledger tip {:?}",
				notification.hash
			);
		}
	}
}

fn catch_up<C, B>(client: &C)
where
	C: HeaderBackend<Block> + StorageProvider<Block, B>,
	B: Backend<Block>,
{
	let info = client.info();
	let genesis = info.genesis_hash;
	let mut hash = info.best_hash;
	let mut staged = false;

	for _ in 0..CATCH_UP_MAX {
		match tag_hash(client, hash) {
			Some(true) => staged = true,
			Some(false) => break,
			None => {},
		}
		if hash == genesis {
			break;
		}
		let Some(parent) = client.header(hash).ok().flatten().map(|h| *h.parent_hash()) else {
			break;
		};
		hash = parent;
	}

	if genesis != info.best_hash && tag_hash(client, genesis) == Some(true) {
		staged = true;
	}

	if staged {
		log::info!(
			target: LOG_TARGET,
			"Caught up hash-tagging of anchored ledger tips (durable on next on_finalize flush)"
		);
	}
}

/// `Some(true)` if a new wrapper was staged, `Some(false)` if that hash already
/// tags this tip, `None` if `StateKey` is missing or not an Anchored ledger
/// state of a known version.
fn tag_hash<C, B>(client: &C, hash: <Block as BlockT>::Hash) -> Option<bool>
where
	C: StorageProvider<Block, B>,
	B: Backend<Block>,
{
	let storage_key = StorageKey(StateKey::<Runtime>::hashed_key().to_vec());
	let bytes = client.storage(hash, &storage_key).ok().flatten()?;
	let state_key = LedgerStateKey::decode(&mut &bytes.0[..]).ok()?;
	if !matches!(state_key, LedgerStateKey::Anchored(_)) {
		log::debug!(
			target: LOG_TARGET,
			"Skipping hash-tag at {hash:?}: StateKey is not Anchored"
		);
		return None;
	}
	midnight_node_ledger::tag_anchored_tip(state_key.bytes(), hash.as_ref())
}
