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
	ledger_storage_local, mn_ledger_local,
	types::{InvalidError, MalformedError, SystemTransactionError},
};

use ledger_storage_local::db::DB;
use mn_ledger_local::error::{
	MalformedTransaction, SystemTransactionError as LedgerSystemTransactionError,
	TransactionInvalid,
};

impl<D: DB> From<TransactionInvalid<D>> for InvalidError {
	fn from(error: TransactionInvalid<D>) -> Self {
		use InvalidError as Ie;
		use TransactionInvalid as Ti;

		match error {
			Ti::EffectsMismatch { .. } => Ie::EffectsMismatch,
			Ti::ContractAlreadyDeployed(..) => Ie::ContractAlreadyDeployed,
			Ti::ContractNotPresent(..) => Ie::ContractNotPresent,
			Ti::Zswap(..) => Ie::Zswap,
			Ti::Transcript(..) => Ie::Transcript,
			Ti::InsufficientClaimable { .. } => Ie::InsufficientClaimable,
			Ti::VerifierKeyNotFound(..) => Ie::VerifierKeyNotFound,
			Ti::VerifierKeyAlreadyPresent(..) => Ie::VerifierKeyAlreadyPresent,
			Ti::ReplayCounterMismatch(..) => Ie::ReplayCounterMismatch,
			Ti::ReplayProtectionViolation(..) => Ie::ReplayProtectionViolation,
			Ti::BalanceCheckOutOfBounds { .. } => Ie::BalanceCheckOutOfBounds,
			Ti::InputNotInUtxos(..) => Ie::InputNotInUtxos,
			Ti::DustDoubleSpend(..) => Ie::DustDoubleSpend,
			Ti::DustDeregistrationNotRegistered(..) => Ie::DustDeregistrationNotRegistered,
			Ti::GenerationInfoAlreadyPresent(..) => Ie::GenerationInfoAlreadyPresent,
			Ti::InvariantViolation(..) => Ie::InvariantViolation,
			Ti::RewardTooSmall { .. } => Ie::RewardTooSmall,
			other => {
				log::warn!("Unmapped TransactionInvalid variant: {other:?}");
				Ie::UnknownError
			},
		}
	}
}

impl From<LedgerSystemTransactionError> for SystemTransactionError {
	fn from(error: LedgerSystemTransactionError) -> Self {
		use LedgerSystemTransactionError as Lste;
		use SystemTransactionError as Ste;

		match error {
			Lste::IllegalPayout { .. } => Ste::IllegalPayout,
			Lste::InsufficientTreasuryFunds { .. } => Ste::InsufficientTreasuryFunds,
			Lste::CommitmentAlreadyPresent { .. } => Ste::CommitmentAlreadyPresent,
			Lste::ReplayProtectionFailure(_) => Ste::ReplayProtectionFailure,
			Lste::IllegalReserveDistribution { .. } => Ste::IllegalReserveDistribution,
			Lste::GenerationInfoAlreadyPresent(_) => Ste::GenerationInfoAlreadyPresent,
			Lste::InvalidBasisPoints(_) => Ste::InvalidBasisPoints,
			Lste::InvariantViolation(_) => Ste::InvariantViolation,
			Lste::TreasuryDisabled => Ste::TreasuryDisabled,
		}
	}
}

