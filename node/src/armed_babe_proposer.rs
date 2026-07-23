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

//! Proposer wrapper that attaches a BABE `SecondaryPlain` pre-runtime digest to authored blocks
//! while the consensus engine has the flip to BABE armed.
//!
//! During the AURA→BABE migration the chain keeps producing blocks with AURA, but once governance
//! arms the flip ([`ArmedBabe`]/[`ScheduledFlip`]) every produced block must already carry a BABE
//! pre-runtime digest so the network is running BABE digests before the flip commits. The digest is
//! a [`SecondaryPlain`] entry — `{ authority_index, slot }`, no VRF/signature — where `slot` is the
//! block's AURA slot and `authority_index` is `slot % n_authorities`, i.e. the same index the AURA
//! logic uses to pick the slot's author.
//!
//! The gate ([`ConsensusEngineApi::should_emit_babe_preruntime_digest`]) is read from the runtime at
//! the parent block: it is false in `Aura` (a BABE digest would be rejected on import) and `Babe`
//! (BABE authors its own digests), true only in the armed window.
//!
//! The wrapper composes over the inner proposer (e.g. `PartnerChainsProposerFactory`) and only
//! appends to the digest logs, so the AURA pre-digest stays first and the import path — the AURA
//! verifier ignores non-AURA digests, and the consensus-engine pallet accepts BABE digests once
//! armed — needs no changes.
//!
//! [`ArmedBabe`]: pallet_consensus_engine
//! [`ScheduledFlip`]: pallet_consensus_engine
//! [`SecondaryPlain`]: sp_consensus_babe::digests::PreDigest::SecondaryPlain

use futures::FutureExt;
use midnight_primitives_consensus_engine::ConsensusEngineApi;
use parity_scale_codec::Encode;
use sp_api::ProvideRuntimeApi;
use sp_consensus::{Environment, ProposeArgs, Proposer};
use sp_consensus_aura::AuraApi;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_babe::{
	BABE_ENGINE_ID,
	digests::{PreDigest, SecondaryPlainPreDigest},
};
use sp_consensus_slots::Slot;
use sp_runtime::traits::{Block as BlockT, Header as _};
use sp_runtime::{Digest, DigestItem};
use std::future::Future;
use std::marker::PhantomData;
use std::sync::Arc;

const LOG_TARGET: &str = "armed-babe-predigest";

/// Proposer factory wrapper. See the [module docs](self).
pub struct ArmedBabeProposerFactory<B, E, C> {
	inner: E,
	client: Arc<C>,
	_phantom: PhantomData<B>,
}

impl<B, E, C> ArmedBabeProposerFactory<B, E, C> {
	/// Wrap `inner`, reading the arming state and AURA authorities from `client`.
	pub fn new(inner: E, client: Arc<C>) -> Self {
		Self { inner, client, _phantom: PhantomData }
	}
}

impl<B, E, C> Environment<B> for ArmedBabeProposerFactory<B, E, C>
where
	B: BlockT,
	E: Environment<B>,
	C: ProvideRuntimeApi<B> + Send + Sync + 'static,
	C::Api: ConsensusEngineApi<B> + AuraApi<B, AuraId>,
{
	type Proposer = ArmedBabeProposer<B, E::Proposer>;
	type CreateProposer =
		Box<dyn Future<Output = Result<Self::Proposer, Self::Error>> + Send + Unpin + 'static>;
	type Error = <E as Environment<B>>::Error;

	fn init(&mut self, parent_header: &<B as BlockT>::Header) -> Self::CreateProposer {
		// Resolve the arming state (and authority count) at the parent up front; the proposer only
		// needs the AURA slot, which it reads from inherent data at `propose` time.
		let emit_with_authorities = babe_emit_context(&self.client, parent_header);
		Box::new(self.inner.init(parent_header).map(move |res| {
			res.map(|proposer| ArmedBabeProposer::new(proposer, emit_with_authorities))
		}))
	}
}

/// `Some(n_authorities)` when the parent's runtime says a BABE pre-digest should be emitted and the
/// AURA authority set is non-empty; `None` otherwise (not armed, empty set, or query failure).
fn babe_emit_context<B, C>(client: &Arc<C>, parent_header: &<B as BlockT>::Header) -> Option<u32>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: ConsensusEngineApi<B> + AuraApi<B, AuraId>,
{
	let parent_hash = parent_header.hash();
	let api = client.runtime_api();

	// A runtime older than `ConsensusEngineApi` v2 lacks this method; any failure means "not armed".
	if !api.should_emit_babe_preruntime_digest(parent_hash).unwrap_or(false) {
		return None;
	}

	match api.authorities(parent_hash) {
		Ok(authorities) if !authorities.is_empty() => Some(authorities.len() as u32),
		Ok(_) => {
			log::warn!(
				target: LOG_TARGET,
				"Armed for BABE pre-digest but the AURA authority set at {parent_hash:?} is empty; \
				 not attaching a pre-digest.",
			);
			None
		},
		Err(err) => {
			log::warn!(
				target: LOG_TARGET,
				"Armed for BABE pre-digest but failed to read AURA authorities at {parent_hash:?}: \
				 {err}; not attaching a pre-digest.",
			);
			None
		},
	}
}

