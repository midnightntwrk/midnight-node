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

use super::{
	base_crypto_local, coin_structure_local, helpers_local, ledger_storage_local,
	midnight_serialize_local, mn_ledger_local, onchain_runtime_local, transient_crypto_local,
	zswap_local,
};

use super::LOG_TARGET;
pub use super::types::{self, DeserializationError, LedgerApiError, SerializationError};

use base_crypto_local::hash::HashOutput;
use coin_structure_local::coin::UserAddress as UserAddressLedger;
use ledger_storage_local::{
	WellBehavedHasher,
	arena::{ArenaHash, TypedArenaKey},
	db::DB,
};
use midnight_serialize_local::{Deserializable, Tagged};

pub mod ledger;
mod transaction;

pub(crate) type Ledger<D> = ledger::Ledger<D>;
pub(crate) type LedgerParameters = mn_ledger_local::structure::LedgerParameters;
pub(crate) type ContractState<D> = onchain_runtime_local::state::ContractState<D>;
pub(crate) type ZswapState<D> = zswap_local::ledger::State<D>;
pub(crate) type ContractAddress = coin_structure_local::contract::ContractAddress;
pub(crate) type DustPublicKey = mn_ledger_local::dust::DustPublicKey;
pub(crate) type UserAddress = coin_structure_local::coin::UserAddress;
pub(crate) type SystemTransaction = mn_ledger_local::structure::SystemTransaction;
pub(crate) type CNightGeneratesDustEvent = mn_ledger_local::structure::CNightGeneratesDustEvent;
pub(crate) type Transaction<S, D> = transaction::Transaction<S, D>;
pub(crate) type TransactionInvalid<D> = mn_ledger_local::error::TransactionInvalid<D>;
pub(crate) type TransactionOperation = transaction::Operation;
pub(crate) type TransactionIdentifier = mn_ledger_local::structure::TransactionIdentifier;
pub(crate) type TransactionAppliedStage<D> = ledger::AppliedStage<D>;

pub(crate) trait SerializableError {
	fn error() -> SerializationError;
}

pub(crate) trait DeserializableError {
	fn error() -> DeserializationError;
}

impl SerializableError for ContractAddress {
	fn error() -> SerializationError {
		SerializationError::ContractAddress
	}
}

impl DeserializableError for ContractAddress {
	fn error() -> DeserializationError {
		DeserializationError::ContractAddress
	}
}

impl DeserializableError for DustPublicKey {
	fn error() -> DeserializationError {
		DeserializationError::DustPublicKey
	}
}

impl<T, H: WellBehavedHasher> SerializableError for TypedArenaKey<T, H> {
	fn error() -> SerializationError {
		SerializationError::TypedArenaKey
	}
}

impl<H: WellBehavedHasher> SerializableError for ArenaHash<H> {
	fn error() -> SerializationError {
		SerializationError::ArenaHash
	}
}

impl<T, H: WellBehavedHasher> DeserializableError for TypedArenaKey<T, H> {
	fn error() -> DeserializationError {
		DeserializationError::TypedArenaKey
	}
}

impl SerializableError for TransactionIdentifier {
	fn error() -> SerializationError {
		SerializationError::TransactionIdentifier
	}
}

impl<D: DB> SerializableError for ContractState<D> {
	fn error() -> SerializationError {
		SerializationError::ContractState
	}
}

impl<D: DB> SerializableError for ZswapState<D> {
	fn error() -> SerializationError {
		SerializationError::ZswapState
	}
}

impl SerializableError for SystemTransaction {
	fn error() -> SerializationError {
		SerializationError::SystemTransaction
	}
}

impl DeserializableError for SystemTransaction {
	fn error() -> DeserializationError {
		DeserializationError::SystemTransaction
	}
}

impl SerializableError for CNightGeneratesDustEvent {
	fn error() -> SerializationError {
		SerializationError::CNightGeneratesDustEvent
	}
}

impl DeserializableError for CNightGeneratesDustEvent {
	fn error() -> DeserializationError {
		DeserializationError::CNightGeneratesDustEvent
	}
}

pub(crate) struct Api {}

impl Api {
	pub fn new() -> Self {
		Self {}
	}

	pub fn night_address(&self, bytes: impl AsRef<[u8]>) -> Result<UserAddress, LedgerApiError> {
		let address = bytes.as_ref().try_into().map_err(|e| {
			log::error!(target: LOG_TARGET, "Error deserializing UserAddress: {e:?}");
			LedgerApiError::Deserialization(DeserializationError::UserAddress)
		})?;

		Ok(UserAddressLedger(HashOutput(address)))
	}

	pub fn tagged_deserialize<T>(&self, bytes: &[u8]) -> Result<T, LedgerApiError>
	where
		T: Deserializable + DeserializableError + Tagged + 'static,
	{
		let kind = core::any::type_name::<T>();
		let error = LedgerApiError::Deserialization(<T as DeserializableError>::error());

		midnight_serialize_local::tagged_deserialize(bytes).map_err(|e| {
			log::error!(target: LOG_TARGET, "Error deserializing: {kind:?}: {e:?}");
			error
		})
	}

