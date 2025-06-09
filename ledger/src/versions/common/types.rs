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

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::PalletError;
use parity_scale_codec::{Decode, Encode};
use scale_info_derive::TypeInfo;
use sp_runtime::RuntimeDebug;

use DeserializationError::{
	ContractAddress as DeserializationContractAddress, LedgerState as DeserializationLedgerState,
	NetworkId, PublicKey, Transaction,
};
use SerializationError::{
	ContractAddress as SerializationContractAddress, ContractState, ContractStateToJson,
	LedgerParameters, LedgerState as SerializationLedgerState, MerkleTreeDigest,
	TransactionIdentifier, UnknownType, ZswapState,
};
use TransactionError::{Invalid, Malformed, SystemTransaction};

#[derive(RuntimeDebug, Encode, Decode, Clone, TypeInfo, PalletError)]
pub enum InvalidError {
	EffectsMismatch,
	ContractAlreadyDeployed,
	ContractNotPresent,
	Zswap,
	Transcript,
	InsufficientClaimable,
	VerifierKeyNotFound,
	VerifierKeyAlreadyPresent,
	ReplayCounterMismatch,
	UnknownError,
}

#[derive(RuntimeDebug, Encode, Decode, Clone, TypeInfo, PalletError)]
pub enum SystemTransactionError {
	IllegalMint,
	InsufficientTreasuryFunds,
	CommitmentAlreadyPresent,
	UnknownError,
	ReplayProtectionFailure,
}

#[derive(RuntimeDebug, Encode, Decode, Clone, TypeInfo, PalletError)]
pub enum MalformedError {
	VerifierKeyNotSet,
	TransactionTooLarge,
	VerifierKeyTooLarge,
	VerifierKeyNotPresent,
	ContractNotPresent,
	InvalidProof,
	BindingCommitmentOpeningInvalid,
	NotNormalized,
	FallibleWithoutCheckpoint,
	ClaimReceiveFailed,
	ClaimSpendFailed,
	ClaimNullifierFailed,
	ClaimCallFailed,
	InvalidSchnorrProof,
	UnclaimedCoinCom,
	UnclaimedNullifier,
	Unbalanced,
	Zswap,
	BuiltinDecode,
	GuaranteedLimit,
	MergingContracts,
	CantMergeTypes,
	ClaimOverflow,
	ClaimCoinMismatch,
	KeyNotInCommittee,
	InvalidCommitteeSignature,
	ThresholdMissed,
	TooManyZswapEntries,
	UnknownError,
}

#[derive(RuntimeDebug, Encode, Decode, Clone, TypeInfo, PalletError)]
pub enum DeserializationError {
	NetworkId,
	Transaction,
	LedgerState,
	ContractAddress,
	PublicKey,
	VersionedArenaKey,
	UserAddress,
}

#[derive(RuntimeDebug, Encode, Decode, Clone, TypeInfo, PalletError)]
pub enum SerializationError {
	TransactionIdentifier,
	ZswapState,
	LedgerState,
	LedgerParameters,
	ContractAddress,
	ContractState,
	ContractStateToJson,
	UnknownType,
	MerkleTreeDigest,
	VersionedArenaKey,
}

#[derive(RuntimeDebug, Encode, Decode, Clone, TypeInfo, PalletError)]
pub enum TransactionError {
	Invalid(InvalidError),
	Malformed(MalformedError),
	SystemTransaction(SystemTransactionError),
}

#[derive(RuntimeDebug, Encode, Decode, Clone, TypeInfo)]
pub enum LedgerApiError {
	Deserialization(DeserializationError),
	Serialization(SerializationError),
	Transaction(TransactionError),
	LedgerCacheError,
	NoLedgerState,
	LedgerStateScaleDecodingError,
	ContractCallCostError,
}

