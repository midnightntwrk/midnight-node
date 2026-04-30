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

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::PalletError;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info_derive::TypeInfo;

pub use super::super::BlockContext;

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

#[derive(Debug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError, PartialEq)]
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
	ReplayProtectionViolation,
	BalanceCheckOutOfBounds,
	InputNotInUtxos,
	DustDoubleSpend,
	DustDeregistrationNotRegistered,
	GenerationInfoAlreadyPresent,
	InvariantViolation,
	RewardTooSmall,
	// Granular variants flattened from nested enums in TransactionInvalid.
	// The catch-all parent variants (Zswap, ReplayProtectionViolation) above remain
	// as fallbacks for #[non_exhaustive] upstream additions we don't yet recognise.
	ZswapInvalidNullifierAlreadyPresent,
	ZswapInvalidCommitmentAlreadyPresent,
	ZswapInvalidUnknownMerkleRoot,
	ReplayProtectionViolationIntentTtlExpired,
	ReplayProtectionViolationIntentTtlTooFarInFuture,
	ReplayProtectionViolationIntentAlreadyExists,
	DivideByZero,
	MerkleTreeError,
	ZswapInvalidMerkleTreeError,
	UnknownError,
}

#[derive(Debug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError, PartialEq)]
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
	TreasuryDisabled,
	MerkleTreeError,
	// Granular variants flattened from TransactionApplicationError nested in
	// ReplayProtectionFailure. The ReplayProtectionFailure catch-all above
	// remains as a fallback for upstream additions.
	ReplayProtectionFailureIntentTtlExpired,
	ReplayProtectionFailureIntentTtlTooFarInFuture,
	ReplayProtectionFailureIntentAlreadyExists,
}

#[derive(Debug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError, PartialEq)]
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
	InvalidNetworkId,
	IllegallyDeclaredGuaranteed,
	FeeCalculation,
	InvalidDustRegistrationSignature,
	InvalidDustSpendProof,
	OutOfDustValidityWindow,
	MultipleDustRegistrationsForKey,
	InsufficientDustForRegistrationFee,
	MalformedContractDeploy,
	IntentSignatureVerificationFailure,
	IntentSignatureKeyMismatch,
	IntentSegmentIdCollision,
	IntentAtGuaranteedSegmentId,
	UnsupportedProofVersion,
	GuaranteedTranscriptVersion,
	FallibleTranscriptVersion,
	TransactionApplicationError,
	BalanceCheckOutOfBounds,
	BalanceCheckConversionFailure,
	PedersenCheckFailure,
	EffectsCheckFailure,
	DisjointCheckFailure,
	SequencingCheckFailure,
	InputsNotSorted,
	OutputsNotSorted,
	DuplicateInputs,
	InputsSignaturesLengthMismatch,
	// Granular variants flattened from nested enums in MalformedTransaction.
	// The catch-all parent variants above remain as fallbacks for #[non_exhaustive]
	// upstream additions we don't yet recognise.
	EffectsCheckRealCallsSubsetCheckFailure,
	EffectsCheckAllCommitmentsSubsetCheckFailure,
	EffectsCheckRealUnshieldedSpendsSubsetCheckFailure,
	EffectsCheckClaimedUnshieldedSpendsUniquenessFailure,
	EffectsCheckClaimedCallsUniquenessFailure,
	EffectsCheckNullifiersNeqClaimedNullifiers,
	EffectsCheckCommitmentsNeqClaimedShieldedReceives,
	SequencingCheckCallSequencingViolation,
	SequencingCheckSequencingCorrelationViolation,
	SequencingCheckGuaranteedInFallibleContextViolation,
	SequencingCheckFallibleInGuaranteedContextViolation,
	SequencingCheckCausalityConstraintViolation,
	SequencingCheckCallHasEmptyTranscripts,
	DisjointCheckShieldedInputsDisjointFailure,
	DisjointCheckShieldedOutputsDisjointFailure,
	DisjointCheckUnshieldedInputsDisjointFailure,
	TransactionApplicationIntentTtlExpired,
	TransactionApplicationIntentTtlTooFarInFuture,
	TransactionApplicationIntentAlreadyExists,
	FeeCalculationOutsideTimeToDismiss,
	FeeCalculationBlockLimitExceeded,
	MalformedContractDeployNonZeroBalance,
	MalformedContractDeployIncorrectChargedState,
	ZswapMalformedInvalidProof,
	ZswapMalformedContractSentCiphertext,
	ZswapMalformedNonDisjointCoinMerge,
	ZswapMalformedNotNormalized,
	UnknownError,
}

