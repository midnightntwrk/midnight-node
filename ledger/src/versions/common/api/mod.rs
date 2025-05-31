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

use super::{
	base_crypto_local, coin_structure_local, ledger_storage_local, midnight_serialize_local,
	mn_ledger_local, onchain_runtime_local, transient_crypto_local, zswap_local,
};

use super::LOG_TARGET;
pub use super::types::{self, DeserializationError, LedgerApiError, SerializationError};
use core::any::TypeId;

use base_crypto_local::{hash::HashOutput, signatures::Signature};
use coin_structure_local::coin::UserAddress as UserAddressLedger;
use ledger_storage_local::{
	DefaultDB, WellBehavedHasher,
	arena::VersionedArenaKey,
	db::{DB, ParityDb},
};
use midnight_serialize_local::{Deserializable, NetworkId};

pub mod ledger;
mod transaction;

pub(crate) type Ledger<D> = ledger::Ledger<D>;
pub(crate) type LedgerParameters = mn_ledger_local::structure::LedgerParameters;
pub(crate) type ContractState<D> = onchain_runtime_local::state::ContractState<D>;
pub(crate) type ContractStateValue<D> = onchain_runtime_local::state::StateValue<D>;
pub(crate) type ZswapState<D> = zswap_local::ledger::State<D>;
pub(crate) type ContractAddress = coin_structure_local::contract::ContractAddress;
pub(crate) type UserAddress = coin_structure_local::coin::UserAddress;
pub(crate) type OutputInstructionUnshielded =
	mn_ledger_local::structure::OutputInstructionUnshielded;
pub(crate) type SystemTransaction = mn_ledger_local::structure::SystemTransaction;
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

impl<H: WellBehavedHasher> SerializableError for VersionedArenaKey<H> {
	fn error() -> SerializationError {
		SerializationError::VersionedArenaKey
	}
}

impl<H: WellBehavedHasher> DeserializableError for VersionedArenaKey<H> {
	fn error() -> DeserializationError {
		DeserializationError::VersionedArenaKey
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

pub(crate) struct Api {
	network_id: NetworkId,
}

impl Api {
	pub fn new(network_id: NetworkId) -> Self {
		Self { network_id }
	}

	pub fn night_address(&self, bytes: impl AsRef<[u8]>) -> Result<UserAddress, LedgerApiError> {
		let address = bytes.as_ref().try_into().map_err(|e| {
			log::error!(target: LOG_TARGET, "Error deserializing UserAddress: {:?}", e);
			LedgerApiError::Deserialization(DeserializationError::UserAddress)
		})?;

		Ok(UserAddressLedger(HashOutput(address)))
	}

	pub fn deserialize<T>(&self, bytes: &[u8]) -> Result<T, LedgerApiError>
	where
		T: Deserializable + DeserializableError + 'static,
	{
		let kind = core::any::type_name::<T>();
		let error = LedgerApiError::Deserialization(<T as DeserializableError>::error());
		let mut bytes = bytes.to_vec();
		let transaction_type_ids = [
			TypeId::of::<Transaction<Signature, DefaultDB>>(),
			TypeId::of::<Transaction<Signature, ParityDb>>(),
		];
		if transaction_type_ids.contains(&TypeId::of::<T>()) {
			bytes = hex::decode(bytes).map_err(|e| {
                log::error!(target: LOG_TARGET, "Error deserializing: {:?}: {:?}, hex::decode {:?}", kind, error, e); 
                error.clone()
                }
            )?;
		}

		midnight_serialize_local::deserialize(bytes.as_slice(), self.network_id).map_err(|e| {
			log::error!(target: LOG_TARGET, "Error deserializing: {:?}: {:?}", kind, e);
			error
		})
	}

	pub fn serialize<T>(&self, value: &T) -> Result<Vec<u8>, LedgerApiError>
	where
		T: midnight_serialize_local::Serializable + SerializableError + 'static,
	{
		let size = midnight_serialize_local::Serializable::serialized_size(value);
		let mut bytes = Vec::with_capacity(size);
		let error = LedgerApiError::Serialization(<T as SerializableError>::error());

		midnight_serialize_local::serialize(value, &mut bytes, self.network_id).map_err(|e| {
			log::error!(target: LOG_TARGET, "Error serializing: {:?}: {:?}", error, e);
			error
		})?;

		Ok(bytes)
	}
}

pub(crate) fn new(network_id: NetworkId) -> Api {
	Api::new(network_id)
}
