// This file is part of midnight-ledger.
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

use crate::dust::{DustGenerationInfo, DustNullifier, DustRegistration, DustSpend, InitialNonce};
use crate::error::coin::UserAddress;
use crate::structure::MAX_SUPPLY;
use crate::structure::{ClaimKind, ContractOperationVersion, Utxo, UtxoOutput, UtxoSpend};
use base_crypto::cost_model::CostDuration;
use base_crypto::fab::{Alignment, Value};
use base_crypto::hash::HashOutput;
use base_crypto::signatures::VerifyingKey;
use base_crypto::time::Timestamp;
use coin_structure::coin::{self, Commitment, Nullifier, PublicAddress, TokenType};
use coin_structure::contract::ContractAddress;
use derive_where::derive_where;
use onchain_runtime::context::Effects;
use onchain_runtime::error::TranscriptRejected;
use onchain_runtime::state::EntryPointBuf;
use onchain_runtime::transcript::Transcript;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use storage::db::DB;
use transient_crypto::curve::EmbeddedGroupAffine;
use transient_crypto::curve::Fr;
use transient_crypto::merkle_tree::InvalidUpdate;
use transient_crypto::proofs::{KeyLocation, ProvingError, VerifyingError};
use zswap::error::MalformedOffer;
use zswap::{Input, Output};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvariantViolation {
    NightBalance(u128),
}

impl Display for InvariantViolation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            InvariantViolation::NightBalance(x) => {
                write!(
                    f,
                    "total supply of night is {MAX_SUPPLY}, but the transaction would imply a total of: {x}"
                )
            }
        }
    }
}

impl From<InvariantViolation> for SystemTransactionError {
    fn from(e: InvariantViolation) -> Self {
        SystemTransactionError::InvariantViolation(e)
    }
}

impl<D: DB> From<InvariantViolation> for TransactionInvalid<D> {
    fn from(e: InvariantViolation) -> Self {
        TransactionInvalid::InvariantViolation(e)
    }
}

#[derive(Debug)]
pub enum SystemTransactionError {
    IllegalPayout {
        claimed_amount: Option<u128>,
        supply: u128,
        bridged_amount: Option<u128>,
        locked: u128,
    },
    InsufficientTreasuryFunds {
        requested: Option<u128>,
        actual: u128,
        token_type: TokenType,
    },
    CommitmentAlreadyPresent(Commitment),
    ReplayProtectionFailure(TransactionApplicationError),
    IllegalReserveDistribution {
        distributed_amount: u128,
        reserve_supply: u128,
    },
    GenerationInfoAlreadyPresent(DustGenerationInfo),
    InvalidBasisPoints(u32),
    InvariantViolation(InvariantViolation),
    TreasuryDisabled,
    MerkleTreeError(InvalidUpdate),
}

impl Display for SystemTransactionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SystemTransactionError::IllegalPayout {
                claimed_amount: Some(amount),
                supply,
                bridged_amount: None,
                locked: _,
            } => write!(
                f,
                "illegal payout of {amount} native tokens, exceeding remaining supply of {supply}"
            ),
            SystemTransactionError::IllegalPayout {
                claimed_amount: None,
                supply: _,
                bridged_amount: Some(amount),
                locked,
            } => write!(
                f,
                "illegal bridge of {amount} native tokens, exceeding remaining pool of {locked}"
            ),
            SystemTransactionError::IllegalPayout {
                claimed_amount: None,
                supply,
                bridged_amount: None,
                locked,
            } => write!(
                f,
                "illegal bridge or payout of > 2^128 native tokens, exceeding remaining reserve pool of {supply} and/or bridge pool of {locked}"
            ),
            SystemTransactionError::IllegalPayout {
                claimed_amount: Some(amount_claimed),
                supply,
                bridged_amount: Some(amount_bridged),
                locked,
            } => write!(
                f,
                "illegal payout of {amount_claimed} native tokens, exceeding remaining supply of {supply}; illegal bridge of {amount_bridged} native tokens, exceeding remaining pool of {locked}"
            ),
            SystemTransactionError::InsufficientTreasuryFunds {
                requested: Some(requested),
                actual,
                token_type,
            } => write!(
                f,
                "insufficient funds in the treasury; {requested} of token {token_type:?} requested, but only {actual} available"
            ),
            SystemTransactionError::InsufficientTreasuryFunds {
                requested: None,
                actual,
                token_type,
            } => write!(
                f,
                "insufficient funds in the treasury; > 2^128 of token {token_type:?} requested, but only {actual} available"
            ),
            SystemTransactionError::CommitmentAlreadyPresent(cm) => {
                write!(f, "faerie-gold attempt with commitment {:?}", cm)
            }
            SystemTransactionError::ReplayProtectionFailure(e) => {
                write!(f, "Replay protection violation: {e}")
            }
            SystemTransactionError::IllegalReserveDistribution {
                distributed_amount,
                reserve_supply,
            } => {
                write!(
                    f,
                    "illegal distribution of {distributed_amount} reserve tokens, exceeding remaining supply of {reserve_supply}"
                )
            }
            SystemTransactionError::GenerationInfoAlreadyPresent(gen_info) => write!(
                f,
                "attempted to insert new Dust generation info {:?}, but this already exists",
                gen_info
            ),
            SystemTransactionError::InvalidBasisPoints(bp) => {
                write!(
                    f,
                    "cardano_to_midnight_bridge_fee_basis_points must be less than 10_000, but was set to: {bp}"
                )
            }
            SystemTransactionError::InvariantViolation(e) => e.fmt(f),
            SystemTransactionError::TreasuryDisabled => write!(
                f,
                "invalid attempt to access treasury; the treasury is disabled until governance for it has been agreed"
            ),
            SystemTransactionError::MerkleTreeError(e) => e.fmt(f),
        }
    }
}