#[derive(Debug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError, PartialEq)]
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

#[derive(Debug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError, PartialEq)]
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
	ArenaHash,
}

#[derive(Debug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError, PartialEq)]
pub enum TransactionError {
	Invalid(InvalidError),
	Malformed(MalformedError),
	SystemTransaction(SystemTransactionError),
}

#[derive(Debug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError, PartialEq)]
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
	GetTransactionContextError,
	ContractNotPresent,
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
				SerializationError::ArenaHash => {
					write!(f, "Error serializing: ArenaHash")
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
			LedgerApiError::GetTransactionContextError => {
				write!(f, "Error while getting transaction context")
			},
			LedgerApiError::ContractNotPresent => {
				write!(f, "Error, contract is not present")
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
				SerializationError::ArenaHash => 63,
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
					InvalidError::ReplayProtectionViolation => 193,
					InvalidError::BalanceCheckOutOfBounds => 194,
					InvalidError::InputNotInUtxos => 195,
					InvalidError::DustDoubleSpend => 196,
					InvalidError::DustDeregistrationNotRegistered => 197,
					InvalidError::GenerationInfoAlreadyPresent => 198,
					InvalidError::InvariantViolation => 199,
					InvalidError::RewardTooSmall => 200,
					InvalidError::ZswapInvalidNullifierAlreadyPresent => 239,
					InvalidError::ZswapInvalidCommitmentAlreadyPresent => 240,
					InvalidError::ZswapInvalidUnknownMerkleRoot => 241,
					InvalidError::ReplayProtectionViolationIntentTtlExpired => 242,
					InvalidError::ReplayProtectionViolationIntentTtlTooFarInFuture => 243,
					InvalidError::ReplayProtectionViolationIntentAlreadyExists => 244,
					InvalidError::DivideByZero => 248,
					InvalidError::MerkleTreeError => 249,
					InvalidError::ZswapInvalidMerkleTreeError => 250,
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
					MalformedError::InvalidNetworkId => 166,
					MalformedError::IllegallyDeclaredGuaranteed => 167,
					MalformedError::FeeCalculation => 168,
					MalformedError::InvalidDustRegistrationSignature => 169,
					MalformedError::InvalidDustSpendProof => 170,
					MalformedError::OutOfDustValidityWindow => 171,
					MalformedError::MultipleDustRegistrationsForKey => 172,
					MalformedError::InsufficientDustForRegistrationFee => 173,
					MalformedError::MalformedContractDeploy => 174,
					MalformedError::IntentSignatureVerificationFailure => 175,
					MalformedError::IntentSignatureKeyMismatch => 176,
					MalformedError::IntentSegmentIdCollision => 177,
					MalformedError::IntentAtGuaranteedSegmentId => 178,
					MalformedError::UnsupportedProofVersion => 179,
					MalformedError::GuaranteedTranscriptVersion => 180,
					MalformedError::FallibleTranscriptVersion => 181,
					MalformedError::TransactionApplicationError => 182,
					MalformedError::BalanceCheckOutOfBounds => 183,
					MalformedError::BalanceCheckConversionFailure => 184,
					MalformedError::PedersenCheckFailure => 185,
					MalformedError::EffectsCheckFailure => 186,
					MalformedError::DisjointCheckFailure => 187,
					MalformedError::SequencingCheckFailure => 188,
					MalformedError::InputsNotSorted => 189,
					MalformedError::OutputsNotSorted => 190,
					MalformedError::DuplicateInputs => 191,
					MalformedError::InputsSignaturesLengthMismatch => 192,
					MalformedError::EffectsCheckRealCallsSubsetCheckFailure => 212,
					MalformedError::EffectsCheckAllCommitmentsSubsetCheckFailure => 213,
					MalformedError::EffectsCheckRealUnshieldedSpendsSubsetCheckFailure => 214,
					MalformedError::EffectsCheckClaimedUnshieldedSpendsUniquenessFailure => 215,
					MalformedError::EffectsCheckClaimedCallsUniquenessFailure => 216,
					MalformedError::EffectsCheckNullifiersNeqClaimedNullifiers => 217,
					MalformedError::EffectsCheckCommitmentsNeqClaimedShieldedReceives => 218,
					MalformedError::SequencingCheckCallSequencingViolation => 219,
					MalformedError::SequencingCheckSequencingCorrelationViolation => 220,
					MalformedError::SequencingCheckGuaranteedInFallibleContextViolation => 221,
					MalformedError::SequencingCheckFallibleInGuaranteedContextViolation => 222,
					MalformedError::SequencingCheckCausalityConstraintViolation => 223,
					MalformedError::SequencingCheckCallHasEmptyTranscripts => 224,
					MalformedError::DisjointCheckShieldedInputsDisjointFailure => 225,
					MalformedError::DisjointCheckShieldedOutputsDisjointFailure => 226,
					MalformedError::DisjointCheckUnshieldedInputsDisjointFailure => 227,
					MalformedError::TransactionApplicationIntentTtlExpired => 228,
					MalformedError::TransactionApplicationIntentTtlTooFarInFuture => 229,
					MalformedError::TransactionApplicationIntentAlreadyExists => 230,
					MalformedError::FeeCalculationOutsideTimeToDismiss => 231,
					MalformedError::FeeCalculationBlockLimitExceeded => 232,
					MalformedError::MalformedContractDeployNonZeroBalance => 233,
					MalformedError::MalformedContractDeployIncorrectChargedState => 234,
					MalformedError::ZswapMalformedInvalidProof => 235,
					MalformedError::ZswapMalformedContractSentCiphertext => 236,
					MalformedError::ZswapMalformedNonDisjointCoinMerge => 237,
					MalformedError::ZswapMalformedNotNormalized => 238,
					MalformedError::UnknownError => 139,
				},
				SystemTransaction(e) => match e {
					SystemTransactionError::IllegalPayout => 201,
					SystemTransactionError::InsufficientTreasuryFunds => 202,
					SystemTransactionError::CommitmentAlreadyPresent => 203,
					SystemTransactionError::UnknownError => 204,
					SystemTransactionError::ReplayProtectionFailure => 205,
					SystemTransactionError::IllegalReserveDistribution => 206,
					SystemTransactionError::GenerationInfoAlreadyPresent => 207,
					SystemTransactionError::InvalidBasisPoints => 208,
					SystemTransactionError::InvariantViolation => 209,
					SystemTransactionError::TreasuryDisabled => 210,
					SystemTransactionError::MerkleTreeError => 211,
					SystemTransactionError::ReplayProtectionFailureIntentTtlExpired => 245,
					SystemTransactionError::ReplayProtectionFailureIntentTtlTooFarInFuture => 246,
					SystemTransactionError::ReplayProtectionFailureIntentAlreadyExists => 247,
				},
			},
			// Reserved from [150-255) for future Errors
			LedgerApiError::LedgerCacheError => 150,
			LedgerApiError::NoLedgerState => 151,
			LedgerApiError::LedgerStateScaleDecodingError => 152,
			LedgerApiError::ContractCallCostError => 153,
			LedgerApiError::BlockLimitExceededError => 154,
			LedgerApiError::FeeCalculationError => 155,
			LedgerApiError::ContractNotPresent => 156,
			LedgerApiError::GetTransactionContextError => 165,
			// Error in the Host API, not coming from Ledger
			LedgerApiError::HostApiError => 255,
		}
	}
}

