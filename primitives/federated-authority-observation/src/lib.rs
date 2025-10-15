//! # Federated Authority Observation Primitives
//!
//! This module provides primitives for observing federated authority changes from the main chain.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sidechain_domain::McBlockHash;
use sp_inherents::InherentIdentifier;
use sp_runtime::Vec;

#[cfg(feature = "std")]
use std::borrow::Cow;

/// The inherent identifier for federated authority observation
pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"faobsrve";

#[derive(Eq, Debug, Clone, PartialEq, TypeInfo, Default, Encode, Decode, PartialOrd, Ord)]
pub struct AuthorityMemberPublicKey(pub Vec<u8>);

/// Placeholder structure for federated authority data from main chain
/// This will contain sr25519 public keys for federated authorities
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
// #[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct FederatedAuthorityData {
	/// List of sr25519 authority public keys
	pub council_authorities: Vec<AuthorityMemberPublicKey>,
	/// List of sr25519 authority public keys
	pub technical_committee_authorities: Vec<AuthorityMemberPublicKey>,
	/// Main chain block hash this data was observed at
	pub mc_block_hash: McBlockHash,
}

/// Error type for federated authority observation inherents
#[derive(Encode, Debug)]
#[cfg_attr(feature = "std", derive(Decode))]
pub enum InherentError {
	/// The inherent data could not be decoded
	DecodeFailed,
	/// Other error
	#[cfg(feature = "std")]
	Other(Cow<'static, str>),
}

impl sp_inherents::IsFatalError for InherentError {
	fn is_fatal_error(&self) -> bool {
		true
	}
}
