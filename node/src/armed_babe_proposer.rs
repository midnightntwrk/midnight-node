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

//! Proposer wrapper that attaches a synthetic BABE `SecondaryPlain` pre-runtime digest to authored
//! blocks while the consensus engine has the flip to BABE armed.
//!
//! During the AURA→BABE migration the chain keeps producing blocks with AURA, but once governance
//! arms the flip ([`ArmedBabe`]/[`ScheduledFlip`]) every produced block must carry a synthetic BABE
//! pre-runtime. The digest is a [`SecondaryPlain`] entry — `{ authority_index, slot }`,
//! where `slot` is the block's AURA slot and `authority_index` is the same index the AURA logic uses
//! to pick the slot's author.
//!
//! The gate ([`ConsensusEngineApi::should_emit_babe_preruntime_digest`]) is read from the runtime at
//! the parent block: it is false in `Aura` (a BABE digest would be rejected on import) and `Babe`
//! (BABE authors its own digests), true only in the armed window.
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
use sp_consensus_aura::inherents::INHERENT_IDENTIFIER as AURA_INHERENT_IDENTIFIER;
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
		if let Some(n_authorities) = self.emit_with_authorities
			&& let Some(slot) = aura_slot_from_inherents(&args.inherent_data)
		{
			let digest = babe_secondary_plain_digest(slot, n_authorities);
			let mut logs = Vec::from(args.inherent_digests.logs());
			logs.push(digest);
			args.inherent_digests = Digest { logs };
		}
		self.inner.propose(args)
	}
}

/// Build the BABE `SecondaryPlain` pre-digest for this block.
fn babe_secondary_plain_digest(slot: Slot, n_authorities: u32) -> DigestItem {
	// `n_authorities` is guaranteed non-zero by `babe_emit_context`.
	let authority_index = (u64::from(slot) % u64::from(n_authorities)) as u32;
	let pre_digest = PreDigest::SecondaryPlain(SecondaryPlainPreDigest { authority_index, slot });
	log::debug!(
		target: LOG_TARGET,
		"Attaching BABE SecondaryPlain pre-digest (slot {slot:?}, authority_index {authority_index})",
	);
	DigestItem::PreRuntime(BABE_ENGINE_ID, pre_digest.encode())
}

/// Read the AURA slot the block is being authored for from its inherent data.
fn aura_slot_from_inherents(inherent_data: &sp_inherents::InherentData) -> Option<Slot> {
	inherent_data
		.get_data::<sp_consensus_aura::inherents::InherentType>(&AURA_INHERENT_IDENTIFIER)
		.ok()
		.flatten()
}

#[cfg(test)]
mod tests {
	use super::*;
	use midnight_primitives_consensus_engine::ActiveEngine;
	use sp_api::{ApiRef, ProvideRuntimeApi};
	use sp_consensus::Proposal;
	use sp_consensus_aura::SlotDuration;
	use sp_consensus_babe::digests::CompatibleDigestItem;
	use sp_core::sr25519;
	use sp_runtime::traits::Header as HeaderT;
	use std::sync::Mutex;
	use std::time::Duration;

	type Block = sp_runtime::generic::Block<
		sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>,
		sp_runtime::OpaqueExtrinsic,
	>;
	type Header = <Block as BlockT>::Header;
	type Hash = <Block as BlockT>::Hash;

	/// The arguments the inner proposer was called with.
	#[derive(Clone, Default)]
	struct CapturedArgs(Arc<Mutex<Option<ProposeArgs<Block>>>>);

	impl CapturedArgs {
		fn take(&self) -> ProposeArgs<Block> {
			self.0.lock().unwrap().take().expect("inner proposer was not called")
		}

		fn digest_logs(&self) -> Vec<DigestItem> {
			self.take().inherent_digests.logs().to_vec()
		}
	}

	/// Inner proposer that records the [`ProposeArgs`] it was called with and then fails.
	///
	/// The wrapper's whole job is to hand the right arguments to the inner proposer, so the mock
	/// can report failure instead of building a [`Proposal`], which would need a block and a set
	/// of storage changes that no assertion here looks at.
	struct MockProposer {
		captured: CapturedArgs,
	}

	impl Proposer<Block> for MockProposer {
		type Error = sp_consensus::Error;
		type Proposal = futures::future::Ready<Result<Proposal<Block>, sp_consensus::Error>>;

		fn propose(self, args: ProposeArgs<Block>) -> Self::Proposal {
			*self.captured.0.lock().unwrap() = Some(args);
			futures::future::ready(Err(sp_consensus::Error::ClientImport("mock".into())))
		}
	}

	/// Inner proposer factory handing out [`MockProposer`]s and recording the parents it saw.
	#[derive(Clone, Default)]
	struct MockEnvironment {
		captured: CapturedArgs,
		parents: Arc<Mutex<Vec<Header>>>,
	}

