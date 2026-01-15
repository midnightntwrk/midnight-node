// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! FederatedOps Datum Parser for Permissioned Candidates
//!
//! This module provides types and parsing logic for the Aiken `FederatedOps` datum format
//! used by the `federated_ops_forever` contract.
//!
//! ## Why This Parser Is Needed
//!
//! The FederatedOps datum structure is **structurally identical** to the partner-chains SDK
//! `VersionedGenericDatum` format, but there is a **semantic mismatch** in the last field:
//!
//! | Format | Structure | Last Field Meaning |
//! |--------|-----------|-------------------|
//! | SDK `VersionedGenericDatum` | `[data, appendix, version]` | Datum format version (0 or 1) |
//! | Aiken `FederatedOps` | `[data, appendix, logic_round]` | Governance logic round (0, 1, 2, ...) |
//!
//! The SDK interprets the last field as a version selector:
//! - version=0 → parse appendix as V0 (flat: `[sidechain_key, aura_key, grandpa_key]`)
//! - version=1 → parse appendix as V1 (named: `[partner_chains_key, [[key_id, key_bytes], ...]]`)
//! - version≥2 → error "Unknown version"
//!
//! Since FederatedOps always uses V1 appendix format but `logic_round` can be 0, 2, 3, etc.,
//! the SDK parsing fails when `logic_round != 1`. This parser always interprets the appendix
//! as V1 format, ignoring the `logic_round` value for parsing purposes.
//!
//! ## Aiken FederatedOps Datum Format (from types.ak)
//!
//! ```text
//! @list
//! pub type FederatedOps {
//!   data: Data,                                    // Usually Unit (121([]))
//!   appendix: List<PermissionedCandidateDatumV1>,  // The actual candidates (always V1 format)
//!   logic_round: Int,                              // Governance versioning, NOT datum format version
//! }
//!
//! @list
//! pub type PermissionedCandidateDatumV1 {
//!   partner_chains_key: SidechainPublicKey,  // ECDSA key (33 bytes compressed)
//!   keys: List<CandidateKey>,                // [aura, grandpa, etc.]
//! }
//!
//! @list
//! pub type CandidateKey {
//!   id: ByteArray,    // 4-byte identifier (e.g., "aura", "gran")
//!   bytes: ByteArray, // Key bytes
//! }
//! ```
//!
//! Note: The appendix format matches partner-chains SDK V1 exactly. The only difference
//! is that `logic_round` is used for governance upgrades, not datum format versioning.

use cardano_serialization_lib::PlutusData;
use sidechain_domain::{
	CandidateKey, CandidateKeys, PermissionedCandidateData, SidechainPublicKey,
};
use sp_runtime::KeyTypeId;
use std::error::Error;

/// AURA key type id (from sp_runtime::key_types)
const AURA_KEY_TYPE: KeyTypeId = KeyTypeId(*b"aura");
/// GRANDPA key type id (from sp_runtime::key_types)
const GRANDPA_KEY_TYPE: KeyTypeId = KeyTypeId(*b"gran");

/// Represents a parsed FederatedOps datum
#[derive(Debug, Clone)]
pub struct FederatedOpsDatum {
	/// The data field (usually Unit/empty)
	#[allow(dead_code)]
	pub data: PlutusData,
	/// List of permissioned candidates from the appendix field
	pub candidates: Vec<FederatedOpsCandidate>,
	/// The logic round number
	#[allow(dead_code)]
	pub logic_round: u64,
}

/// Represents a single permissioned candidate from the FederatedOps datum
#[derive(Debug, Clone)]
pub struct FederatedOpsCandidate {
	/// The partner chains (sidechain) ECDSA public key (33 bytes compressed)
	pub sidechain_public_key: Vec<u8>,
	/// The Aura session key (32 bytes Sr25519)
	pub aura_public_key: Vec<u8>,
	/// The Grandpa session key (32 bytes Ed25519)
	pub grandpa_public_key: Vec<u8>,
}

/// A key with its identifier from the FederatedOps datum
#[derive(Debug, Clone)]
pub struct FederatedOpsCandidateKey {
	/// 4-byte identifier (e.g., b"aura", b"gran")
	pub id: Vec<u8>,
	/// Key bytes
	pub bytes: Vec<u8>,
}

