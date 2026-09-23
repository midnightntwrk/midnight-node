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

//! RPC endpoint for ledger-state collection sizes — principally the unshielded
//! UTXO set size, which nothing else on the chain exposes.
//!
//! The UTXO set lives in the midnight-ledger arena, a content-addressed blob
//! outside the Substrate trie, so no `state_getStorage` query can reach it. This
//! method resolves it the same way the warp ledger-sync server does: read the raw
//! `pallet_midnight::StateKey` from the trie at the requested block, then ask the
//! ledger crate to resolve that key in the node's already-open arena. That keeps
//! it **entirely node-side** — no runtime API, so shipping it is a binary swap
//! rather than a runtime upgrade.
//!
//! Every number served is an O(1) annotation read, not a traversal (see
//! `midnight_node_ledger::ledger_stats`). The per-block memo below means repeated
//! polling of the same block costs one hash comparison and touches the arena — and
//! therefore the arena's process-global locks, which block execution also takes —
//! exactly once.

use jsonrpsee::{
	core::RpcResult,
	proc_macros::rpc,
	types::error::{ErrorObject, ErrorObjectOwned, INTERNAL_ERROR_CODE, INVALID_PARAMS_CODE},
};
use midnight_node_ledger::types::LedgerStats as InnerLedgerStats;
use sc_client_api::{Backend, StorageProvider};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use std::{marker::PhantomData, sync::Arc, sync::Mutex};

use crate::warp_ledger_sync::read_state_key;

/// Collection sizes held at the ledger-state root.
///
/// The `u64` counts serialize as JSON numbers: the UTXO set would have to reach
/// 9e15 entries to strain a double, which it cannot.
///
/// `unshieldedUtxoStars` is a `u128` and is serialized as a decimal string.
/// JSON itself imposes no bound on integer literals, so a number here would be
/// perfectly legal — but `JSON.parse` maps every number onto a double, and NIGHT
/// in Stars genuinely exceeds `Number.MAX_SAFE_INTEGER` (the 24e9 NIGHT max
/// supply is 2.4e16 Stars, against a 9.007e15 ceiling). A client would have to
/// opt into a BigInt-aware parser to read it losslessly, and one that did not
/// would be silently wrong rather than loudly broken. A string makes the
/// precision requirement explicit; `pattern` below keeps it machine-checkable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LedgerStats {
	/// Live unshielded UTXOs — the UTXO set size.
	pub unshielded_utxo_count: u64,
	/// NIGHT held across those UTXOs, in atomic Stars (1 NIGHT = 1e6 Stars), as a
	/// decimal string.
	#[schemars(regex(pattern = r"^[0-9]+$"))]
	pub unshielded_utxo_stars: String,
	/// Zswap note commitments ever created.
	pub zswap_commitment_count: u64,
	/// Zswap nullifiers, i.e. shielded notes ever spent.
	pub zswap_nullifier_count: u64,
	/// Live shielded notes: commitments minus nullifiers.
	pub zswap_live_note_count: u64,
	/// DUST commitments ever created.
	pub dust_commitment_count: u64,
	/// DUST nullifiers, i.e. DUST notes ever spent.
	pub dust_nullifier_count: u64,
	/// Live DUST notes: commitments minus nullifiers.
	pub dust_live_note_count: u64,
	/// Contracts currently deployed.
	pub contract_count: u64,
}

impl From<InnerLedgerStats> for LedgerStats {
	fn from(s: InnerLedgerStats) -> Self {
		LedgerStats {
			unshielded_utxo_count: s.unshielded_utxo_count,
			unshielded_utxo_stars: s.unshielded_utxo_stars.to_string(),
			zswap_commitment_count: s.zswap_commitment_count,
			zswap_nullifier_count: s.zswap_nullifier_count,
			zswap_live_note_count: s.zswap_commitment_count.saturating_sub(s.zswap_nullifier_count),
			dust_commitment_count: s.dust_commitment_count,
			dust_nullifier_count: s.dust_nullifier_count,
			dust_live_note_count: s.dust_commitment_count.saturating_sub(s.dust_nullifier_count),
			contract_count: s.contract_count,
		}
	}
}

#[derive(Debug)]
pub enum LedgerStatsError {
	/// The requested block hash is not known to this node.
	UnknownBlock,
	/// Reading `pallet_midnight::StateKey` from the trie failed. On a pruned node
	/// this is the expected outcome for a block outside the retained state window.
	StateKeyUnavailable(String),
	/// The pallet holds no `StateKey` at that block.
	NoStateKey,
	/// The arena could not resolve the state the `StateKey` points at.
	LedgerUnavailable(String),
}

impl std::fmt::Display for LedgerStatsError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			LedgerStatsError::UnknownBlock => write!(f, "requested block is unknown to this node"),
			LedgerStatsError::StateKeyUnavailable(reason) => write!(
				f,
				"unable to read ledger StateKey at the requested block \
				 (a pruned node has no state for older blocks): {reason}"
			),
			LedgerStatsError::NoStateKey => {
				write!(f, "pallet StateKey not present at the requested block")
			},
			LedgerStatsError::LedgerUnavailable(reason) => {
				write!(f, "unable to read ledger state: {reason}")
			},
		}
	}
}