impl Error for SystemTransactionError {
    fn cause(&self) -> Option<&dyn Error> {
        match self {
            SystemTransactionError::MerkleTreeError(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum TransactionInvalid<D: DB> {
    EffectsMismatch {
        declared: Box<Effects<D>>,
        actual: Box<Effects<D>>,
    },
    ContractAlreadyDeployed(ContractAddress),
    ContractNotPresent(ContractAddress),
    Zswap(zswap::error::TransactionInvalid),
    Transcript(onchain_runtime::error::TranscriptRejected<D>),
    InsufficientClaimable {
        requested: u128,
        claimable: u128,
        claimant: UserAddress,
        kind: ClaimKind,
    },
    VerifierKeyNotFound(EntryPointBuf, ContractOperationVersion),
    VerifierKeyAlreadyPresent(EntryPointBuf, ContractOperationVersion),
    ReplayCounterMismatch(ContractAddress),
    ReplayProtectionViolation(TransactionApplicationError),
    BalanceCheckOutOfBounds {
        token_type: TokenType,
        current_balance: u128,
        operation_value: u128,
        operation: BalanceOperation,
    },
    InputNotInUtxos(Box<Utxo>),
    DustDoubleSpend(DustNullifier),
    DustDeregistrationNotRegistered(UserAddress),
    GenerationInfoAlreadyPresent(DustGenerationInfo),
    InvariantViolation(InvariantViolation),
    RewardTooSmall {
        claimed: u128,
        minimum: u128,
    },
    DivideByZero,
    MerkleTreeError(InvalidUpdate),
}

impl<D: DB> Display for TransactionInvalid<D> {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        use TransactionInvalid::*;
        match self {
            EffectsMismatch { declared, actual } => write!(
                formatter,
                "declared effects {declared:?} don't match computed effects {actual:?}"
            ),
            ContractNotPresent(addr) => {
                write!(formatter, "call to non-existant contract {:?}", addr)
            }
            ContractAlreadyDeployed(addr) => {
                write!(formatter, "contract already deployed {:?}", addr)
            }
            Zswap(err) => err.fmt(formatter),
            Transcript(err) => err.fmt(formatter),
            InsufficientClaimable {
                requested,
                claimable,
                claimant,
                kind,
            } => {
                write!(
                    formatter,
                    "insufficient funds for requested {kind} claim: {requested} tokens of Night requested by {claimant:?}; only {claimable} available"
                )
            }
            VerifierKeyNotFound(ep, ver) => write!(
                formatter,
                "the verifier key for {ep:?} version {ver:?} was not present"
            ),
            VerifierKeyAlreadyPresent(ep, ver) => write!(
                formatter,
                "the verifier key for {ep:?} version {ver:?} was already present"
            ),
            ReplayCounterMismatch(addr) => write!(
                formatter,
                "the signed counter for {addr:?} did not match the expected one; likely replay attack"
            ),
            ReplayProtectionViolation(err) => {
                write!(formatter, "replay protection has been violated: {err:?}")
            }
            BalanceCheckOutOfBounds {
                token_type,
                current_balance,
                operation_value,
                operation,
            } => {
                let (reason_str, to_from, op_str) = match operation {
                    BalanceOperation::Addition => ("overflow", "to", "add"),
                    BalanceOperation::Subtraction => ("underflow", "from", "subtract"),
                };
                write!(
                    formatter,
                    "Balance check failed: couldn't {op_str} {operation_value} {to_from} {current_balance} for token {token_type:?}: {reason_str}"
                )
            }
            InputNotInUtxos(utxo) => write!(formatter, "input missing from utxos set: {:?}", utxo),
            DustDoubleSpend(nullifier) => write!(
                formatter,
                "attempted to double spend Dust UTXO with nullifier {nullifier:?}"
            ),
            DustDeregistrationNotRegistered(addr) => write!(
                formatter,
                "attempted to deregister the Dust address associated with the Night address {addr:?}, but no such registration exists"
            ),
            RewardTooSmall { claimed, minimum } => write!(
                formatter,
                "claimed reward ({claimed} STARs) below payout threshold ({minimum} STARs)"
            ),
            GenerationInfoAlreadyPresent(gen_info) => write!(
                formatter,
                "attempted to insert new Dust generation info {:?}, but this already exists",
                gen_info
            ),
            InvariantViolation(e) => e.fmt(formatter),
            DivideByZero => write!(formatter, "attempted to divide by zero"),
            MerkleTreeError(err) => err.fmt(formatter),
        }
    }
}

impl<D: DB> Error for TransactionInvalid<D> {
    fn cause(&self) -> Option<&dyn Error> {
        match self {
            TransactionInvalid::Zswap(e) => Some(e),
            TransactionInvalid::Transcript(e) => Some(e),
            TransactionInvalid::ReplayProtectionViolation(e) => Some(e),
            _ => None,
        }
    }
}

impl<D: DB> From<zswap::error::TransactionInvalid> for TransactionInvalid<D> {
    fn from(err: zswap::error::TransactionInvalid) -> TransactionInvalid<D> {
        TransactionInvalid::Zswap(err)
    }
}

impl<D: DB> From<onchain_runtime::error::TranscriptRejected<D>> for TransactionInvalid<D> {
    fn from(err: onchain_runtime::error::TranscriptRejected<D>) -> TransactionInvalid<D> {
        TransactionInvalid::Transcript(err)
    }
}

impl<D: DB> From<DustStateError> for TransactionInvalid<D> {
    fn from(err: DustStateError) -> TransactionInvalid<D> {
        match err {
            DustStateError::GenerationInfoAlreadyPresent(gen_info) => {
                TransactionInvalid::GenerationInfoAlreadyPresent(gen_info)
            }
            DustStateError::MerkleTreeError(err) => TransactionInvalid::MerkleTreeError(err),
        }
    }
}

impl From<DustStateError> for SystemTransactionError {
    fn from(err: DustStateError) -> SystemTransactionError {
        match err {
            DustStateError::GenerationInfoAlreadyPresent(gen_info) => {
                SystemTransactionError::GenerationInfoAlreadyPresent(gen_info)
            }
            DustStateError::MerkleTreeError(err) => SystemTransactionError::MerkleTreeError(err),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum FeeCalculationError {
    OutsideTimeToDismiss {
        time_to_dismiss: CostDuration,
        allowed_time_to_dismiss: CostDuration,
        size: u64,
    },
    BlockLimitExceeded,
}

impl Display for FeeCalculationError {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        match self {
            FeeCalculationError::BlockLimitExceeded => write!(
                formatter,
                "exceeded block limit in transaction fee computation"
            ),
            FeeCalculationError::OutsideTimeToDismiss {
                time_to_dismiss,
                allowed_time_to_dismiss,
                size,
            } => write!(
                formatter,
                "exceeded the maximum time to dismiss for transaction size; this transaction would take {time_to_dismiss:?} to dismiss, but given its size of {size} bytes, it may take at most {allowed_time_to_dismiss:?}"
            ),
        }
    }
}

impl Error for FeeCalculationError {}

#[derive(Debug)]
#[non_exhaustive]
pub enum MalformedContractDeploy {
    NonZeroBalance(std::collections::BTreeMap<TokenType, u128>),
    IncorrectChargedState,
}

impl Display for MalformedContractDeploy {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        use MalformedContractDeploy::*;
        match self {
            NonZeroBalance(balance) => {
                let filtered: std::collections::BTreeMap<_, _> =
                    balance.iter().filter(|&(_, value)| *value > 0).collect();
                write!(
                    formatter,
                    "contract deployment was sent with the non-zero balance members: {:?}",
                    filtered
                )
            }
            IncorrectChargedState => write!(
                formatter,
                "contract deployment contained an incorrectly computed map of charged keys"
            ),
        }
    }
}

impl Error for MalformedContractDeploy {}

#[derive(Debug)]
#[non_exhaustive]
#[allow(clippy::large_enum_variant)]
pub enum MalformedTransaction<D: DB> {
    InvalidNetworkId {
        expected: String,
        found: String,
    },
    VerifierKeyNotSet {
        address: ContractAddress,
        operation: EntryPointBuf,
    },
    TransactionTooLarge {
        tx_size: usize,
        limit: u64,
    },
    VerifierKeyTooLarge {
        actual: u64,
        limit: u64,
    },
    VerifierKeyNotPresent {
        address: ContractAddress,
        operation: EntryPointBuf,
    },
    ContractNotPresent(ContractAddress),
    InvalidProof(VerifyingError),
    BindingCommitmentOpeningInvalid,
    NotNormalized,
    FallibleWithoutCheckpoint,
    IllegallyDeclaredGuaranteed,
    ClaimReceiveFailed(coin::Commitment),
    ClaimSpendFailed(coin::Commitment),
    ClaimNullifierFailed(coin::Nullifier),
    InvalidSchnorrProof,
    UnclaimedCoinCom(coin::Commitment),
    UnclaimedNullifier(coin::Nullifier),
    Unbalanced(TokenType, i128), // Think this is unused now?
    Zswap(zswap::error::MalformedOffer),
    BuiltinDecode(base_crypto::fab::InvalidBuiltinDecode),
    FeeCalculation(FeeCalculationError),
    CantMergeTypes,
    ClaimOverflow,
    ClaimCoinMismatch,
    KeyNotInCommittee {
        address: ContractAddress,
        key_id: usize,
    },
    InvalidCommitteeSignature {
        address: ContractAddress,
        key_id: usize,
    },
    InvalidDustRegistrationSignature {
        registration: Box<DustRegistration<(), D>>,
    },
    InvalidDustSpendProof {
        declared_time: Timestamp,
        dust_spend: Box<DustSpend<(), D>>,
    },
    OutOfDustValidityWindow {
        dust_ctime: Timestamp,
        validity_start: Timestamp,
        validity_end: Timestamp,
    },
    MultipleDustRegistrationsForKey {
        key: VerifyingKey,
    },
    InsufficientDustForRegistrationFee {
        registration: Box<DustRegistration<(), D>>,
        available_dust: u128,
    },
    ThresholdMissed {
        address: ContractAddress,
        signatures: usize,
        threshold: usize,
    },
    TooManyZswapEntries,
    MalformedContractDeploy(MalformedContractDeploy),
    IntentSignatureVerificationFailure,
    IntentSignatureKeyMismatch,
    IntentSegmentIdCollision(u16),
    IntentAtGuaranteedSegmentId,
    UnsupportedProofVersion {
        op_version: String,
    },
    GuaranteedTranscriptVersion {
        op_version: String,
    },
    FallibleTranscriptVersion {
        op_version: String,
    },
    TransactionApplicationError(TransactionApplicationError),
    BalanceCheckOutOfBounds {
        token_type: TokenType,
        segment: u16,
        current_balance: i128,
        operation_value: i128,
        operation: BalanceOperation,
    },
    BalanceCheckConversionFailure {
        token_type: TokenType,
        segment: u16,
        operation_value: u128,
    },
    PedersenCheckFailure {
        expected: Box<EmbeddedGroupAffine>,
        calculated: Box<EmbeddedGroupAffine>,
    },
    BalanceCheckOverspend {
        token_type: TokenType,
        segment: u16,
        overspent_value: i128,
    },
    EffectsCheckFailure(EffectsCheckError),
    DisjointCheckFailure(DisjointCheckError<D>),
    SequencingCheckFailure(SequencingCheckError),
    InputsNotSorted(Vec<UtxoSpend>),
    OutputsNotSorted(Vec<UtxoOutput>),
    DuplicateInputs(Vec<UtxoSpend>),
    InputsSignaturesLengthMismatch {
        inputs: Vec<UtxoSpend>,
        erased_signatures: Vec<()>,
    },
}

#[derive(Clone, Debug)]
pub enum SequencingCheckError {
    CallSequencingViolation {
        call_predecessor: u32,
        call_successor: u32,
        call_predecessor_address: ContractAddress,
        call_successor_address: ContractAddress,
    },
    SequencingCorrelationViolation {
        address_1: ContractAddress,
        address_2: ContractAddress,
        call_position_1: u32,
        call_position_2: u32,
    },
    GuaranteedInFallibleContextViolation {
        caller: u32,
        callee: u32,
        caller_address: ContractAddress,
        callee_address: ContractAddress,
    },
    FallibleInGuaranteedContextViolation {
        caller: u32,
        callee: u32,
        caller_address: ContractAddress,
        callee_address: ContractAddress,
    },
    CausalityConstraintViolation {
        call_predecessor: u32,
        call_successor: u32,
        call_predecessor_address: ContractAddress,
        call_successor_address: ContractAddress,
        segment_id_predecessor: u16,
        segment_id_successor: u16,
    },
    CallHasEmptyTranscripts {
        segment_id: u16,
        addr: ContractAddress,
        call_index: u32,
    },
}

impl std::fmt::Display for SequencingCheckError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SequencingCheckError::CallSequencingViolation {
                call_predecessor: call_position_x,
                call_successor: call_position_succ_x,
                call_predecessor_address: call_x_address,
                call_successor_address: call_succ_x_address,
            } => {
                write!(
                    formatter,
                    "sequencing violation: Expected call position {call_position_x} (for call at address: {call_x_address:?}) < {call_position_succ_x} (for call at at address: {call_succ_x_address:?}), but this ordering constraint was violated"
                )
            }
            SequencingCheckError::SequencingCorrelationViolation {
                address_1,
                address_2,
                call_position_1,
                call_position_2,
            } => {
                write!(
                    formatter,
                    "sequencing correlation violation: The order of addresses ({address_1:?} vs {address_2:?}) does not match the order of call positions ({call_position_1} vs {call_position_2}); expected both orderings to be consistent"
                )
            }
            SequencingCheckError::GuaranteedInFallibleContextViolation {
                caller: parent_call_id,
                callee: child_call_id,
                caller_address: parent_address,
                callee_address: child_address,
            } => {
                write!(
                    formatter,
                    "fallible context violation: Call at position {child_call_id} (address: {child_address:?}) contains a guaranteed transcript but is called from a fallible context in call at position {parent_call_id} (address: {parent_address:?})"
                )
            }
            SequencingCheckError::FallibleInGuaranteedContextViolation {
                caller: parent_call_id,
                callee: child_call_id,
                caller_address: parent_address,
                callee_address: child_address,
            } => {
                write!(
                    formatter,
                    "guaranteed context violation: Call at position {child_call_id} (address: {child_address:?}) contains a fallible transcript but is called from a guaranteed context in call at position {parent_call_id} (address: {parent_address:?})"
                )
            }
            SequencingCheckError::CausalityConstraintViolation {
                call_predecessor,
                call_successor,
                call_predecessor_address,
                call_successor_address,
                segment_id_predecessor,
                segment_id_successor,
            } => {
                write!(
                    formatter,
                    "causality violation: Calls must be arranged to ensure causality constraints are met, but found call at segment_id: {segment_id_predecessor} (address: {call_predecessor_address:?}, position: {call_predecessor}) with fallible transcript and call at segment_id: {segment_id_successor} (address: {call_successor_address:?}, position: {call_successor}) with guaranteed transcript"
                )
            }
            SequencingCheckError::CallHasEmptyTranscripts {
                segment_id,
                addr,
                call_index,
            } => {
                write!(
                    formatter,
                    "call composition violation: Calls cannot have empty guaranteed and fallible transcripts, but found violating call at segment_id: {segment_id} (address: {addr:?}, position: {call_index})"
                )
            }
        }
    }
}

impl Error for SequencingCheckError {}

#[derive_where(Clone, Debug)]
pub enum DisjointCheckError<D: DB> {
    ShieldedInputsDisjointFailure {
        shielded_inputs: std::collections::BTreeSet<Input<(), D>>,
        transient_inputs: std::collections::BTreeSet<Input<(), D>>,
    },
    ShieldedOutputsDisjointFailure {
        shielded_outputs: std::collections::BTreeSet<Output<(), D>>,
        transient_outputs: std::collections::BTreeSet<Output<(), D>>,
    },
    UnshieldedInputsDisjointFailure {
        unshielded_inputs: std::collections::BTreeSet<UtxoSpend>,
        offer_inputs: std::collections::BTreeSet<UtxoSpend>,
    },
}

impl<D: DB> std::fmt::Display for DisjointCheckError<D> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            DisjointCheckError::ShieldedInputsDisjointFailure {
                shielded_inputs,
                transient_inputs,
            } => {
                write!(
                    formatter,
                    "shielded_inputs and transient_inputs must be disjoint. shielded_inputs: {shielded_inputs:?}, transient_inputs: {transient_inputs:?}"
                )
            }
            DisjointCheckError::ShieldedOutputsDisjointFailure {
                shielded_outputs,
                transient_outputs,
            } => {
                write!(
                    formatter,
                    "shielded_outputs and transient_outputs must be disjoint. shielded_outputs: {shielded_outputs:?}, transient_outputs: {transient_outputs:?}"
                )
            }
            DisjointCheckError::UnshieldedInputsDisjointFailure {
                unshielded_inputs,
                offer_inputs,
            } => {
                write!(
                    formatter,
                    "unshielded_inputs and offer_inputs must be disjoint. unshielded_inputs: {unshielded_inputs:?}, offer_inputs: {offer_inputs:?}"
                )
            }
        }
    }
}

