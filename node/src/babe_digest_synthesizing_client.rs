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

//! Client wrapper for BABE that synthesizes a `SecondaryPlain` pre-digest on headers that
//! carry AURA but no BABE pre-runtime.
//!
//! `sc-consensus-babe` reads headers through the client it is given and `expect`s a BABE
//! pre-digest on them: `prune_finalized` reads the finalized header (once inside
//! `sc_consensus_babe::block_import()` construction and again on every epoch-change import),
//! and `import_block` reads the parent header for its slot-must-increase check. On a chain
//! migrated from AURA those can be plain AURA headers, and the `expect`s abort the node.
//! Serving BABE's header reads through this wrapper makes them panic-free in every finality
//! state, including a historical sync whose local finality lags in pre-migration history.
//!
//! Decision and payload are derived only from the header digests:
//! - if a BABE pre-runtime is already present → unchanged;
//! - else if an AURA pre-digest is present → append `SecondaryPlain` with the same `slot` and
//!   `authority_index = 0`.
//!
//! # Safety invariant (audited against `polkadot-stable2606`)
//!
//! Upstream consumes only `.slot()` from pre-digests of headers it reads through the client
//! (`prune_finalized` and `import_block`'s parent check). The slot is copied from the AURA
//! pre-digest, so it is the real slot on the shared 6s clock and the pruning / slot-increase
//! arithmetic stays coherent. `authority_index` is never read from client-served headers, so
//! it is fixed to `0` — it is decorative, not a claim about the block's author. The block
//! under import is always verified against its own network-provided header, never through
//! this wrapper, so real BABE blocks still need real digests. The
//! `upstream_find_pre_digest_accepts_wrapper_served_header` test pins this contract.
//!
//! # Scope
//!
//! Appending a digest changes the header's encoding, so `.hash()` of a served header no
//! longer matches the real block hash. Hand this wrapper ONLY to
//! `sc_consensus_babe::block_import` / `import_queue`; never to sync, RPC, GRANDPA, or chain
//! selection. Its trait surface is intentionally limited to what BABE needs — keep it that
//! way.

use parity_scale_codec::Encode;
use sc_client_api::{AuxStore, PreCommitActions, UsageProvider};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::{HeaderBackend, HeaderMetadata, Info};
use sp_consensus_aura::digests::CompatibleDigestItem as AuraCompatibleDigestItem;
use sp_consensus_babe::BABE_ENGINE_ID;
use sp_consensus_babe::digests::{PreDigest, SecondaryPlainPreDigest};
use sp_runtime::DigestItem;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, NumberFor};
use std::sync::Arc;

const LOG_TARGET: &str = "babe-digest-synthesizing-client";

/// Client wrapper that may attach a synthetic BABE `SecondaryPlain` digest when BABE reads headers.
pub struct BabeDigestSynthesizingClient<Client> {
	inner: Arc<Client>,
}

impl<Client> Clone for BabeDigestSynthesizingClient<Client> {
	fn clone(&self) -> Self {
		Self { inner: self.inner.clone() }
	}
}

impl<Client> BabeDigestSynthesizingClient<Client> {
	pub fn new(inner: Arc<Client>) -> Self {
		Self { inner }
	}

	pub fn inner(&self) -> &Arc<Client> {
		&self.inner
	}
}

/// If `header` has AURA and no BABE pre-runtime, append a mirroring BABE `SecondaryPlain`.
///
/// The synthesized digest carries the AURA slot and `authority_index = 0`; the index is never
/// consumed by upstream readers of client-served headers (see module docs).
pub fn synthesize_babe_digest_if_needed<Header>(header: &mut Header)
where
	Header: HeaderT,
{
	if header
		.digest()
		.logs()
		.iter()
		.any(|log| matches!(log.as_pre_runtime(), Some((id, _)) if id == BABE_ENGINE_ID))
	{
		return;
	}

	let Some(slot) = header
		.digest()
		.logs()
		.iter()
		.find_map(AuraCompatibleDigestItem::<()>::as_aura_pre_digest)
	else {
		return;
	};

	let pre_digest =
		PreDigest::SecondaryPlain(SecondaryPlainPreDigest { authority_index: 0, slot });
	header
		.digest_mut()
		.push(DigestItem::PreRuntime(BABE_ENGINE_ID, pre_digest.encode()));

	log::trace!(target: LOG_TARGET, "Synthesized BABE SecondaryPlain: slot={slot:?}");
}

