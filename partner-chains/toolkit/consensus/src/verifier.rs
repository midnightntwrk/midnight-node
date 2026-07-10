use crate::InherentDigest;
use crate::inherent_check::check_partner_chains_inherents;
use sc_consensus::block_import::BlockImportParams;
use sc_consensus::import_queue::Verifier;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_consensus_slots::Slot;
use sp_inherents::CreateInherentDataProviders;
use sp_runtime::traits::Block as BlockT;
use std::{marker::PhantomData, sync::Arc};

/// Extracts the authoring slot from a block header's pre-runtime digests.
///
/// Abstracts over the consensus mechanism (Aura, Babe, etc.) so that
/// [`PartnerChainsVerifier`] does not depend on a specific block production gadget.
pub trait SlotExtractor<B: BlockT>: Send + Sync + 'static {
	/// Extract the slot under which the block was authored from its header.
	fn extract_slot(header: &B::Header) -> Result<Slot, String>;
}

/// Partner Chains verifier wrapper.
///
/// Wraps an inner `Verifier` (e.g. the Aura verifier) and checks block inherents against
/// inherent data recreated from Partner Chains data sources, parameterised by the block's
/// slot and the value of its [`InherentDigest`] (e.g. the mainchain reference hash).
///
/// The block body is withheld from the inner verifier, so that consensus-specific checks
/// (seal, equivocation) still run, but the inner verifier's own inherent check is skipped.
/// That check would run against inherent data missing the Partner Chains inherents and
/// reject any block containing them. This verifier performs the single, complete inherent
/// check itself.
///
/// This covers consensus stacks that check inherents in the import queue verifier, as
/// Aura does. For stacks that check inherents during block import instead (e.g. BABE),
/// use [`PartnerChainsBlockImport`](crate::PartnerChainsBlockImport) in addition.
///
/// Note that nothing in the `Verifier` trait promises that withholding the body skips
/// only the inherent check — it is an implicit contract with the wrapped implementation.
/// For Aura it is pinned by the `aura_verifier_contract` integration tests, which wrap
/// the real `sc_consensus_aura` verifier; re-check the contract when wrapping a
/// different verifier or upgrading the consensus crates.
///
/// Generic over:
/// - `Inner`: the wrapped verifier (e.g. `AuraVerifier`)
/// - `C`: the client (for runtime API calls)
/// - `CIDP`: creates inherent data providers parameterised by `(Slot, ID::Value)`
/// - `SE`: extracts the slot from the block header
/// - `ID`: the [`InherentDigest`] carrying inherent data in the block header
pub struct PartnerChainsVerifier<Inner, C, CIDP, B: BlockT, SE, ID> {
	inner: Inner,
	client: Arc<C>,
	create_inherent_data_providers: CIDP,
	_phantom: PhantomData<(B, SE, ID)>,
}

impl<Inner, C, CIDP, B: BlockT, SE, ID> PartnerChainsVerifier<Inner, C, CIDP, B, SE, ID> {
	/// Creates a new verifier wrapping `inner`.
	pub fn new(inner: Inner, client: Arc<C>, create_inherent_data_providers: CIDP) -> Self {
		Self { inner, client, create_inherent_data_providers, _phantom: PhantomData }
	}
}