impl<D: DB> Error for DisjointCheckError<D> {}

#[derive(Clone, Debug)]
pub struct SubsetCheckFailure<T> {
    pub superset: Vec<T>,
    pub subset: Vec<T>,
}

#[derive(Clone, Debug)]
pub enum EffectsCheckError {
    RealCallsSubsetCheckFailure(SubsetCheckFailure<(u16, (ContractAddress, HashOutput, Fr))>),
    AllCommitmentsSubsetCheckFailure(SubsetCheckFailure<(u16, Commitment)>),
    #[allow(clippy::type_complexity)]
    RealUnshieldedSpendsSubsetCheckFailure(
        SubsetCheckFailure<((u16, bool), ((TokenType, PublicAddress), u128))>,
    ),
    ClaimedUnshieldedSpendsUniquenessFailure(Vec<((u16, Commitment), usize)>),
    #[allow(clippy::type_complexity)]
    ClaimedCallsUniquenessFailure(Vec<((u16, (ContractAddress, HashOutput, Fr)), usize)>),
    NullifiersNEClaimedNullifiers {
        nullifiers: Vec<(u16, Nullifier, ContractAddress)>,
        claimed_nullifiers: Vec<(u16, Nullifier, ContractAddress)>,
    },
    CommitmentsNEClaimedShieldedReceives {
        commitments: Vec<(u16, Commitment, ContractAddress)>,
        claimed_shielded_receives: Vec<(u16, Commitment, ContractAddress)>,
    },
}

