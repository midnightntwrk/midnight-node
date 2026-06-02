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
	Zswap(ZswapInvalidErrorCode),
	Transcript,
	InsufficientClaimable,
	VerifierKeyNotFound,
	VerifierKeyAlreadyPresent,
	ReplayCounterMismatch,
	ReplayProtectionViolation(TransactionApplicationErrorCode),
	BalanceCheckOutOfBounds,
	InputNotInUtxos,
	DustDoubleSpend,
	DustDeregistrationNotRegistered,
	GenerationInfoAlreadyPresent,
	InvariantViolation,
	RewardTooSmall,
	DivideByZero,
	MerkleTreeError,
	/// Catch-all for `#[non_exhaustive]` upstream additions we don't yet recognise.
	UnknownError,
}

#[derive(Debug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError, PartialEq)]
pub enum ZswapInvalidErrorCode {
	NullifierAlreadyPresent,
	CommitmentAlreadyPresent,
	UnknownMerkleRoot,
	MerkleTreeError,
	Unknown,
}

/// Discriminants of upstream `TransactionApplicationError`. Reused under
/// `InvalidError::ReplayProtectionViolation`, `MalformedError::TransactionApplication`,
/// and `SystemTransactionError::ReplayProtectionFailure`.
#[derive(Debug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError, PartialEq)]
pub enum TransactionApplicationErrorCode {
	IntentTtlExpired,
	IntentTtlTooFarInFuture,
	IntentAlreadyExists,
}

#[derive(Debug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError, PartialEq)]
pub enum SystemTransactionError {
	IllegalPayout,
	InsufficientTreasuryFunds,
	CommitmentAlreadyPresent,
	ReplayProtectionFailure(TransactionApplicationErrorCode),
	IllegalReserveDistribution,
	GenerationInfoAlreadyPresent,
	InvalidBasisPoints,
	InvariantViolation,
	TreasuryDisabled,
	MerkleTreeError,
	UnknownError,
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
	Zswap(MalformedZswapErrorCode),
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
	FeeCalculation(FeeCalculationErrorCode),
	InvalidDustRegistrationSignature,
	InvalidDustSpendProof,
	OutOfDustValidityWindow,
	MultipleDustRegistrationsForKey,
	InsufficientDustForRegistrationFee,
	MalformedContractDeploy(MalformedContractDeployErrorCode),
	IntentSignatureVerificationFailure,
	IntentSignatureKeyMismatch,
	IntentSegmentIdCollision,
	IntentAtGuaranteedSegmentId,
	UnsupportedProofVersion,
	GuaranteedTranscriptVersion,
	FallibleTranscriptVersion,
	TransactionApplication(TransactionApplicationErrorCode),
	BalanceCheckOutOfBounds,
	BalanceCheckConversionFailure,
	PedersenCheckFailure,
	EffectsCheck(EffectsCheckErrorCode),
	DisjointCheck(DisjointCheckErrorCode),
	SequencingCheck(SequencingCheckErrorCode),
	InputsNotSorted,
	OutputsNotSorted,
	DuplicateInputs,
	InputsSignaturesLengthMismatch,
	/// Catch-all for `#[non_exhaustive]` upstream additions we don't yet recognise.
	UnknownError,
}

#[derive(Debug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError, PartialEq)]
pub enum MalformedZswapErrorCode {
	InvalidProof,
	ContractSentCiphertext,
	NonDisjointCoinMerge,
	NotNormalized,
	Unknown,
}

#[derive(Debug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError, PartialEq)]
pub enum FeeCalculationErrorCode {
	OutsideTimeToDismiss,
	BlockLimitExceeded,
}

#[derive(Debug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError, PartialEq)]
pub enum MalformedContractDeployErrorCode {
	NonZeroBalance,
	IncorrectChargedState,
	Unknown,
}

#[derive(Debug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError, PartialEq)]
pub enum EffectsCheckErrorCode {
	RealCallsSubsetCheckFailure,
	AllCommitmentsSubsetCheckFailure,
	RealUnshieldedSpendsSubsetCheckFailure,
	ClaimedUnshieldedSpendsUniquenessFailure,
	ClaimedCallsUniquenessFailure,
	NullifiersNeqClaimedNullifiers,
	CommitmentsNeqClaimedShieldedReceives,
}

#[derive(Debug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError, PartialEq)]
pub enum DisjointCheckErrorCode {
	ShieldedInputsDisjointFailure,
	ShieldedOutputsDisjointFailure,
	UnshieldedInputsDisjointFailure,
}