#[async_trait::async_trait]
impl<Inner, C, CIDP, B, SE, ID> Verifier<B> for PartnerChainsVerifier<Inner, C, CIDP, B, SE, ID>
where
	B: BlockT,
	Inner: Verifier<B>,
	C: ProvideRuntimeApi<B> + Send + Sync,
	C::Api: BlockBuilderApi<B> + ApiExt<B>,
	CIDP: CreateInherentDataProviders<B, (Slot, ID::Value)> + Send + Sync,
	SE: SlotExtractor<B>,
	ID: InherentDigest + Send + Sync + 'static,
{
	async fn verify(
		&self,
		mut block: BlockImportParams<B>,
	) -> Result<BlockImportParams<B>, String> {
		// Skip checks that include execution, e.g. when importing only the state after warp sync.
		if block.with_state() || block.state_action.skip_execution_checks() {
			return self.inner.verify(block).await;
		}

		// Withhold the body from the inner verifier. All of its header-level consensus
		// checks (e.g. for Aura: seal signature, slot author, slot-not-in-future,
		// equivocation) run regardless of the body; the only body-gated logic is its
		// inherent check, which would run against inherent data missing the Partner
		// Chains inherents and reject any block containing them. The complete inherent
		// check is performed below instead.
		let body = block.body.take();
		let mut block = self.inner.verify(block).await?;
		block.body = body;

		check_partner_chains_inherents::<B, C, CIDP, SE, ID>(
			&self.client,
			&self.create_inherent_data_providers,
			&block.header,
			block.body.as_ref(),
			block.post_hash(),
		)
		.await?;

		Ok(block)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::test_support::*;
	use sp_runtime::{DigestItem, OpaqueExtrinsic};
	use std::sync::atomic::{AtomicBool, Ordering};

	/// Post-digest the inner verifier stub leaves on verified blocks, standing in for
	/// the consensus seal a real verifier (e.g. Aura) moves into the post-digests.
	const INNER_VERIFIER_SEAL: &[u8] = b"inner-verifier-seal";

	/// Stand-in for a consensus verifier (e.g. Aura).
	struct InnerVerifier {
		fail: bool,
	}

	#[async_trait::async_trait]
	impl Verifier<Block> for InnerVerifier {
		async fn verify(
			&self,
			mut block: BlockImportParams<Block>,
		) -> Result<BlockImportParams<Block>, String> {
			if self.fail {
				return Err("rejected by the inner verifier".to_string());
			}
			block.post_digests.push(DigestItem::Other(INNER_VERIFIER_SEAL.to_vec()));
			Ok(block)
		}
	}

	type TestVerifier = PartnerChainsVerifier<
		InnerVerifier,
		TestClient,
		TestCIDP,
		Block,
		TestSlotExtractor,
		TestInherentDigest,
	>;

	fn test_verifier(
		inner_fail: bool,
		fail_inherent_check: bool,
	) -> (TestVerifier, Arc<AtomicBool>) {
		let (client, check_inherents_called) = test_client(fail_inherent_check);
		let verifier = PartnerChainsVerifier::new(
			InnerVerifier { fail: inner_fail },
			client,
			test_create_inherent_data_providers(),
		);
		(verifier, check_inherents_called)
	}

	fn has_inner_verifier_seal(block: &BlockImportParams<Block>) -> bool {
		block
			.post_digests
			.iter()
			.any(|item| matches!(item, DigestItem::Other(data) if data == INNER_VERIFIER_SEAL))
	}

	#[tokio::test]
	async fn accepts_block_passing_consensus_and_inherent_checks() {
		let (verifier, check_inherents_called) = test_verifier(false, false);

		let verified = verifier
			.verify(block_import_params(Some(vec![])))
			.await
			.expect("verification succeeds");

		// The consensus checks of the inner verifier ran and their outcome (e.g. the
		// extracted seal) is passed on, together with the body, for import.
		assert!(has_inner_verifier_seal(&verified));
		assert_eq!(verified.body, Some(Vec::<OpaqueExtrinsic>::new()));
		// The inherent check ran against inherent data recreated from the slot and
		// inherent digest of the header (asserted in the test CIDP).
		assert!(check_inherents_called.load(Ordering::SeqCst));
	}

	#[tokio::test]
	async fn rejects_block_when_inner_verifier_rejects() {
		let (verifier, check_inherents_called) = test_verifier(true, false);

		let error = match verifier.verify(block_import_params(Some(vec![]))).await {
			Err(error) => error,
			Ok(_) => panic!("verification should fail"),
		};

		assert!(error.contains("rejected by the inner verifier"));
		// A block failing consensus checks is rejected without the cost of recreating
		// Partner Chains inherent data.
		assert!(!check_inherents_called.load(Ordering::SeqCst));
	}

	#[tokio::test]
	async fn rejects_block_when_inherent_check_fails() {
		let (verifier, check_inherents_called) = test_verifier(false, true);

		let error = match verifier.verify(block_import_params(Some(vec![]))).await {
			Err(error) => error,
			Ok(_) => panic!("verification should fail"),
		};

		assert!(error.contains("Inherent check failed"));
		assert!(check_inherents_called.load(Ordering::SeqCst));
	}

	#[tokio::test]
	async fn skips_inherent_check_for_header_only_import() {
		let (verifier, check_inherents_called) = test_verifier(false, false);

		let verified =
			verifier.verify(block_import_params(None)).await.expect("verification succeeds");

		assert!(has_inner_verifier_seal(&verified));
		assert!(verified.body.is_none());
		assert!(!check_inherents_called.load(Ordering::SeqCst));
	}
}