	impl Environment<Block> for MockEnvironment {
		type Proposer = MockProposer;
		type CreateProposer =
			Box<dyn Future<Output = Result<MockProposer, sp_consensus::Error>> + Send + Unpin>;
		type Error = sp_consensus::Error;

		fn init(&mut self, parent_header: &Header) -> Self::CreateProposer {
			self.parents.lock().unwrap().push(parent_header.clone());
			Box::new(futures::future::ready(Ok(MockProposer { captured: self.captured.clone() })))
		}
	}

	/// Runtime API answering the two queries [`babe_emit_context`] makes.
	#[derive(Clone)]
	struct TestApi {
		/// `None` stands for a runtime that does not have the method, i.e. a failing call.
		should_emit: Option<bool>,
		/// `None` stands for a failing `authorities` call.
		authorities: Option<Vec<AuraId>>,
	}

	impl TestApi {
		/// Armed, with `n` AURA authorities.
		fn armed(n: usize) -> Self {
			let authorities =
				(0..n).map(|i| AuraId::from(sr25519::Public::from_raw([i as u8; 32]))).collect();
			Self { should_emit: Some(true), authorities: Some(authorities) }
		}

		fn with_should_emit(self, should_emit: Option<bool>) -> Self {
			Self { should_emit, ..self }
		}

		fn with_authorities(self, authorities: Option<Vec<AuraId>>) -> Self {
			Self { authorities, ..self }
		}
	}

	impl ProvideRuntimeApi<Block> for TestApi {
		type Api = TestApi;

