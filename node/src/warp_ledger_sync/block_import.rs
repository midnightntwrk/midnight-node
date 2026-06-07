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
//! single-writer; see `warp-ledger-sync-m1.4a-spike.md`).
//!
//! [`GatedBlockImport`] wraps the import queue's block import and, while
//! [`RecoveryGate::ledger_recovery_in_progress`] is true, **rejects** the import of blocks that
//! would execute against the arena, with a transient [`ConsensusError::ClientImport`] error so the
//! sync engine re-requests them once recovery completes.
//!
//! Two things are critical and were learned the hard way:
//! 1. **Reject, don't block.** The import queue has a single worker; an `import_block` that *awaits*
//!    until recovery would occupy that worker and starve the state-sync target-block import below,
//!    deadlocking warp (state sync waits on the worker ← held by the gated block ← waits on
//!    recovery ← waits on state sync). Returning an error frees the worker immediately.
//! 2. **Never gate the state-sync target block.** `with_state()` is true only for that block (its
//!    state is *imported*, not executed — no arena access), and state sync must import it *before*
//!    the monitor can recover the arena. Gating it would deadlock for the same reason.
//!
//! On a full sync the gate is never armed, so this is a pure passthrough.

use sc_consensus::{BlockCheckParams, BlockImport, BlockImportParams, ImportResult};
use sp_consensus::Error as ConsensusError;
use sp_runtime::traits::Block as BlockT;

use std::sync::Arc;

use super::oracle::RecoveryGate;

/// Wraps an inner [`BlockImport`], rejecting `import_block` for arena-executing blocks until the
/// warp-recovered ledger arena is verified (see module docs).
#[derive(Clone)]
pub struct GatedBlockImport<Inner> {
	inner: Inner,
	gate: Arc<RecoveryGate>,
}

impl<Inner> GatedBlockImport<Inner> {
	pub fn new(inner: Inner, gate: Arc<RecoveryGate>) -> Self {
		Self { inner, gate }
	}
}

#[async_trait::async_trait]
impl<B, Inner> BlockImport<B> for GatedBlockImport<Inner>
where
	B: BlockT,
	Inner: BlockImport<B, Error = ConsensusError> + Send + Sync,
{
	type Error = ConsensusError;

	async fn check_block(&self, block: BlockCheckParams<B>) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).await
	}

	async fn import_block(&self, block: BlockImportParams<B>) -> Result<ImportResult, Self::Error> {
		// Defer only execution-bearing blocks (post-warp blocks). The state-sync target block carries
		// imported state (`with_state()` true, no runtime execution) and must always be let through —
		// recovery can't even start until state sync imports it.
		if !block.with_state() && self.gate.ledger_recovery_in_progress() {
			// Return MissingState (not an Err): the ledger arena this block needs to execute isn't
			// recovered yet. substrate treats MissingState as "obsolete, not bad" — it does NOT drop
			// the peer and does NOT restart sync, and the block is re-requested by normal sync once
			// recovery completes. Returning an Err instead maps to `BlockImportError::Other`, which
			// triggers `chain_sync.restart()` on every deferred block (a restart-storm that churns
			// peers); awaiting would instead block the single import-queue worker and deadlock the
			// state-sync target import. MissingState avoids both.
			return Ok(ImportResult::MissingState);
		}
		self.inner.import_block(block).await
	}
}