impl From<FederatedOpsCandidate> for PermissionedCandidateData {
	fn from(candidate: FederatedOpsCandidate) -> Self {
		let keys = CandidateKeys(vec![
			CandidateKey::new(AURA_KEY_TYPE, candidate.aura_public_key),
			CandidateKey::new(GRANDPA_KEY_TYPE, candidate.grandpa_public_key),
		]);

		Self { sidechain_public_key: SidechainPublicKey(candidate.sidechain_public_key), keys }
	}
}

/// Known key identifiers in FederatedOps datum
pub mod key_ids {
	/// Aura session key identifier
	pub const AURA: &[u8] = b"aura";
	/// Grandpa session key identifier
	pub const GRANDPA: &[u8] = b"gran";
}

impl FederatedOpsDatum {
	/// Parse a PlutusData datum as a FederatedOps structure
	///
	/// Expected format (with @list annotation):
	/// ```text
	/// [data, [[partner_chains_key, [[key_id, key_bytes], ...]], ...], logic_round]
	/// ```
	pub fn from_plutus_data(datum: &PlutusData) -> Result<Self, Box<dyn Error + Send + Sync>> {
		// FederatedOps uses @list annotation, so it's a list: [data, appendix, logic_round]
		let list: Vec<PlutusData> = datum
			.as_list()
			.ok_or("Expected FederatedOps to be a list (uses @list annotation)")?
			.into_iter()
			.cloned()
			.collect();

		if list.len() < 3 {
			return Err(format!(
				"Expected at least 3 elements in FederatedOps list, got {}",
				list.len()
			)
			.into());
		}

		// Element 0: data (usually Unit)
		let data = list[0].clone();

		// Element 1: appendix - List<PermissionedCandidateDatumV1>
		let appendix_list: Vec<PlutusData> = list[1]
			.as_list()
			.ok_or("Expected appendix field to be a list")?
			.into_iter()
			.cloned()
			.collect();

		let mut candidates = Vec::with_capacity(appendix_list.len());
		for (idx, candidate_data) in appendix_list.iter().enumerate() {
			match Self::parse_candidate(candidate_data) {
				Ok(candidate) => candidates.push(candidate),
				Err(e) => {
					log::warn!("Failed to parse candidate at index {}: {}. Skipping.", idx, e);
				},
			}
		}

		// Element 2: logic_round
		let logic_round_bigint =
			list[2].as_integer().ok_or("Expected logic_round to be an integer")?;
		let logic_round: u64 = logic_round_bigint
			.as_u64()
			.ok_or("Expected logic_round to be a non-negative integer that fits in u64")?
			.into();

		Ok(Self { data, candidates, logic_round })
	}

	/// Parse a single PermissionedCandidateDatumV1 from PlutusData
	///
	/// Expected format (with @list annotation):
	/// ```text
	/// [partner_chains_key, [[key_id, key_bytes], ...]]
	/// ```
	fn parse_candidate(
		data: &PlutusData,
	) -> Result<FederatedOpsCandidate, Box<dyn Error + Send + Sync>> {
		let list: Vec<PlutusData> = data
			.as_list()
			.ok_or("Expected PermissionedCandidateDatumV1 to be a list")?
			.into_iter()
			.cloned()
			.collect();

		if list.len() < 2 {
			return Err(format!(
				"Expected at least 2 elements in PermissionedCandidateDatumV1, got {}",
				list.len()
			)
			.into());
		}

		// Element 0: partner_chains_key (SidechainPublicKey = ByteArray)
		let sidechain_public_key =
			list[0].as_bytes().ok_or("Expected partner_chains_key to be bytes")?;

		// Validate sidechain key length (33 bytes for compressed ECDSA)
		if sidechain_public_key.len() != 33 {
			return Err(format!(
				"Expected 33 bytes for sidechain public key (compressed ECDSA), got {}",
				sidechain_public_key.len()
			)
			.into());
		}

		// Element 1: keys - List<CandidateKey>
		let keys = Self::parse_candidate_keys(&list[1])?;

		// Find aura and grandpa keys by their identifiers
		let aura_key = keys
			.iter()
			.find(|k| k.id == key_ids::AURA)
			.ok_or("Missing 'aura' key in candidate")?;

		let grandpa_key = keys
			.iter()
			.find(|k| k.id == key_ids::GRANDPA)
			.ok_or("Missing 'gran' (grandpa) key in candidate")?;

		// Validate key lengths (32 bytes for Sr25519/Ed25519)
		if aura_key.bytes.len() != 32 {
			return Err(format!(
				"Expected 32 bytes for Aura key (Sr25519), got {}",
				aura_key.bytes.len()
			)
			.into());
		}

		if grandpa_key.bytes.len() != 32 {
			return Err(format!(
				"Expected 32 bytes for Grandpa key (Ed25519), got {}",
				grandpa_key.bytes.len()
			)
			.into());
		}

		Ok(FederatedOpsCandidate {
			sidechain_public_key,
			aura_public_key: aura_key.bytes.clone(),
			grandpa_public_key: grandpa_key.bytes.clone(),
		})
	}

