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
//! [`GatedBlockImport`] wraps the import queue's block import and **holds** `import_block` while
//! [`RecoveryGate::ledger_recovery_in_progress`] is true, so no block executes against the arena
//! until recovery is verified. On a full sync the gate is never armed, so this is a pure
//! passthrough. Recovery does not depend on block import (it fetches over a side protocol from
//! already-connected peers), so holding the import worker cannot deadlock recovery.

use std::{sync::Arc, time::Duration};

use sc_consensus::{BlockCheckParams, BlockImport, BlockImportParams, ImportResult};
use sp_runtime::traits::Block as BlockT;

use super::{LOG_TARGET, oracle::RecoveryGate};

/// How often to re-check the gate while holding a block import.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Wraps an inner [`BlockImport`], deferring `import_block` until the warp-recovered ledger arena is
/// verified (see module docs).
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
	Inner: BlockImport<B> + Send + Sync,
{
	type Error = Inner::Error;

	async fn check_block(&self, block: BlockCheckParams<B>) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).await
	}

	async fn import_block(&self, block: BlockImportParams<B>) -> Result<ImportResult, Self::Error> {
		// `with_state()` is true only for the state-sync target block, whose state is *imported*
		// (`StateAction::ApplyChanges(StorageChanges::Import)`) — no runtime execution, so no arena
		// access. That import MUST be allowed even while recovery is pending: state sync has to
		// complete before the monitor can recover the arena (gating it would deadlock —
		// state-sync waits on import, import waits on `ledger_verified`, which waits on recovery,
		// which waits on state-sync). Only blocks that *execute* against the arena (post-warp
		// blocks N+1…, `with_state() == false`) are held until recovery is verified.
		if !block.with_state() && self.gate.ledger_recovery_in_progress() {
			log::debug!(
				target: LOG_TARGET,
				"Holding block import until the warp-recovered ledger arena is verified"
			);
			while self.gate.ledger_recovery_in_progress() {
				tokio::time::sleep(POLL_INTERVAL).await;
			}
			log::debug!(target: LOG_TARGET, "Ledger arena verified; resuming block import");
		}
		self.inner.import_block(block).await
	}
}
