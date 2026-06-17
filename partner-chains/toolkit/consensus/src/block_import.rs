use crate::{InherentDigest, SlotExtractor, VerificationContextSink};
use sc_consensus::block_import::{BlockCheckParams, BlockImport, BlockImportParams, ImportResult};
use sp_consensus::Error as ConsensusError;
use sp_runtime::traits::{Block as BlockT, Header};
use std::marker::PhantomData;

/// Partner Chains block import wrapper, for consensus stacks that check inherents
/// during block import rather than in the import queue verifier (e.g. BABE).
///
/// Makes the wrapped import's inherent check aware of the Partner Chains inherents. It
/// extracts the block's slot (via [`SlotExtractor`]) and its [`InherentDigest`] value from the
/// header, injects them into the shared
/// [`CreateInherentDataProviders`](sp_inherents::CreateInherentDataProviders) via
/// [`VerificationContextSink`], then delegates the import to the wrapped import.
///
/// The wrapped import recreates its inherent data through that same
/// `CreateInherentDataProviders` — now parameterised by the block's slot and inherent digest —
/// so its own inherent check reproduces the Partner Chains inherents and accepts the block,
/// while its other consensus logic (epoch changes, equivocation reporting, …) runs unchanged
/// against the complete block.
///
/// Nodes whose consensus checks inherents in the verifier (e.g. Aura) do not need this wrapper:
/// use [`PartnerChainsVerifier`](crate::PartnerChainsVerifier) alone.
///
/// Generic over:
/// - `Inner`: the wrapped consensus block import (e.g. `BabeBlockImport`)
/// - `CIDP`: the shared inherent-data-provider factory the inner import also uses; must
///   implement [`VerificationContextSink`] so this wrapper can parameterise it
/// - `SE`: extracts the slot from the block header
/// - `ID`: the [`InherentDigest`] carrying inherent data in the block header
pub struct PartnerChainsBlockImport<Inner, CIDP, B: BlockT, SE, ID> {
	inner: Inner,
	create_inherent_data_providers: CIDP,
	_phantom: PhantomData<fn() -> (B, SE, ID)>,
}

impl<Inner, CIDP, B: BlockT, SE, ID> PartnerChainsBlockImport<Inner, CIDP, B, SE, ID> {
	/// Creates a new block import wrapping `inner`, sharing `create_inherent_data_providers` with it.
	pub fn new(inner: Inner, create_inherent_data_providers: CIDP) -> Self {
		Self { inner, create_inherent_data_providers, _phantom: PhantomData }
	}
}

#[async_trait::async_trait]
impl<Inner, CIDP, B, SE, ID> BlockImport<B> for PartnerChainsBlockImport<Inner, CIDP, B, SE, ID>
where
	B: BlockT,
	Inner: BlockImport<B, Error = ConsensusError> + Send + Sync,
	CIDP: VerificationContextSink<ID>,
	SE: SlotExtractor<B>,
	ID: InherentDigest + Send + Sync + 'static,
{
	type Error = ConsensusError;

	async fn check_block(&self, block: BlockCheckParams<B>) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).await
	}

	async fn import_block(
		&self,
		block: BlockImportParams<B>,
	) -> Result<ImportResult, Self::Error> {
		// Skip checks that include execution, e.g. when importing only the state after warp sync.
		if block.with_state() || block.state_action.skip_execution_checks() {
			return self.inner.import_block(block).await;
		}

		// Parameterise the shared inherent-data-provider factory with the slot and inherent
		// digest of this block, so the inner import's inherent check recreates the Partner
		// Chains inherents instead of rejecting them.
		let slot =
			SE::extract_slot(&block.header).map_err(|e| ConsensusError::Other(e.into()))?;
		let digest_value = ID::value_from_digest(block.header.digest().logs()).map_err(|e| {
			ConsensusError::Other(
				format!(
					"Failed to retrieve inherent digest from header of block {:?}: {e}",
					block.header.hash()
				)
				.into(),
			)
		})?;
		self.create_inherent_data_providers.set_verification_context(slot, digest_value);

		self.inner.import_block(block).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::test_support::*;
	use sc_consensus::block_import::ForkChoiceStrategy;
	use sp_consensus_slots::Slot;
	use std::sync::Arc;
	use std::sync::Mutex;

	/// Innermost import (e.g. a consensus import): records the body it received.
	struct InnerImport {
		received_body: Arc<Mutex<Option<Option<Vec<<Block as BlockT>::Extrinsic>>>>>,
		fail: bool,
	}

	#[async_trait::async_trait]
	impl BlockImport<Block> for InnerImport {
		type Error = ConsensusError;

		async fn check_block(
			&self,
			_block: BlockCheckParams<Block>,
		) -> Result<ImportResult, Self::Error> {
			Ok(ImportResult::imported(false))
		}

		async fn import_block(
			&self,
			block: BlockImportParams<Block>,
		) -> Result<ImportResult, Self::Error> {
			if self.fail {
				return Err(ConsensusError::Other("rejected by the inner import".into()));
			}
			*self.received_body.lock().unwrap() = Some(block.body);
			Ok(ImportResult::imported(false))
		}
	}

	type TestImport =
		PartnerChainsBlockImport<InnerImport, TestCIDP, Block, TestSlotExtractor, TestInherentDigest>;

	fn test_import(
		inner_fail: bool,
	) -> (TestImport, TestCIDP, Arc<Mutex<Option<Option<Vec<<Block as BlockT>::Extrinsic>>>>>) {
		let received_body = Arc::new(Mutex::new(None));
		let cidp = TestCIDP::default();
		let inner = InnerImport { received_body: received_body.clone(), fail: inner_fail };
		let import = PartnerChainsBlockImport::new(inner, cidp.clone());
		(import, cidp, received_body)
	}

	fn importable(body: Option<Vec<<Block as BlockT>::Extrinsic>>) -> BlockImportParams<Block> {
		let mut block = block_import_params(body);
		block.fork_choice = Some(ForkChoiceStrategy::LongestChain);
		block
	}

	#[tokio::test]
	async fn parameterises_cidp_then_delegates_with_body() {
		let (import, cidp, received_body) = test_import(false);

		import.import_block(importable(Some(vec![]))).await.expect("import succeeds");

		// The CIDP was parameterised, and the inner import received the complete block.
		assert_eq!(cidp.taken_context(), Some((Slot::from(TEST_SLOT), TEST_DIGEST_VALUE)));
		assert_eq!(*received_body.lock().unwrap(), Some(Some(vec![])));
	}

	#[tokio::test]
	async fn propagates_inner_import_rejection() {
		let (import, _cidp, received_body) = test_import(true);

		let result = import.import_block(importable(Some(vec![]))).await;

		assert!(matches!(result, Err(ConsensusError::Other(e)) if e.to_string().contains("rejected by the inner import")));
		assert!(received_body.lock().unwrap().is_none());
	}
}
