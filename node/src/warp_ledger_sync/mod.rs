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

use parity_scale_codec::Decode;
use sc_client_api::{Backend, StorageKey, StorageProvider};
use sp_runtime::traits::Block as BlockT;

/// Log target shared by the warp ledger-sync server and client.
pub(crate) const LOG_TARGET: &str = "midnight-ledger-sync";

/// Raw storage key for `pallet_midnight::StateKey`: `twox_128("Midnight") ++ twox_128("StateKey")`.
/// No runtime API exposes `StateKey` (it has only a `#[pallet::getter]`), so both the server and
/// client read it by raw key.
pub(crate) fn state_key_storage_key() -> StorageKey {
	let mut key = Vec::with_capacity(32);
	key.extend_from_slice(&sp_crypto_hashing::twox_128(b"Midnight"));
	key.extend_from_slice(&sp_crypto_hashing::twox_128(b"StateKey"));
	StorageKey(key)
}

/// Read and decode the on-chain `StateKey` at `hash` — the tagged `TypedArenaKey<Ledger>` bytes the
/// ledger arena is keyed by. The storage value is `SCALE(Vec<u8>)`; the inner bytes are the key.
/// Returns `Ok(None)` if the pallet has no `StateKey` at that block.
pub(crate) fn read_state_key<B, Client, BE>(
	client: &Client,
	hash: B::Hash,
) -> Result<Option<Vec<u8>>, sp_blockchain::Error>
where
	B: BlockT,
	BE: Backend<B>,
	Client: StorageProvider<B, BE>,
{
	match client.storage(hash, &state_key_storage_key())? {
		Some(raw) => {
			let inner = Vec::<u8>::decode(&mut &raw.0[..]).map_err(|e| {
				sp_blockchain::Error::Backend(format!("failed to decode pallet StateKey: {e}"))
			})?;
			Ok(Some(inner))
		},
		None => Ok(None),
	}
}