	pub fn deserialize<T>(&self, mut bytes: &[u8]) -> Result<T, LedgerApiError>
	where
		T: Deserializable + DeserializableError + 'static,
	{
		let kind = core::any::type_name::<T>();
		let error = LedgerApiError::Deserialization(<T as DeserializableError>::error());

		<T as Deserializable>::deserialize(&mut bytes, 0).map_err(|e| {
			log::error!(target: LOG_TARGET, "Error deserializing: {kind:?}: {e:?}");
			error
		})
	}

	pub fn tagged_serialize<T>(&self, value: &T) -> Result<Vec<u8>, LedgerApiError>
	where
		T: midnight_serialize_local::Serializable + SerializableError + Tagged + 'static,
	{
		let size = midnight_serialize_local::tagged_serialized_size(value);
		let mut bytes = Vec::with_capacity(size);
		let error = LedgerApiError::Serialization(<T as SerializableError>::error());

		midnight_serialize_local::tagged_serialize(value, &mut &mut bytes).map_err(|e| {
			log::error!(target: LOG_TARGET, "Error serializing: {error:?}: {e:?}");
			error
		})?;

		Ok(bytes)
	}

	pub fn serialize<T>(&self, value: &T) -> Result<Vec<u8>, LedgerApiError>
	where
		T: midnight_serialize_local::Serializable + SerializableError + 'static,
	{
		let size = midnight_serialize_local::Serializable::serialized_size(value);
		let mut bytes = Vec::with_capacity(size);
		let error = LedgerApiError::Serialization(<T as SerializableError>::error());

		value.serialize(&mut bytes).map_err(|e| {
			log::error!(target: LOG_TARGET, "Error serializing: {error:?}: {e:?}");
			error
		})?;

		Ok(bytes)
	}
}

pub(crate) fn new() -> Api {
	Api::new()
}

/// Validates that `bytes` decode to a well-formed `DustPublicKey`.
///
/// The `BoundedVec<u8, ConstU32<33>>` envelope on `DustPublicKeyBytes` enforces
/// only the wire length. The Fr-range check requires actually attempting the
/// `DustPublicKey` deserialisation, since values whose 33-byte encoding sits
/// above the Bls12-381 Fr modulus pass the length check but fail downstream
/// circuit use. This helper performs that check without emitting the
/// `log::error!` line that `Api::deserialize` produces on failure — call sites
/// that filter inputs upstream (e.g. the cNight-observation inherent-data
/// provider) treat invalid registrations as a per-UTXO non-fatal outcome, so
/// the deserialise failure must not surface as an `error`-severity log line.
pub fn dust_public_key_is_valid(bytes: &[u8]) -> bool {
	<DustPublicKey as Deserializable>::deserialize(&mut &*bytes, 0).is_ok()
}

#[cfg(test)]
mod tests {
	//! Unit tests for the `dust_public_key_is_valid` validator.
	//!
	//! Refs: shieldedtech/shielded-security-engineering#233, PM-22301
	use super::dust_public_key_is_valid;
	use super::midnight_serialize_local;
	use super::mn_ledger_local::dust::{DustPublicKey, DustSecretKey};

	/// Build a deterministic, valid DustPublicKey byte vector.
	///
	/// Uses `DustSecretKey::derive_secret_key` to avoid pulling `rand` into the
	/// ledger crate's dev-dependencies — the derivation is deterministic so the
	/// test does not need entropy.
	fn known_good_dust_public_key_bytes() -> Vec<u8> {
		let sk = DustSecretKey::derive_secret_key(&[0u8; 32]);
		let pk: DustPublicKey = DustPublicKey::from(sk);
		let mut bytes =
			Vec::with_capacity(midnight_serialize_local::Serializable::serialized_size(&pk));
		midnight_serialize_local::Serializable::serialize(&pk, &mut bytes)
			.expect("DustPublicKey serializes cleanly");
		// Sanity-check the serialized form against the wire-length envelope
		// (DustPublicKeyBytes is BoundedVec<u8, ConstU32<33>>): the encoding
		// must fit. If this ever stops being true the IDP-level filter would
		// need a different validator factoring — surface that here.
		assert!(bytes.len() <= 33, "serialized DustPublicKey length {} > 33", bytes.len());
		bytes
	}

	#[test]
	fn dust_public_key_is_valid_accepts_known_good() {
		let bytes = known_good_dust_public_key_bytes();
		assert!(dust_public_key_is_valid(&bytes), "known-good DustPublicKey bytes were rejected");
	}

	#[test]
	fn dust_public_key_is_valid_rejects_high_byte_set() {
		// A 33-byte vector with the leading byte 0xff forces the encoded value
		// above the Bls12-381 Fr modulus (~2^254), so deserialisation must fail.
		let bytes = vec![0xffu8; 33];
		assert!(
			!dust_public_key_is_valid(&bytes),
			"out-of-range DustPublicKey bytes were accepted"
		);
	}

	#[test]
	fn dust_public_key_is_valid_rejects_empty() {
		assert!(!dust_public_key_is_valid(&[]), "empty bytes were accepted as a DustPublicKey");
	}

	#[test]
	fn dust_public_key_is_valid_rejects_too_short() {
		let bytes = vec![0u8; 1];
		assert!(!dust_public_key_is_valid(&bytes), "single byte was accepted as a DustPublicKey");
	}
}