impl<D: DB> From<MalformedTransaction<D>> for MalformedError {
	fn from(error: MalformedTransaction<D>) -> Self {
		use MalformedError as Me;
		use MalformedTransaction as Mt;

		match error {
			Mt::VerifierKeyNotSet { .. } => Me::VerifierKeyNotSet,
			Mt::TransactionTooLarge { .. } => Me::TransactionTooLarge,
			Mt::VerifierKeyTooLarge { .. } => Me::VerifierKeyTooLarge,
			Mt::VerifierKeyNotPresent { .. } => Me::VerifierKeyNotPresent,
			Mt::ContractNotPresent(..) => Me::ContractNotPresent,
			Mt::InvalidProof(..) => Me::InvalidProof,
			Mt::BindingCommitmentOpeningInvalid => Me::BindingCommitmentOpeningInvalid,
			Mt::NotNormalized => Me::NotNormalized,
			Mt::FallibleWithoutCheckpoint => Me::FallibleWithoutCheckpoint,
			Mt::ClaimReceiveFailed(..) => Me::ClaimReceiveFailed,
			Mt::ClaimSpendFailed(..) => Me::ClaimSpendFailed,
			Mt::ClaimNullifierFailed(..) => Me::ClaimNullifierFailed,
			Mt::InvalidSchnorrProof => Me::InvalidSchnorrProof,
			Mt::UnclaimedCoinCom(..) => Me::UnclaimedCoinCom,
			Mt::UnclaimedNullifier(..) => Me::UnclaimedNullifier,
			Mt::Unbalanced(..) => Me::Unbalanced,
			Mt::Zswap(..) => Me::Zswap,
			Mt::BuiltinDecode(..) => Me::BuiltinDecode,
			Mt::CantMergeTypes => Me::CantMergeTypes,
			Mt::ClaimOverflow => Me::ClaimOverflow,
			Mt::ClaimCoinMismatch => Me::ClaimCoinMismatch,
			Mt::KeyNotInCommittee { .. } => Me::KeyNotInCommittee,
			Mt::InvalidCommitteeSignature { .. } => Me::InvalidCommitteeSignature,
			Mt::ThresholdMissed { .. } => Me::ThresholdMissed,
			Mt::TooManyZswapEntries => Me::TooManyZswapEntries,
			Mt::BalanceCheckOverspend { .. } => Me::BalanceCheckOverspend,
			Mt::InvalidNetworkId { .. } => Me::InvalidNetworkId,
			Mt::IllegallyDeclaredGuaranteed => Me::IllegallyDeclaredGuaranteed,
			Mt::FeeCalculation(..) => Me::FeeCalculation,
			Mt::InvalidDustRegistrationSignature { .. } => Me::InvalidDustRegistrationSignature,
			Mt::InvalidDustSpendProof { .. } => Me::InvalidDustSpendProof,
			Mt::OutOfDustValidityWindow { .. } => Me::OutOfDustValidityWindow,
			Mt::MultipleDustRegistrationsForKey { .. } => Me::MultipleDustRegistrationsForKey,
			Mt::InsufficientDustForRegistrationFee { .. } => Me::InsufficientDustForRegistrationFee,
			Mt::MalformedContractDeploy(..) => Me::MalformedContractDeploy,
			Mt::IntentSignatureVerificationFailure => Me::IntentSignatureVerificationFailure,
			Mt::IntentSignatureKeyMismatch => Me::IntentSignatureKeyMismatch,
			Mt::IntentSegmentIdCollision(..) => Me::IntentSegmentIdCollision,
			Mt::IntentAtGuaranteedSegmentId => Me::IntentAtGuaranteedSegmentId,
			Mt::UnsupportedProofVersion { .. } => Me::UnsupportedProofVersion,
			Mt::GuaranteedTranscriptVersion { .. } => Me::GuaranteedTranscriptVersion,
			Mt::FallibleTranscriptVersion { .. } => Me::FallibleTranscriptVersion,
			Mt::TransactionApplicationError(..) => Me::TransactionApplicationError,
			Mt::BalanceCheckOutOfBounds { .. } => Me::BalanceCheckOutOfBounds,
			Mt::BalanceCheckConversionFailure { .. } => Me::BalanceCheckConversionFailure,
			Mt::PedersenCheckFailure { .. } => Me::PedersenCheckFailure,
			Mt::EffectsCheckFailure(..) => Me::EffectsCheckFailure,
			Mt::DisjointCheckFailure(..) => Me::DisjointCheckFailure,
			Mt::SequencingCheckFailure(..) => Me::SequencingCheckFailure,
			Mt::InputsNotSorted(..) => Me::InputsNotSorted,
			Mt::OutputsNotSorted(..) => Me::OutputsNotSorted,
			Mt::DuplicateInputs(..) => Me::DuplicateInputs,
			Mt::InputsSignaturesLengthMismatch { .. } => Me::InputsSignaturesLengthMismatch,
			other => {
				log::warn!("Unmapped MalformedTransaction variant: {other:?}");
				Me::UnknownError
			},
		}
	}
}
