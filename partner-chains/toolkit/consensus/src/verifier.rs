use crate::{InherentDigest, VerificationContextSink};
use sc_consensus::block_import::BlockImportParams;
use sc_consensus::import_queue::Verifier;
use sp_consensus_slots::Slot;
use sp_runtime::traits::{Block as BlockT, Header};
use std::marker::PhantomData;

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
/// Wraps an inner `Verifier` (e.g. the Aura verifier) and makes its inherent check aware of
/// the Partner Chains inherents. It extracts the block's slot (via [`SlotExtractor`]) and its
/// [`InherentDigest`] value (e.g. the mainchain reference hash) from the header, injects them
/// into the shared [`CreateInherentDataProviders`](sp_inherents::CreateInherentDataProviders)
/// via [`VerificationContextSink`], then delegates verification to the inner verifier.
///
/// The wrapped verifier recreates its inherent data through that same
/// `CreateInherentDataProviders` — now parameterised by the block's slot and inherent digest —
/// so its own inherent check reproduces the Partner Chains inherents and accepts the block,
/// rather than rejecting its Partner Chains inherent extrinsics. This avoids reimplementing the
/// inherent check here and keeps the inner verifier's other consensus checks (seal,
/// equivocation, …) intact.
///
/// This covers consensus stacks that check inherents in the import queue verifier, as Aura
/// does. For stacks that check inherents during block import instead (e.g. BABE), use
/// [`PartnerChainsBlockImport`](crate::PartnerChainsBlockImport).
///
/// Generic over:
/// - `Inner`: the wrapped verifier (e.g. `AuraVerifier`)
/// - `CIDP`: the shared inherent-data-provider factory the inner verifier also uses; must
///   implement [`VerificationContextSink`] so this wrapper can parameterise it
/// - `SE`: extracts the slot from the block header
/// - `ID`: the [`InherentDigest`] carrying inherent data in the block header
pub struct PartnerChainsVerifier<Inner, CIDP, B: BlockT, SE, ID> {
	inner: Inner,
	create_inherent_data_providers: CIDP,
	_phantom: PhantomData<fn() -> (B, SE, ID)>,
}

impl<Inner, CIDP, B: BlockT, SE, ID> PartnerChainsVerifier<Inner, CIDP, B, SE, ID> {
	/// Creates a new verifier wrapping `inner`, sharing `create_inherent_data_providers` with it.
	pub fn new(inner: Inner, create_inherent_data_providers: CIDP) -> Self {
		Self { inner, create_inherent_data_providers, _phantom: PhantomData }
	}
}

#[async_trait::async_trait]
impl<Inner, CIDP, B, SE, ID> Verifier<B> for PartnerChainsVerifier<Inner, CIDP, B, SE, ID>
where
	B: BlockT,
	Inner: Verifier<B>,
	CIDP: VerificationContextSink<ID>,
	SE: SlotExtractor<B>,
	ID: InherentDigest + Send + Sync + 'static,
{
	async fn verify(
		&self,
		block: BlockImportParams<B>,
	) -> Result<BlockImportParams<B>, String> {
		// Skip checks that include execution, e.g. when importing only the state after warp sync.
		if block.with_state() || block.state_action.skip_execution_checks() {
			return self.inner.verify(block).await;
		}

		// Parameterise the shared inherent-data-provider factory with the slot and inherent
		// digest of this block, so the inner verifier's inherent check recreates the Partner
		// Chains inherents instead of rejecting them.
		let slot = SE::extract_slot(&block.header)?;
		let digest_value =
			ID::value_from_digest(block.header.digest().logs()).map_err(|e| {
				format!(
					"Failed to retrieve inherent digest from header of block {:?}: {e}",
					block.header.hash()
				)
			})?;
		self.create_inherent_data_providers.set_verification_context(slot, digest_value);

		self.inner.verify(block).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::test_support::*;
	use sp_runtime::{DigestItem, OpaqueExtrinsic};

	/// Post-digest the inner verifier stub leaves on verified blocks, standing in for
	/// the consensus seal a real verifier (e.g. Aura) moves into the post-digests.
	const INNER_VERIFIER_SEAL: &[u8] = b"inner-verifier-seal";

	/// Stand-in for a consensus verifier (e.g. Aura). Its inherent check is represented by
	/// the shared CIDP: a real inner verifier recreates inherent data through it, so we treat
	/// "the CIDP was parameterised" as the contract this wrapper must uphold.
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

	type TestVerifier =
		PartnerChainsVerifier<InnerVerifier, TestCIDP, Block, TestSlotExtractor, TestInherentDigest>;

	fn test_verifier(inner_fail: bool) -> (TestVerifier, TestCIDP) {
		let cidp = TestCIDP::default();
		let verifier = PartnerChainsVerifier::new(InnerVerifier { fail: inner_fail }, cidp.clone());
		(verifier, cidp)
	}

	fn has_inner_verifier_seal(block: &BlockImportParams<Block>) -> bool {
		block
			.post_digests
			.iter()
			.any(|item| matches!(item, DigestItem::Other(data) if data == INNER_VERIFIER_SEAL))
	}

	#[tokio::test]
	async fn parameterises_cidp_then_delegates_with_body() {
		let (verifier, cidp) = test_verifier(false);

		let verified = verifier
			.verify(block_import_params(Some(vec![])))
			.await
			.expect("verification succeeds");

		// The inner verifier ran (its seal is present) and received the full block body.
		assert!(has_inner_verifier_seal(&verified));
		assert_eq!(verified.body, Some(Vec::<OpaqueExtrinsic>::new()));
		// The shared CIDP was parameterised with the slot and inherent digest of the header.
		assert_eq!(cidp.taken_context(), Some((Slot::from(TEST_SLOT), TEST_DIGEST_VALUE)));
	}

	#[tokio::test]
	async fn propagates_inner_verifier_rejection() {
		let (verifier, _cidp) = test_verifier(true);

		let error = match verifier.verify(block_import_params(Some(vec![]))).await {
			Err(error) => error,
			Ok(_) => panic!("verification should fail"),
		};

		assert!(error.contains("rejected by the inner verifier"));
	}

	#[tokio::test]
	async fn skips_parameterisation_for_state_import() {
		let (verifier, cidp) = test_verifier(false);

		let mut block = block_import_params(Some(vec![]));
		// `Skip` makes `skip_execution_checks()` true, exercising the wrapper's early-return path.
		block.state_action = sc_consensus::StateAction::Skip;

		let verified = verifier.verify(block).await.expect("verification succeeds");

		// Such imports delegate straight to the inner verifier without parameterising the CIDP.
		assert!(has_inner_verifier_seal(&verified));
		assert_eq!(cidp.taken_context(), None);
	}
}
