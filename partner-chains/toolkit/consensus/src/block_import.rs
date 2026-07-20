use crate::inherent_check::check_partner_chains_inherents;
use crate::{InherentDigest, SlotExtractor};
use sc_consensus::block_import::{BlockCheckParams, BlockImport, BlockImportParams, ImportResult};
use sp_consensus::Error as ConsensusError;
use sp_consensus_slots::Slot;
use sp_inherents::CreateInherentDataProviders;
use sp_runtime::traits::Block as BlockT;
use std::{marker::PhantomData, sync::Arc};

/// Intermediate key under which [`PartnerChainsBlockImport`] stashes the block body
/// for [`PartnerChainsBodyRestore`] to put back.
const BODY_INTERMEDIATE_KEY: &[u8] = b"partner-chains/body";

/// Partner Chains block import wrapper, for consensus stacks that check inherents
/// during block import rather than in the import queue verifier.
///
/// Some consensus block imports (e.g. `BabeBlockImport`) check the block's inherents
/// with inherent data created from the parent hash only, which cannot include the
/// Partner Chains inherents — any block containing them would be rejected. Analogously
/// to [`PartnerChainsVerifier`](crate::PartnerChainsVerifier) on the verifier level,
/// this wrapper performs the complete Partner Chains inherent check itself and
/// withholds the block body from the wrapped import, so that the consensus logic
/// (e.g. epoch changes, equivocation reporting) still runs while its body-gated
/// inherent check is skipped.
///
/// Since the imports below the consensus one need the body (the client executes and
/// stores the block), the body is stashed as an import intermediate and must be
/// restored by [`PartnerChainsBodyRestore`] placed directly beneath the consensus
/// import. For BABE the import chain composes as:
///
/// ```text
/// PartnerChainsBlockImport<BabeBlockImport<PartnerChainsBodyRestore<GrandpaBlockImport<...>>>>
/// ```
///
/// Nodes whose consensus checks inherents in the verifier (e.g. Aura) do not need
/// this wrapper: use [`PartnerChainsVerifier`](crate::PartnerChainsVerifier) alone.
///
/// Note that nothing in the `BlockImport` trait promises that withholding the body skips
/// only the inherent check — it is an implicit contract with the wrapped implementation
/// (for `BabeBlockImport`, equivocation reporting and epoch handling are header-based).
/// For BABE it is pinned by the `babe_block_import_contract` integration tests, which
/// wrap the real `BabeBlockImport`; re-check the contract when wrapping a different
/// consensus block import or upgrading the consensus crates.
pub struct PartnerChainsBlockImport<Inner, C, CIDP, B: BlockT, SE, ID> {
	inner: Inner,
	client: Arc<C>,
	create_inherent_data_providers: CIDP,
	_phantom: PhantomData<(B, SE, ID)>,
}

impl<Inner, C, CIDP, B: BlockT, SE, ID> PartnerChainsBlockImport<Inner, C, CIDP, B, SE, ID> {
	/// Creates a new block import wrapping `inner`.
	pub fn new(inner: Inner, client: Arc<C>, create_inherent_data_providers: CIDP) -> Self {
		Self { inner, client, create_inherent_data_providers, _phantom: PhantomData }
	}
}

#[async_trait::async_trait]
impl<Inner, C, CIDP, B, SE, ID> BlockImport<B>
	for PartnerChainsBlockImport<Inner, C, CIDP, B, SE, ID>
where
	B: BlockT,
	Inner: BlockImport<B, Error = ConsensusError> + Send + Sync,
	C: sp_api::ProvideRuntimeApi<B> + Send + Sync,
	C::Api: sp_block_builder::BlockBuilder<B> + sp_api::ApiExt<B>,
	CIDP: CreateInherentDataProviders<B, (Slot, ID::Value)> + Send + Sync,
	SE: SlotExtractor<B>,
	ID: InherentDigest + Send + Sync + 'static,
{
	type Error = ConsensusError;

	async fn check_block(&self, block: BlockCheckParams<B>) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).await
	}

	async fn import_block(
		&self,
		mut block: BlockImportParams<B>,
	) -> Result<ImportResult, Self::Error> {
		// Skip checks that include execution, e.g. when importing only the state after warp sync.
		if block.with_state() || block.state_action.skip_execution_checks() {
			return self.inner.import_block(block).await;
		}

		check_partner_chains_inherents::<B, C, CIDP, SE, ID>(
			&self.client,
			&self.create_inherent_data_providers,
			&block.header,
			block.body.as_ref(),
			block.post_hash(),
		)
		.await
		.map_err(ConsensusError::ClientImport)?;

		// Withhold the body from the wrapped consensus import so its body-gated inherent
		// check — which would run against inherent data missing the Partner Chains
		// inherents — is skipped. PartnerChainsBodyRestore puts the body back beneath it.
		if let Some(body) = block.body.take() {
			block.insert_intermediate(BODY_INTERMEDIATE_KEY, body);
		}

		self.inner.import_block(block).await
	}
}

/// Restores the block body stashed by [`PartnerChainsBlockImport`].
///
/// Must be placed in the block import chain directly beneath the consensus import
/// wrapped by [`PartnerChainsBlockImport`], so that the imports below it (e.g. GRANDPA
/// and the client) receive the complete block. See [`PartnerChainsBlockImport`].
pub struct PartnerChainsBodyRestore<Inner, B: BlockT> {
	inner: Inner,
	_phantom: PhantomData<B>,
}

impl<Inner, B: BlockT> PartnerChainsBodyRestore<Inner, B> {
	/// Creates a new block import wrapping `inner`.
	pub fn new(inner: Inner) -> Self {
		Self { inner, _phantom: PhantomData }
	}
}