impl std::error::Error for LedgerStatsError {}

impl From<LedgerStatsError> for ErrorObjectOwned {
	fn from(value: LedgerStatsError) -> Self {
		let code = match value {
			LedgerStatsError::UnknownBlock | LedgerStatsError::NoStateKey => INVALID_PARAMS_CODE,
			_ => INTERNAL_ERROR_CODE,
		};
		ErrorObject::owned(code, value.to_string(), None::<()>)
	}
}

#[rpc(client, server)]
pub trait LedgerStatsApi<BlockHash> {
	/// Returns the collection sizes held at the ledger-state root, most notably
	/// the unshielded UTXO set size.
	///
	/// Queries the best block unless `at` names one. Every value is read from a
	/// trie-root annotation, so the cost is independent of the size of the state.
	/// Note that a node running with default state pruning only retains trie state
	/// for recent blocks, so an older `at` will fail on such a node even though the
	/// arena itself still holds the data.
	#[method(name = "midnight_ledgerStats")]
	fn ledger_stats(&self, at: Option<BlockHash>) -> RpcResult<LedgerStats>;
}

pub struct LedgerStatsRpc<C, Block: BlockT, BE> {
	client: Arc<C>,
	/// Whether the ledger arena uses the unified ParityDb layout — selects the DB
	/// instantiation, as in the warp ledger-sync server.
	unified: bool,
	/// Memo of the last answer, keyed by block hash. Repeated polling of the tip
	/// (the overwhelmingly common case) then never touches the arena.
	cache: Mutex<Option<(Block::Hash, LedgerStats)>>,
	_phantom: PhantomData<(Block, BE)>,
}

impl<C, Block: BlockT, BE> LedgerStatsRpc<C, Block, BE> {
	pub fn new(client: Arc<C>, unified: bool) -> Self {
		Self { client, unified, cache: Mutex::new(None), _phantom: PhantomData }
	}
}

impl<C, Block, BE> LedgerStatsRpc<C, Block, BE>
where
	Block: BlockT,
	BE: Backend<Block> + Send + Sync + 'static,
	C: HeaderBackend<Block> + StorageProvider<Block, BE> + Send + Sync + 'static,
{
	/// Resolve the stats for `at` from the arena. Split out so the memo lock can be
	/// held across the whole fill without the locking dance obscuring the read.
	fn read_stats(&self, at: Block::Hash) -> Result<LedgerStats, LedgerStatsError> {
		if self
			.client
			.header(at)
			.map_err(|e| LedgerStatsError::StateKeyUnavailable(e.to_string()))?
			.is_none()
		{
			return Err(LedgerStatsError::UnknownBlock);
		}

		let state_key = read_state_key::<Block, C, BE>(&self.client, at)
			.map_err(|e| LedgerStatsError::StateKeyUnavailable(e.to_string()))?
			.ok_or(LedgerStatsError::NoStateKey)?;

		Ok(midnight_node_ledger::ledger_stats(self.unified, &state_key)
			.map_err(LedgerStatsError::LedgerUnavailable)?
			.into())
	}
}

impl<C, Block, BE> LedgerStatsApiServer<Block::Hash> for LedgerStatsRpc<C, Block, BE>
where
	Block: BlockT,
	BE: Backend<Block> + Send + Sync + 'static,
	C: HeaderBackend<Block> + StorageProvider<Block, BE> + Send + Sync + 'static,
{
	fn ledger_stats(&self, at: Option<Block::Hash>) -> RpcResult<LedgerStats> {
		let at = at.unwrap_or_else(|| self.client.info().best_hash);

		// The memo lock is held across the *fill*, not just the lookup. Releasing it
		// between observing a miss and storing the result would let every concurrent
		// caller see the same miss and pile onto the arena's process-global locks
		// together — precisely the contention this memo exists to avoid, and worst
		// exactly when it matters, as clients poll a newly produced block. Serialising
		// on this mutex instead means one caller reads the arena and the rest wake to
		// a hit. The read is O(1), so the wait it imposes is bounded.
		let mut cache = match self.cache.lock() {
			Ok(guard) => Some(guard),
			// A panic during an earlier fill poisons the mutex. Losing the memo is
			// survivable; refusing every later call is not, so fall through to an
			// uncached read rather than propagating the poison.
			Err(_) => None,
		};

		if let Some(guard) = cache.as_ref()
			&& let Some((hash, stats)) = guard.as_ref()
			&& *hash == at
		{
			return Ok(stats.clone());
		}

		let stats = self.read_stats(at)?;

		if let Some(guard) = cache.as_mut() {
			**guard = Some((at, stats.clone()));
		}

		Ok(stats)
	}
}
