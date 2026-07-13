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
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[cfg(feature = "std")]
use sp_core::{ByteArray, sr25519};

/// The inherent identifier for federated authority observation
pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"faobsrve";

/// Alias for mainchain member identifier (28 bytes PolicyId)
pub type MainchainMember = PolicyId;

#[cfg(feature = "std")]
#[derive(Debug, Clone, Default, Serialize, Deserialize, serde_valid::Validate)]
pub struct FederatedAuthorityAddresses {
	#[validate(pattern = r"^(addr|addr_test)1[0-9a-z]{1,108}$")]
	pub council_address: String,

	#[serde(with = "hex")]
	pub council_policy_id: [u8; 28],

	#[validate(pattern = r"^(addr|addr_test)1[0-9a-z]{1,108}$")]
	pub technical_committee_address: String,

	#[serde(with = "hex")]
	pub technical_committee_policy_id: [u8; 28],
}

/// Convert Ed25519 public key to MainchainMember by taking first 28 bytes
#[cfg(feature = "std")]
pub fn ed25519_to_mainchain_member(public: sp_core::ed25519::Public) -> MainchainMember {
	let bytes = public.0;
	let mut mainchain_bytes = [0u8; 28];
	mainchain_bytes.copy_from_slice(&bytes[..28]);
	PolicyId(mainchain_bytes)
}

/// Custom serializer for vector of sr25519 public keys to hex-encoded strings with 0x prefix
#[cfg(feature = "std")]
fn vec_sr25519_to_vec_hex<S>(
	keys: &alloc::vec::Vec<sp_core::sr25519::Public>,
	serializer: S,
) -> Result<S::Ok, S::Error>
where
	S: Serializer,
{
	use serde::ser::SerializeSeq;
	let mut seq = serializer.serialize_seq(Some(keys.len()))?;
	for key in keys {
		let hex_str = alloc::format!("0x{}", hex::encode(key.0));
		seq.serialize_element(&hex_str)?;
	}
	seq.end()
}

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

/// Versioned enum for governance authority datums decoded from mainchain
/// This allows for future schema changes while maintaining backward compatibility
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GovernanceAuthorityDatums {
	R0(GovernanceAuthorityDatumR0),
}

/// Governance authority datum format for round 0 (initial version)
/// Contains the list of authorities and the round number
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceAuthorityDatumR0 {
	pub authorities: Vec<(AuthorityMemberPublicKey, MainchainMember)>,
	pub round: u8,
}

/// Authorities data with round information
/// This structure is used to represent decoded authority data from the mainchain
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct AuthoritiesData {
	/// List of tuples (sr25519 authority public key, mainchain member hash)
	pub authorities: Vec<(AuthorityMemberPublicKey, MainchainMember)>,
	/// Round number from the mainchain datum (currently unused but available for future use)
	pub round: u8,
}

impl From<GovernanceAuthorityDatums> for AuthoritiesData {
	fn from(datum: GovernanceAuthorityDatums) -> Self {
		match datum {
			GovernanceAuthorityDatums::R0(r0) => {
				AuthoritiesData { authorities: r0.authorities, round: r0.round }
			},
		}
	}
}

/// Placeholder structure for federated authority data from main chain
/// This will contain sr25519 public keys and mainchain member hashes for federated authorities
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct FederatedAuthorityData {
	/// Council authorities data including round information
	pub council_authorities: AuthoritiesData,
	/// Technical committee authorities data including round information
	pub technical_committee_authorities: AuthoritiesData,
	/// Main chain block hash this data was observed at
	pub mc_block_hash: McBlockHash,
}

/// Error type for federated authority observation inherents
#[derive(Encode, Debug)]
#[cfg_attr(feature = "std", derive(Decode))]
pub enum InherentError {
	/// The inherent data could not be decoded
	DecodeFailed,
	/// Mismatch between inherent checked Council members and the ones reported in the inherent tx
	CouncilMembersMismatch,
	/// Mismatch between inherent checked Technical Committee members and the ones reported in the inherent tx
	TechnicalCommitteeMembersMismatch,
	/// The number of members exceeds the limits
	TooManyMembers,
	/// Other error
	#[cfg(feature = "std")]
	Other(Cow<'static, str>),
}

impl sp_inherents::IsFatalError for InherentError {
	fn is_fatal_error(&self) -> bool {
		true
	}
}

/// Custom serializer for vector of mainchain member hashes to hex-encoded strings without 0x prefix
#[cfg(feature = "std")]
fn vec_mainchain_member_to_vec_hex<S>(
	members: &alloc::vec::Vec<MainchainMember>,
	serializer: S,
) -> Result<S::Ok, S::Error>
where
	S: Serializer,
{
	use serde::ser::SerializeSeq;
	let mut seq = serializer.serialize_seq(Some(members.len()))?;
	for member in members {
		let hex_str = hex::encode(member.0);
		seq.serialize_element(&hex_str)?;
	}
	seq.end()
}

/// Custom deserializer for vector of hex-encoded mainchain member hashes (MainchainMember)
#[cfg(feature = "std")]
fn vec_hex_to_vec_mainchain_member<'de, D>(
	deserializer: D,
) -> Result<alloc::vec::Vec<MainchainMember>, D::Error>
where
	D: Deserializer<'de>,
{
	let strings: alloc::vec::Vec<alloc::string::String> =
		alloc::vec::Vec::deserialize(deserializer)?;
	strings
		.into_iter()
		.map(|s| MainchainMember::decode_hex(&s).map_err(serde::de::Error::custom))
		.collect()
}

/// Custom serializer for PolicyId to hex-encoded string without 0x prefix
#[cfg(feature = "std")]
fn policy_id_to_hex<S>(policy_id: &PolicyId, serializer: S) -> Result<S::Ok, S::Error>
where
	S: Serializer,
{
	let hex_str = hex::encode(policy_id.0);
	serializer.serialize_str(&hex_str)
}

/// Custom deserializer for PolicyId from hex-encoded string (with or without 0x prefix)
#[cfg(feature = "std")]
fn hex_to_policy_id<'de, D>(deserializer: D) -> Result<PolicyId, D::Error>
where
	D: Deserializer<'de>,
{
	let s: alloc::string::String = alloc::string::String::deserialize(deserializer)?;
	PolicyId::decode_hex(&s).map_err(serde::de::Error::custom)
}

/// Configuration for observing a governance body
#[cfg(feature = "std")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthBodyConfig {
	/// The Cardano script address for this governance body
	pub address: String,
	/// The policy ID for the native asset associated with this governance body
	#[serde(serialize_with = "policy_id_to_hex", deserialize_with = "hex_to_policy_id")]
	pub policy_id: PolicyId,
	/// Initial members of this governance body (for genesis)
	#[serde(
		serialize_with = "vec_sr25519_to_vec_hex",
		deserialize_with = "vec_hex_to_vec_sr25519"
	)]
	pub members: Vec<sp_core::sr25519::Public>,
	/// Initial mainchain member hashes (for genesis)
	#[serde(
		serialize_with = "vec_mainchain_member_to_vec_hex",
		deserialize_with = "vec_hex_to_vec_mainchain_member"
	)]
	pub members_mainchain: Vec<MainchainMember>,
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