impl std::fmt::Display for EffectsCheckError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            EffectsCheckError::RealCallsSubsetCheckFailure(e) => write!(
                formatter,
                "claimed_calls is not a subset of real_calls. {}",
                e
            ),
            EffectsCheckError::AllCommitmentsSubsetCheckFailure(e) => write!(
                formatter,
                "claimed_shielded_spends is not a subset of all_commitments. \n {}",
                e
            ),
            EffectsCheckError::RealUnshieldedSpendsSubsetCheckFailure(e) => write!(
                formatter,
                "claimed_unshielded_spends is not a subset of real_unshielded_spends. \n {:?}",
                e
            ),
            EffectsCheckError::ClaimedUnshieldedSpendsUniquenessFailure(items) => {
                write!(formatter, "non-unique spends found: {:?}", items)
            }
            EffectsCheckError::ClaimedCallsUniquenessFailure(items) => {
                write!(formatter, "non-unique claimed calls found: {:?}", items)
            }
            EffectsCheckError::NullifiersNEClaimedNullifiers {
                nullifiers,
                claimed_nullifiers,
            } => write!(
                formatter,
                "all contract-associated nullifiers must be claimed by exactly one instance of the same contract in the same segment. nullifiers: {nullifiers:?}, claimed_nullifiers: {claimed_nullifiers:?}",
            ),
            EffectsCheckError::CommitmentsNEClaimedShieldedReceives {
                commitments,
                claimed_shielded_receives,
            } => write!(
                formatter,
                "all contract-associated commitments must be claimed by exactly one instance of the same contract in the same segment. commitments: {commitments:?}, claimed_shielded_receives: {claimed_shielded_receives:?}"
            ),
        }
    }
}