impl<Block, Client> HeaderBackend<Block> for BabeDigestSynthesizingClient<Client>
where
	Block: BlockT,
	Client: HeaderBackend<Block>,
{
	fn header(&self, hash: Block::Hash) -> sp_blockchain::Result<Option<Block::Header>> {
		let mut header = self.inner.header(hash)?;
		if let Some(ref mut h) = header {
			synthesize_babe_digest_if_needed(h);
		}
		Ok(header)
	}

	fn info(&self) -> Info<Block> {
		self.inner.info()
	}

	fn status(&self, hash: Block::Hash) -> sp_blockchain::Result<sp_blockchain::BlockStatus> {
		self.inner.status(hash)
	}

	fn number(&self, hash: Block::Hash) -> sp_blockchain::Result<Option<NumberFor<Block>>> {
		self.inner.number(hash)
	}

	fn hash(&self, number: NumberFor<Block>) -> sp_blockchain::Result<Option<Block::Hash>> {
		self.inner.hash(number)
	}
}

impl<Block, Client> HeaderMetadata<Block> for BabeDigestSynthesizingClient<Client>
where
	Block: BlockT,
	Client: HeaderMetadata<Block, Error = sp_blockchain::Error>,
{
	type Error = sp_blockchain::Error;

	fn header_metadata(
		&self,
		hash: Block::Hash,
	) -> Result<sp_blockchain::CachedHeaderMetadata<Block>, Self::Error> {
		self.inner.header_metadata(hash)
	}

	fn insert_header_metadata(
		&self,
		hash: Block::Hash,
		header_metadata: sp_blockchain::CachedHeaderMetadata<Block>,
	) {
		self.inner.insert_header_metadata(hash, header_metadata)
	}

	fn remove_header_metadata(&self, hash: Block::Hash) {
		self.inner.remove_header_metadata(hash)
	}
}

impl<Client> AuxStore for BabeDigestSynthesizingClient<Client>
where
	Client: AuxStore,
{
	fn insert_aux<
		'a,
		'b: 'a,
		'c: 'a,
		I: IntoIterator<Item = &'a (&'c [u8], &'c [u8])>,
		D: IntoIterator<Item = &'a &'b [u8]>,
	>(
		&self,
		insert: I,
		delete: D,
	) -> sp_blockchain::Result<()> {
		self.inner.insert_aux(insert, delete)
	}

	fn get_aux(&self, key: &[u8]) -> sp_blockchain::Result<Option<Vec<u8>>> {
		self.inner.get_aux(key)
	}
}

impl<Block, Client> ProvideRuntimeApi<Block> for BabeDigestSynthesizingClient<Client>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>,
{
	type Api = Client::Api;

	fn runtime_api(&self) -> sp_api::ApiRef<'_, Self::Api> {
		self.inner.runtime_api()
	}
}

impl<Block, Client> UsageProvider<Block> for BabeDigestSynthesizingClient<Client>
where
	Block: BlockT,
	Client: UsageProvider<Block>,
{
	fn usage_info(&self) -> sc_client_api::ClientInfo<Block> {
		self.inner.usage_info()
	}
}

