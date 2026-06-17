//! Partner Chains consensus components.
//!
//! This crate provides the pieces needed to attach Partner Chains inherent data to block
//! headers and validate it during block import, independently of the block production
//! gadget in use:
//! * [`InherentDigest`] — maps inherent data to header digest items and back
//! * [`PartnerChainsProposerFactory`] — wraps a `Proposer` to add [`InherentDigest`] items
//!   to the header of each produced block
//! * [`VerificationContextSink`] — implemented by the inherent-data-provider factory so the
//!   wrappers can inject the block's slot and [`InherentDigest`] value before delegating
//! * [`PartnerChainsBlockImport`] — wraps a consensus block import so it recreates the Partner
//!   Chains inherents (via the shared, parameterised `CreateInherentDataProviders`) during its
//!   own inherent check, for stacks that check inherents during block import (e.g. BABE)
//! * [`PartnerChainsVerifier`] — wraps an import-queue `Verifier` the same way, for stacks that
//!   check inherents in the verifier instead (e.g. Aura)
//! * [`SlotExtractor`] — implemented by the node for its block production gadget
//!   (e.g. via `sc_consensus_aura::find_pre_digest` for Aura)

mod block_import;
mod block_proposal;
mod inherent_digest;
#[cfg(test)]
mod test_support;
mod verification_context;
mod verifier;

pub use block_import::PartnerChainsBlockImport;
pub use block_proposal::{PartnerChainsProposer, PartnerChainsProposerFactory};
pub use inherent_digest::InherentDigest;
pub use verification_context::VerificationContextSink;
pub use verifier::{PartnerChainsVerifier, SlotExtractor};