impl Error for EffectsCheckError {}

impl<T: std::fmt::Debug> Display for SubsetCheckFailure<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let SubsetCheckFailure { superset, subset } = self;
        write!(formatter, "subset: {subset:?} \n superset: {superset:?}")
    }
}

#[derive(Clone, Copy, Debug)]
pub enum BalanceOperation {
    Addition,
    Subtraction,
}

#[derive(Clone, Copy, Debug)]
pub enum BalanceCheckFailureReason {
    TypeConversionFailure,
    OutOfBounds,
}

fn sanitize_network_id(network_id: &str) -> String {
    let char_not_permitted = |ch: char| !ch.is_ascii_alphanumeric() && ch != '-';
    network_id.replace(char_not_permitted, "�")
}

impl<D: DB> Display for MalformedTransaction<D> {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        use MalformedTransaction::*;
        match self {
            InvalidNetworkId { expected, found } => {
                let expected = sanitize_network_id(expected);
                let found = sanitize_network_id(found);
                write!(
                    formatter,
                    "invalid network ID - expect '{expected}' found '{found}'"
                )
            }
            ContractNotPresent(addr) => {
                write!(formatter, "call to non-existant contract {:?}", addr)
            }
            VerifierKeyNotPresent { address, operation } => write!(
                formatter,
                "operation {address:?}/{operation:?} does not have a verifier key",
            ),
            InvalidProof(err) => {
                write!(formatter, "failed to verify proof: ")?;
                err.fmt(formatter)
            }
            TransactionTooLarge { tx_size, limit } => write!(
                formatter,
                "transaction too large (size: {tx_size}, limit: {limit})"
            ),
            VerifierKeyTooLarge { actual, limit } => write!(
                formatter,
                "verifier key for operation too large for deserialization (size: {actual}, limit: {limit})"
            ),
            VerifierKeyNotSet { address, operation } => write!(
                formatter,
                "tried to deploy {:?}/{:?} without a verifier key",
                address, operation
            ),
            BindingCommitmentOpeningInvalid => write!(
                formatter,
                "transaction binding commitment was incorrectly opened"
            ),
            IllegallyDeclaredGuaranteed => write!(
                formatter,
                "guaranteed segment (0) declared in a context where only fallible segments are permitted"
            ),
            FallibleWithoutCheckpoint => write!(
                formatter,
                "fallible transcript did not start with a checkpoint"
            ),
            NotNormalized => write!(formatter, "transaction is not in normal form"),
            ClaimReceiveFailed(com) => write!(
                formatter,
                "failed to claim coin commitment receive for {:?}",
                com
            ),
            ClaimSpendFailed(com) => write!(
                formatter,
                "failed to claim coin commitment spend for {:?}",
                com
            ),
            ClaimNullifierFailed(nul) => write!(
                formatter,
                "failed to claim coin commitment nullifier for {:?}",
                nul
            ),
            InvalidSchnorrProof => write!(
                formatter,
                "failed to verify Fiat-Shamir transformed Schnorr proof"
            ),
            UnclaimedCoinCom(com) => write!(
                formatter,
                "a contract-owned coin output was left unclaimed: {:?}",
                com
            ),
            UnclaimedNullifier(nul) => write!(
                formatter,
                "a contract-owned coin input was unauthorized: {:?}",
                nul
            ),
            Unbalanced(tt, bal) => write!(
                formatter,
                "the transaction has negative balance {} in token type {:?}",
                bal, tt
            ),
            Zswap(err) => err.fmt(formatter),
            BuiltinDecode(err) => err.fmt(formatter),
            FeeCalculation(err) => err.fmt(formatter),
            CantMergeTypes => write!(
                formatter,
                "attempted to merge transaction types that are not mergable"
            ),
            ClaimOverflow => write!(formatter, "claimed coin value overflows deltas"),
            ClaimCoinMismatch => write!(
                formatter,
                "declared coin in ClaimRewards doesn't match real coin"
            ),
            KeyNotInCommittee { address, key_id } => write!(
                formatter,
                "declared signture for key id {key_id} does not correspond to a committee member for contract {address:?}"
            ),
            InvalidCommitteeSignature { address, key_id } => write!(
                formatter,
                "signature for key id {key_id} invalid for contract {address:?}"
            ),
            InvalidDustRegistrationSignature { registration } => write!(
                formatter,
                "failed to verify signature of dust registration: {registration:?}"
            ),
            InvalidDustSpendProof {
                declared_time,
                dust_spend,
            } => write!(
                formatter,
                "dust spend proof failed to verify; this is just as likely a disagreement on dust state on the declared time ({declared_time:?}) as the proof being invalid: {dust_spend:?}"
            ),
            OutOfDustValidityWindow {
                dust_ctime,
                validity_start,
                validity_end,
            } => write!(
                formatter,
                "dust is outside of its validity window (declared time: {dust_ctime:?}, window: [{validity_start:?}, {validity_end:?}])"
            ),
            MultipleDustRegistrationsForKey { key } => write!(
                formatter,
                "multiple dust registrations for key in the same intent: {key:?}"
            ),
            InsufficientDustForRegistrationFee {
                registration,
                available_dust,
            } => write!(
                formatter,
                "insufficient dust to cover registration fee allowance: {available_dust} available, {} requested",
                registration.allow_fee_payment
            ),
            ThresholdMissed {
                address,
                signatures,
                threshold,
            } => write!(
                formatter,
                "threshold update for contract {address:?} does not meet required threshold ({signatures}/{threshold} signatures)"
            ),
            TooManyZswapEntries => write!(
                formatter,
                "excessive Zswap entries exceeding 2^16 safety margin"
            ),
            MalformedContractDeploy(mcd) => write!(formatter, "{:?}", mcd),
            IntentSignatureVerificationFailure => write!(
                formatter,
                "signature verification failed for supplied intent"
            ),
            IntentSignatureKeyMismatch => write!(
                formatter,
                "supplied signing key does not match verifying key"
            ),
            IntentSegmentIdCollision(segment_id) => write!(
                formatter,
                "key (segment_id) collision during intents merge: {:?}",
                segment_id
            ),
            IntentAtGuaranteedSegmentId => {
                write!(formatter, "intents are not allowed at segment_id: 0")
            }
            UnsupportedProofVersion { op_version } => write!(
                formatter,
                "unsupported proof version provided for contract operation: {op_version}"
            ),
            GuaranteedTranscriptVersion { op_version } => write!(
                formatter,
                "unsupported guaranteed transcript version provided for contract operation: {op_version}"
            ),
            FallibleTranscriptVersion { op_version } => write!(
                formatter,
                "unsupported fallible transcript version provided for contract operation: {op_version}"
            ),
            TransactionApplicationError(transaction_application_error) => write!(
                formatter,
                "transaction application error detected during verification: {transaction_application_error}"
            ),
            BalanceCheckOutOfBounds {
                token_type,
                segment,
                current_balance,
                operation_value,
                operation,
            } => {
                let (reason_str, to_from, op_str) = match operation {
                    BalanceOperation::Addition => ("overflow", "to", "add"),
                    BalanceOperation::Subtraction => ("underflow", "from", "subtract"),
                };
                write!(
                    formatter,
                    "Balance check failed: couldn't {op_str} {operation_value} {to_from} {current_balance} for token {token_type:?} in segment {segment}: {reason_str}"
                )
            }
            BalanceCheckConversionFailure {
                token_type,
                segment,
                operation_value,
            } => {
                write!(
                    formatter,
                    "Balance check failed: couldn't convert {operation_value} to type i128 for token {token_type:?} in segment {segment}"
                )
            }
            PedersenCheckFailure {
                expected,
                calculated,
            } => write!(
                formatter,
                "binding commitment calculation mismatch: expected {expected:?}, but calculated {calculated:?}"
            ),
            BalanceCheckOverspend {
                token_type,
                segment,
                overspent_value,
            } => {
                write!(
                    formatter,
                    "invalid balance {overspent_value} for token {token_type:?} in segment {segment}; balance must be positive"
                )
            }
            EffectsCheckFailure(effects_check) => effects_check.fmt(formatter),
            DisjointCheckFailure(disjoint_check) => disjoint_check.fmt(formatter),
            SequencingCheckFailure(sequencing_check) => sequencing_check.fmt(formatter),
            InputsNotSorted(utxo_spends) => {
                write!(
                    formatter,
                    "unshielded offer validation error: inputs are not sorted: {:?}",
                    utxo_spends
                )
            }
            OutputsNotSorted(utxo_outputs) => {
                write!(
                    formatter,
                    "unshielded offer validation error: outputs are not sorted: {:?}",
                    utxo_outputs
                )
            }
            DuplicateInputs(utxo_spends) => {
                write!(
                    formatter,
                    "unshielded offer validation error: found duplicate inputs: {:?}",
                    utxo_spends
                )
            }
            InputsSignaturesLengthMismatch {
                inputs,
                erased_signatures,
            } => {
                write!(
                    formatter,
                    "unshielded offer action validation error: mismatch between number of inputs ({}) and signatures ({})",
                    inputs.len(),
                    erased_signatures.len()
                )
            }
        }
    }
}

