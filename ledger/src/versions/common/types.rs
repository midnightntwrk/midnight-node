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
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode};
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

#[derive(RuntimeDebug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError)]
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

#[derive(RuntimeDebug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError)]
pub enum SystemTransactionError {
	IllegalPayout,
	InsufficientTreasuryFunds,
	CommitmentAlreadyPresent,
	UnknownError,
	ReplayProtectionFailure,
	IllegalReserveDistribution,
	GenerationInfoAlreadyPresent,
	InvalidBasisPoints,
	InvariantViolation,
}

#[derive(RuntimeDebug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError)]
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
	BalanceCheckOverspend,
	UnknownError,
}

#[derive(RuntimeDebug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError)]
pub enum DeserializationError {
	NetworkId,
	Transaction,
	LedgerState,
	ContractAddress,
	PublicKey,
	TypedArenaKey,
	VersionedArenaKey,
	UserAddress,
	SystemTransaction,
	DustPublicKey,
	CNightGeneratesDustActionType,
	CNightGeneratesDustEvent,
}

#[derive(RuntimeDebug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError)]
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
	TypedArenaKey,
	VersionedArenaKey,
	CNightGeneratesDustEvent,
	SystemTransaction,
}

#[derive(RuntimeDebug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError)]
pub enum TransactionError {
	Invalid(InvalidError),
	Malformed(MalformedError),
	SystemTransaction(SystemTransactionError),
}

