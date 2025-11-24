//! Custom Payload Provider definition, containing the BeefyIds and each their stakes (BeefyStakes)
//! The payload is scale encoded tuple of the (MMR Root, BeefyStakes)
//!
//!

use core::marker::PhantomData;
use std::sync::Arc;

use midnight_primitives_beefy::{
	BeefyStakes, BeefyStakesApi,
	known_payloads::{
		CURRENT_BEEFY_AUTHORITY_SET, CURRENT_BEEFY_STAKES_ID, NEXT_BEEFY_AUTHORITY_SET,
		NEXT_BEEFY_STAKES_ID,
	},
};

use parity_scale_codec::Encode;
use sp_api::ProvideRuntimeApi;
use sp_consensus_beefy::{
	MmrRootHash, Payload, PayloadProvider,
	ecdsa_crypto::AuthorityId,
	known_payloads::MMR_ROOT_ID,
	mmr::{BeefyAuthoritySet, find_mmr_root_digest},
};
use sp_core::H256;
use sp_mmr_primitives::MmrApi;
use sp_runtime::traits::{Block, Header, NumberFor};

/// Adds `current` beefy stakes and `next` beefy stakes along with the Mmr Root
pub struct MmrRootAndBeefyStakesProvder<B, R> {
	runtime: Arc<R>,
	_phantom: PhantomData<B>,
}

impl<B, R> Clone for MmrRootAndBeefyStakesProvder<B, R> {
	fn clone(&self) -> Self {
		Self { runtime: self.runtime.clone(), _phantom: PhantomData }
	}
}

impl<B, R> MmrRootAndBeefyStakesProvder<B, R>
where
	B: Block,
	R: ProvideRuntimeApi<B>,
	R::Api: MmrApi<B, MmrRootHash, NumberFor<B>> + BeefyStakesApi<B, H256, AuthorityId>,
{
	/// Create new Payload provider with the tuple (MMR Root, BeefyStakes) as payload.
	pub fn new(runtime: Arc<R>) -> Self {
		Self { runtime, _phantom: PhantomData }
	}

	/// Simple wrapper that gets MMR root from header digests or from client state.
	fn mmr_root_from_digest_or_runtime(&self, header: &B::Header) -> Option<MmrRootHash> {
		find_mmr_root_digest::<B>(header).or_else(|| {
			self.runtime.runtime_api().mmr_root(header.hash()).ok().and_then(|r| r.ok())
		})
	}

	/// Gets the current Beef Stakes from client state
	fn current_beefy_stakes(&self, header: &B::Header) -> Option<BeefyStakes<AuthorityId>> {
		self.runtime.runtime_api().current_beefy_stakes(header.hash()).ok()
	}

	/// Gets the next Beef Stakes from client state
	fn next_beefy_stakes(&self, header: &B::Header) -> Option<BeefyStakes<AuthorityId>> {
		self.runtime.runtime_api().next_beefy_stakes(header.hash()).ok().unwrap_or(None)
	}

	/// Returns the authority set of the current beef stakes
	fn compute_current_authority_set(
		&self,
		header: &B::Header,
		beefy_stakes: BeefyStakes<AuthorityId>,
	) -> Option<BeefyAuthoritySet<H256>> {
		self.runtime
			.runtime_api()
			.compute_current_authority_set(header.hash(), beefy_stakes)
			.ok()
	}

	/// Returns the authority set of the next beef stakes
	fn compute_next_authority_set(
		&self,
		header: &B::Header,
		beefy_stakes: BeefyStakes<AuthorityId>,
	) -> Option<BeefyAuthoritySet<H256>> {
		self.runtime
			.runtime_api()
			.compute_next_authority_set(header.hash(), beefy_stakes)
			.ok()
	}
}

impl<B: Block, R> PayloadProvider<B> for MmrRootAndBeefyStakesProvder<B, R>
where
	B: Block,
	R: ProvideRuntimeApi<B>,
	R::Api: MmrApi<B, MmrRootHash, NumberFor<B>> + BeefyStakesApi<B, H256, AuthorityId>,
{
	fn payload(&self, header: &<B as Block>::Header) -> Option<Payload> {
		// get the mmr root
		let mmr_root = self.mmr_root_from_digest_or_runtime(header)?;

		// get the current and next beefy stakes
		let current_beefy_stakes = self.current_beefy_stakes(header)?;
		log::trace!("🥩 Current Beefy Stakes: {current_beefy_stakes:#?}");

		let current_authority_set =
			self.compute_current_authority_set(header, current_beefy_stakes.clone())?;
		log::trace!("🥩 Current Beefy Authority Set: {current_authority_set:#?}");

		match self.next_beefy_stakes(header) {
			Some(next_beefy_stakes) => {
				log::trace!("🥩 Next Beefy Stakes: {next_beefy_stakes:#?}");

				let next_authority_set =
					self.compute_next_authority_set(header, next_beefy_stakes.clone())?;
				log::trace!("🥩 Next Beefy Authority Set: {next_authority_set:#?}");
				Some(
					Payload::from_single_entry(MMR_ROOT_ID, mmr_root.encode())
						.push_raw(CURRENT_BEEFY_STAKES_ID, current_beefy_stakes.encode())
						.push_raw(CURRENT_BEEFY_AUTHORITY_SET, current_authority_set.encode())
						.push_raw(NEXT_BEEFY_STAKES_ID, next_beefy_stakes.encode())
						.push_raw(NEXT_BEEFY_AUTHORITY_SET, next_authority_set.encode()),
				)
			},
			None => Some(
				Payload::from_single_entry(MMR_ROOT_ID, mmr_root.encode())
					.push_raw(CURRENT_BEEFY_STAKES_ID, current_beefy_stakes.encode())
					.push_raw(CURRENT_BEEFY_AUTHORITY_SET, current_authority_set.encode()),
			),
		}
	}
}
