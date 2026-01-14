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

//! Aiken FederatedOps Datum Parser for Permissioned Candidates
//!
//! This module provides types and parsing logic for the Aiken `FederatedOps` datum format
//! used by the `federated_ops_forever` contract. The datum structure differs from the
//! partner-chains SDK expected format, so we parse and convert it here.
//!
//! ## Aiken FederatedOps Datum Format (from types.ak)
//!
//! ```text
//! @list
//! pub type FederatedOps {
//!   data: Data,                                    // Usually Unit (121([]))
//!   appendix: List<PermissionedCandidateDatumV1>,  // The actual candidates
//!   logic_round: Int,
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
//! ## Partner-chains SDK Expected Format
//!
//! The SDK expects `[[sidechain_key, aura_key, grandpa_key], ...]` but the Aiken format
//! uses a more structured approach with key identifiers.

use cardano_serialization_lib::PlutusData;
use sidechain_domain::{CandidateKey, CandidateKeys, PermissionedCandidateData, SidechainPublicKey};
use sp_runtime::KeyTypeId;
use std::error::Error;

/// AURA key type id (from sp_runtime::key_types)
const AURA_KEY_TYPE: KeyTypeId = KeyTypeId(*b"aura");
/// GRANDPA key type id (from sp_runtime::key_types)
const GRANDPA_KEY_TYPE: KeyTypeId = KeyTypeId(*b"gran");

/// Represents a parsed Aiken FederatedOps datum
#[derive(Debug, Clone)]
pub struct AikenFederatedOpsDatum {
	/// The data field (usually Unit/empty)
	#[allow(dead_code)]
	pub data: PlutusData,
	/// List of permissioned candidates from the appendix field
	pub candidates: Vec<AikenPermissionedCandidate>,
	/// The logic round number
	#[allow(dead_code)]
	pub logic_round: u64,
}

/// Represents a single permissioned candidate from the Aiken datum
#[derive(Debug, Clone)]
pub struct AikenPermissionedCandidate {
	/// The partner chains (sidechain) ECDSA public key (33 bytes compressed)
	pub sidechain_public_key: Vec<u8>,
	/// The Aura session key (32 bytes Sr25519)
	pub aura_public_key: Vec<u8>,
	/// The Grandpa session key (32 bytes Ed25519)
	pub grandpa_public_key: Vec<u8>,
}

/// A key with its identifier from the Aiken datum
#[derive(Debug, Clone)]
pub struct AikenCandidateKey {
	/// 4-byte identifier (e.g., b"aura", b"gran")
	pub id: Vec<u8>,
	/// Key bytes
	pub bytes: Vec<u8>,
}

impl From<AikenPermissionedCandidate> for PermissionedCandidateData {
	fn from(candidate: AikenPermissionedCandidate) -> Self {
		// Build the CandidateKeys from the parsed aura and grandpa keys
		let keys = CandidateKeys(vec![
			CandidateKey::new(AURA_KEY_TYPE, candidate.aura_public_key),
			CandidateKey::new(GRANDPA_KEY_TYPE, candidate.grandpa_public_key),
		]);

		Self {
			sidechain_public_key: SidechainPublicKey(candidate.sidechain_public_key),
			keys,
		}
	}
}

impl AikenPermissionedCandidate {
	/// Convert to the partner-chains SDK PermissionedCandidateData type
	pub fn to_permissioned_candidate_data(&self) -> PermissionedCandidateData {
		// Build the CandidateKeys from the parsed aura and grandpa keys
		let keys = CandidateKeys(vec![
			CandidateKey::new(AURA_KEY_TYPE, self.aura_public_key.clone()),
			CandidateKey::new(GRANDPA_KEY_TYPE, self.grandpa_public_key.clone()),
		]);

		PermissionedCandidateData {
			sidechain_public_key: SidechainPublicKey(self.sidechain_public_key.clone()),
			keys,
		}
	}
}

/// Known key identifiers in Aiken FederatedOps datum
pub mod key_ids {
	/// Aura session key identifier
	pub const AURA: &[u8] = b"aura";
	/// Grandpa session key identifier
	pub const GRANDPA: &[u8] = b"gran";
}