		fn runtime_api(&self) -> ApiRef<'_, Self::Api> {
			self.clone().into()
		}
	}

	fn api_error(msg: &'static str) -> sp_api::ApiError {
		sp_api::ApiError::Application(Box::<dyn std::error::Error + Send + Sync>::from(msg))
	}

	sp_api::mock_impl_runtime_apis! {
		impl ConsensusEngineApi<Block> for TestApi {
			#[advanced]
			fn active_engine(&self, _: Hash) -> Result<ActiveEngine, sp_api::ApiError> {
				unimplemented!("not read by the proposer")
			}

			#[advanced]
			fn should_emit_babe_preruntime_digest(&self, _: Hash) -> Result<bool, sp_api::ApiError> {
				self.should_emit.ok_or_else(|| api_error("no such runtime api"))
			}
		}

		impl AuraApi<Block, AuraId> for TestApi {
			#[advanced]
			fn slot_duration(&self, _: Hash) -> Result<SlotDuration, sp_api::ApiError> {
				unimplemented!("not read by the proposer")
			}

			#[advanced]
			fn authorities(&self, _: Hash) -> Result<Vec<AuraId>, sp_api::ApiError> {
				self.authorities.clone().ok_or_else(|| api_error("state unavailable"))
			}
		}
	}

	fn parent_header() -> Header {
		Header::new(
			7,
			Default::default(),
			Default::default(),
			Default::default(),
			Digest::default(),
		)
	}

	fn inherents_with_slot(slot: u64) -> sp_inherents::InherentData {
		let mut data = sp_inherents::InherentData::new();
		data.put_data(AURA_INHERENT_IDENTIFIER, &Slot::from(slot)).unwrap();
		data
	}

	/// A digest item that is not a BABE pre-runtime, standing in for whatever the inner
	/// consensus already put into the block.
	fn unrelated_log() -> DigestItem {
		DigestItem::Other(b"unrelated".to_vec())
	}

	fn propose_args(slot: Option<u64>) -> ProposeArgs<Block> {
		ProposeArgs {
			inherent_data: slot.map(inherents_with_slot).unwrap_or_default(),
			inherent_digests: Digest { logs: vec![unrelated_log()] },
			..Default::default()
		}
	}

	fn babe_slot_and_index(item: &DigestItem) -> (Slot, u32) {
		match <DigestItem as CompatibleDigestItem>::as_babe_pre_digest(item).unwrap() {
			PreDigest::SecondaryPlain(d) => (d.slot, d.authority_index),
			other => panic!("expected SecondaryPlain, got {other:?}"),
		}
	}

	/// Runs the wrapped proposer and returns the digest logs the inner proposer received.
	fn propose_and_capture(
		emit_with_authorities: Option<u32>,
		args: ProposeArgs<Block>,
	) -> Vec<DigestItem> {
		let captured = CapturedArgs::default();
		let proposer = ArmedBabeProposer::<Block, _>::new(
			MockProposer { captured: captured.clone() },
			emit_with_authorities,
		);

		// The mock captures the arguments before returning its (failing) future.
		drop(proposer.propose(args));

		captured.digest_logs()
	}

	#[test]
	fn proposer_attaches_secondary_plain_digest_after_the_existing_logs() {
		// slot 10, 3 authorities -> authority_index 10 % 3 == 1, slot preserved.
		let logs = propose_and_capture(Some(3), propose_args(Some(10)));

		let [existing, babe] = logs.as_slice() else {
			panic!("expected the existing log plus the BABE pre-digest, got {logs:?}");
		};
		assert_eq!(*existing, unrelated_log());
		assert_eq!(babe_slot_and_index(babe), (Slot::from(10), 1));
	}

	#[test]
	fn proposer_wraps_authority_index_modulo_authority_count() {
		let logs = propose_and_capture(Some(3), propose_args(Some(9)));
		assert_eq!(babe_slot_and_index(&logs[1]), (Slot::from(9), 0));
	}

	#[test]
	fn proposer_attaches_nothing_when_not_armed() {
		let logs = propose_and_capture(None, propose_args(Some(10)));
		assert_eq!(logs, vec![unrelated_log()]);
	}

	#[test]
	fn proposer_attaches_nothing_without_an_aura_slot_in_the_inherent_data() {
		let logs = propose_and_capture(Some(3), propose_args(None));
		assert_eq!(logs, vec![unrelated_log()]);
	}

	#[test]
	fn proposer_passes_the_remaining_propose_args_through_untouched() {
		let captured = CapturedArgs::default();
		let proposer = ArmedBabeProposer::<Block, _>::new(
			MockProposer { captured: captured.clone() },
			Some(3),
		);

		drop(proposer.propose(ProposeArgs {
			max_duration: Duration::from_millis(1234),
			block_size_limit: Some(4321),
			..propose_args(Some(10))
		}));

		let args = captured.take();
		assert_eq!(args.max_duration, Duration::from_millis(1234));
		assert_eq!(args.block_size_limit, Some(4321));
		// The inherent data is handed over as it came in — the slot is only read from it.
		assert_eq!(aura_slot_from_inherents(&args.inherent_data), Some(Slot::from(10)));
	}

	/// Initializes the wrapped factory against `api` and returns the digest logs that the
	/// inner proposer receives for a block authored in `slot`.
	fn init_and_propose(api: TestApi, slot: u64) -> Vec<DigestItem> {
		let inner = MockEnvironment::default();
		let captured = inner.captured.clone();
		let mut factory = ArmedBabeProposerFactory::new(inner, Arc::new(api));

		let proposer = futures::executor::block_on(factory.init(&parent_header()))
			.expect("inner factory does not fail");
		drop(proposer.propose(propose_args(Some(slot))));

		captured.digest_logs()
	}

	#[test]
	fn proposer_attaches_digest_when_the_runtime_reports_the_flip_as_armed() {
		// slot 6, 4 authorities -> authority_index 6 % 4 == 2.
		let logs = init_and_propose(TestApi::armed(4), 6);

		assert_eq!(babe_slot_and_index(&logs[1]), (Slot::from(6), 2));
	}

	#[test]
	fn proposer_attaches_nothing_when_the_runtime_reports_the_flip_as_not_armed() {
		let logs = init_and_propose(TestApi::armed(4).with_should_emit(Some(false)), 6);

		assert_eq!(logs, vec![unrelated_log()]);
	}

	#[test]
	fn proposer_attaches_nothing_when_the_runtime_does_not_have_the_api() {
		let logs = init_and_propose(TestApi::armed(4).with_should_emit(None), 6);

		assert_eq!(logs, vec![unrelated_log()]);
	}

	#[test]
	fn proposer_attaches_nothing_when_the_aura_authority_set_is_empty() {
		let logs = init_and_propose(TestApi::armed(4).with_authorities(Some(vec![])), 6);

		assert_eq!(logs, vec![unrelated_log()]);
	}

	#[test]
	fn proposer_attaches_nothing_when_the_authorities_cannot_be_read() {
		let logs = init_and_propose(TestApi::armed(4).with_authorities(None), 6);

		assert_eq!(logs, vec![unrelated_log()]);
	}

	#[test]
	fn proposer_factory_initializes_the_inner_factory_with_the_same_parent() {
		let inner = MockEnvironment::default();
		let parents = inner.parents.clone();
		let mut factory = ArmedBabeProposerFactory::new(inner, Arc::new(TestApi::armed(4)));

		let _ = futures::executor::block_on(factory.init(&parent_header()))
			.expect("inner factory does not fail");

		assert_eq!(*parents.lock().unwrap(), vec![parent_header()]);
	}

	#[test]
	fn aura_slot_is_read_from_the_inherent_data() {
		assert_eq!(aura_slot_from_inherents(&inherents_with_slot(42)), Some(Slot::from(42)));
	}

	#[test]
	fn aura_slot_is_absent_without_the_aura_inherent() {
		assert_eq!(aura_slot_from_inherents(&sp_inherents::InherentData::new()), None);
	}
}