impl<D: DB> Error for MalformedTransaction<D> {
    fn cause(&self) -> Option<&dyn Error> {
        match self {
            MalformedTransaction::MalformedContractDeploy(e) => Some(e),
            MalformedTransaction::TransactionApplicationError(e) => Some(e),
            MalformedTransaction::EffectsCheckFailure(e) => Some(e),
            MalformedTransaction::DisjointCheckFailure(e) => Some(e),
            MalformedTransaction::SequencingCheckFailure(e) => Some(e),
            _ => None,
        }
    }
}

impl<D: DB> From<zswap::error::MalformedOffer> for MalformedTransaction<D> {
    fn from(err: zswap::error::MalformedOffer) -> MalformedTransaction<D> {
        MalformedTransaction::Zswap(err)
    }
}

impl<D: DB> From<base_crypto::fab::InvalidBuiltinDecode> for MalformedTransaction<D> {
    fn from(err: base_crypto::fab::InvalidBuiltinDecode) -> MalformedTransaction<D> {
        MalformedTransaction::BuiltinDecode(err)
    }
}

impl<D: DB> From<FeeCalculationError> for MalformedTransaction<D> {
    fn from(err: FeeCalculationError) -> MalformedTransaction<D> {
        MalformedTransaction::FeeCalculation(err)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub enum TransactionApplicationError {
    IntentTtlExpired(Timestamp, Timestamp),
    IntentTtlTooFarInFuture(Timestamp, Timestamp),
    IntentAlreadyExists,
}

impl Display for TransactionApplicationError {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        match self {
            TransactionApplicationError::IntentTtlExpired(ttl, current_block) => write!(
                formatter,
                "Intent TTL has expired. TTL: {:?}, Current block: {:?}",
                ttl, current_block
            ),
            TransactionApplicationError::IntentTtlTooFarInFuture(ttl, max_allowed) => write!(
                formatter,
                "Intent TTL is too far in the future. TTL: {:?}, Maximum allowed: {:?}",
                ttl, max_allowed
            ),
            TransactionApplicationError::IntentAlreadyExists => write!(
                formatter,
                "Intent already exists; duplicate intents are not allowed",
            ),
        }
    }
}

impl Error for TransactionApplicationError {}

#[derive(Debug)]
#[non_exhaustive]
pub enum QueryFailed<D: DB> {
    MissingCall,
    InvalidContract(ContractAddress),
    InvalidInput { value: Value, ty: Alignment },
    Runtime(onchain_runtime::error::TranscriptRejected<D>),
    Zswap(zswap::error::OfferCreationFailed),
}

impl<D: DB> Display for QueryFailed<D> {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        use QueryFailed::*;
        match self {
            MissingCall => write!(formatter, "attempted to run query prior to starting a call"),
            InvalidContract(addr) => write!(formatter, "contract {:?} does not exist", addr),
            InvalidInput { value, ty } => write!(
                formatter,
                "invalid input value {:?} for type {:?}",
                value, ty
            ),
            Runtime(err) => err.fmt(formatter),
            Zswap(err) => err.fmt(formatter),
        }
    }
}

