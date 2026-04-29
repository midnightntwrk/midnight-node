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
	zswap_local,
};

use ledger_storage_local::db::DB;
use mn_ledger_local::error::{
	DisjointCheckError, EffectsCheckError, FeeCalculationError, MalformedContractDeploy,
	MalformedTransaction, SequencingCheckError,
	SystemTransactionError as LedgerSystemTransactionError, TransactionApplicationError,
	TransactionInvalid,
};
use zswap_local::error::{MalformedOffer, TransactionInvalid as ZswapTransactionInvalid};

// Version-specific helper modules live at `ledger_X::error_ext`. Each version
// supplies its own implementation that matches only the variants it knows
// about, so future upstream additions fall through to the `UnknownError + log`
// arm rather than being silently misclassified.
use super::super::error_ext;

impl<D: DB> From<TransactionInvalid<D>> for InvalidError {
	fn from(error: TransactionInvalid<D>) -> Self {
		use InvalidError as Ie;
		use TransactionInvalid as Ti;

		match error {
			Ti::EffectsMismatch { .. } => Ie::EffectsMismatch,
			Ti::ContractAlreadyDeployed(..) => Ie::ContractAlreadyDeployed,
			Ti::ContractNotPresent(..) => Ie::ContractNotPresent,
			Ti::Zswap(e) => match e {
				ZswapTransactionInvalid::NullifierAlreadyPresent(..) => {
					Ie::ZswapInvalidNullifierAlreadyPresent
				},
				ZswapTransactionInvalid::CommitmentAlreadyPresent(..) => {
					Ie::ZswapInvalidCommitmentAlreadyPresent
				},
				ZswapTransactionInvalid::UnknownMerkleRoot(..) => Ie::ZswapInvalidUnknownMerkleRoot,
				#[allow(unreachable_patterns)]
				other => match error_ext::try_convert_extra_zswap_invalid(other) {
					Ok(ie) => ie,
					Err(other) => {
						log::warn!("Unmapped zswap TransactionInvalid variant: {other:?}");
						Ie::UnknownError
					},
				},
			},
			Ti::Transcript(..) => Ie::Transcript,
			Ti::InsufficientClaimable { .. } => Ie::InsufficientClaimable,
			Ti::VerifierKeyNotFound(..) => Ie::VerifierKeyNotFound,
			Ti::VerifierKeyAlreadyPresent(..) => Ie::VerifierKeyAlreadyPresent,
			Ti::ReplayCounterMismatch(..) => Ie::ReplayCounterMismatch,
			Ti::ReplayProtectionViolation(e) => match e {
				TransactionApplicationError::IntentTtlExpired(..) => {
					Ie::ReplayProtectionViolationIntentTtlExpired
				},
				TransactionApplicationError::IntentTtlTooFarInFuture(..) => {
					Ie::ReplayProtectionViolationIntentTtlTooFarInFuture
				},
				TransactionApplicationError::IntentAlreadyExists => {
					Ie::ReplayProtectionViolationIntentAlreadyExists
				},
			},
			Ti::BalanceCheckOutOfBounds { .. } => Ie::BalanceCheckOutOfBounds,
			Ti::InputNotInUtxos(..) => Ie::InputNotInUtxos,
			Ti::DustDoubleSpend(..) => Ie::DustDoubleSpend,
			Ti::DustDeregistrationNotRegistered(..) => Ie::DustDeregistrationNotRegistered,
			Ti::GenerationInfoAlreadyPresent(..) => Ie::GenerationInfoAlreadyPresent,
			Ti::InvariantViolation(..) => Ie::InvariantViolation,
			Ti::RewardTooSmall { .. } => Ie::RewardTooSmall,
			Ti::DivideByZero => Ie::DivideByZero,
			other => match error_ext::try_convert_extra_invalid(other) {
				Ok(ie) => ie,
				Err(other) => {
					log::warn!("Unmapped TransactionInvalid variant: {other:?}");
					Ie::UnknownError
				},
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
			Lste::ReplayProtectionFailure(e) => match e {
				TransactionApplicationError::IntentTtlExpired(..) => {
					Ste::ReplayProtectionFailureIntentTtlExpired
				},
				TransactionApplicationError::IntentTtlTooFarInFuture(..) => {
					Ste::ReplayProtectionFailureIntentTtlTooFarInFuture
				},
				TransactionApplicationError::IntentAlreadyExists => {
					Ste::ReplayProtectionFailureIntentAlreadyExists
				},
			},
			Lste::IllegalReserveDistribution { .. } => Ste::IllegalReserveDistribution,
			Lste::GenerationInfoAlreadyPresent(_) => Ste::GenerationInfoAlreadyPresent,
			Lste::InvalidBasisPoints(_) => Ste::InvalidBasisPoints,
			Lste::InvariantViolation(_) => Ste::InvariantViolation,
			Lste::TreasuryDisabled => Ste::TreasuryDisabled,
			#[allow(unreachable_patterns)]
			other => match error_ext::try_convert_extra_system_tx(other) {
				Ok(ste) => ste,
				Err(other) => {
					log::warn!("Unmapped SystemTransactionError variant: {other:?}");
					Ste::UnknownError
				},
			},
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
			Mt::Zswap(e) => match e {
				MalformedOffer::InvalidProof(..) => Me::ZswapMalformedInvalidProof,
				MalformedOffer::ContractSentCiphertext { .. } => {
					Me::ZswapMalformedContractSentCiphertext
				},
				MalformedOffer::NonDisjointCoinMerge => Me::ZswapMalformedNonDisjointCoinMerge,
				MalformedOffer::NotNormalized => Me::ZswapMalformedNotNormalized,
			},
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
			Mt::FeeCalculation(e) => match e {
				FeeCalculationError::OutsideTimeToDismiss { .. } => {
					Me::FeeCalculationOutsideTimeToDismiss
				},
				FeeCalculationError::BlockLimitExceeded => Me::FeeCalculationBlockLimitExceeded,
			},
			Mt::InvalidDustRegistrationSignature { .. } => Me::InvalidDustRegistrationSignature,
			Mt::InvalidDustSpendProof { .. } => Me::InvalidDustSpendProof,
			Mt::OutOfDustValidityWindow { .. } => Me::OutOfDustValidityWindow,
			Mt::MultipleDustRegistrationsForKey { .. } => Me::MultipleDustRegistrationsForKey,
			Mt::InsufficientDustForRegistrationFee { .. } => Me::InsufficientDustForRegistrationFee,
			Mt::MalformedContractDeploy(e) => match e {
				MalformedContractDeploy::NonZeroBalance(..) => {
					Me::MalformedContractDeployNonZeroBalance
				},
				MalformedContractDeploy::IncorrectChargedState => {
					Me::MalformedContractDeployIncorrectChargedState
				},
				_ => Me::MalformedContractDeploy,
			},
			Mt::IntentSignatureVerificationFailure => Me::IntentSignatureVerificationFailure,
			Mt::IntentSignatureKeyMismatch => Me::IntentSignatureKeyMismatch,
			Mt::IntentSegmentIdCollision(..) => Me::IntentSegmentIdCollision,
			Mt::IntentAtGuaranteedSegmentId => Me::IntentAtGuaranteedSegmentId,
			Mt::UnsupportedProofVersion { .. } => Me::UnsupportedProofVersion,
			Mt::GuaranteedTranscriptVersion { .. } => Me::GuaranteedTranscriptVersion,
			Mt::FallibleTranscriptVersion { .. } => Me::FallibleTranscriptVersion,
			Mt::TransactionApplicationError(e) => match e {
				TransactionApplicationError::IntentTtlExpired(..) => {
					Me::TransactionApplicationIntentTtlExpired
				},
				TransactionApplicationError::IntentTtlTooFarInFuture(..) => {
					Me::TransactionApplicationIntentTtlTooFarInFuture
				},
				TransactionApplicationError::IntentAlreadyExists => {
					Me::TransactionApplicationIntentAlreadyExists
				},
			},
			Mt::BalanceCheckOutOfBounds { .. } => Me::BalanceCheckOutOfBounds,
			Mt::BalanceCheckConversionFailure { .. } => Me::BalanceCheckConversionFailure,
			Mt::PedersenCheckFailure { .. } => Me::PedersenCheckFailure,
			Mt::EffectsCheckFailure(e) => match e {
				EffectsCheckError::RealCallsSubsetCheckFailure(..) => {
					Me::EffectsCheckRealCallsSubsetCheckFailure
				},
				EffectsCheckError::AllCommitmentsSubsetCheckFailure(..) => {
					Me::EffectsCheckAllCommitmentsSubsetCheckFailure
				},
				EffectsCheckError::RealUnshieldedSpendsSubsetCheckFailure(..) => {
					Me::EffectsCheckRealUnshieldedSpendsSubsetCheckFailure
				},
				EffectsCheckError::ClaimedUnshieldedSpendsUniquenessFailure(..) => {
					Me::EffectsCheckClaimedUnshieldedSpendsUniquenessFailure
				},
				EffectsCheckError::ClaimedCallsUniquenessFailure(..) => {
					Me::EffectsCheckClaimedCallsUniquenessFailure
				},
				EffectsCheckError::NullifiersNEClaimedNullifiers { .. } => {
					Me::EffectsCheckNullifiersNeqClaimedNullifiers
				},
				EffectsCheckError::CommitmentsNEClaimedShieldedReceives { .. } => {
					Me::EffectsCheckCommitmentsNeqClaimedShieldedReceives
				},
			},
			Mt::DisjointCheckFailure(e) => match e {
				DisjointCheckError::ShieldedInputsDisjointFailure { .. } => {
					Me::DisjointCheckShieldedInputsDisjointFailure
				},
				DisjointCheckError::ShieldedOutputsDisjointFailure { .. } => {
					Me::DisjointCheckShieldedOutputsDisjointFailure
				},
				DisjointCheckError::UnshieldedInputsDisjointFailure { .. } => {
					Me::DisjointCheckUnshieldedInputsDisjointFailure
				},
			},
			Mt::SequencingCheckFailure(e) => match e {
				SequencingCheckError::CallSequencingViolation { .. } => {
					Me::SequencingCheckCallSequencingViolation
				},
				SequencingCheckError::SequencingCorrelationViolation { .. } => {
					Me::SequencingCheckSequencingCorrelationViolation
				},
				SequencingCheckError::GuaranteedInFallibleContextViolation { .. } => {
					Me::SequencingCheckGuaranteedInFallibleContextViolation
				},
				SequencingCheckError::FallibleInGuaranteedContextViolation { .. } => {
					Me::SequencingCheckFallibleInGuaranteedContextViolation
				},
				SequencingCheckError::CausalityConstraintViolation { .. } => {
					Me::SequencingCheckCausalityConstraintViolation
				},
				SequencingCheckError::CallHasEmptyTranscripts { .. } => {
					Me::SequencingCheckCallHasEmptyTranscripts
				},
			},
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
