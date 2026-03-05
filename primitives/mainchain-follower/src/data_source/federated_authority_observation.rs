// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
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

use crate::{
	FederatedAuthorityObservationDataSource,
	data_source::candidates_data_source::observed_async_trait, db::get_governance_body_utxo,
};
use cardano_serialization_lib::PlutusData;
use derive_new::new;
use midnight_primitives_federated_authority_observation::{
	AuthoritiesData, AuthorityMemberPublicKey, FederatedAuthorityData,
	FederatedAuthorityObservationConfig, GovernanceAuthorityDatumR0, GovernanceAuthorityDatums,
};
use partner_chains_db_sync_data_sources::McFollowerMetrics;
use sidechain_domain::{McBlockHash, PolicyId};
pub use sqlx::PgPool;

#[derive(new)]
pub struct FederatedAuthorityObservationDataSourceImpl {
	pub pool: PgPool,
	pub metrics_opt: Option<McFollowerMetrics>,
	#[allow(dead_code)]
	cache_size: u16,
}

observed_async_trait!(
impl FederatedAuthorityObservationDataSource for FederatedAuthorityObservationDataSourceImpl {
	async fn get_federated_authority_data(
		&self,
		config: &FederatedAuthorityObservationConfig,
		mc_block_hash: &McBlockHash,
	) -> Result<FederatedAuthorityData, Box<dyn std::error::Error + Send + Sync>> {
		// Get block number from hash
		let block = crate::db::get_block_by_hash(&self.pool, mc_block_hash.clone()).await?;

		let block_number = match block {
			Some(b) => b.block_number.0,
			None => {
				return Err(format!("Block not found for hash: {:?}", mc_block_hash).into());
			},
		};

		// Query council UTXO
		let council_utxo = get_governance_body_utxo(
			&self.pool,
			&config.council.address,
			&config.council.policy_id,
			block_number,
		)
		.await?;

		let council_authorities: AuthoritiesData = match council_utxo {
			Some(utxo) => match Self::decode_governance_datum(&utxo.full_datum.0) {
				Ok(datum) => AuthoritiesData::from(datum),
				Err(e) => {
					log::warn!(
						"Failed to decode council datum in Cardano block {}: {}. Using empty list.",
						utxo.block_number.0,
						e,
					);
					AuthoritiesData { authorities: vec![], round: 0 }
				},
			},
			None => {
				log::warn!(
					"No council UTXO found for Cardano block {} (address: {}, policy_id: {}). Using empty list.",
					block_number,
					config.council.address,
					config.council.policy_id
				);
				AuthoritiesData { authorities: vec![], round: 0 }
			},
		};

		// Query technical committee UTXO
		let technical_committee_utxo = get_governance_body_utxo(
			&self.pool,
			&config.technical_committee.address,
			&config.technical_committee.policy_id,
			block_number,
		)
		.await?;

		let technical_committee_authorities: AuthoritiesData = match technical_committee_utxo {
			Some(utxo) => match Self::decode_governance_datum(&utxo.full_datum.0) {
				Ok(datum) => AuthoritiesData::from(datum),
				Err(e) => {
					log::warn!(
						"Failed to decode technical committee datum in Cardano block {}: {}. Using empty list.",
						utxo.block_number.0,
						e,
					);
					AuthoritiesData { authorities: vec![], round: 0 }
				},
			},
			None => {
				log::warn!(
					"No technical committee UTXO found for Cardano block {} (address: {}, policy_id: {}). Using empty list.",
					block_number,
					config.technical_committee.address,
					config.technical_committee.policy_id
				);
				AuthoritiesData { authorities: vec![], round: 0 }
			},
		};

		Ok(FederatedAuthorityData {
			council_authorities,
			technical_committee_authorities,
			mc_block_hash: mc_block_hash.clone(),
		})
	}
}
);