#[async_trait::async_trait]
impl<Inner, B> BlockImport<B> for PartnerChainsBodyRestore<Inner, B>
where
	B: BlockT,
	Inner: BlockImport<B, Error = ConsensusError> + Send + Sync,
{
	type Error = ConsensusError;

	async fn check_block(&self, block: BlockCheckParams<B>) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).await
	}

	async fn import_block(
		&self,
		mut block: BlockImportParams<B>,
	) -> Result<ImportResult, Self::Error> {
		match block.remove_intermediate::<Vec<B::Extrinsic>>(BODY_INTERMEDIATE_KEY) {
			Ok(body) => block.body = Some(body),
			// Expected when the outer wrapper did not stash a body (header-only /
			// warp-sync import, or a mis-composed pipeline with no PartnerChainsBlockImport).
			Err(ConsensusError::NoIntermediate) => {},
			// Intermediate present under our key but wrong type — fail loudly rather than
			// silently importing a body-less block.
			Err(err) => {
				log::error!(
					target: "partner-chains-consensus",
					"Partner Chains body intermediate has unexpected type: {err}"
				);
				return Err(err);
			},
		}
		self.inner.import_block(block).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::test_support::*;
	use sc_consensus::block_import::ForkChoiceStrategy;
	use std::sync::Mutex;
	use std::sync::atomic::{AtomicBool, Ordering};

	/// Stand-in for a consensus block import (e.g. `BabeBlockImport`): records whether
	/// it received the block body, as its inherent check is gated on its presence.
	struct ConsensusImportStub<Inner> {
		inner: Inner,
		saw_body: Arc<AtomicBool>,
	}

	#[async_trait::async_trait]
	impl<Inner: BlockImport<Block, Error = ConsensusError> + Send + Sync> BlockImport<Block>
		for ConsensusImportStub<Inner>
	{
		type Error = ConsensusError;

		async fn check_block(
			&self,
			block: BlockCheckParams<Block>,
		) -> Result<ImportResult, Self::Error> {
			self.inner.check_block(block).await
		}

		async fn import_block(
			&self,
			block: BlockImportParams<Block>,
		) -> Result<ImportResult, Self::Error> {
			self.saw_body.store(block.body.is_some(), Ordering::SeqCst);
			self.inner.import_block(block).await
		}
	}

	/// Innermost import (e.g. the client): records the body it received.
	struct TerminalImport {
		received_body: Arc<Mutex<Option<Option<Vec<<Block as BlockT>::Extrinsic>>>>>,
	}

	#[async_trait::async_trait]
	impl BlockImport<Block> for TerminalImport {
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
			*self.received_body.lock().unwrap() = Some(block.body);
			Ok(ImportResult::imported(false))
		}
	}

	struct Sandwich {
		import: PartnerChainsBlockImport<
			ConsensusImportStub<PartnerChainsBodyRestore<TerminalImport, Block>>,
			TestClient,
			TestCIDP,
			Block,
			TestSlotExtractor,
			TestInherentDigest,
		>,
		check_inherents_called: Arc<AtomicBool>,
		consensus_saw_body: Arc<AtomicBool>,
		terminal_received_body: Arc<Mutex<Option<Option<Vec<<Block as BlockT>::Extrinsic>>>>>,
	}

	/// Composes the documented chain:
	/// `PartnerChainsBlockImport<Consensus<PartnerChainsBodyRestore<Terminal>>>`.
	fn sandwich(fail_inherent_check: bool) -> Sandwich {
		let (client, check_inherents_called) = test_client(fail_inherent_check);
		let consensus_saw_body = Arc::new(AtomicBool::new(false));
		let terminal_received_body = Arc::new(Mutex::new(None));

		let terminal = TerminalImport { received_body: terminal_received_body.clone() };
		let consensus = ConsensusImportStub {
			inner: PartnerChainsBodyRestore::new(terminal),
			saw_body: consensus_saw_body.clone(),
		};
		let import =
			PartnerChainsBlockImport::new(consensus, client, test_create_inherent_data_providers());

		Sandwich { import, check_inherents_called, consensus_saw_body, terminal_received_body }
	}

	fn importable(body: Option<Vec<<Block as BlockT>::Extrinsic>>) -> BlockImportParams<Block> {
		let mut block = block_import_params(body);
		block.fork_choice = Some(ForkChoiceStrategy::LongestChain);
		block
	}

	#[tokio::test]
	async fn checks_inherents_and_delivers_body_to_imports_beneath_the_consensus_one() {
		let sandwich = sandwich(false);

		sandwich
			.import
			.import_block(importable(Some(vec![])))
			.await
			.expect("import succeeds");

		assert!(sandwich.check_inherents_called.load(Ordering::SeqCst));
		// The consensus import must not see the body (its inherent check is suppressed),
		// while the import below the restore stage receives the complete block.
		assert!(!sandwich.consensus_saw_body.load(Ordering::SeqCst));
		assert_eq!(*sandwich.terminal_received_body.lock().unwrap(), Some(Some(vec![])));
	}

	#[tokio::test]
	async fn rejects_block_when_inherent_check_fails_without_importing() {
		let sandwich = sandwich(true);

		let result = sandwich.import.import_block(importable(Some(vec![]))).await;

		assert!(
			matches!(result, Err(ConsensusError::ClientImport(e)) if e.contains("Inherent check failed"))
		);
		assert!(sandwich.terminal_received_body.lock().unwrap().is_none());
	}

	#[tokio::test]
	async fn passes_header_only_import_through_without_inherent_check() {
		let sandwich = sandwich(false);

		sandwich.import.import_block(importable(None)).await.expect("import succeeds");

		assert!(!sandwich.check_inherents_called.load(Ordering::SeqCst));
		assert_eq!(*sandwich.terminal_received_body.lock().unwrap(), Some(None));
	}
}