/// Proposer wrapper. See the [module docs](self).
pub struct ArmedBabeProposer<B: BlockT, P> {
	inner: P,
	/// `Some(n_authorities)` if a BABE pre-digest should be attached, else `None`.
	emit_with_authorities: Option<u32>,
	_phantom: PhantomData<B>,
}

impl<B: BlockT, P> ArmedBabeProposer<B, P> {
	fn new(inner: P, emit_with_authorities: Option<u32>) -> Self {
		Self { inner, emit_with_authorities, _phantom: PhantomData }
	}
}

impl<B, P> Proposer<B> for ArmedBabeProposer<B, P>
where
	B: BlockT,
	P: Proposer<B>,
{
	type Error = <P as Proposer<B>>::Error;
	type Proposal = <P as Proposer<B>>::Proposal;

	fn propose(self, mut args: ProposeArgs<B>) -> Self::Proposal {
		if let Some(n_authorities) = self.emit_with_authorities {
			if let Some(digest) = babe_secondary_plain_digest(&args.inherent_data, n_authorities) {
				let mut logs = Vec::from(args.inherent_digests.logs());
				logs.push(digest);
				args.inherent_digests = Digest { logs };
			}
		}
		self.inner.propose(args)
	}
}

/// Build the BABE `SecondaryPlain` pre-digest for this block, or `None` if the AURA slot is missing
/// from the inherent data (in which case the block is authored without it rather than failing).
fn babe_secondary_plain_digest(
	inherent_data: &sp_inherents::InherentData,
	n_authorities: u32,
) -> Option<DigestItem> {
	let slot = aura_slot_from_inherents(inherent_data)?;
	// `n_authorities` is guaranteed non-zero by `babe_emit_context`.
	let authority_index = (u64::from(slot) % u64::from(n_authorities)) as u32;
	let pre_digest = PreDigest::SecondaryPlain(SecondaryPlainPreDigest { authority_index, slot });
	log::debug!(
		target: LOG_TARGET,
		"Attaching BABE SecondaryPlain pre-digest (slot {slot:?}, authority_index {authority_index})",
	);
	Some(DigestItem::PreRuntime(BABE_ENGINE_ID, pre_digest.encode()))
}

/// Read the AURA slot the block is being authored for from its inherent data.
fn aura_slot_from_inherents(inherent_data: &sp_inherents::InherentData) -> Option<Slot> {
	inherent_data
		.get_data::<sp_consensus_aura::inherents::InherentType>(
			&sp_consensus_aura::inherents::INHERENT_IDENTIFIER,
		)
		.ok()
		.flatten()
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_consensus_babe::digests::CompatibleDigestItem;

	fn inherents_with_slot(slot: u64) -> sp_inherents::InherentData {
		let mut data = sp_inherents::InherentData::new();
		data.put_data(
			sp_consensus_aura::inherents::INHERENT_IDENTIFIER,
			&Slot::from(slot),
		)
		.unwrap();
		data
	}

	fn babe_slot_and_index(item: &DigestItem) -> (Slot, u32) {
		match <DigestItem as CompatibleDigestItem>::as_babe_pre_digest(item).unwrap() {
			PreDigest::SecondaryPlain(d) => (d.slot, d.authority_index),
			other => panic!("expected SecondaryPlain, got {other:?}"),
		}
	}

	#[test]
	fn emits_secondary_plain_with_slot_and_derived_authority_index() {
		// slot 10, 3 authorities -> authority_index 10 % 3 == 1, slot preserved.
		let item = babe_secondary_plain_digest(&inherents_with_slot(10), 3).unwrap();
		assert_eq!(babe_slot_and_index(&item), (Slot::from(10), 1));
	}

	#[test]
	fn authority_index_wraps_modulo_authorities() {
		let item = babe_secondary_plain_digest(&inherents_with_slot(9), 3).unwrap();
		// 9 % 3 == 0
		assert_eq!(babe_slot_and_index(&item), (Slot::from(9), 0));
	}

	#[test]
	fn no_digest_without_aura_slot() {
		assert!(babe_secondary_plain_digest(&sp_inherents::InherentData::new(), 3).is_none());
	}
}