impl FederatedAuthorityObservationDataSourceImpl {
	/// Decode PlutusData containing governance body members
	///
	/// Expected format (VersionedMultisig)
	/// ```text
	/// [
	///   [total_signers: Int, {...(CborBytes, Sr25519Keys)}],  // Multisig (also @list)
	///   logic_round: Int
	/// ]
	/// ```
	/// The first element (Multisig) contains:
	/// - total_signers: the threshold number of signers required
	/// - a map where the key is CBOR-encoded Cardano public key hash (32 bytes, first 4 bytes ditched for 28-byte PolicyId)
	///   and Sr25519Keys is a 32-byte public key
	///
	/// Returns a GovernanceAuthorityDatums enum containing the authorities and round
	fn decode_governance_datum(
		datum: &PlutusData,
	) -> Result<GovernanceAuthorityDatums, Box<dyn std::error::Error + Send + Sync>> {
		// The new format uses @list annotation, so VersionedMultisig is a list: [data, logic_round]
		// where data (Multisig) is also a list: [total_signers, members_map]
		let versioned_list: Vec<PlutusData> = datum
			.as_list()
			.ok_or("Expected PlutusData to be a list (VersionedMultisig with @list annotation)")?
			.into_iter()
			.cloned()
			.collect();

		if versioned_list.len() < 2 {
			return Err(format!(
				"Expected at least 2 elements in VersionedMultisig list, got {}",
				versioned_list.len()
			)
			.into());
		}

		// Get the 'data' field (index 0) which is Multisig: [total_signers, members_map]
		let data_field = versioned_list.first().ok_or("Expected index 0 to exist")?;
		let data_list: Vec<PlutusData> = data_field
			.as_list()
			.ok_or("Expected 'data' field (Multisig) to be a list")?
			.into_iter()
			.cloned()
			.collect();

		if data_list.len() < 2 {
			return Err(format!(
				"Expected at least 2 elements in Multisig list, got {}",
				data_list.len()
			)
			.into());
		}

		// Get the 'logic_round' field (index 1)
		let round_field = versioned_list.get(1).ok_or("Expected index 1 to exist")?;
		let round_bigint = round_field
			.as_integer()
			.ok_or("Expected 'logic_round' field to be an integer")?;
		// Convert BigInt to u64, then to u8
		let round_u64: u64 = round_bigint
			.as_u64()
			.ok_or("Expected 'logic_round' to be a non-negative integer that fits in u64")?
			.into();
		let round =
			u8::try_from(round_u64).map_err(|_| "Expected 'logic_round' to fit in u8 (0-255)")?;

		// Get the members map from data_list[1]
		let members_data =
			data_list.get(1).ok_or("Expected index 1 to exist in the Multisig list")?;

		let mut authority_members = Vec::new();

		// Try to parse as a map (Pairs<NativeScriptSigner, Sr25519PubKey>)
		if let Some(members_map) = members_data.as_map() {
			// Iterate over map keys
			let keys: Vec<PlutusData> = members_map.keys().into_iter().cloned().collect();
			for i in 0..keys.len() {
				let key = keys.get(i).ok_or("Index {i:?} not found in members_map keys")?;

				// Extract the Cardano public key hash from the map key
				// The key is CBOR-encoded (32 bytes), we need to ditch the first 4 bytes
				let key_bytes = match key.as_bytes() {
					Some(bytes) => bytes,
					None => {
						log::warn!("Map key at index {} is not bytes, skipping", i);
						continue;
					},
				};

				// Extract 28 bytes for MainchainMember by skipping first 4 bytes
				if key_bytes.len() != 32 {
					return Err(format!(
						"Expected 32 bytes for Cardano public key hash, got {}",
						key_bytes.len()
					)
					.into());
				}
				let mainchain_member_bytes = &key_bytes[4..32];
				let mainchain_member = {
					let mut bytes = [0u8; 28];
					bytes.copy_from_slice(mainchain_member_bytes);
					PolicyId(bytes)
				};

				// Get the value for this key
				// PlutusMapValues is a collection of PlutusData elements
				let values = match members_map.get(key) {
					Some(v) => v,
					None => continue,
				};

				// For our datum, each key maps to a single Sr25519 public key
				// Get the first (and only) element from PlutusMapValues
				let value_data = match values.get(0) {
					Some(v) => v,
					None => {
						log::warn!("Map value at index {} is empty, skipping", i);
						continue;
					},
				};

				// The value should be the Sr25519 key (32 bytes)
				let sr25519_key_data = match value_data.as_bytes() {
					Some(bytes) => bytes,
					None => {
						log::warn!("Map value at index {} is not bytes, skipping", i);
						continue;
					},
				};

				// Sr25519 public keys are exactly 32 bytes
				if sr25519_key_data.len() != 32 {
					return Err(format!(
						"Expected 32 bytes for Sr25519 public key, got {}.",
						sr25519_key_data.len()
					)
					.into());
				}

				authority_members
					.push((AuthorityMemberPublicKey(sr25519_key_data.to_vec()), mainchain_member));
			}
		} else {
			return Err("Expected second element to be a map".into());
		}

		Ok(GovernanceAuthorityDatums::R0(GovernanceAuthorityDatumR0 {
			authorities: authority_members,
			round,
		}))
	}
}
