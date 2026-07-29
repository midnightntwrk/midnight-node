use cardano_serialization_lib::{
	ConstrPlutusData, Credential, Ed25519KeyHash, PlutusData, RewardAddress, ScriptHash,
};
use midnight_primitives_cnight_observation::{
	CardanoPosition, CardanoRewardAddressBytes, CreateData, DeregistrationData, DustPublicKeyBytes,
	ObservedUtxo, ObservedUtxoData, ObservedUtxoHeader, SpendData, TimestampUnixMillis,
	UtxoIndexInTx,
};
use midnight_primitives_federated_authority_observation::{
	AuthorityMemberPublicKey, GovernanceAuthorityDatumR0, GovernanceAuthorityDatums,
};
use midnight_primitives_mainchain_follower::data_source::cnight_observation::RegistrationDatumDecodeError;
use partner_chains_plutus_data::registered_candidates::RegisterValidatorDatum;
use sidechain_domain::{
	CandidateKeys, CrossChainPublicKey, CrossChainSignature, MainchainKeyHash, McBlockHash,
	McTxHash, PolicyId, StakeDelegation, StakePoolPublicKey, UtxoId, UtxoIndex, UtxoInfo,
};
use std::{collections::HashMap, convert::TryFrom};
use tonic::Status;

use crate::grpc::midnight_state::{
	EpochCandidate, StakePoolEntry, UtxoEvent, UtxoId as UtxoIdProto, utxo_event::Kind,
};

#[derive(Debug)]
#[allow(clippy::enum_variant_names)] // the shared prefix is the point: each names an invalid input
pub enum CandidateConversionError {
	InvalidDatum,
	InvalidHashLength,
	InvalidIndex,
}

#[allow(clippy::result_large_err)]
pub fn observed_utxo_from_event(
	event: UtxoEvent,
	cardano_network: u8,
) -> Result<ObservedUtxo, tonic::Status> {
	let kind = event.kind.ok_or_else(|| Status::internal("missing utxo event kind"))?;

	match kind {
		Kind::AssetCreate(e) => {
			let header = ObservedUtxoHeader {
				tx_position: CardanoPosition {
					block_hash: McBlockHash(hash32(e.block_hash)?),
					block_number: e.block_number as u32,
					block_timestamp: TimestampUnixMillis(e.block_timestamp_unix_millis),
					tx_index_in_block: e.tx_index,
				},
				tx_hash: McTxHash(hash32(e.tx_hash.clone())?),
				utxo_tx_hash: McTxHash(hash32(e.tx_hash.clone())?),
				utxo_index: UtxoIndexInTx(e.output_index as u16),
			};

			Ok(ObservedUtxo {
				header,
				data: ObservedUtxoData::AssetCreate(CreateData {
					owner: e
						.address
						.try_into()
						.map_err(|_| tonic::Status::internal("Invalid address length"))?,
					value: e.quantity as u128,
					utxo_tx_hash: e
						.tx_hash
						.try_into()
						.map_err(|_| tonic::Status::internal("Invalid tx hash length"))?,
					utxo_tx_index: e.output_index as u16,
				}),
			})
		},

		Kind::AssetSpend(e) => {
			let header = ObservedUtxoHeader {
				tx_position: CardanoPosition {
					block_hash: McBlockHash(hash32(e.block_hash)?),
					block_number: e.block_number as u32,
					block_timestamp: TimestampUnixMillis(e.block_timestamp_unix_millis),
					tx_index_in_block: e.tx_index,
				},
				tx_hash: McTxHash(hash32(e.spending_tx_hash.clone())?),
				utxo_tx_hash: McTxHash(hash32(e.utxo_tx_hash.clone())?),
				utxo_index: UtxoIndexInTx(e.utxo_index as u16),
			};

			Ok(ObservedUtxo {
				header,
				data: ObservedUtxoData::AssetSpend(SpendData {
					value: e.quantity as u128,
					owner: e
						.address
						.try_into()
						.map_err(|_| tonic::Status::internal("Invalid address length"))?,
					utxo_tx_hash: McTxHash(hash32(e.utxo_tx_hash)?),
					utxo_tx_index: e.utxo_index as u16,
					spending_tx_hash: McTxHash(hash32(e.spending_tx_hash)?),
				}),
			})
		},

		Kind::Registration(e) => {
			let header = ObservedUtxoHeader {
				tx_position: CardanoPosition {
					block_hash: McBlockHash(hash32(e.block_hash)?),
					block_number: e.block_number as u32,
					block_timestamp: TimestampUnixMillis(e.block_timestamp_unix_millis),
					tx_index_in_block: e.tx_index,
				},
				tx_hash: McTxHash(hash32(e.tx_hash.clone())?),
				utxo_tx_hash: McTxHash(hash32(e.tx_hash)?),
				utxo_index: UtxoIndexInTx(e.output_index as u16),
			};

			let datum = PlutusData::from_bytes(e.full_datum)
				.map_err(|e| tonic::Status::internal(format!("Invalid CBOR datum: {e}")))?;

			let constr = datum
				.as_constr_plutus_data()
				.ok_or_else(|| tonic::Status::internal("Deregistration datum not Constr"))?;

			let (credential, dust_public_key) = decode_registration_datum(constr)
				.map_err(|e| tonic::Status::internal(format!("Datum decode error: {e}")))?;

			let reward_address = RewardAddress::new(cardano_network, &credential);

			let reward_bytes: [u8; 29] = reward_address
				.to_address()
				.to_bytes()
				.try_into()
				.map_err(|_| tonic::Status::internal("Invalid reward address length"))?;

			Ok(ObservedUtxo {
				header,
				data: ObservedUtxoData::Registration(
					midnight_primitives_cnight_observation::RegistrationData {
						cardano_reward_address: CardanoRewardAddressBytes(reward_bytes),
						dust_public_key,
					},
				),
			})
		},

		Kind::Deregistration(e) => {
			let header = ObservedUtxoHeader {
				tx_position: CardanoPosition {
					block_hash: McBlockHash(hash32(e.block_hash)?),
					block_number: e.block_number as u32,
					block_timestamp: TimestampUnixMillis(e.block_timestamp_unix_millis),
					tx_index_in_block: e.tx_index,
				},
				tx_hash: McTxHash(hash32(e.tx_hash.clone())?),
				utxo_tx_hash: McTxHash(hash32(e.utxo_tx_hash)?),
				utxo_index: UtxoIndexInTx(e.utxo_index as u16),
			};

			let datum = PlutusData::from_bytes(e.full_datum)
				.map_err(|e| tonic::Status::internal(format!("Invalid CBOR datum: {e}")))?;

			let constr = datum
				.as_constr_plutus_data()
				.ok_or_else(|| tonic::Status::internal("Deregistration datum not Constr"))?;

			let (credential, dust_public_key) = decode_registration_datum(constr)
				.map_err(|e| tonic::Status::internal(format!("Datum decode error: {e}")))?;

			let reward_address = RewardAddress::new(cardano_network, &credential);

			let reward_bytes: [u8; 29] = reward_address
				.to_address()
				.to_bytes()
				.try_into()
				.map_err(|_| tonic::Status::internal("Invalid reward address length"))?;

			Ok(ObservedUtxo {
				header,
				data: ObservedUtxoData::Deregistration(DeregistrationData {
					cardano_reward_address: CardanoRewardAddressBytes(reward_bytes),
					dust_public_key,
				}),
			})
		},
	}
}