impl<D: DB> Error for QueryFailed<D> {}

impl<D: DB> From<zswap::error::OfferCreationFailed> for QueryFailed<D> {
    fn from(err: zswap::error::OfferCreationFailed) -> QueryFailed<D> {
        QueryFailed::Zswap(err)
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum TransactionConstructionError {
    TransactionEmpty,
    UnfinishedCall {
        address: ContractAddress,
        operation: EntryPointBuf,
    },
    ProofFailed(ProvingError),
    MissingVerifierKey {
        address: ContractAddress,
        operation: EntryPointBuf,
    },
}

impl Display for TransactionConstructionError {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        use TransactionConstructionError::*;
        match self {
            TransactionEmpty => write!(formatter, "attempted to create empty transaction"),
            UnfinishedCall { address, operation } => write!(
                formatter,
                "unfinished call to {:?}/{:?}",
                address, operation
            ),
            ProofFailed(err) => {
                err.fmt(formatter)?;
                write!(formatter, " -- while assembling transaction")
            }
            MissingVerifierKey { address, operation } => write!(
                formatter,
                "attempted to create proof for {:?}/{:?}, which lacks a verifier key",
                address, operation,
            ),
        }
    }
}

impl Error for TransactionConstructionError {}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum TransactionProvingError<D: DB> {
    LeftoverEntries {
        address: ContractAddress,
        entry_point: EntryPointBuf,
        entries: Box<Transcript<D>>,
    },
    RanOutOfEntries {
        address: ContractAddress,
        entry_point: EntryPointBuf,
    },
    MissingKeyset(KeyLocation),
    Proving(ProvingError),
    Tokio(std::io::Error),
}

impl<D: DB> Display for TransactionProvingError<D> {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        use TransactionProvingError::*;
        match self {
            LeftoverEntries {
                address,
                entry_point,
                entries,
            } => write!(
                formatter,
                "too many transcript entries for {:?}/{:?}: {:?} leftover",
                address, entry_point, entries
            ),
            RanOutOfEntries {
                address,
                entry_point,
            } => write!(
                formatter,
                "ran out of transcript entries for {:?}/{:?}",
                address, entry_point
            ),
            MissingKeyset(keyloc) => write!(
                formatter,
                "attempted proof, but couldn't find keys with ID {keyloc:?}"
            ),
            Proving(e) => e.fmt(formatter),
            Tokio(e) => e.fmt(formatter),
        }
    }
}