	/// Parse the keys list from a PermissionedCandidateDatumV1
	///
	/// Expected format:
	/// ```text
	/// [[key_id, key_bytes], ...]
	/// ```
	fn parse_candidate_keys(
		data: &PlutusData,
	) -> Result<Vec<FederatedOpsCandidateKey>, Box<dyn Error + Send + Sync>> {
		let keys_list: Vec<PlutusData> = data
			.as_list()
			.ok_or("Expected keys to be a list")?
			.into_iter()
			.cloned()
			.collect();

		let mut keys = Vec::with_capacity(keys_list.len());

		for key_data in keys_list.iter() {
			let key_list: Vec<PlutusData> = key_data
				.as_list()
				.ok_or("Expected CandidateKey to be a list [id, bytes]")?
				.into_iter()
				.cloned()
				.collect();

			if key_list.len() < 2 {
				return Err(format!(
					"Expected at least 2 elements in CandidateKey, got {}",
					key_list.len()
				)
				.into());
			}

			let id = key_list[0].as_bytes().ok_or("Expected key id to be bytes")?;

			let bytes = key_list[1].as_bytes().ok_or("Expected key bytes to be bytes")?;

			keys.push(FederatedOpsCandidateKey { id, bytes });
		}

		Ok(keys)
	}
}

/// Configuration for the FederatedOps data source
#[derive(Debug, Clone)]
pub struct FederatedOpsConfig {
	/// The policy ID of the federated_ops_forever contract
	pub policy_id: sidechain_domain::PolicyId,
}

/// A wrapper data source that parses FederatedOps datums for permissioned candidates.
///
/// This data source delegates to the underlying `CandidatesDataSourceImpl` for most operations,
/// but overrides the permissioned candidates query to parse the FederatedOps datum format instead of
/// the partner-chains SDK expected format.
pub struct MidnightAuthoritySelectionDataSource<T> {
	/// The inner data source (CandidatesDataSourceImpl or similar)
	inner: T,
	/// Database connection pool for querying federated_ops_forever UTxOs
	pool: sqlx::PgPool,
	/// Configuration for the FederatedOps data source
	config: FederatedOpsConfig,
}

impl<T> MidnightAuthoritySelectionDataSource<T> {
	/// Create a new Midnight authority selection data source
	pub fn new(inner: T, pool: sqlx::PgPool, config: FederatedOpsConfig) -> Self {
		Self { inner, pool, config }
	}

	/// Query the federated_ops_forever UTxO and parse the datum to extract permissioned candidates
	pub async fn get_permissioned_candidates(
		&self,
		block_number: u32,
	) -> Result<Vec<FederatedOpsCandidate>, Box<dyn Error + Send + Sync>> {
		// Query the UTxO by policy ID only (no need for script address with one-shot minting)
		let utxo =
			crate::db::get_utxo_by_policy_id(&self.pool, &self.config.policy_id, block_number)
				.await?;

		match utxo {
			Some(row) => {
				let datum = FederatedOpsDatum::from_plutus_data(&row.full_datum.0)?;
				Ok(datum.candidates)
			},
			None => {
				log::warn!(
					"No federated_ops_forever UTxO found at block {} (policy_id: {})",
					block_number,
					self.config.policy_id
				);
				Ok(vec![])
			},
		}
	}

	/// Get a reference to the inner data source
	pub fn inner(&self) -> &T {
		&self.inner
	}

	/// Get a mutable reference to the inner data source
	pub fn inner_mut(&mut self) -> &mut T {
		&mut self.inner
	}