impl core::fmt::Display for LedgerApiError {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			LedgerApiError::Deserialization(error) => match error {
				NetworkId => write!(f, "Error deserializing: NetworkId"),
				Transaction => write!(f, "Error deserializing: Transaction"),
				DeserializationLedgerState => write!(f, "Error deserializing: LedgerState"),
				DeserializationContractAddress => write!(f, "Error deserializing: Address"),
				PublicKey => write!(f, "Error deserializing: PublicKey"),
				DeserializationError::VersionedArenaKey => {
					write!(f, "Error deserializing: VersionedArenaKey")
				},
				DeserializationError::UserAddress => {
					write!(f, "Error deserializing: UserAddress")
				},
			},
			LedgerApiError::Serialization(error) => match error {
				TransactionIdentifier => write!(f, "Error serializing: TransactionIdentifier"),
				ZswapState => write!(f, "Error serializing: ZswapState"),
				SerializationLedgerState => write!(f, "Error serializing: LedgerState"),
				LedgerParameters => write!(f, "Error serializing: LedgerParameters"),
				SerializationContractAddress => write!(f, "Error serializing: Address"),
				ContractState => write!(f, "Error serializing: ContractState"),
				ContractStateToJson => write!(f, "Error serializing: ContractStateToJson"),
				UnknownType => write!(f, "Error serializing: UnknownType"),
				MerkleTreeDigest => write!(f, "Error serializing: MerkleTreeDigest"),
				SerializationError::VersionedArenaKey => {
					write!(f, "Error serializing: VersionedArenaKey")
				},
			},
			LedgerApiError::Transaction(error) => match error {
				Invalid(e) => write!(f, "Transaction Error: Invalid({:?})", e),
				Malformed(e) => write!(f, "Transaction Error: Malformed({:?})", e),
				SystemTransaction(e) => write!(f, "Transaction Error: SystemTransaction({:?})", e),
			},
			LedgerApiError::LedgerCacheError => {
				write!(f, "Error with Ledger Cache: poisoned lock")
			},
			LedgerApiError::NoLedgerState => {
				write!(f, "Error, LedgerState is not present")
			},
			LedgerApiError::LedgerStateScaleDecodingError => {
				write!(f, "Error, it was not possible to SCALE decode the Ledger State")
			},
			LedgerApiError::ContractCallCostError => {
				write!(f, "Error, it was not possible calculate the cost of a Contract Call")
			},
		}
	}
}

impl From<LedgerApiError> for u8 {
	fn from(value: LedgerApiError) -> Self {
		match value {
			// Reserved from [0-50)
			LedgerApiError::Deserialization(error) => match error {
				NetworkId => 0,
				Transaction => 1,
				DeserializationLedgerState => 2,
				DeserializationContractAddress => 3,
				PublicKey => 4,
				DeserializationError::VersionedArenaKey => 5,
				DeserializationError::UserAddress => 6,
			},
			// Reserved from [50-100)
			LedgerApiError::Serialization(error) => match error {
				TransactionIdentifier => 50,
				SerializationLedgerState => 51,
				LedgerParameters => 52,
				SerializationContractAddress => 53,
				ContractState => 54,
				ContractStateToJson => 55,
				ZswapState => 56,
				UnknownType => 57,
				MerkleTreeDigest => 58,
				SerializationError::VersionedArenaKey => 59,
			},
			// Reserved from [100-150)
			LedgerApiError::Transaction(error) => match error {
				Invalid(_) => 100,
				Malformed(_) => 101,
				SystemTransaction(_) => 102,
			},
			// Reserved from [150-255) for future Errors
			LedgerApiError::LedgerCacheError => 150,
			LedgerApiError::NoLedgerState => 151,
			LedgerApiError::LedgerStateScaleDecodingError => 152,
			LedgerApiError::ContractCallCostError => 153,
		}
	}
}

// Implement the `std::error::Error` trait only when `std` is enabled.
#[cfg(feature = "std")]
impl std::error::Error for LedgerApiError {}