impl<Block, Client> PreCommitActions<Block> for BabeDigestSynthesizingClient<Client>
where
	Block: BlockT,
	Client: PreCommitActions<Block>,
{
	fn register_import_action(&self, action: sc_client_api::OnImportAction<Block>) {
		self.inner.register_import_action(action)
	}

	fn register_finality_action(&self, action: sc_client_api::OnFinalityAction<Block>) {
		self.inner.register_finality_action(action)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_consensus_aura::AURA_ENGINE_ID;
	use sp_consensus_babe::digests::CompatibleDigestItem as BabeCompatibleDigestItem;
	use sp_consensus_slots::Slot;
	use sp_runtime::generic::{Block as GenBlock, Header as GenHeader};
	use sp_runtime::traits::BlakeTwo256;
	use sp_runtime::{Digest, OpaqueExtrinsic};
	use std::collections::HashMap;

	type Block = GenBlock<GenHeader<u32, BlakeTwo256>, OpaqueExtrinsic>;
	type Header = <Block as BlockT>::Header;

	fn aura_digest(slot: u64) -> DigestItem {
		DigestItem::PreRuntime(AURA_ENGINE_ID, Slot::from(slot).encode())
	}

	fn header_with(logs: Vec<DigestItem>) -> Header {
		Header::new(1, Default::default(), Default::default(), Default::default(), Digest { logs })
	}

	#[test]
	fn synthesizes_secondary_plain_matching_aura() {
		let mut header = header_with(vec![aura_digest(7)]);
		synthesize_babe_digest_if_needed(&mut header);

		let babe = header.digest().logs().iter().find_map(|l| l.as_babe_pre_digest()).unwrap();
		match babe {
			PreDigest::SecondaryPlain(p) => {
				assert_eq!(p.slot, Slot::from(7u64));
				assert_eq!(p.authority_index, 0);
			},
			other => panic!("unexpected {other:?}"),
		}
	}

	#[test]
	fn leaves_existing_babe_digest_alone() {
		let slot = Slot::from(3u64);
		let existing =
			PreDigest::SecondaryPlain(SecondaryPlainPreDigest { authority_index: 9, slot });
		let mut header = header_with(vec![
			aura_digest(3),
			DigestItem::PreRuntime(BABE_ENGINE_ID, existing.encode()),
		]);
		synthesize_babe_digest_if_needed(&mut header);

		let babe_logs: Vec<_> = header
			.digest()
			.logs()
			.iter()
			.filter(|l| matches!(l.as_pre_runtime(), Some((id, _)) if id == BABE_ENGINE_ID))
			.collect();
		assert_eq!(babe_logs.len(), 1);
		let babe = babe_logs[0].as_babe_pre_digest().unwrap();
		assert!(matches!(
			babe,
			PreDigest::SecondaryPlain(SecondaryPlainPreDigest { authority_index: 9, .. })
		));
	}

	#[test]
	fn skips_when_no_aura_digest() {
		let mut header = header_with(vec![]);
		synthesize_babe_digest_if_needed(&mut header);
		assert!(header.digest().logs().is_empty());
	}

	/// Minimal [`HeaderBackend`] serving headers from a map; only `header()` is exercised.
	struct MapHeaderBackend {
		headers: HashMap<<Block as BlockT>::Hash, Header>,
	}

	impl HeaderBackend<Block> for MapHeaderBackend {
		fn header(&self, hash: <Block as BlockT>::Hash) -> sp_blockchain::Result<Option<Header>> {
			Ok(self.headers.get(&hash).cloned())
		}

		fn info(&self) -> Info<Block> {
			unimplemented!("not used by these tests")
		}

		fn status(
			&self,
			_hash: <Block as BlockT>::Hash,
		) -> sp_blockchain::Result<sp_blockchain::BlockStatus> {
			unimplemented!("not used by these tests")
		}

		fn number(
			&self,
			_hash: <Block as BlockT>::Hash,
		) -> sp_blockchain::Result<Option<NumberFor<Block>>> {
			unimplemented!("not used by these tests")
		}

		fn hash(
			&self,
			_number: NumberFor<Block>,
		) -> sp_blockchain::Result<Option<<Block as BlockT>::Hash>> {
			unimplemented!("not used by these tests")
		}
	}

	/// Pins the upstream contract this wrapper exists to satisfy: a pre-arm-style header
	/// (AURA pre-digest, no BABE) served through the wrapper must be accepted by
	/// `sc_consensus_babe::find_pre_digest` — the exact call `prune_finalized` and
	/// `import_block`'s parent check make on client-read headers — and yield the AURA slot.
	/// Fails loudly on an SDK bump if `find_pre_digest` semantics change.
	#[test]
	fn upstream_find_pre_digest_accepts_wrapper_served_header() {
		// Non-zero block number: upstream grandfathers number-0 headers with a dummy digest,
		// which would pass this test without exercising the synthesis path.
		let header = Header::new(
			5,
			Default::default(),
			Default::default(),
			Default::default(),
			Digest { logs: vec![aura_digest(42)] },
		);
		let hash = header.hash();
		let backend = MapHeaderBackend { headers: HashMap::from([(hash, header)]) };
		let wrapped = BabeDigestSynthesizingClient::new(Arc::new(backend));

		let served = wrapped.header(hash).unwrap().expect("header is in the backend");
		let pre_digest = sc_consensus_babe::find_pre_digest::<Block>(&served)
			.expect("upstream parser must accept the synthesized digest");

		assert_eq!(pre_digest.slot(), Slot::from(42u64), "slot must be the AURA slot");
		assert!(matches!(pre_digest, PreDigest::SecondaryPlain(_)));
	}

	/// The wrapper must not disturb upstream parsing of a header that already carries a real
	/// BABE pre-digest (armed-window and post-flip headers).
	#[test]
	fn upstream_find_pre_digest_reads_the_real_digest_when_present() {
		let real = PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
			authority_index: 9,
			slot: Slot::from(3u64),
		});
		let header = Header::new(
			5,
			Default::default(),
			Default::default(),
			Default::default(),
			Digest {
				logs: vec![aura_digest(3), DigestItem::PreRuntime(BABE_ENGINE_ID, real.encode())],
			},
		);
		let hash = header.hash();
		let backend = MapHeaderBackend { headers: HashMap::from([(hash, header)]) };
		let wrapped = BabeDigestSynthesizingClient::new(Arc::new(backend));

		let served = wrapped.header(hash).unwrap().expect("header is in the backend");
		let pre_digest = sc_consensus_babe::find_pre_digest::<Block>(&served)
			.expect("a single real BABE pre-digest must parse");

		assert!(matches!(
			pre_digest,
			PreDigest::SecondaryPlain(SecondaryPlainPreDigest { authority_index: 9, .. })
		));
	}
}