impl AikenFederatedOpsDatum {
	/// Parse a PlutusData datum as an Aiken FederatedOps structure
	///
	/// Expected format (with @list annotation):
	/// ```text
	/// [data, [[partner_chains_key, [[key_id, key_bytes], ...]], ...], logic_round]
	/// ```
	pub fn from_plutus_data(
		datum: &PlutusData,
	) -> Result<Self, Box<dyn Error + Send + Sync>> {
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
					log::warn!(
						"Failed to parse candidate at index {}: {}. Skipping.",
						idx,
						e
					);
				},
			}
		}

		// Element 2: logic_round
		let logic_round_bigint = list[2]
			.as_integer()
			.ok_or("Expected logic_round to be an integer")?;
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
	) -> Result<AikenPermissionedCandidate, Box<dyn Error + Send + Sync>> {
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
		let sidechain_public_key = list[0]
			.as_bytes()
			.ok_or("Expected partner_chains_key to be bytes")?;

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

		Ok(AikenPermissionedCandidate {
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
	) -> Result<Vec<AikenCandidateKey>, Box<dyn Error + Send + Sync>> {
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

			let id = key_list[0]
				.as_bytes()
				.ok_or("Expected key id to be bytes")?;

			let bytes = key_list[1]
				.as_bytes()
				.ok_or("Expected key bytes to be bytes")?;

			keys.push(AikenCandidateKey { id, bytes });
		}

		Ok(keys)
	}
}

/// Configuration for the Aiken FederatedOps data source
#[derive(Debug, Clone)]
pub struct AikenFederatedOpsConfig {
	/// The policy ID of the federated_ops_forever contract
	pub policy_id: sidechain_domain::PolicyId,
}

/// A wrapper data source that parses Aiken FederatedOps datums for permissioned candidates.
///
/// This data source delegates to the underlying `CandidatesDataSourceImpl` for most operations,
/// but overrides the permissioned candidates query to parse the Aiken datum format instead of
/// the partner-chains SDK expected format.
pub struct MidnightAuthoritySelectionDataSource<T> {
	/// The inner data source (CandidatesDataSourceImpl or similar)
	inner: T,
	/// Database connection pool for querying federated_ops_forever UTxOs
	pool: sqlx::PgPool,
	/// Configuration for the Aiken data source
	config: AikenFederatedOpsConfig,
}

impl<T> MidnightAuthoritySelectionDataSource<T> {
	/// Create a new Midnight authority selection data source
	pub fn new(inner: T, pool: sqlx::PgPool, config: AikenFederatedOpsConfig) -> Self {
		Self { inner, pool, config }
	}

	/// Query the federated_ops_forever UTxO and parse the Aiken datum to extract permissioned candidates
	pub async fn get_aiken_permissioned_candidates(
		&self,
		block_number: u32,
	) -> Result<Vec<AikenPermissionedCandidate>, Box<dyn Error + Send + Sync>> {
		// Query the UTxO by policy ID only (no need for script address with one-shot minting)
		let utxo =
			crate::db::get_utxo_by_policy_id(&self.pool, &self.config.policy_id, block_number)
				.await?;

		match utxo {
			Some(row) => {
				let datum = AikenFederatedOpsDatum::from_plutus_data(&row.full_datum.0)?;
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

	/// Convert Aiken candidates to partner-chains PermissionedCandidateData format
	pub fn convert_candidates(
		candidates: Vec<AikenPermissionedCandidate>,
	) -> Vec<PermissionedCandidateData> {
		candidates.into_iter().map(|c| c.into()).collect()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use cardano_serialization_lib::{BigInt, PlutusList};

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
		let unit_data = PlutusData::new_empty_constr_plutus_data(&BigInt::from(0u64));

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
		let parsed = AikenFederatedOpsDatum::from_plutus_data(&datum).unwrap();

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

		let parsed = AikenFederatedOpsDatum::from_plutus_data(&datum).unwrap();

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
		let parsed = AikenFederatedOpsDatum::from_plutus_data(&datum).unwrap();
		assert!(parsed.candidates.is_empty());
	}
}
