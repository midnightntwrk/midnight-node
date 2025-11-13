//! Custom Payload Provider definition, containing the BeefyIds and each their stakes (BeefStakes)
//! The payload is scale encoded tuple of the (MMR Root, BeefStakes)
//!
//!

use core::marker::PhantomData;
use std::sync::Arc;

use midnight_node_runtime::beefy::{BeefStakesApi, DoubleBeefStakes};

use parity_scale_codec::Encode;
use sp_api::ProvideRuntimeApi;
use sp_consensus_beefy::{
	BeefyPayloadId, MmrRootHash, Payload, PayloadProvider, known_payloads,
	mmr::find_mmr_root_digest,
};
use sp_mmr_primitives::MmrApi;
use sp_runtime::traits::{Block, Header, NumberFor};

/// Id to identify this custom payload
pub const CURR_BEEF_STAKES_ID: BeefyPayloadId = *b"cs";
pub const NEXT_BEEF_STAKES_ID: BeefyPayloadId = *b"ns";

pub struct RootAndBeefStakesProvider<B, R> {
	runtime: Arc<R>,
	_phantom: PhantomData<B>,
}

impl<B, R> Clone for RootAndBeefStakesProvider<B, R> {
	fn clone(&self) -> Self {
		Self { runtime: self.runtime.clone(), _phantom: PhantomData }
	}
}

impl<B, R> RootAndBeefStakesProvider<B, R>
where
	B: Block,
	R: ProvideRuntimeApi<B>,
	R::Api: MmrApi<B, MmrRootHash, NumberFor<B>> + BeefStakesApi<B>,
{
	/// Create new Payload provider with the tuple (MMR Root, BeefStakes) as payload.
	pub fn new(runtime: Arc<R>) -> Self {
		Self { runtime, _phantom: PhantomData }
	}

	/// Simple wrapper that gets MMR root from header digests or from client state.
	fn mmr_root_from_digest_or_runtime(&self, header: &B::Header) -> Option<MmrRootHash> {
		find_mmr_root_digest::<B>(header).or_else(|| {
			self.runtime.runtime_api().mmr_root(header.hash()).ok().and_then(|r| r.ok())
		})
	}

	/// Gets the Beef Stakes from client state
	fn get_beef_stakes(
		&self,
		header: &B::Header,
	) -> Option<DoubleBeefStakes<midnight_node_runtime::Runtime>> {
		self.runtime.runtime_api().beef_stakes(header.hash()).ok()
	}
}

impl<B: Block, R> PayloadProvider<B> for RootAndBeefStakesProvider<B, R>
where
	B: Block,
	R: ProvideRuntimeApi<B>,
	R::Api: MmrApi<B, MmrRootHash, NumberFor<B>> + BeefStakesApi<B>,
{
	fn payload(&self, header: &B::Header) -> Option<Payload> {
		log::info!("XXXXXXXXXXXXXXXXXXXXXXXXXXXX Generating Beefy Payload:");

		// get the mmr root
		let mmr_root = self.mmr_root_from_digest_or_runtime(header)?;
		// get the current and next beef stakes
		let (current_beef_stakes, next_beef_stakes) = self.get_beef_stakes(header)?;

		// push to payload
		log::info!(
			"XXXXXXXXXXXXXXXXXXXXXXXXXXXX Pushing current beefy stakes to payload: {:?}",
			current_beef_stakes
		);
		log::info!(
			"XXXXXXXXXXXXXXXXXXXXXXXXXXXX Pushing next beefy stakes to payload: {:?}",
			next_beef_stakes
		);

		Some(
			Payload::from_single_entry(known_payloads::MMR_ROOT_ID, mmr_root.encode())
				.push_raw(CURR_BEEF_STAKES_ID, current_beef_stakes.encode())
				.push_raw(NEXT_BEEF_STAKES_ID, next_beef_stakes.encode()),
		)
	}
}