// Free function rather than `TryFrom`: with the `embedded` feature the proto
// types are re-exported from the acropolis crate, so a trait impl would have
// both sides foreign (orphan rule).
pub(crate) fn registration_from_candidate(
	value: EpochCandidate,
) -> Result<(StakePoolPublicKey, sidechain_domain::RegistrationData), CandidateConversionError> {
	{
		let utxo_info = utxo_info_from_candidate(&value)?;

		let tx_inputs = value
			.tx_inputs
			.into_iter()
			.map(utxo_id_from_proto)
			.collect::<Result<Vec<_>, _>>()?;

		let datum = RegisterValidatorDatum::try_from(
			PlutusData::from_bytes(value.full_datum)
				.map_err(|_| CandidateConversionError::InvalidDatum)?,
		)
		.map_err(|_| CandidateConversionError::InvalidDatum)?;

		match datum {
			RegisterValidatorDatum::V0 {
				stake_ownership,
				sidechain_pub_key,
				sidechain_signature,
				registration_utxo,
				own_pkh: _own_pkh,
				aura_pub_key,
				grandpa_pub_key,
			} => Ok((
				stake_ownership.pub_key,
				sidechain_domain::RegistrationData {
					mainchain_signature: stake_ownership.signature,
					// For now we use the same key for both cross chain and sidechain actions
					cross_chain_pub_key: CrossChainPublicKey(sidechain_pub_key.0.clone()),
					cross_chain_signature: CrossChainSignature(sidechain_signature.0.clone()),
					sidechain_signature,
					sidechain_pub_key,
					keys: CandidateKeys(vec![aura_pub_key.into(), grandpa_pub_key.into()]),
					registration_utxo,
					tx_inputs,
					utxo_info,
				},
			)),
			RegisterValidatorDatum::V1 {
				stake_ownership,
				sidechain_pub_key,
				sidechain_signature,
				registration_utxo,
				own_pkh: _own_pkh,
				keys,
			} => Ok((
				stake_ownership.pub_key,
				sidechain_domain::RegistrationData {
					mainchain_signature: stake_ownership.signature,
					// For now we use the same key for both cross chain and sidechain actions
					cross_chain_pub_key: CrossChainPublicKey(sidechain_pub_key.0.clone()),
					cross_chain_signature: CrossChainSignature(sidechain_signature.0.clone()),
					sidechain_signature,
					sidechain_pub_key,
					keys,
					registration_utxo,
					tx_inputs,
					utxo_info,
				},
			)),
		}
	}
}