// Implement the `std::error::Error` trait only when `std` is enabled.
#[cfg(feature = "std")]
impl std::error::Error for LedgerApiError {}

#[cfg(test)]
mod tests {
	use super::*;
	use parity_scale_codec::Decode;
	use std::collections::HashMap;

	/// Enumerate every `LedgerApiError` value by brute-force SCALE decoding all byte
	/// sequences up to the maximum nesting depth (3 bytes: LedgerApiError → TransactionError
	/// → inner error enum). Only exact-length decodes are kept (no leftover bytes).
	fn all_ledger_api_errors() -> Vec<LedgerApiError> {
		let mut result = Vec::new();
		for depth in 1..=3u32 {
			for n in 0..256u32.pow(depth) {
				let bytes: Vec<u8> = (0..depth).map(|i| ((n >> (8 * i)) & 0xFF) as u8).collect();
				let mut slice: &[u8] = &bytes;
				if let Ok(e) = LedgerApiError::decode(&mut slice)
					&& slice.is_empty()
				{
					result.push(e);
				}
			}
		}
		result
	}

	#[test]
	fn error_codes_are_unique() {
		let mut seen: HashMap<u8, String> = HashMap::new();
		for error in all_ledger_api_errors() {
			let desc = format!("{error}");
			let code: u8 = error.into();
			if let Some(existing) = seen.get(&code) {
				panic!("Error code {code} used by both '{existing}' and '{desc}'");
			}
			seen.insert(code, desc);
		}
	}
}