	/// Consume self and return the inner data source
	pub fn into_inner(self) -> T {
		self.inner
	}

	/// Convert FederatedOps candidates to partner-chains PermissionedCandidateData format
	pub fn convert_candidates(
		candidates: Vec<FederatedOpsCandidate>,
	) -> Vec<PermissionedCandidateData> {
		candidates.into_iter().map(|c| c.into()).collect()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use cardano_serialization_lib::{BigInt, BigNum, PlutusList};

	fn create_candidate_key(id: &[u8], key_bytes: &[u8]) -> PlutusData {
		let mut key_list = PlutusList::new();
		key_list.add(&PlutusData::new_bytes(id.to_vec()));
		key_list.add(&PlutusData::new_bytes(key_bytes.to_vec()));
		PlutusData::new_list(&key_list)
	}

	fn create_candidate(sidechain_key: &[u8], aura_key: &[u8], grandpa_key: &[u8]) -> PlutusData {
		let mut keys_list = PlutusList::new();
		keys_list.add(&create_candidate_key(b"aura", aura_key));
		keys_list.add(&create_candidate_key(b"gran", grandpa_key));

		let mut candidate_list = PlutusList::new();
		candidate_list.add(&PlutusData::new_bytes(sidechain_key.to_vec()));
		candidate_list.add(&PlutusData::new_list(&keys_list));

		PlutusData::new_list(&candidate_list)
	}

	fn create_federated_ops(candidates: Vec<PlutusData>, logic_round: u64) -> PlutusData {
		// Create Unit data (constr 0 with empty fields)
		let unit_data = PlutusData::new_empty_constr_plutus_data(&BigNum::zero());

		let mut appendix_list = PlutusList::new();
		for candidate in candidates {
			appendix_list.add(&candidate);
		}

		let mut federated_ops_list = PlutusList::new();
		federated_ops_list.add(&unit_data);
		federated_ops_list.add(&PlutusData::new_list(&appendix_list));
		federated_ops_list.add(&PlutusData::new_integer(&BigInt::from(logic_round)));

		PlutusData::new_list(&federated_ops_list)
	}

	#[test]
	fn test_parse_empty_federated_ops() {
		let datum = create_federated_ops(vec![], 0);
		let parsed = FederatedOpsDatum::from_plutus_data(&datum).unwrap();

		assert!(parsed.candidates.is_empty());
		assert_eq!(parsed.logic_round, 0);
	}

	#[test]
	fn test_parse_federated_ops_with_candidate() {
		// Create a 33-byte compressed ECDSA key
		let sidechain_key = vec![0x02u8; 33];
		// Create 32-byte session keys
		let aura_key = vec![0xAAu8; 32];
		let grandpa_key = vec![0xBBu8; 32];

		let candidate = create_candidate(&sidechain_key, &aura_key, &grandpa_key);
		let datum = create_federated_ops(vec![candidate], 5);

		let parsed = FederatedOpsDatum::from_plutus_data(&datum).unwrap();

		assert_eq!(parsed.candidates.len(), 1);
		assert_eq!(parsed.logic_round, 5);

		let candidate = &parsed.candidates[0];
		assert_eq!(candidate.sidechain_public_key, sidechain_key);
		assert_eq!(candidate.aura_public_key, aura_key);
		assert_eq!(candidate.grandpa_public_key, grandpa_key);
	}

	#[test]
	fn test_parse_federated_ops_missing_grandpa_key() {
		// Create candidate without grandpa key - should fail
		let sidechain_key = vec![0x02u8; 33];
		let aura_key = vec![0xAAu8; 32];

		let mut keys_list = PlutusList::new();
		keys_list.add(&create_candidate_key(b"aura", &aura_key));
		// No grandpa key

		let mut candidate_list = PlutusList::new();
		candidate_list.add(&PlutusData::new_bytes(sidechain_key));
		candidate_list.add(&PlutusData::new_list(&keys_list));

		let candidate = PlutusData::new_list(&candidate_list);
		let datum = create_federated_ops(vec![candidate], 0);

		// Should succeed but with empty candidates (invalid candidate is skipped)
		let parsed = FederatedOpsDatum::from_plutus_data(&datum).unwrap();
		assert!(parsed.candidates.is_empty());
	}
}
