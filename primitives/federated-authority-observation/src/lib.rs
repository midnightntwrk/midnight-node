//! # Federated Authority Observation Primitives
//!
//! This module provides primitives for observing federated authority changes from the main chain.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sidechain_domain::McBlockHash;
use sidechain_domain::{MainchainAddress, PolicyId};
use sp_api::decl_runtime_apis;
use sp_inherents::InherentIdentifier;
use sp_runtime::Vec;

#[cfg(feature = "std")]
use std::borrow::Cow;

#[cfg(feature = "std")]
use serde::{Deserialize, Deserializer, Serialize};

#[cfg(feature = "std")]
use sp_core::{ByteArray, sr25519};

/// The inherent identifier for federated authority observation
pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"faobsrve";

/// Custom deserializer for vector of hex-encoded sr25519 public keys
#[cfg(feature = "std")]
fn vec_hex_to_vec_sr25519<'de, D>(
	deserializer: D,
) -> Result<alloc::vec::Vec<sp_core::sr25519::Public>, D::Error>
where
	D: Deserializer<'de>,
{
	let strings: alloc::vec::Vec<alloc::string::String> =
		alloc::vec::Vec::deserialize(deserializer)?;
	strings
		.into_iter()
		.map(|s| {
			let s = s.strip_prefix("0x").ok_or_else(|| {
				serde::de::Error::custom(
					"sr25519 hex public key expected to be prepended with `0x`",
				)
			})?;
			let bytes = hex::decode(s).map_err(serde::de::Error::custom)?;
			sr25519::Public::from_slice(&bytes)
				.map_err(|_| serde::de::Error::custom("Invalid sr25519 public key length"))
		})
		.collect()
}

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

/// Configuration for observing a governance body
#[cfg(feature = "std")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthBodyConfig {
	/// The Cardano script address for this governance body
	pub address: String,
	/// The policy ID for the native asset associated with this governance body
	pub policy_id: String,
	/// Initial members of this governance body (for genesis)
	#[serde(deserialize_with = "vec_hex_to_vec_sr25519")]
	pub members: Vec<sp_core::sr25519::Public>,
}

/// Configuration for Federated Authority Observation
#[cfg(feature = "std")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FederatedAuthorityObservationConfig {
	/// Council governance body configuration
	pub council: AuthBodyConfig,
	/// Technical Committee governance body configuration
	pub technical_committee: AuthBodyConfig,
}

decl_runtime_apis! {
	pub trait FederatedAuthorityObservationApi {
		/// Get the Council contract address on Cardano
		fn get_council_address() -> MainchainAddress;
		/// Get the Council policy id on Cardano
		fn get_council_policy_id() -> PolicyId;
		/// Get the Tecnical Committee contract address on Cardano
		fn get_technical_committee_address() -> MainchainAddress;
		/// Get the Tecnical Committee policy id on Cardano
		fn get_technical_committee_policy_id() -> PolicyId;
	}
}