#[derive(Debug, Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PalletError, PartialEq)]
pub enum SequencingCheckErrorCode {
	CallSequencingViolation,
	SequencingCorrelationViolation,
	GuaranteedInFallibleContextViolation,
	FallibleInGuaranteedContextViolation,
	CausalityConstraintViolation,
	CallHasEmptyTranscripts,
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
	BeneficiaryNotFound,
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
			LedgerApiError::BeneficiaryNotFound => {
				write!(f, "Error, beneficiary is not found")
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
					InvalidError::Zswap(zi) => match zi {
						ZswapInvalidErrorCode::NullifierAlreadyPresent => 239,
						ZswapInvalidErrorCode::CommitmentAlreadyPresent => 240,
						ZswapInvalidErrorCode::UnknownMerkleRoot => 241,
						ZswapInvalidErrorCode::MerkleTreeError => 250,
						ZswapInvalidErrorCode::Unknown => 103,
					},
					InvalidError::Transcript => 104,
					InvalidError::InsufficientClaimable => 105,
					InvalidError::VerifierKeyNotFound => 106,
					InvalidError::VerifierKeyAlreadyPresent => 107,
					InvalidError::ReplayCounterMismatch => 108,
					InvalidError::ReplayProtectionViolation(rpv) => match rpv {
						TransactionApplicationErrorCode::IntentTtlExpired => 242,
						TransactionApplicationErrorCode::IntentTtlTooFarInFuture => 243,
						TransactionApplicationErrorCode::IntentAlreadyExists => 244,
					},
					InvalidError::BalanceCheckOutOfBounds => 194,
					InvalidError::InputNotInUtxos => 195,
					InvalidError::DustDoubleSpend => 196,
					InvalidError::DustDeregistrationNotRegistered => 197,
					InvalidError::GenerationInfoAlreadyPresent => 198,
					InvalidError::InvariantViolation => 199,
					InvalidError::RewardTooSmall => 200,
					InvalidError::DivideByZero => 248,
					InvalidError::MerkleTreeError => 249,
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
					MalformedError::Zswap(z) => match z {
						MalformedZswapErrorCode::InvalidProof => 235,
						MalformedZswapErrorCode::ContractSentCiphertext => 236,
						MalformedZswapErrorCode::NonDisjointCoinMerge => 237,
						MalformedZswapErrorCode::NotNormalized => 238,
						MalformedZswapErrorCode::Unknown => 127,
					},
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
					MalformedError::FeeCalculation(f) => match f {
						FeeCalculationErrorCode::OutsideTimeToDismiss => 231,
						FeeCalculationErrorCode::BlockLimitExceeded => 232,
					},
					MalformedError::InvalidDustRegistrationSignature => 169,
					MalformedError::InvalidDustSpendProof => 170,
					MalformedError::OutOfDustValidityWindow => 171,
					MalformedError::MultipleDustRegistrationsForKey => 172,
					MalformedError::InsufficientDustForRegistrationFee => 173,
					MalformedError::MalformedContractDeploy(d) => match d {
						MalformedContractDeployErrorCode::NonZeroBalance => 233,
						MalformedContractDeployErrorCode::IncorrectChargedState => 234,
						MalformedContractDeployErrorCode::Unknown => 174,
					},
					MalformedError::IntentSignatureVerificationFailure => 175,
					MalformedError::IntentSignatureKeyMismatch => 176,
					MalformedError::IntentSegmentIdCollision => 177,
					MalformedError::IntentAtGuaranteedSegmentId => 178,
					MalformedError::UnsupportedProofVersion => 179,
					MalformedError::GuaranteedTranscriptVersion => 180,
					MalformedError::FallibleTranscriptVersion => 181,
					MalformedError::TransactionApplication(t) => match t {
						TransactionApplicationErrorCode::IntentTtlExpired => 228,
						TransactionApplicationErrorCode::IntentTtlTooFarInFuture => 229,
						TransactionApplicationErrorCode::IntentAlreadyExists => 230,
					},
					MalformedError::BalanceCheckOutOfBounds => 183,
					MalformedError::BalanceCheckConversionFailure => 184,
					MalformedError::PedersenCheckFailure => 185,
					MalformedError::EffectsCheck(ec) => match ec {
						EffectsCheckErrorCode::RealCallsSubsetCheckFailure => 212,
						EffectsCheckErrorCode::AllCommitmentsSubsetCheckFailure => 213,
						EffectsCheckErrorCode::RealUnshieldedSpendsSubsetCheckFailure => 214,
						EffectsCheckErrorCode::ClaimedUnshieldedSpendsUniquenessFailure => 215,
						EffectsCheckErrorCode::ClaimedCallsUniquenessFailure => 216,
						EffectsCheckErrorCode::NullifiersNeqClaimedNullifiers => 217,
						EffectsCheckErrorCode::CommitmentsNeqClaimedShieldedReceives => 218,
					},
					MalformedError::DisjointCheck(dc) => match dc {
						DisjointCheckErrorCode::ShieldedInputsDisjointFailure => 225,
						DisjointCheckErrorCode::ShieldedOutputsDisjointFailure => 226,
						DisjointCheckErrorCode::UnshieldedInputsDisjointFailure => 227,
					},
					MalformedError::SequencingCheck(sc) => match sc {
						SequencingCheckErrorCode::CallSequencingViolation => 219,
						SequencingCheckErrorCode::SequencingCorrelationViolation => 220,
						SequencingCheckErrorCode::GuaranteedInFallibleContextViolation => 221,
						SequencingCheckErrorCode::FallibleInGuaranteedContextViolation => 222,
						SequencingCheckErrorCode::CausalityConstraintViolation => 223,
						SequencingCheckErrorCode::CallHasEmptyTranscripts => 224,
					},
					MalformedError::InputsNotSorted => 189,
					MalformedError::OutputsNotSorted => 190,
					MalformedError::DuplicateInputs => 191,
					MalformedError::InputsSignaturesLengthMismatch => 192,
					MalformedError::UnknownError => 139,
				},
				SystemTransaction(e) => match e {
					SystemTransactionError::IllegalPayout => 201,
					SystemTransactionError::InsufficientTreasuryFunds => 202,
					SystemTransactionError::CommitmentAlreadyPresent => 203,
					SystemTransactionError::ReplayProtectionFailure(rpv) => match rpv {
						TransactionApplicationErrorCode::IntentTtlExpired => 245,
						TransactionApplicationErrorCode::IntentTtlTooFarInFuture => 246,
						TransactionApplicationErrorCode::IntentAlreadyExists => 247,
					},
					SystemTransactionError::IllegalReserveDistribution => 206,
					SystemTransactionError::GenerationInfoAlreadyPresent => 207,
					SystemTransactionError::InvalidBasisPoints => 208,
					SystemTransactionError::InvariantViolation => 209,
					SystemTransactionError::TreasuryDisabled => 210,
					SystemTransactionError::MerkleTreeError => 211,
					SystemTransactionError::UnknownError => 204,
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
			LedgerApiError::BeneficiaryNotFound => 157,
			LedgerApiError::GetTransactionContextError => 165,
			// Error in the Host API, not coming from Ledger
			LedgerApiError::HostApiError => 255,
		}
	}
}