impl<D: DB> Error for TransactionProvingError<D> {
    fn cause(&self) -> Option<&dyn Error> {
        match self {
            TransactionProvingError::Tokio(e) => Some(e),
            _ => None,
        }
    }
}

impl<D: DB> From<ProvingError> for TransactionProvingError<D> {
    fn from(err: ProvingError) -> TransactionProvingError<D> {
        TransactionProvingError::Proving(err)
    }
}

#[derive(Debug)]
pub enum PartitionFailure<D: DB> {
    Transcript(TranscriptRejected<D>),
    NonForest,
    GuaranteedOnlyUnsatisfied,
    IllegalSegmentZero,
    Merge(MalformedOffer),
}

impl<D: DB> From<TranscriptRejected<D>> for PartitionFailure<D> {
    fn from(err: TranscriptRejected<D>) -> Self {
        PartitionFailure::Transcript(err)
    }
}

impl<D: DB> Display for PartitionFailure<D> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            PartitionFailure::NonForest => {
                write!(f, "call graph was not a forest; cannot partition")
            }
            PartitionFailure::Transcript(e) => e.fmt(f),
            PartitionFailure::GuaranteedOnlyUnsatisfied => write!(
                f,
                "transaction could not be constructed to satisfy 'guaranteed only' segment specifier: the call was too expensive"
            ),
            PartitionFailure::IllegalSegmentZero => {
                write!(f, "illegal manual specification of segment 0")
            }
            PartitionFailure::Merge(e) => write!(f, "failed zswap merge: {e}"),
        }
    }
}

impl<D: DB> Error for PartitionFailure<D> {
    fn cause(&self) -> Option<&dyn Error> {
        match self {
            PartitionFailure::Transcript(err) => Some(err),
            PartitionFailure::Merge(err) => Some(err),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum EventReplayError {
    NonLinearInsertion {
        expected_next: u64,
        received: u64,
        tree_name: &'static str,
    },
    DtimeUpdateForUntracked {
        updated: u64,
        tracked_up_to_index: u64,
    },
    EventForPastTime {
        synced: Timestamp,
        event: Timestamp,
    },
    MerkleTreeError(InvalidUpdate),
}

impl Display for EventReplayError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use EventReplayError::*;
        match self {
            NonLinearInsertion {
                expected_next,
                received,
                tree_name,
            } => write!(
                f,
                "values inserted non-linearly into {tree_name} tree; expected to insert index {expected_next}, but received {received}."
            ),
            DtimeUpdateForUntracked {
                updated,
                tracked_up_to_index,
            } => write!(
                f,
                "attempted to update the dtime of a dust generation entry that isn't tracked; tracking up to index {tracked_up_to_index}, but received an update for {updated}"
            ),
            EventForPastTime { synced, event } => write!(
                f,
                "received an event with a timestamp prior to the time already synced to (synced to: {synced:?}, event time: {event:?})"
            ),
            MerkleTreeError(err) => err.fmt(f),
        }
    }
}

impl Error for EventReplayError {
    fn cause(&self) -> Option<&dyn Error> {
        match self {
            EventReplayError::MerkleTreeError(err) => Some(err),
            _ => None,
        }
    }
}

impl From<InvalidUpdate> for EventReplayError {
    fn from(err: InvalidUpdate) -> Self {
        EventReplayError::MerkleTreeError(err)
    }
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum DustLocalStateError {
    GenerationIndexNotFound {
        generation_index: u64,
    },
    NonLinearInsertion {
        expected_next: u64,
        received: u64,
        tree_name: &'static str,
    },
    WrongGenerationInfo {
        generation_index: u64,
    },
    CommitmentIndexNotFound {
        commitment_index: u64,
    },
    BackingNightNotFound {
        backing_night: InitialNonce,
    },
    MerkleTreeError(InvalidUpdate),
}

impl Display for DustLocalStateError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use DustLocalStateError::*;
        match self {
            GenerationIndexNotFound { generation_index } => write!(
                f,
                "failed to find generation info for generation index {generation_index}"
            ),
            NonLinearInsertion {
                expected_next,
                received,
                tree_name,
            } => write!(
                f,
                "values inserted non-linearly into {tree_name} tree; expected to insert index {expected_next}, but received {received}."
            ),
            WrongGenerationInfo { generation_index } => write!(
                f,
                "generation info for generation index {generation_index} is invalid"
            ),
            CommitmentIndexNotFound { commitment_index } => write!(
                f,
                "failed to find commitment for commitment index {commitment_index}"
            ),
            BackingNightNotFound { backing_night } => write!(
                f,
                "failed to find generation info for backing night {:?}",
                backing_night.0
            ),
            MerkleTreeError(err) => err.fmt(f),
        }
    }
}

impl Error for DustLocalStateError {}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum DustStateError {
    GenerationInfoAlreadyPresent(DustGenerationInfo),
    MerkleTreeError(InvalidUpdate),
}

impl Display for DustStateError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use DustStateError::*;
        match self {
            GenerationInfoAlreadyPresent(gen_info) => write!(
                f,
                "attempted to insert new Dust generation info {:?}, but this already exists",
                gen_info
            ),
            MerkleTreeError(err) => err.fmt(f),
        }
    }
}

impl Error for DustStateError {}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct BlockLimitExceeded;

impl Display for BlockLimitExceeded {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "exceeded block limit during post-block update declaration"
        )
    }
}

impl Error for BlockLimitExceeded {}
