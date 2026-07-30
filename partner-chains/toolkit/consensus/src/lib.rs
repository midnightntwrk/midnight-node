//! Partner Chains consensus components.
//!
//! This crate provides the pieces needed to attach Partner Chains inherent data to block
//! headers and validate it during block import, independently of the block production
//! gadget in use:
//! * [`InherentDigest`] — maps inherent data to header digest items and back
//! * [`PartnerChainsProposerFactory`] — wraps a `Proposer` to add [`InherentDigest`] items
//!   to the header of each produced block
//! * [`PartnerChainsBlockImport`] and [`PartnerChainsBodyRestore`] — wrap a consensus block
//!   import to run the full Partner Chains inherent check against inherent data recreated
//!   from Partner Chains data sources, for stacks that check inherents during block import
//!   (e.g. BABE)
//! * [`PartnerChainsVerifier`] — wraps an import-queue `Verifier` to run the same check, for
//!   stacks that check inherents in the verifier instead (e.g. Aura)
//! * [`SlotExtractor`] — implemented by the node for its block production gadget
//!   (e.g. via `sc_consensus_aura::find_pre_digest` for Aura)

mod block_import;
mod block_proposal;
mod inherent_check;
mod verifier;

pub use block_import::{PartnerChainsBlockImport, PartnerChainsBodyRestore};
pub use block_proposal::{PartnerChainsProposer, PartnerChainsProposerFactory};
pub use sp_partner_chains_consensus::InherentDigest;
pub use verifier::{PartnerChainsVerifier, SlotExtractor};

#[cfg(test)]
mod aura_verifier_contract;
#[cfg(test)]
mod babe_block_import_contract;
#[cfg(test)]
mod test_support;