/// u8 codes that were assigned in earlier revisions and shipped on `main`,
/// but are no longer produced by `From<LedgerApiError> for u8`. Do not reuse
/// these values for new variants — they were observable to mempool clients
/// (via `InvalidTransaction::Custom`) and reuse would silently collide on
/// the wire. Pick a fresh code instead. See `retired_codes_are_not_reused`.
#[allow(dead_code)]
const RETIRED_U8_ERROR_CODES: &[u8] = &[168, 182, 186, 187, 188, 193, 205];

// Implement the `std::error::Error` trait only when `std` is enabled.
#[cfg(feature = "std")]
impl std::error::Error for LedgerApiError {}

#[cfg(test)]
mod tests {
	use super::*;
	use parity_scale_codec::Decode;
	use std::collections::HashMap;

	/// Enumerate every `LedgerApiError` value via SCALE decoding up to the maximum
	/// nesting depth (4 bytes: LedgerApiError → TransactionError → inner error enum
	/// → sub-enum). A naive 256^4 loop would do 4.3B iterations; instead we recurse
	/// only into prefixes that probe-decode positively when zero-padded to full length
	/// — i.e., carrier prefixes that need more bytes. Variant index 0 is valid in
	/// every sub-enum we use, so a single zero-padded probe is sufficient.
	fn all_ledger_api_errors() -> Vec<LedgerApiError> {
		const MAX_DEPTH: usize = 4;

		fn try_decode_exact(bytes: &[u8]) -> Option<LedgerApiError> {
			let mut slice = bytes;
			match LedgerApiError::decode(&mut slice) {
				Ok(e) if slice.is_empty() => Some(e),
				_ => None,
			}
		}

		fn is_productive_carrier(prefix: &[u8]) -> bool {
			let mut probe = prefix.to_vec();
			probe.resize(MAX_DEPTH, 0);
			try_decode_exact(&probe).is_some()
		}

		fn recurse(prefix: &mut Vec<u8>, result: &mut Vec<LedgerApiError>) {
			if prefix.len() >= MAX_DEPTH {
				return;
			}
			for byte in 0..=255u8 {
				prefix.push(byte);
				if let Some(e) = try_decode_exact(prefix) {
					result.push(e);
				} else if is_productive_carrier(prefix) {
					recurse(prefix, result);
				}
				prefix.pop();
			}
		}

		let mut result = Vec::new();
		let mut prefix = Vec::with_capacity(MAX_DEPTH);
		recurse(&mut prefix, &mut result);
		result
	}

	#[test]
	fn retired_codes_are_not_reused() {
		for error in all_ledger_api_errors() {
			let desc = format!("{error}");
			let code: u8 = error.into();
			assert!(
				!RETIRED_U8_ERROR_CODES.contains(&code),
				"retired error code {code} reused by '{desc}'",
			);
		}
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
