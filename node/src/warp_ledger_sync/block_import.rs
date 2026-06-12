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

//! Block-import gate for warp ledger-sync.
//!
//! After warp + state-sync the node holds the trie at the target block but an **empty ledger
//! arena**, while the recovery monitor is still fetching + importing the arena snapshot. The
//! authoring [`SyncOracle`](super::oracle::MidnightSyncOracle) alone is not enough: the sync engine
//! would still import announced post-warp blocks, each of which executes the runtime against the
//! arena — hitting `NoLedgerState`, and worse, racing the recovery writer (the arena is
//! single-writer).
//!
//! [`GatedBlockImport`] wraps the import queue's block import and, while
//! [`RecoveryGate::ledger_recovery_in_progress`] is true, **rejects** the import of exactly those
//! blocks that would *execute* against the arena, with a [`ConsensusError::ClientImport`] error.
//!
//! ## What is gated (and what must never be)
//!
//! A block import only touches the arena if it executes the runtime, so the gate keys off
//! [`StateAction`]:
//! - `ApplyChanges` — **pass**. The state-sync target block (its state is *imported*, not
//!   executed). State sync must import it *before* recovery can even start; gating it deadlocks
//!   warp. (Also locally-authored blocks, but authoring is separately gated by the oracle.)
//! - `Skip` — **pass**. Gap-sync (block-history) blocks are downloaded with `skip_execution:
//!   true`: they import headers/bodies only and never execute. Gating them silently broke block
//!   history download after warp.
//! - `Execute` — **defer** (would unconditionally execute).
//! - `ExecuteIfPossible` — defer **iff the parent state is present** (that is the exact condition
//!   under which the client executes the block; with the parent state pruned/absent it imports
//!   without execution and never touches the arena). Post-warp, the only blocks with a present
//!   parent state are the descendants of the warp target — precisely the dangerous ones.
//!
//! ## Why defer = `await` the gate (and why the scoping above makes that safe)
//!
//! Every alternative was tried and failed in a distinct, live-observed way:
//! 1. **`Ok(ImportResult::MissingState)` → permanent sync wedge.** substrate treats `MissingState`
//!    as "obsolete, not bad": no peer drop, no restart — and *nothing else*. But `chain_sync`
//!    already advanced `best_queued_number` when it queued the blocks, so after the silent swallow,
//!    sync believes blocks exist that are not in the DB. Peer ancestor searches then probe
//!    `best_queued` (miss — never imported) and descend into the warp gap `1..target` (miss —
//!    headers absent), resolving the common block to **genesis**; `block_requests()` deems common=0
//!    "too far behind" and immediately restarts the ancestor search, forever. Observed live as a
//!    ~8,000 req/s ancestry hot loop with all peers stuck in `AncestorSearch` (announcements
//!    ignored, no block or gap requests, node pinned at the warp target for 9+ hours).
//! 2. **`Err(ClientImport)` → reputation-banned by every peer.** The error maps to
//!    `BlockImportError::Other` → `chain_sync.restart()`, which *does* keep `best_queued_number`
//!    consistent — but each restart re-issues an **identical** block request (same start, same
//!    count), and substrate's `BlockRequestHandler` bans peers that repeat the same request
//!    (`Same block request multiple times`, rep = i32::MIN, disconnect). Within a minute all
//!    serving peers ban the warp node; with the connections gone, even the arena fetch starves
//!    (`Refused`), so recovery itself can wedge. Observed live: 53 restarts, 72 ban/reconnect
//!    cycles, 0 sync peers.
//! 3. **`await` until released — correct, *given the `would_execute` scoping*.** Holding the
//!    import-queue worker used to deadlock when the gate covered all `!with_state()` blocks (the
//!    state-sync target import could end up queued behind a held block). But a block can only be
//!    `would_execute` *after* its ancestor — the state-sync target — has already imported (before
//!    that, no parent state exists anywhere), and arena recovery itself uses only the client +
//!    request-response network, never the import queue. So nothing recovery depends on can sit
//!    behind the await. Each block is requested **once** (no duplicate-request bans), queued
//!    blocks match sync's bookkeeping (no wedge), download read-ahead is bounded by substrate's
//!    `MAX_DOWNLOAD_AHEAD`, and when the gate opens the worker simply drains the backlog in order.
//!
//! On a full sync the gate is never armed, so this is a pure passthrough.

use sc_client_api::Backend;
use sc_consensus::{BlockCheckParams, BlockImport, BlockImportParams, ImportResult, StateAction};
use sp_consensus::Error as ConsensusError;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, One, Saturating};

use std::{marker::PhantomData, sync::Arc};

use super::oracle::RecoveryGate;

/// Wraps an inner [`BlockImport`], deferring (with a transient error) the import of blocks that
/// would execute against the ledger arena until the warp-recovered arena is verified (see module
/// docs).
pub struct GatedBlockImport<B, Inner, BE> {
	inner: Inner,
	gate: Arc<RecoveryGate>,
	backend: Arc<BE>,
	_phantom: PhantomData<B>,
}

impl<B, Inner: Clone, BE> Clone for GatedBlockImport<B, Inner, BE> {
	fn clone(&self) -> Self {
		Self {
			inner: self.inner.clone(),
			gate: self.gate.clone(),
			backend: self.backend.clone(),
			_phantom: PhantomData,
		}
	}
}

impl<B, Inner, BE> GatedBlockImport<B, Inner, BE> {
	pub fn new(inner: Inner, gate: Arc<RecoveryGate>, backend: Arc<BE>) -> Self {
		Self { inner, gate, backend, _phantom: PhantomData }
	}
}

impl<B, Inner, BE> GatedBlockImport<B, Inner, BE>
where
	B: BlockT,
	BE: Backend<B>,
{
	/// Whether importing `block` would execute the runtime (and therefore touch the ledger arena).
	/// See module docs for the per-`StateAction` reasoning.
	fn would_execute(&self, block: &BlockImportParams<B>) -> bool {
		match block.state_action {
			StateAction::ApplyChanges(_) | StateAction::Skip => false,
			StateAction::Execute => true,
			StateAction::ExecuteIfPossible => {
				let parent_hash = *block.header.parent_hash();
				let parent_number = (*block.header.number()).saturating_sub(One::one());
				self.backend.have_state_at(parent_hash, parent_number)
			},
		}
	}
}

#[async_trait::async_trait]
impl<B, Inner, BE> BlockImport<B> for GatedBlockImport<B, Inner, BE>
where
	B: BlockT,
	BE: Backend<B> + 'static,
	Inner: BlockImport<B, Error = ConsensusError> + Send + Sync,
{
	type Error = ConsensusError;

	async fn check_block(&self, block: BlockCheckParams<B>) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).await
	}

	async fn import_block(&self, block: BlockImportParams<B>) -> Result<ImportResult, Self::Error> {
		if self.gate.ledger_recovery_in_progress() && self.would_execute(&block) {
			// Hold the import (and with it the import-queue worker) until the arena is recovered —
			// safe because nothing recovery depends on goes through the import queue once a
			// `would_execute` block exists; see module docs ("Why defer = await").
			log::debug!(
				target: super::LOG_TARGET,
				"Holding import of #{} ({:?}) until ledger arena recovery completes",
				block.header.number(),
				block.post_hash(),
			);
			self.gate.wait_until_released().await;
		}
		self.inner.import_block(block).await
	}
}
