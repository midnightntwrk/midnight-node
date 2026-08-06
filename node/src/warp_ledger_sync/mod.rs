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
//!   (`alloc`/`persist_tagged(target number)`/`flush`, no restart).
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

use frame_support::traits::StorageVersion;
use midnight_node_ledger::types::LedgerStateKey;
use midnight_node_runtime::Runtime;
use parity_scale_codec::Decode;
use sc_client_api::{Backend, StorageKey, StorageProvider};
use sp_runtime::traits::Block as BlockT;

/// Log target shared by the warp ledger-sync server and client.
pub(crate) const LOG_TARGET: &str = "midnight-ledger-sync";

/// Read and decode the on-chain `pallet_midnight::StateKey` at `hash` — the tagged
/// `TypedArenaKey<Ledger>` bytes the ledger arena is keyed by. No runtime API exposes `StateKey` (it
/// has only a `#[pallet::getter]`), so the server and client read it by storage key. Returns
/// `Ok(None)` if the pallet has no `StateKey` at that block.
///
/// Which layout the value is in is decided by pallet-midnight's on-chain storage version — the same
/// authority `Pallet::state_key` uses: pre-`STATE_KEY_ENUM_VERSION` chains stored raw `Vec<u8>`,
/// later ones store `LedgerStateKey`. Sniffing the bytes instead would misdecode a legacy key whose
/// length is a multiple of 256 as `LedgerStateKey::Transient` (its `Compact` length prefix then
/// reads as variant 1 followed by a short, valid-looking inner `Vec`).
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

	let version_key =
		StorageKey(StorageVersion::storage_key::<pallet_midnight::Pallet<Runtime>>().to_vec());
	let version = client
		.storage(hash, &version_key)?
		.and_then(|v| StorageVersion::decode(&mut &v.0[..]).ok())
		// An absent key is version 0, as `on_chain_storage_version` reads it.
		.unwrap_or_default();

	let inner = if version < pallet_midnight::STATE_KEY_ENUM_VERSION {
		Vec::<u8>::decode(&mut &raw.0[..])
	} else {
		LedgerStateKey::decode(&mut &raw.0[..]).map(LedgerStateKey::into_bytes)
	}
	.map_err(|e| {
		sp_blockchain::Error::Backend(format!(
			"failed to decode pallet StateKey ({version:?}): {e}"
		))
	})?;

	Ok(Some(inner))
}
