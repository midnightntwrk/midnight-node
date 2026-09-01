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

//! Trustless warp-sync extension for Midnight.
//!
//! Standard Substrate warp-sync recovers headers, GRANDPA finality, and the runtime state trie,
//! but **not** the Midnight ledger arena (the content-addressed blob behind
//! `pallet_midnight::StateKey`, which lives outside the trie). A warp-synced node therefore holds a
//! valid `StateKey` pointing into an empty arena and fails on the next block. This module adds a
//! side request/response protocol that recovers the arena after warp+state-sync completes, with
//! full cryptographic verification against the `StateKey` the trie already recovered.
//!
//! Module map:
//! - [`protocol`] — wire message types, codec, protocol naming, range serving + reassembly.
//! - [`server`] — serves the `Ledger`-rooted arena blob at a finalized target block as byte ranges.
//! - [`client`] — fetches the blob from peers and hands it to the ledger crate for verification +
//!   import. Verification against the on-chain `StateKey` and persistence live in
//!   `midnight_node_ledger::import_verified_ledger_snapshot`, which reuses the arena's **native**
//!   multi-pass deserializer (`Arena::deserialize_sp`) for untrusted input rather than a bespoke
//!   re-hash, then asserts the recomputed root equals `StateKey` before persisting in-process
//!   (`alloc`/`persist`/`flush`, no restart).
//! - [`monitor`] — detects warp completion, captures the target block, drives [`client`], releases
//!   the gate. [`oracle`] keeps AURA from authoring until recovery is verified.

pub mod block_import;
pub mod client;
pub mod monitor;
pub mod oracle;
pub mod protocol;
pub mod server;

#[cfg(test)]
mod integration_tests;

use midnight_node_runtime::Runtime;
use parity_scale_codec::Decode;
use sc_client_api::{Backend, StorageKey, StorageProvider};
use sp_runtime::traits::Block as BlockT;

/// Log target shared by the warp ledger-sync server and client.
pub(crate) const LOG_TARGET: &str = "midnight-ledger-sync";

/// Read and decode the on-chain `pallet_midnight::StateKey` at `hash` — the tagged
/// `TypedArenaKey<Ledger>` bytes the ledger arena is keyed by. No runtime API exposes `StateKey` (it
/// has only a `#[pallet::getter]`), so the server and client read it by storage key. The stored
/// value is `SCALE(Vec<u8>)`; the inner bytes are the key. Returns `Ok(None)` if the pallet has no
/// `StateKey` at that block.
pub(crate) fn read_state_key<B, Client, BE>(
	client: &Client,
	hash: B::Hash,
) -> Result<Option<Vec<u8>>, sp_blockchain::Error>
where
	B: BlockT,
	BE: Backend<B>,
	Client: StorageProvider<B, BE>,
{
	let key = StorageKey(pallet_midnight::StateKey::<Runtime>::hashed_key().to_vec());
	let Some(raw) = client.storage(hash, &key)? else { return Ok(None) };

	let inner = Vec::<u8>::decode(&mut &raw.0[..]).map_err(|e| {
		sp_blockchain::Error::Backend(format!("failed to decode pallet StateKey: {e}"))
	})?;

	Ok(Some(inner))
}

/// Raw storage key for `pallet_cnight_observation::PreForkStateKey`:
/// `twox_128("CNightObservation") ++ twox_128("PreForkStateKey")`.
///
/// Read by raw key for the same reason [`state_key_storage_key`] is: no runtime API exposes it.
// ponytail: transient ledger 8 -> 9 hardfork machinery; delete together with
// `pallet_cnight_observation::migrations::v2` once that fork is behind us.
pub(crate) fn pre_fork_state_key_storage_key() -> StorageKey {
	let mut key = Vec::with_capacity(32);
	key.extend_from_slice(&sp_crypto_hashing::twox_128(b"CNightObservation"));
	key.extend_from_slice(&sp_crypto_hashing::twox_128(b"PreForkStateKey"));
	StorageKey(key)
}

/// Whether `pallet_cnight_observation::PreForkStateKey` is set at `hash`, i.e. whether the cNIGHT
/// dust generation replay (`pallet_cnight_observation::migrations::v2`) is mid-flight at that
/// block. Presence is the whole answer — the value is a ledger-8 arena root this node cannot
/// resolve anyway — so it is never decoded.
///
/// The key is written in the hardfork upgrade block and killed when the replay completes *or*
/// cancels, so it marks exactly the blocks whose execution needs the pre-fork arena.
pub(crate) fn has_pre_fork_state_key<B, Client, BE>(
	client: &Client,
	hash: B::Hash,
) -> Result<bool, sp_blockchain::Error>
where
	B: BlockT,
	BE: Backend<B>,
	Client: StorageProvider<B, BE>,
{
	Ok(client.storage(hash, &pre_fork_state_key_storage_key())?.is_some())
}

#[cfg(test)]
mod storage_key_tests {
	use midnight_node_runtime::Runtime;

	/// Both raw keys above are hand-built from pallet/item name strings, so a pallet rename is a
	/// silent break: the server would stop finding `StateKey`, and — worse, because it fails
	/// *open* — the dust-replay guard would stop firing with no compile error. Pin them to the
	/// real runtime's derived keys.
	#[test]
	fn raw_storage_keys_match_the_runtime() {
		assert_eq!(
			super::state_key_storage_key().0,
			pallet_midnight::StateKey::<Runtime>::hashed_key().to_vec(),
			"pallet_midnight::StateKey moved; update state_key_storage_key()"
		);
		assert_eq!(
			super::pre_fork_state_key_storage_key().0,
			pallet_cnight_observation::PreForkStateKey::<Runtime>::hashed_key().to_vec(),
			"pallet_cnight_observation::PreForkStateKey moved; update pre_fork_state_key_storage_key()"
		);
	}
}