pub(crate) fn utxo_id_from_proto(value: UtxoIdProto) -> Result<UtxoId, CandidateConversionError> {
	{
		let tx_hash = value
			.tx_hash
			.try_into()
			.map_err(|_| CandidateConversionError::InvalidHashLength)?;

		let index = value.index.try_into().map_err(|_| CandidateConversionError::InvalidIndex)?;

		Ok(UtxoId { tx_hash: McTxHash(tx_hash), index: UtxoIndex(index) })
	}
}

pub(crate) fn utxo_info_from_candidate(
	value: &EpochCandidate,
) -> Result<UtxoInfo, CandidateConversionError> {
	{
		let tx_hash: [u8; 32] = value
			.utxo_tx_hash
			.as_slice()
			.try_into()
			.map_err(|_| CandidateConversionError::InvalidHashLength)?;

		let index = value
			.utxo_index
			.try_into()
			.map_err(|_| CandidateConversionError::InvalidIndex)?;

		let epoch_number = sidechain_domain::McEpochNumber(
			value
				.epoch_number
				.try_into()
				.map_err(|_| CandidateConversionError::InvalidIndex)?,
		);

		let block_number = sidechain_domain::McBlockNumber(
			value
				.block_number
				.try_into()
				.map_err(|_| CandidateConversionError::InvalidIndex)?,
		);

		Ok(UtxoInfo {
			utxo_id: UtxoId { tx_hash: McTxHash(tx_hash), index: UtxoIndex(index) },
			epoch_number,
			block_number,
			slot_number: sidechain_domain::McSlotNumber(value.slot_number),
			tx_index_within_block: sidechain_domain::McTxIndexInBlock(value.tx_index),
		})
	}
}

#[allow(clippy::result_large_err)]
pub fn hash32(bytes: Vec<u8>) -> Result<[u8; 32], Status> {
	<[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| Status::internal("invalid hash length"))
}

pub fn make_stake_map(
	stake_pool_entries: Vec<StakePoolEntry>,
) -> Result<HashMap<MainchainKeyHash, StakeDelegation>, CandidateConversionError> {
	stake_pool_entries
		.into_iter()
		.map(|e| {
			let hash: [u8; 28] = e
				.pool_hash
				.try_into()
				.map_err(|_| CandidateConversionError::InvalidHashLength)?;

			Ok((MainchainKeyHash(hash), StakeDelegation(e.stake)))
		})
		.collect()
}

pub fn get_stake_delegation(
	stake_map: &HashMap<MainchainKeyHash, StakeDelegation>,
	stake_pool_pub_key: &StakePoolPublicKey,
) -> Option<StakeDelegation> {
	if stake_map.is_empty() {
		None
	} else {
		Some(
			stake_map
				.get(&MainchainKeyHash::from_vkey(&stake_pool_pub_key.0))
				.cloned()
				.unwrap_or(StakeDelegation(0)),
		)
	}
}

pub fn decode_registration_datum(
	datum: ConstrPlutusData,
) -> Result<(Credential, DustPublicKeyBytes), RegistrationDatumDecodeError> {
	// We use a Vec here because the `get` method on `PlutusList` can panic
	let list: Vec<PlutusData> = datum.data().into_iter().cloned().collect();

	let Some(cardano_credential) = list.first().and_then(|d| d.as_constr_plutus_data()) else {
		return Err(RegistrationDatumDecodeError::CardanoCredentialNotBytes);
	};

	let credential = match u64::from(cardano_credential.alternative()) {
		0 => cardano_credential
			.data()
			.into_iter()
			.next()
			.and_then(|d| d.as_bytes())
			.and_then(|hash_bytes| Ed25519KeyHash::from_bytes(hash_bytes).ok())
			.map(|hash| Credential::from_keyhash(&hash))
			.ok_or(RegistrationDatumDecodeError::CardanoCredentialInvalidKeyHash)?,
		1 => cardano_credential
			.data()
			.into_iter()
			.next()
			.and_then(|d| d.as_bytes())
			.and_then(|hash_bytes| ScriptHash::from_bytes(hash_bytes).ok())
			.map(|hash| Credential::from_scripthash(&hash))
			.ok_or(RegistrationDatumDecodeError::CardanoCredentialInvalidScriptHash)?,
		tag => {
			return Err(RegistrationDatumDecodeError::CardanoCredentialInvalidTag(tag));
		},
	};

	let Some(dust_address) = list.get(1).and_then(|d| d.as_bytes()) else {
		return Err(RegistrationDatumDecodeError::DustAddressNotBytes);
	};

	let dust_addr_length = dust_address.len();
	let Ok(dust_address) = <DustPublicKeyBytes>::try_from(dust_address) else {
		return Err(RegistrationDatumDecodeError::DustAddressInvalidLength(dust_addr_length));
	};

	Ok((credential, dust_address))
}

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
pub fn decode_governance_datum(
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
	let members_data = data_list.get(1).ok_or("Expected index 1 to exist in the Multisig list")?;

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