#[derive(RuntimeDebug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError)]
pub enum LedgerApiError {
	Deserialization(DeserializationError),
	Serialization(SerializationError),
	Transaction(TransactionError),
	LedgerCacheError,
	NoLedgerState,
	LedgerStateScaleDecodingError,
	ContractCallCostError,
	BlockLimitExceededError,
	FeeCalculationError,
	HostApiError,
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
				DeserializationError::TypedArenaKey => {
					write!(f, "Error deserializing: TypedArenaKey")
				},
				DeserializationError::VersionedArenaKey => {
					write!(f, "Error deserializing: VersionedArenaKey")
				},
				DeserializationError::UserAddress => {
					write!(f, "Error deserializing: UserAddress")
				},
				DeserializationError::SystemTransaction => {
					write!(f, "Error deserializing: SystemTransaction")
				},
				DeserializationError::DustPublicKey => {
					write!(f, "Error deserializing: DustPublicKey")
				},
				DeserializationError::CNightGeneratesDustActionType => {
					write!(f, "Error deserializing: CNightGeneratesDustActionType")
				},
				DeserializationError::CNightGeneratesDustEvent => {
					write!(f, "Error deserializing: CNightGeneratesDustEvent")
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
				SerializationError::TypedArenaKey => {
					write!(f, "Error serializing: TypedArenaKey")
				},
				SerializationError::VersionedArenaKey => {
					write!(f, "Error serializing: VersionedArenaKey")
				},
				SerializationError::CNightGeneratesDustEvent => {
					write!(f, "Error serializing: CNightGeneratesDustEvent")
				},
				SerializationError::SystemTransaction => {
					write!(f, "Error serializing: SystemTransaction")
				},
			},
			LedgerApiError::Transaction(error) => match error {
				Invalid(e) => write!(f, "Transaction Error: Invalid({e:?})"),
				Malformed(e) => write!(f, "Transaction Error: Malformed({e:?})"),
				SystemTransaction(e) => write!(f, "Transaction Error: SystemTransaction({e:?})"),
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
			LedgerApiError::BlockLimitExceededError => {
				write!(f, "Error, exceeded block limit during post-block update declaration")
			},
			LedgerApiError::FeeCalculationError => {
				write!(f, "Error, exceeded block limit during transaction application")
			},
			LedgerApiError::HostApiError => {
				write!(f, "Error while processing the transaction in the host API")
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
				DeserializationError::TypedArenaKey => 7,
				DeserializationError::SystemTransaction => 8,
				DeserializationError::DustPublicKey => 9,
				DeserializationError::CNightGeneratesDustActionType => 10,
				DeserializationError::CNightGeneratesDustEvent => 11,
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
				SerializationError::TypedArenaKey => 60,
				SerializationError::CNightGeneratesDustEvent => 61,
				SerializationError::SystemTransaction => 62,
			},
			// Reserved from [100-150)
			LedgerApiError::Transaction(error) => match error {
				Invalid(e) => match e {
					InvalidError::EffectsMismatch => 100,
					InvalidError::ContractAlreadyDeployed => 101,
					InvalidError::ContractNotPresent => 102,
					InvalidError::Zswap => 103,
					InvalidError::Transcript => 104,
					InvalidError::InsufficientClaimable => 105,
					InvalidError::VerifierKeyNotFound => 106,
					InvalidError::VerifierKeyAlreadyPresent => 107,
					InvalidError::ReplayCounterMismatch => 108,
					InvalidError::UnknownError => 109,
				},
				Malformed(e) => match e {
					MalformedError::VerifierKeyNotSet => 110,
					MalformedError::TransactionTooLarge => 111,
					MalformedError::VerifierKeyTooLarge => 112,
					MalformedError::VerifierKeyNotPresent => 113,
					MalformedError::ContractNotPresent => 114,
					MalformedError::InvalidProof => 115,
					MalformedError::BindingCommitmentOpeningInvalid => 116,
					MalformedError::NotNormalized => 117,
					MalformedError::FallibleWithoutCheckpoint => 118,
					MalformedError::ClaimReceiveFailed => 119,
					MalformedError::ClaimSpendFailed => 120,
					MalformedError::ClaimNullifierFailed => 121,
					MalformedError::ClaimCallFailed => 122,
					MalformedError::InvalidSchnorrProof => 123,
					MalformedError::UnclaimedCoinCom => 124,
					MalformedError::UnclaimedNullifier => 125,
					MalformedError::Unbalanced => 126,
					MalformedError::Zswap => 127,
					MalformedError::BuiltinDecode => 128,
					MalformedError::GuaranteedLimit => 129,
					MalformedError::MergingContracts => 130,
					MalformedError::CantMergeTypes => 131,
					MalformedError::ClaimOverflow => 132,
					MalformedError::ClaimCoinMismatch => 133,
					MalformedError::KeyNotInCommittee => 134,
					MalformedError::InvalidCommitteeSignature => 135,
					MalformedError::ThresholdMissed => 136,
					MalformedError::TooManyZswapEntries => 137,
					MalformedError::BalanceCheckOverspend => 138,
					MalformedError::UnknownError => 139,
				},
				SystemTransaction(e) => match e {
					SystemTransactionError::IllegalPayout => 139,
					SystemTransactionError::InsufficientTreasuryFunds => 140,
					SystemTransactionError::CommitmentAlreadyPresent => 141,
					SystemTransactionError::UnknownError => 142,
					SystemTransactionError::ReplayProtectionFailure => 143,
					SystemTransactionError::IllegalReserveDistribution => 144,
					SystemTransactionError::GenerationInfoAlreadyPresent => 145,
					SystemTransactionError::InvalidBasisPoints => 146,
					SystemTransactionError::InvariantViolation => 147,
				},
			},
			// Reserved from [150-255) for future Errors
			LedgerApiError::LedgerCacheError => 150,
			LedgerApiError::NoLedgerState => 151,
			LedgerApiError::LedgerStateScaleDecodingError => 152,
			LedgerApiError::ContractCallCostError => 153,
			LedgerApiError::BlockLimitExceededError => 154,
			LedgerApiError::FeeCalculationError => 155,
			// Error in the Host API, not coming from Ledger
			LedgerApiError::HostApiError => 255,
		}
	}
}

// Implement the `std::error::Error` trait only when `std` is enabled.
#[cfg(feature = "std")]
impl std::error::Error for LedgerApiError {}
