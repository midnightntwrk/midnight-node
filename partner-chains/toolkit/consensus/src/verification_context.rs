use crate::InherentDigest;
use sp_consensus_slots::Slot;
use std::sync::Arc;

/// Lets the Partner Chains wrappers inject the slot and [`InherentDigest`] value of the
/// block being verified/imported into the [`CreateInherentDataProviders`] that the wrapped
/// consensus gadget uses.
///
/// The wrappers ([`PartnerChainsVerifier`](crate::PartnerChainsVerifier),
/// [`PartnerChainsBlockImport`](crate::PartnerChainsBlockImport)) extract the slot (via a
/// [`SlotExtractor`](crate::SlotExtractor)) and the [`InherentDigest`] value from the block
/// header and call [`set_verification_context`](Self::set_verification_context) immediately
/// before delegating to the wrapped verifier/import. The wrapped gadget then recreates its
/// inherent data through the *same* `CreateInherentDataProviders` instance, which reproduces
/// the Partner Chains inherents parameterised by those values — so its own inherent check
/// passes instead of rejecting the block's Partner Chains inherent extrinsics.
///
/// Because [`CreateInherentDataProviders::create_inherent_data_providers`](sp_inherents::CreateInherentDataProviders::create_inherent_data_providers)
/// takes `&self`, implementors stash the context behind interior mutability. The wrappers set
/// it directly before delegating, and block verification/import runs sequentially in the
/// import queue, so the next `create_inherent_data_providers` call observes the value set for
/// the block currently being processed.
pub trait VerificationContextSink<ID: InherentDigest>: Send + Sync {
	/// Record the slot and [`InherentDigest`] value of the block about to be verified/imported,
	/// to parameterise the next inherent-data-provider creation.
	fn set_verification_context(&self, slot: Slot, inherent_digest: ID::Value);
}

impl<T, ID> VerificationContextSink<ID> for Arc<T>
where
	T: VerificationContextSink<ID> + ?Sized,
	ID: InherentDigest,
{
	fn set_verification_context(&self, slot: Slot, inherent_digest: ID::Value) {
		(**self).set_verification_context(slot, inherent_digest)
	}
}
