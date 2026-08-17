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

//! Block-import wrapper that batch-verifies the ZK proofs of a received block up front.
//!
//! Wraps the inner (grandpa) block import used by the **import queue** — the path that executes
//! received blocks. Before delegating, it batch-verifies every Midnight transaction's proofs in a
//! single aggregate crypto call against the ledger state at the block's parent, warming the
//! process-global proof cache so the subsequent `execute_block` → `pre_dispatch` →
//! `get_verified_transaction` skips the (now-deferred) inline crypto.
//!
//! **Fail-fast, no fallback:** if the aggregate verification fails, the block is rejected with a
//! `ConsensusError`. This is safe to enable: a block with any invalid proof is invalid, and
//! rejecting it early is equivalent to the inline path rejecting it during execution.
//!
//! Authored blocks (which arrive with precomputed storage changes, `StateAction::ApplyChanges`)
//! and bodiless blocks are skipped — authored blocks are covered by the mempool ingress path and
//! never re-execute here.

use crate::batch_verify::{BatchVerifier, BatchVerifyError};
use midnight_node_runtime::opaque::Block;
use parity_scale_codec::{Decode, Encode};
use sc_consensus::{BlockCheckParams, BlockImport, BlockImportParams, ImportResult, StateAction};
use sp_consensus::Error as ConsensusError;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};

const LOG_TARGET: &str = "midnight::batch_verify";

/// Seconds added to the parent block's timestamp when assembling the batch `BlockContext`, to
/// approximate the imported block's own time (~one AURA slot). Only affects the non-crypto
/// `well_formed` checks, which are re-run downstream — so an approximation is safe.
const BLOCK_IMPORT_TBLOCK_EXTRA_SECS: u64 = 6;

/// Extracts the serialized Midnight transactions (`send_mn_transaction` payloads) from a block
/// body, mirroring the decode/match in `filtering_pool`.
fn extract_midnight_txs(body: &[<Block as BlockT>::Extrinsic]) -> Vec<Vec<u8>> {
	let mut txs = Vec::new();
	for xt in body {
		let Ok(decoded) = midnight_node_runtime::UncheckedExtrinsic::decode(&mut &xt.encode()[..])
		else {
			continue;
		};
		if let midnight_node_runtime::RuntimeCall::Midnight(
			midnight_node_runtime::MidnightCall::send_mn_transaction { midnight_tx },
		) = decoded.function
		{
			txs.push(midnight_tx);
		}
	}
	txs
}

/// `BlockImport` wrapper performing up-front batch proof verification of received blocks.
#[derive(Clone)]
pub struct BatchVerifyBlockImport<Inner> {
	inner: Inner,
	verifier: BatchVerifier,
	enabled: bool,
}

impl<Inner> BatchVerifyBlockImport<Inner> {
	pub fn new(inner: Inner, verifier: BatchVerifier, enabled: bool) -> Self {
		Self { inner, verifier, enabled }
	}

	/// Batch-verifies the Midnight proofs of a received block up front, warming the proof cache.
	///
	/// Returns `Err(reason)` **only** when the block must be rejected — a genuine invalid proof
	/// surfaced by the fail-fast aggregate check. Every skip case (feature disabled, authored
	/// block, no body, pre-ledger-9, no Midnight txs) and every setup/availability failure returns
	/// `Ok(())` and leaves verification to the downstream inline path: a transient inability to
	/// batch-verify must never reject a possibly-valid block.
	fn maybe_batch_verify(&self, block: &BlockImportParams<Block>) -> Result<(), String> {
		if !self.enabled {
			return Ok(());
		}
		// Only received blocks that still need execution (no precomputed storage changes) are
		// candidates; authored blocks (`ApplyChanges`) are covered by the mempool path and must
		// not be re-verified here.
		if matches!(block.state_action, StateAction::ApplyChanges(_)) {
			return Ok(());
		}
		let Some(body) = block.body.as_ref() else {
			return Ok(());
		};
		let parent_hash = *block.header.parent_hash();
		// Only the active (ledger-9) version supports batch verification; older-version blocks
		// fall through to per-transaction inline verification during execution.
		if !self.verifier.is_ledger_9(parent_hash) {
			return Ok(());
		}
		let txs = extract_midnight_txs(body);
		if txs.is_empty() {
			return Ok(());
		}

		let tx_count = txs.len();
		match self.verifier.batch_verify(
			parent_hash,
			txs,
			/* isolate_on_failure */ false,
			BLOCK_IMPORT_TBLOCK_EXTRA_SECS,
		) {
			Ok(_) => {
				log::debug!(
					target: LOG_TARGET,
					"batch-verified {tx_count} transaction(s) for imported block {:?} \
					 (parent {parent_hash:?})",
					block.header.hash(),
				);
				Ok(())
			},
			// A genuine invalid proof: reject the block early (equivalent to the inline path
			// rejecting it during execution, only sooner).
			Err(BatchVerifyError::ProofInvalid) => {
				Err("aggregate proof verification found an invalid proof".to_string())
			},
			// Could not batch-verify (missing state, version mismatch, etc.): skip and let the
			// downstream inline path verify. Never reject on an availability failure.
			Err(BatchVerifyError::Unavailable(reason)) => {
				log::debug!(
					target: LOG_TARGET,
					"skipping batch verification for block {:?}, falling back to inline: {reason}",
					block.header.hash(),
				);
				Ok(())
			},
		}
	}
}

#[async_trait::async_trait]
impl<Inner> BlockImport<Block> for BatchVerifyBlockImport<Inner>
where
	Inner: BlockImport<Block, Error = ConsensusError> + Send + Sync,
{
	type Error = ConsensusError;

	async fn check_block(
		&self,
		block: BlockCheckParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).await
	}

	async fn import_block(
		&self,
		block: BlockImportParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		if let Err(reason) = self.maybe_batch_verify(&block) {
			log::warn!(target: LOG_TARGET, "rejecting block {:?}: {reason}", block.header.hash());
			return Err(ConsensusError::ClientImport(reason));
		}
		self.inner.import_block(block).await
	}
}
