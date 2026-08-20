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

use crate::dust::DustParameters;
use crate::error::MalformedContractDeploy;
use crate::error::{
    BalanceOperation, DisjointCheckError, EffectsCheckError, MalformedTransaction,
    SequencingCheckError, SubsetCheckFailure, TransactionApplicationError,
};
use crate::primitive::MultiSet;
use crate::structure::{
    BindingKind, ClaimRewardsTransaction, ContractAction, ContractCall, ContractDeploy,
    ErasedIntent, FEE_TOKEN, Intent, LedgerParameters, LedgerState, MaintenanceUpdate,
    PedersenDowngradeable, ProofKind, SingleUpdate, StandardTransaction, Transaction,
    UnshieldedOffer,
};
use crate::structure::{SignatureKind, VerifiedTransaction};
use crate::utils::SortedIter;
use crate::verify::MalformedTransaction::IntentSignatureVerificationFailure;
use base_crypto::hash::HashOutput;
use base_crypto::signatures::VerifyingKey;
use base_crypto::time::{Duration, Timestamp};
use coin_structure::coin::PublicAddress;
use coin_structure::coin::{Commitment, Nullifier, TokenType};
use coin_structure::contract::ContractAddress;
use onchain_runtime::ops::Op;
use onchain_runtime::state::{
    ChargedState, ContractMaintenanceAuthority, ContractOperation, ContractState, EntryPoint,
    EntryPointBuf,
};
use onchain_runtime::transcript::Transcript;
use serialize::{Serializable, Tagged};
use sha2::Digest;
use sha2::Sha256;
use std::collections::{BTreeSet, VecDeque};
use std::ops::Deref;
use std::ops::Mul;
use storage::Storable;
use storage::arena::{ArenaHash, Sp};
use storage::db::DB;
use transient_crypto::commitment::Pedersen;
use transient_crypto::curve::{EmbeddedFr, EmbeddedGroupAffine, Fr};
use transient_crypto::hash::hash_to_curve;
use transient_crypto::merkle_tree::MerkleTreeDigest;
use transient_crypto::repr::FieldRepr;
use zswap::Transient;

pub trait ContractStateExt<D: DB> {
    #[allow(clippy::result_large_err)]
    fn well_formed(&self, address: ContractAddress) -> Result<(), MalformedTransaction<D>>;
}

pub trait StateReference<D: DB> {
    fn stateless_check(
        &self,
        check: impl FnOnce() -> Result<(), MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>>;
    fn param_check(
        &self,
        always: bool,
        check: impl FnOnce(&LedgerParameters) -> Result<(), MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>>;
    fn op_check(
        &self,
        contract: ContractAddress,
        entry_point: &EntryPointBuf,
        check: impl FnOnce(&ContractOperation) -> Result<(), MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>>;
    fn maintenance_check(
        &self,
        contract: ContractAddress,
        check: impl FnOnce(&ContractMaintenanceAuthority) -> Result<(), MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>>;
    fn generationless_fee_availability_check(
        &self,
        parent_intent: &ErasedIntent<D>,
        night_key: &VerifyingKey,
        check: impl FnOnce(u128) -> Result<(), MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>>;
    fn dust_spend_check(
        &self,
        ctime: Timestamp,
        // Dust parameters, commitment tree root associated with the timestamp, generation root associated with the timestamp
        check: impl FnOnce(
            DustParameters,
            MerkleTreeDigest,
            MerkleTreeDigest,
        ) -> Result<(), MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>>;
    fn network_check(&self, network: &str) -> Result<(), MalformedTransaction<D>>;
    fn ref_state_hash(&self) -> ArenaHash<D::Hasher>;
}

fn get_op<D: DB>(
    state: &LedgerState<D>,
    contract: ContractAddress,
    entry_point: &EntryPointBuf,
) -> Result<Sp<ContractOperation, D>, MalformedTransaction<D>> {
    let cstate = state
        .index(contract)
        .ok_or_else(|| MalformedTransaction::ContractNotPresent(contract))?;
    cstate
        .operations
        .get(entry_point)
        .ok_or_else(|| MalformedTransaction::VerifierKeyNotPresent {
            address: contract,
            operation: entry_point.clone(),
        })
}

impl<D: DB> StateReference<D> for LedgerState<D> {
    fn stateless_check(
        &self,
        check: impl FnOnce() -> Result<(), MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>> {
        check()
    }
    fn param_check(
        &self,
        _always: bool,
        check: impl FnOnce(&LedgerParameters) -> Result<(), MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>> {
        check(&self.parameters)
    }
    fn op_check(
        &self,
        contract: ContractAddress,
        entry_point: &EntryPointBuf,
        check: impl FnOnce(&ContractOperation) -> Result<(), MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>> {
        check(&*get_op(self, contract, entry_point)?)
    }
    fn maintenance_check(
        &self,
        contract: ContractAddress,
        check: impl FnOnce(&ContractMaintenanceAuthority) -> Result<(), MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>> {
        check(
            &self
                .index(contract)
                .ok_or_else(|| MalformedTransaction::ContractNotPresent(contract))?
                .maintenance_authority,
        )
    }
    fn generationless_fee_availability_check(
        &self,
        parent_intent: &ErasedIntent<D>,
        night_key: &VerifyingKey,
        check: impl FnOnce(u128) -> Result<(), MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>> {
        let availability = self.dust.generationless_fee_availability(
            &self.utxo,
            parent_intent,
            night_key,
            &self.parameters.dust,
        );
        check(availability)
    }
    fn dust_spend_check(
        &self,
        ctime: Timestamp,
        // Dust parameters, commitment tree root associated with the timestamp, generation root associated with the timestamp
        check: impl FnOnce(
            DustParameters,
            MerkleTreeDigest,
            MerkleTreeDigest,
        ) -> Result<(), MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>> {
        let params = self.parameters.dust;
        let commitment_root = self
            .dust
            .utxo
            .root_history
            .get(ctime)
            .map(|x| x.0)
            .unwrap_or_default();
        let generation_root = self
            .dust
            .generation
            .root_history
            .get(ctime)
            .map(|x| x.0)
            .unwrap_or_default();
        check(params, commitment_root, generation_root)
    }
    fn network_check(&self, network: &str) -> Result<(), MalformedTransaction<D>> {
        if self.network_id == network {
            Ok(())
        } else {
            Err(MalformedTransaction::InvalidNetworkId {
                expected: self.network_id.clone(),
                found: network.into(),
            })
        }
    }
    fn ref_state_hash(&self) -> ArenaHash<D::Hasher> {
        self.state_hash()
    }
}

pub struct RevalidationReference<D: DB> {
    pub previously_validated_state: LedgerState<D>,
    pub new_state: LedgerState<D>,
}

impl<D: DB> StateReference<D> for RevalidationReference<D> {
    fn stateless_check(
        &self,
        _: impl FnOnce() -> Result<(), MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>> {
        // We've already done all of the stateless checks in the first check.
        Ok(())
    }
    fn param_check(
        &self,
        always: bool,
        check: impl FnOnce(&LedgerParameters) -> Result<(), MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>> {
        if always || self.previously_validated_state.parameters != self.new_state.parameters {
            check(&self.new_state.parameters)
        } else {
            Ok(())
        }
    }
    fn op_check(
        &self,
        contract: ContractAddress,
        entry_point: &EntryPointBuf,
        check: impl FnOnce(&ContractOperation) -> Result<(), MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>> {
        let old_op = get_op(&self.previously_validated_state, contract, entry_point)?;
        let new_op = get_op(&self.new_state, contract, entry_point)?;
        if old_op == new_op {
            Ok(())
        } else {
            check(&new_op)
        }
    }
    fn maintenance_check(
        &self,
        contract: ContractAddress,
        check: impl FnOnce(&ContractMaintenanceAuthority) -> Result<(), MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>> {
        let auth_old = &self
            .previously_validated_state
            .index(contract)
            .ok_or_else(|| MalformedTransaction::ContractNotPresent(contract))?
            .maintenance_authority;
        let auth_new = &self
            .new_state
            .index(contract)
            .ok_or_else(|| MalformedTransaction::ContractNotPresent(contract))?
            .maintenance_authority;
        if auth_old == auth_new {
            Ok(())
        } else {
            check(auth_new)
        }
    }
    fn generationless_fee_availability_check(
        &self,
        parent_intent: &ErasedIntent<D>,
        night_key: &VerifyingKey,
        check: impl FnOnce(u128) -> Result<(), MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>> {
        let availability = self.new_state.dust.generationless_fee_availability(
            &self.new_state.utxo,
            parent_intent,
            night_key,
            &self.new_state.parameters.dust,
        );
        check(availability)
    }
    fn dust_spend_check(
        &self,
        ctime: Timestamp,
        // Dust parameters, commitment tree root associated with the timestamp, generation root associated with the timestamp
        check: impl FnOnce(
            DustParameters,
            MerkleTreeDigest,
            MerkleTreeDigest,
        ) -> Result<(), MalformedTransaction<D>>,
    ) -> Result<(), MalformedTransaction<D>> {
        let params_old = self.previously_validated_state.parameters.dust;
        let commitment_root_old = self
            .previously_validated_state
            .dust
            .utxo
            .root_history
            .get(ctime)
            .map(|x| x.0)
            .unwrap_or_default();
        let generation_root_old = self
            .previously_validated_state
            .dust
            .generation
            .root_history
            .get(ctime)
            .map(|x| x.0)
            .unwrap_or_default();
        let params_new = self.new_state.parameters.dust;
        let commitment_root_new = self
            .new_state
            .dust
            .utxo
            .root_history
            .get(ctime)
            .map(|x| x.0)
            .unwrap_or_default();
        let generation_root_new = self
            .new_state
            .dust
            .generation
            .root_history
            .get(ctime)
            .map(|x| x.0)
            .unwrap_or_default();
        if (params_old, commitment_root_old, generation_root_old)
            == (params_new, commitment_root_new, generation_root_new)
        {
            Ok(())
        } else {
            check(params_new, commitment_root_new, generation_root_new)
        }
    }
    fn network_check(&self, network: &str) -> Result<(), MalformedTransaction<D>> {
        if self.new_state.network_id == network {
            Ok(())
        } else {
            Err(MalformedTransaction::InvalidNetworkId {
                expected: self.new_state.network_id.clone(),
                found: network.into(),
            })
        }
    }
    fn ref_state_hash(&self) -> ArenaHash<D::Hasher> {
        self.new_state.state_hash()
    }
}

impl<D: DB> ContractStateExt<D> for ContractState<D> {
    #[instrument(skip(self))]
    #[allow(clippy::result_large_err)]
    fn well_formed(&self, address: ContractAddress) -> Result<(), MalformedTransaction<D>> {
        for a in self.operations.iter() {
            a.1.well_formed(address, a.0.as_ref())?;
        }
        if self.maintenance_authority.counter != 0 {
            return Err(MalformedTransaction::NotNormalized);
        }
        trace!("well formed");
        Ok(())
    }
}

pub trait ContractOperationExt<D: DB> {
    fn well_formed(
        &self,
        address: ContractAddress,
        operation: EntryPoint,
    ) -> Result<(), MalformedTransaction<D>>;
}

impl<D: DB> ContractOperationExt<D> for ContractOperation {
    #[instrument(skip(self))]
    fn well_formed(
        &self,
        address: ContractAddress,
        operation: EntryPoint,
    ) -> Result<(), MalformedTransaction<D>> {
        match &self.v2 {
            Some(_) => Ok(()),
            None => {
                warn!("no verifier key set");
                Err(MalformedTransaction::VerifierKeyNotSet {
                    address,
                    operation: operation.into(),
                })
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProofVerificationMode {
    Real,
    #[cfg(feature = "mock-verify")]
    CalibratedMock,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct WellFormedStrictness {
    pub enforce_balancing: bool,
    pub verify_native_proofs: bool,
    pub verify_contract_proofs: bool,
    pub verify_signatures: bool,
    pub enforce_limits: bool,
    pub proof_verification_mode: ProofVerificationMode,
}

impl Default for WellFormedStrictness {
    fn default() -> Self {
        WellFormedStrictness {
            enforce_balancing: true,
            verify_native_proofs: true,
            verify_contract_proofs: true,
            verify_signatures: true,
            enforce_limits: true,
            proof_verification_mode: ProofVerificationMode::Real,
        }
    }
}

fn no_duplicates<T>(iter: T) -> bool
where
    T: IntoIterator,
    T::Item: Ord + std::hash::Hash,
{
    let mut uniq = BTreeSet::new();
    iter.into_iter().all(|x| uniq.insert(x))
}

impl<S: SignatureKind<D>, D: DB> UnshieldedOffer<S, D> {
    pub fn well_formed(
        self,
        segment_id: u16,
        parent: &ErasedIntent<D>,
    ) -> Result<impl Fn() -> Result<(), MalformedTransaction<D>>, MalformedTransaction<D>> {
        let ins = Vec::from(&self.inputs);
        if !ins.is_sorted() {
            return Err(MalformedTransaction::InputsNotSorted(ins));
        }

        let outs = Vec::from(&self.outputs);
        if !outs.is_sorted() {
            return Err(MalformedTransaction::OutputsNotSorted(outs));
        }

        if !no_duplicates(&ins) {
            return Err(MalformedTransaction::DuplicateInputs(ins));
        }

        let verify_input = parent.data_to_sign(segment_id);

        Ok(move || {
            if self.inputs.len() != self.signatures.len() {
                return Err(MalformedTransaction::InputsSignaturesLengthMismatch {
                    inputs: Vec::from(&self.inputs),
                    erased_signatures: self.signatures.iter().map(|_| ()).collect(),
                });
            }

            for (inp, sig) in self.inputs.iter_deref().zip(self.signatures.iter_deref()) {
                S::signature_verify(&verify_input, inp.owner.clone(), sig)
                    .then_some(())
                    .ok_or(IntentSignatureVerificationFailure)?;
            }
            Ok(())
        })
    }
}

impl<
    S: SignatureKind<D>,
    P: ProofKind<D>,
    B: Storable<D> + Serializable + PedersenDowngradeable<D> + BindingKind<S, P, D>,
    D: DB,
> Intent<S, P, B, D>
{
    pub fn well_formed(
        &self,
        segment_id: u16,
        ref_state: &impl StateReference<D>,
        strictness: WellFormedStrictness,
        tblock: Timestamp,
    ) -> Result<(), MalformedTransaction<D>>
    where
        UnshieldedOffer<S, D>: Clone,
    {
        if segment_id == SEGMENT_GUARANTEED {
            return Err(MalformedTransaction::IntentAtGuaranteedSegmentId);
        }

        let erased = self.erase_proofs().erase_signatures();
        let erased_ref = &erased;

        ref_state.stateless_check(|| {
            self.guaranteed_unshielded_offer
                .as_ref()
                .map(|offer| {
                    B::when_sealed(offer.deref().clone().well_formed(segment_id, erased_ref))
                })
                .transpose()?;
            self.fallible_unshielded_offer
                .as_ref()
                .map(|offer| {
                    B::when_sealed(offer.deref().clone().well_formed(segment_id, erased_ref))
                })
                .transpose()?;
            self.binding_commitment
                .valid(&Intent::challenge_pre_for(self, segment_id))
        })?;

        self.actions
            .iter()
            .try_for_each(|action| action.well_formed(ref_state, strictness, erased_ref))?;
        self.dust_actions
            .as_ref()
            .map(|dust_actions| {
                dust_actions.well_formed(ref_state, strictness, segment_id, erased_ref, tblock)
            })
            .transpose()?;
        Ok(())
    }
}

const SEGMENT_GUARANTEED: u16 = 0;

impl<S: SignatureKind<D>, D: DB> ClaimRewardsTransaction<S, D> {
    fn well_formed(&self) -> Result<(), MalformedTransaction<D>> {
        S::signature_verify(
            &self.erase_signatures().data_to_sign(),
            self.owner.clone(),
            &self.signature,
        )
        .then_some(())
        .ok_or(IntentSignatureVerificationFailure)
    }
}

impl<
    S: SignatureKind<D>,
    P: ProofKind<D> + Storable<D>,
    B: Storable<D> + Serializable + PedersenDowngradeable<D> + BindingKind<S, P, D> + Tagged,
    D: DB,
> Transaction<S, P, B, D>
where
    Transaction<S, P, B, D>: Serializable,
{
    // All checks that can be done without a state.
    #[instrument(skip(self, ref_state), fields(self = ?self.transaction_hash().0, ref_state = ?ref_state.ref_state_hash()))]
    /// Checks if a transaction is well-formed, performing all checks possible
    /// with a moderately stale reference state.
    ///
    /// `enforce_balancing` being set to [None] permits imbalanced transactions,
    /// while [Some]([usize]) informs the balance check of the serialized
    /// transaction size to use for transaction size cost calculation.
    pub fn well_formed(
        &self,
        ref_state: &impl StateReference<D>,
        strictness: WellFormedStrictness,
        tblock: Timestamp,
    ) -> Result<VerifiedTransaction<D>, MalformedTransaction<D>> {
        ref_state.param_check(false, |params| {
            if strictness.enforce_limits
                && Transaction::serialized_size(self) as u64 > params.limits.transaction_byte_limit
            {
                Err(MalformedTransaction::TransactionTooLarge {
                    tx_size: Transaction::serialized_size(self),
                    limit: params.limits.transaction_byte_limit,
                })
            } else {
                Ok(())
            }
        })?;

        match self {
            Transaction::Standard(stx) => {
                ref_state.network_check(&stx.network_id)?;
                ref_state.param_check(true, |params| {
                    stx.ttl_check_weak(tblock, params.global_ttl)
                        .map_err(MalformedTransaction::TransactionApplicationError)
                })?;
                ref_state.stateless_check(|| {
                    stx.guaranteed_coins
                        .as_ref()
                        .map(|x| {
                            P::zswap_well_formed(x, 0).map_err(MalformedTransaction::<D>::from)
                        })
                        .transpose()?;
                    for seg_x_offer in stx.fallible_coins.iter() {
                        if *seg_x_offer.0 == 0 {
                            return Err(MalformedTransaction::IllegallyDeclaredGuaranteed);
                        }
                        P::zswap_well_formed(&seg_x_offer.1.clone(), *seg_x_offer.0.deref())
                            .map_err(|e: zswap::error::MalformedOffer| {
                                MalformedTransaction::<D>::Zswap(e)
                            })?;
                    }
                    stx.disjoint_check()?;
                    stx.effects_check()?;
                    stx.sequencing_check()?;
                    stx.pedersen_check()
                })?;

                ref_state.param_check(false, |params| {
                    let fees = match self.fees(params, true) {
                        Ok(fees) => fees,
                        Err(e) => {
                            if strictness.enforce_balancing {
                                return Err(MalformedTransaction::FeeCalculation(e));
                            } else {
                                0
                            }
                        }
                    };
                    stx.balancing_check(strictness, fees)
                })?;

                for segment_intent in stx.intents.sorted_iter() {
                    if *segment_intent.0 == 0 {
                        return Err(MalformedTransaction::IllegallyDeclaredGuaranteed);
                    }
                    segment_intent.1.well_formed(
                        *segment_intent.0,
                        ref_state,
                        strictness,
                        tblock,
                    )?;
                }

                debug!("transaction well-formed");
                Ok(VerifiedTransaction {
                    inner: self.erase_proofs().erase_signatures(),
                    hash: self.transaction_hash(),
                })
            }
            Transaction::ClaimRewards(mtx) => {
                ref_state.network_check(&mtx.network_id)?;
                ref_state.stateless_check(|| {
                    B::when_sealed(Ok(
                        // There's no point in checking this unless we actually care about signature verification
                        move || <ClaimRewardsTransaction<S, D> as Clone>::clone(mtx).well_formed(),
                    ))
                })?;
                Ok(VerifiedTransaction {
                    inner: self.erase_proofs().erase_signatures(),
                    hash: self.transaction_hash(),
                })
            }
        }
    }
}

impl<S: SignatureKind<D>, P: ProofKind<D>, B: Storable<D>, D: DB> Transaction<S, P, B, D>
where
    Transaction<S, P, B, D>: Serializable,
{
    pub fn balance(
        &self,
        fees: Option<u128>,
    ) -> Result<std::collections::BTreeMap<(TokenType, u16), i128>, MalformedTransaction<D>> {
        match self {
            Self::Standard(stx) => stx.balance(fees),
            Self::ClaimRewards(_) => Ok(std::collections::BTreeMap::new()),
        }
    }
}

impl<S: SignatureKind<D>, P: ProofKind<D>, B: Storable<D>, D: DB> StandardTransaction<S, P, B, D> {
    pub fn balance(
        &self,
        fees: Option<u128>,
    ) -> Result<std::collections::BTreeMap<(TokenType, u16), i128>, MalformedTransaction<D>> {
        self.balance_maybe_deltas_only(fees, false)
    }

    fn balance_maybe_deltas_only(
        &self,
        fees: Option<u128>,
        deltas_only: bool,
    ) -> Result<std::collections::BTreeMap<(TokenType, u16), i128>, MalformedTransaction<D>> {
        let mut res: std::collections::BTreeMap<(TokenType, u16), i128> =
            std::collections::BTreeMap::new();

        fn interpret(operation: BalanceOperation) -> fn(i128, i128) -> Option<i128> {
            match operation {
                BalanceOperation::Addition => |x: i128, y: i128| x.checked_add(y),
                BalanceOperation::Subtraction => |x: i128, y: i128| x.checked_sub(y),
            }
        }

        fn update_balance<T, D: DB>(
            res: &mut std::collections::BTreeMap<(TokenType, u16), i128>,
            token_type: TokenType,
            segment: u16,
            value: T,
            operation: BalanceOperation,
        ) -> Result<(), MalformedTransaction<D>>
        where
            T: TryInto<i128>,
            T: Into<u128>,
            T: Copy,
            T::Error: std::fmt::Debug,
        {
            let bal = res.entry((token_type, segment)).or_insert(0);
            let current_balance = *bal;

            let casted_value = value.try_into().map_err(|_| {
                MalformedTransaction::BalanceCheckConversionFailure {
                    token_type,
                    segment,
                    operation_value: value.into(),
                }
            })?;

            *bal = interpret(operation)(current_balance, casted_value).ok_or(
                MalformedTransaction::BalanceCheckOutOfBounds {
                    token_type,
                    segment,
                    current_balance,
                    operation_value: casted_value,
                    operation,
                },
            )?;

            Ok(())
        }

        // Subtract any fees from the segment 0 `FEE_TOKEN` balance
        if let Some(fee) = fees {
            update_balance(&mut res, FEE_TOKEN, 0, fee, BalanceOperation::Subtraction)?;
        }

        for (segment, intent) in self.intents.sorted_iter() {
            for dust_spend in intent.dust_actions.iter().flat_map(|da| da.spends.iter()) {
                update_balance(
                    &mut res,
                    TokenType::Dust,
                    0,
                    dust_spend.v_fee,
                    BalanceOperation::Addition,
                )?;
            }
            for dust_reg in intent
                .dust_actions
                .iter()
                .flat_map(|da| da.registrations.iter())
            {
                update_balance(
                    &mut res,
                    TokenType::Dust,
                    0,
                    dust_reg.allow_fee_payment,
                    BalanceOperation::Addition,
                )?;
            }
            for (segment, offer) in [
                (0, intent.guaranteed_unshielded_offer.clone()),
                (*segment, intent.fallible_unshielded_offer.clone()),
            ] {
                if let Some(inputs) = offer.clone().map(|o| o.inputs.clone()) {
                    for inp in inputs.iter() {
                        update_balance(
                            &mut res,
                            TokenType::Unshielded(inp.type_),
                            segment,
                            inp.value,
                            BalanceOperation::Addition,
                        )?;
                    }
                }

                if let Some(outputs) = offer.map(|o| o.outputs.clone()) {
                    for out in outputs.iter() {
                        update_balance(
                            &mut res,
                            TokenType::Unshielded(out.type_),
                            segment,
                            out.value,
                            BalanceOperation::Subtraction,
                        )?;
                    }
                }
            }

            if deltas_only {
                continue;
            }
            for call in intent.calls() {
                let transcripts = call
                    .guaranteed_transcript
                    .iter()
                    .map(|t| (0, t))
                    .chain(call.fallible_transcript.iter().map(|t| (*segment, t)));

                for (segment, transcript) in transcripts {
                    for (pre_token, val) in transcript.effects.shielded_mints.clone() {
                        let tt = call.address.custom_shielded_token_type(pre_token);
                        update_balance(
                            &mut res,
                            TokenType::Shielded(tt),
                            segment,
                            val,
                            BalanceOperation::Addition,
                        )?;
                    }

                    for (pre_token, val) in transcript.effects.unshielded_mints.clone() {
                        let tt = call.address.custom_unshielded_token_type(pre_token);
                        update_balance(
                            &mut res,
                            TokenType::Unshielded(tt),
                            segment,
                            val,
                            BalanceOperation::Addition,
                        )?;
                    }

                    // NOTE: This is an input *to* the contract, so an
                    // output of the transaction.
                    for (tt, val) in transcript.effects.unshielded_inputs.clone() {
                        update_balance(&mut res, tt, segment, val, BalanceOperation::Subtraction)?;
                    }

                    // NOTE: This is an output *from* the contract, so an
                    // input to the transaction.
                    for (tt, val) in transcript.effects.unshielded_outputs.clone() {
                        update_balance(&mut res, tt, segment, val, BalanceOperation::Addition)?;
                    }
                }
            }
        }

        for (segment, offer) in self
            .fallible_coins
            .sorted_iter()
            .map(|seg_x_offer| (*seg_x_offer.0.deref(), seg_x_offer.1.deref().clone()))
            .chain(self.guaranteed_coins.iter().map(|o| (0, o.deref().clone())))
        {
            for delta in offer.deltas.iter() {
                let (val, op) = if delta.value < 0 {
                    (
                        i128::unsigned_abs(delta.value),
                        BalanceOperation::Subtraction,
                    )
                } else {
                    (delta.value as u128, BalanceOperation::Addition)
                };
                update_balance(
                    &mut res,
                    TokenType::Shielded(delta.token_type),
                    segment,
                    val,
                    op,
                )?;
            }
        }

        Ok(res)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct CallNode {
    pub segment_id: u16,
    pub addr: ContractAddress,
    pub call_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct CallKey {
    pub addr: ContractAddress,
    pub ep_hash: HashOutput,
    pub commitment: Fr,
}

fn causality_check<D: DB>(
    guaranteed_calls: &BTreeSet<CallNode>,
    fallible_calls: &BTreeSet<CallNode>,
    adjacencies: std::collections::BTreeMap<CallNode, Vec<CallNode>>,
) -> Result<(), MalformedTransaction<D>> {
    let mut queue: VecDeque<CallNode> = fallible_calls.iter().cloned().collect();

    // A transitive closure is implied here.
    // By checking neighbors of every fallible call, we
    // implicitly check transivity.
    //
    // This works because every neighbor is in the set `fallible_calls ∪ guaranteed_calls`.
    while let Some(curr) = queue.pop_front() {
        if let Some(neighbors) = adjacencies.get(&curr) {
            for &succ in neighbors {
                if guaranteed_calls.contains(&succ) {
                    return Err(MalformedTransaction::SequencingCheckFailure(
                        SequencingCheckError::CausalityConstraintViolation {
                            call_predecessor: curr.call_index,
                            call_successor: succ.call_index,
                            call_predecessor_address: curr.addr,
                            call_successor_address: succ.addr,
                            segment_id_predecessor: curr.segment_id,
                            segment_id_successor: succ.segment_id,
                        },
                    ));
                }
            }
        }
    }

    Ok(())
}

fn call_sequencing_check<P: ProofKind<D>, D: DB>(
    context_call_position: u32,
    contextual_call: &ContractCall<P, D>,
    call_lookup: &std::collections::BTreeMap<(ContractAddress, HashOutput, Fr), Vec<u32>>,
) -> Result<Vec<(u32, u64, ContractAddress)>, MalformedTransaction<D>> {
    // A vec of all calls to whom the current call in context (`call1`)
    // makes calls to, which also reside in _this_ intent
    let mut sequenced_sub_calls: Vec<(u32, u64, ContractAddress)> = Vec::new();

    // For all `claimed_contract_calls`, look up that claimed call's position in
    // _this_ intent. If its position is not greater than _this_ call's position (`call1`'s position, that is)
    // we're in an invalid case. In other words, `call1` is calling `call2`, so `call2` must be sequentially/positionally
    // greater than `call1`.
    for transcript in contextual_call
        .guaranteed_transcript
        .as_ref()
        .into_iter()
        .chain(contextual_call.fallible_transcript.as_ref())
    {
        for claimed in transcript.effects.claimed_contract_calls.iter() {
            let (seq, addr, ep, cc) = claimed.deref().into_inner();
            if let Some(cids) = call_lookup.get(&(addr, ep, cc)) {
                for cid2 in cids {
                    if context_call_position >= *cid2 {
                        return Err(MalformedTransaction::SequencingCheckFailure(
                            SequencingCheckError::CallSequencingViolation {
                                call_predecessor: context_call_position,
                                call_successor: *cid2,
                                call_predecessor_address: contextual_call.address,
                                call_successor_address: addr,
                            },
                        ));
                    }
                    sequenced_sub_calls.push((*cid2, seq, addr));
                }
            }
        }
    }

    Ok(sequenced_sub_calls)
}

fn sequencing_correlation_check<D: DB>(
    contextual_call_address: ContractAddress,
    mut sequenced_sub_calls: Vec<(u32, u64, ContractAddress)>,
) -> Result<(), MalformedTransaction<D>> {
    sequenced_sub_calls.sort_by_key(|&(cid, s, _)| (s, cid));

    // View our `sequenced_sub_calls` as windowed pairs.
    //
    // `sequence_id` must correlate with call position.
    // In other words, if the first call is sequentially less than the second call,
    // it must also be positionally less than the second call.
    for w in sequenced_sub_calls.windows(2) {
        let (cid_a, seq_a, addr_a) = w[0];
        let (cid_b, seq_b, _) = w[1];

        if !(seq_a < seq_b && cid_a < cid_b) {
            return Err(MalformedTransaction::SequencingCheckFailure(
                SequencingCheckError::SequencingCorrelationViolation {
                    address_1: contextual_call_address,
                    address_2: addr_a,
                    call_position_1: cid_a,
                    call_position_2: cid_b,
                },
            ));
        }
    }

    Ok(())
}

fn sequencing_context_check<P: ProofKind<D>, D: DB>(
    adjacencies: &mut std::collections::BTreeMap<CallNode, Vec<CallNode>>,
    segment_id: u16,
    calls_in_intent: std::collections::BTreeMap<u32, &ContractCall<P, D>>,
    callers_for_addr: std::collections::BTreeMap<CallKey, Vec<(CallNode, bool)>>,
) -> Result<(), MalformedTransaction<D>> {
    // If a calls `b`, `b` must be contained within the 'lifetime' of the
    // call instruction in `a`.
    // Concretely, this means that:
    // - If the call to `b` in in `a`'s guaranteed section, it *must*
    //   contain only a guaranteed section.
    // - If the call to `b` in in `a`'s fallible section, it *must*
    //   contain only a fallible section.
    for (cid, call) in &calls_in_intent {
        let this_node: CallNode = CallNode {
            segment_id,
            addr: call.address,
            call_index: *cid,
        };
        let is_guaranteed: bool = call.guaranteed_transcript.is_some();
        let is_fallible: bool = call.fallible_transcript.is_some();

        let lookup_key = CallKey {
            addr: call.address,
            ep_hash: call.entry_point.ep_hash(),
            commitment: call.communication_commitment,
        };
        if let Some(callers) = callers_for_addr.get(&lookup_key) {
            for &(caller_node, caller_guar) in callers {
                // If there is a caller to _this_ call, and the caller is guaranteed but _this_ call is
                // fallible, we're in violation of a constraint
                // Specified as:
                // - If the call to `b` is in `a`'s guaranteed section, it *must*
                //   contain only a guaranteed section.
                if caller_guar && is_fallible {
                    return Err(MalformedTransaction::SequencingCheckFailure(
                        SequencingCheckError::FallibleInGuaranteedContextViolation {
                            caller: caller_node.call_index,
                            callee: *cid,
                            caller_address: caller_node.addr,
                            callee_address: call.address,
                        },
                    ));
                }
                // If there is a caller to _this_ call, and the caller is fallible but _this_ call is
                // guaranteed, we're in violation of a constraint
                // Specified as:
                // - If the call to `b` in in `a`'s fallible section, it *must*
                //   contain only a fallible section.
                if !caller_guar && is_guaranteed {
                    return Err(MalformedTransaction::SequencingCheckFailure(
                        SequencingCheckError::GuaranteedInFallibleContextViolation {
                            caller: caller_node.call_index,
                            callee: *cid,
                            caller_address: caller_node.addr,
                            callee_address: call.address,
                        },
                    ));
                }

                // We have an adjacency: caller -> callee
                adjacencies.entry(caller_node).or_default().push(this_node);
            }
        }
    }

    Ok(())
}

// TODO: Document this clearly
#[allow(clippy::type_complexity)]
fn relate_nodes<P: ProofKind<D>, D: DB>(
    guaranteed_calls: &mut BTreeSet<CallNode>,
    fallible_calls: &mut BTreeSet<CallNode>,
    calls_by_address: &mut std::collections::BTreeMap<ContractAddress, Vec<CallNode>>,
    adjacencies: &mut std::collections::BTreeMap<CallNode, Vec<CallNode>>,
    segment_id: u16,
    calls_in_intent: &std::collections::BTreeMap<u32, &ContractCall<P, D>>,
) -> Result<std::collections::BTreeMap<CallKey, Vec<(CallNode, bool)>>, MalformedTransaction<D>> {
    // A map from (address, entry_point_hash, commitment) to caller nodes (+ guaranteed flag)
    let mut callers_for_addr: std::collections::BTreeMap<CallKey, Vec<(CallNode, bool)>> =
        std::collections::BTreeMap::new();

    for (cid, call) in calls_in_intent {
        let this_node: CallNode = CallNode {
            segment_id,
            addr: call.address,
            call_index: *cid,
        };

        let is_guaranteed = call.guaranteed_transcript.is_some();
        let is_fallible = call.fallible_transcript.is_some();

        if is_guaranteed {
            guaranteed_calls.insert(this_node);
        }
        if is_fallible {
            fallible_calls.insert(this_node);
        }
        // TODO: We should change `ContractCall` to make this invalid state unrepresentable rather than erroring, something like (but with a named type rather than `These`):
        // pub struct ContractCall<P: ProofKind<D>, D: DB> {
        //     pub address: ContractAddress,
        //     pub entry_point: EntryPointBuf,
        //     // nb: Vector is *not* sorted
        //     pub transcripts: These<Sp<Transcript<D>, D>, Sp<Transcript<D>, D>>,
        //     pub communication_commitment: Fr,
        //     pub proof: P::Proof,
        // }
        //
        // I've only not done it this way because I'm in a rush, and it'd require loooots of mechanical changes.
        if !(is_guaranteed || is_fallible) {
            return Err(MalformedTransaction::<D>::SequencingCheckFailure(
                SequencingCheckError::CallHasEmptyTranscripts {
                    segment_id,
                    addr: call.address,
                    call_index: *cid,
                },
            ));
        }

        // If a contract is in two intents, the prior precedes the latter
        // TODO: Thomas said:
        // > This seems to have pretty bad worst-case time complexity. Probably O(n^2) in the number of intents, if each has a call to the same contract?
        // > Do we actually need to record an adjacency for each previous node, or just the one immediately preceding this one?
        // I've made the change, but let's verify that it's OK
        if let Some(&prev_node) = calls_by_address.get(&call.address).and_then(|v| v.last()) {
            adjacencies.entry(prev_node).or_default().push(this_node);
        }

        // Add this node to the list of nodes that share the ContractAddress at this key
        calls_by_address
            .entry(call.address)
            .or_default()
            .push(this_node);

        // Record any (a -> b) relationships in this intent, so that later we
        // can add “a causally precedes b” into adjacencies.
        for (transcript, is_guaranteed) in call
            .guaranteed_transcript
            .iter()
            .map(|gt| (gt, true))
            .chain(call.fallible_transcript.iter().map(|ft| (ft, false)))
        {
            for claimed in transcript.effects.claimed_contract_calls.iter() {
                let (_, addr, ep_hash, cc) = claimed.deref().into_inner();
                callers_for_addr
                    .entry(CallKey {
                        addr,
                        ep_hash,
                        commitment: cc,
                    })
                    .or_default()
                    .push((this_node, is_guaranteed));
            }
        }
    }

    Ok(callers_for_addr)
}

impl<
    S: SignatureKind<D>,
    P: ProofKind<D>,
    B: Storable<D> + Serializable + PedersenDowngradeable<D> + BindingKind<S, P, D>,
    D: DB,
> StandardTransaction<S, P, B, D>
{
    #[allow(clippy::mutable_key_type)]
    fn disjoint_check(&self) -> Result<(), MalformedTransaction<D>> {
        let mut shielded_inputs = BTreeSet::new();
        let mut shielded_outputs = BTreeSet::new();
        let mut unshielded_inputs = BTreeSet::new();
        let shielded_offers = self
            .guaranteed_coins
            .clone()
            .map(|x| x.deref().clone())
            .into_iter()
            .chain(self.fallible_coins.values());

        for offer in shielded_offers {
            let mut inputs: BTreeSet<_> = offer.inputs.iter_deref().cloned().collect();
            inputs.extend(offer.transient.iter_deref().map(Transient::as_input));

            let mut outputs: BTreeSet<_> = offer.outputs.iter_deref().cloned().collect();
            outputs.extend(offer.transient.iter_deref().map(Transient::as_output));

            if !(shielded_inputs.is_disjoint(&inputs)) {
                return Err(MalformedTransaction::DisjointCheckFailure(
                    DisjointCheckError::ShieldedInputsDisjointFailure {
                        shielded_inputs: shielded_inputs
                            .into_iter()
                            .map(|x| x.erase_proof())
                            .collect(),
                        transient_inputs: inputs.into_iter().map(|x| x.erase_proof()).collect(),
                    },
                ));
            }

            if !(shielded_outputs.is_disjoint(&outputs)) {
                return Err(MalformedTransaction::DisjointCheckFailure(
                    DisjointCheckError::ShieldedOutputsDisjointFailure {
                        shielded_outputs: shielded_outputs
                            .into_iter()
                            .map(|x| x.erase_proof())
                            .collect(),
                        transient_outputs: outputs.into_iter().map(|x| x.erase_proof()).collect(),
                    },
                ));
            }

            shielded_inputs = shielded_inputs.union(&inputs).cloned().collect();
            shielded_outputs = shielded_outputs.union(&outputs).cloned().collect();
        }

        let intents = self.intents();

        let unshielded_offers = intents.flat_map(|(_, intent)| {
            [
                intent.guaranteed_unshielded_offer.clone(),
                intent.fallible_unshielded_offer.clone(),
            ]
            .into_iter()
        });
        for offer in unshielded_offers {
            let inputs = offer
                .map(|o| o.inputs.iter_deref().cloned().collect())
                .unwrap_or(BTreeSet::new());
            if !(unshielded_inputs.is_disjoint(&inputs)) {
                return Err(MalformedTransaction::<D>::DisjointCheckFailure(
                    DisjointCheckError::UnshieldedInputsDisjointFailure {
                        unshielded_inputs,
                        offer_inputs: inputs,
                    },
                ));
            }
            unshielded_inputs = unshielded_inputs.union(&inputs).cloned().collect();
        }
        Ok(())
    }

    pub fn sequencing_check(&self) -> Result<(), MalformedTransaction<D>> {
        let mut adjacencies: std::collections::BTreeMap<CallNode, Vec<CallNode>> =
            std::collections::BTreeMap::new();

        // ALL guaranteed calls
        let mut guaranteed_calls: BTreeSet<CallNode> = BTreeSet::new();

        // ALL fallible calls
        let mut fallible_calls: BTreeSet<CallNode> = BTreeSet::new();

        let mut intents_sorted: Vec<(u16, Intent<_, _, _, _>)> = self
            .intents
            .iter()
            .map(|sid_x_intent| (*sid_x_intent.0, sid_x_intent.1.deref().clone()))
            .collect();

        // Our checks demand sorted `segment_id`s
        intents_sorted.sort_by_key(|&(sid, _)| sid);

        // This map essentially keeps track of which intents share top level calls to the same address
        // Note that the `ContractAddress` in the key == the `ContractAddress` in the value
        // To be super clear, this is more or less a map from ContractAddress to intents (via segment_id) which contain that ContractAddress
        let mut calls_by_address: std::collections::BTreeMap<ContractAddress, Vec<CallNode>> =
            std::collections::BTreeMap::new();

        for (segment_id, intent) in intents_sorted {
            let calls_in_intent = as_indexed(intent.calls());

            // A mapping from (address, entry_point, communication_commitment) to that call's position within this intent
            // This is so we can easily check that the calling order is valid (sequential) in the next step
            let mut call_lookup: std::collections::BTreeMap<
                (ContractAddress, HashOutput, Fr),
                Vec<u32>,
            > = std::collections::BTreeMap::new();
            for (cid, call) in &calls_in_intent {
                call_lookup
                    .entry((
                        call.address,
                        call.entry_point.ep_hash(),
                        call.communication_commitment,
                    ))
                    .or_default()
                    .push(*cid);
            }

            // If a calls b and c, and the sequence ID of b precedes
            // that of c, then b must precede c in the intent.
            for (cid1, call1) in &calls_in_intent {
                let sequenced_sub_calls = call_sequencing_check(*cid1, *call1, &call_lookup)?;
                sequencing_correlation_check(call1.address, sequenced_sub_calls)?;
            }

            let callers_for_addr = relate_nodes(
                &mut guaranteed_calls,
                &mut fallible_calls,
                &mut calls_by_address,
                &mut adjacencies,
                segment_id,
                &calls_in_intent,
            )?;
            sequencing_context_check(
                &mut adjacencies,
                segment_id,
                calls_in_intent,
                callers_for_addr,
            )?;
        }

        // Enforce causality requirements
        causality_check(&guaranteed_calls, &fallible_calls, adjacencies)
    }

    fn balancing_check(
        &self,
        strictness: WellFormedStrictness,
        fees: u128,
    ) -> Result<(), MalformedTransaction<D>> {
        for ((token_type, segment), bal) in self.balance(Some(fees))?.into_iter() {
            if bal < 0 && strictness.enforce_balancing {
                return Err(MalformedTransaction::<D>::BalanceCheckOverspend {
                    token_type,
                    segment,
                    overspent_value: bal,
                });
            }
        }

        Ok(())
    }

    fn pedersen_check(&self) -> Result<(), MalformedTransaction<D>> {
        let comm_parts: Vec<Pedersen> = self
            .intents()
            .map(|(_, intent)| intent.binding_commitment.downgrade())
            .chain(
                self.guaranteed_coins
                    .iter()
                    .map(|sp_offer| sp_offer.deref().clone())
                    .chain(self.fallible_coins.values())
                    .flat_map(|offer| {
                        offer
                            .inputs
                            .iter()
                            .map(|inp| inp.value_commitment)
                            .chain(offer.outputs.iter().map(|out| -out.value_commitment))
                            .chain(
                                offer
                                    .transient
                                    .iter()
                                    .map(|trans| trans.value_commitment_input),
                            )
                            .chain(
                                offer
                                    .transient
                                    .iter()
                                    .map(|trans| -trans.value_commitment_output),
                            )
                            .collect::<Vec<_>>()
                    }),
            )
            .collect();
        let comm = comm_parts
            .into_iter()
            .fold(EmbeddedGroupAffine::identity(), |a, b| a + b.0);
        let expected = self
            .balance_maybe_deltas_only(None, true)?
            .into_iter()
            .filter_map(|((tt, segment), value)| match tt {
                TokenType::Shielded(tt) => {
                    Some(hash_to_curve(&(tt, segment)).mul(EmbeddedFr::from(value)))
                }
                _ => None,
            })
            .fold(
                EmbeddedGroupAffine::generator() * self.binding_randomness,
                |a, b| a + b,
            );

        if comm != expected {
            return Err(MalformedTransaction::PedersenCheckFailure {
                expected: Box::new(expected),
                calculated: Box::new(comm),
            });
        };

        Ok(())
    }

    #[allow(clippy::type_complexity)]
    fn effects_check(&self) -> Result<(), MalformedTransaction<D>> {
        // We have multisets for the following:
        // - Claimed nullifiers (per segment ID)
        // - Claimed contract calls (per segment ID)
        // - Claimed shielded spends (per segment ID)
        // - Claimed shielded receives (per segment ID)
        // - Claimed unshielded spends (per segment ID)

        // transcripts associate with both the their intent segment, and their
        // logical segment (0 for guarnateed transcripts), as the matching uses
        // the former for calls, and the latter for zswap.
        let calls: Vec<(u16, ContractCall<P, D>)> = self
            .intents
            .iter()
            .flat_map(|seg_x_intent| {
                seg_x_intent
                    .1
                    .deref()
                    .clone()
                    .actions
                    .iter_deref()
                    .filter_map(move |action| match action {
                        ContractAction::Call(call) => {
                            Some((*seg_x_intent.0.deref(), (**call).clone()))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        // Vector of tuple of:
        // - Physical segment ID (which intent the transcript is in)
        // - Logical segment ID (which segment the transcript is executing with)
        // - Transcript
        // - Contract address
        let transcripts: Vec<_> = calls
            .iter()
            .flat_map(|(segment, call)| {
                call.guaranteed_transcript
                    .iter()
                    .map(move |t| (segment, &0, t, call.address))
                    .chain(
                        call.fallible_transcript
                            .iter()
                            .map(move |t| (segment, segment, t, call.address)),
                    )
            })
            .collect();
        let offers: std::collections::BTreeMap<_, _> = self
            .guaranteed_coins
            .iter()
            .map(|sp| (0, sp.deref().clone()))
            .chain(
                self.fallible_coins
                    .iter()
                    .map(|seg_x_offer| (*seg_x_offer.0.deref(), seg_x_offer.1.deref().clone())),
            )
            .collect();
        let commitments: MultiSet<(u16, Commitment, ContractAddress)> =
            offers
                .iter()
                .flat_map(|(segment, offer)| {
                    offer
                        .outputs
                        .iter()
                        .filter_map(|o| o.contract_address.clone().map(|addr| (o.coin_com, addr)))
                        .chain(offer.transient.iter().filter_map(|t| {
                            t.contract_address.clone().map(|addr| (t.coin_com, addr))
                        }))
                        .map(|(com, addr)| (*segment, com, *addr.deref()))
                })
                .collect();
        let nullifiers: MultiSet<(u16, Nullifier, ContractAddress)> =
            offers
                .iter()
                .flat_map(|(segment, offer)| {
                    offer
                        .inputs
                        .iter()
                        .flat_map(|i| i.contract_address.clone().map(|addr| (i.nullifier, addr)))
                        .chain(offer.transient.iter().flat_map(|t| {
                            t.contract_address.clone().map(|addr| (t.nullifier, addr))
                        }))
                        .map(|(nullifier, addr)| (*segment, nullifier, *addr.deref()))
                })
                .collect();
        let claimed_nullifiers: MultiSet<(u16, Nullifier, ContractAddress)> = transcripts
            .iter()
            .flat_map(|(_, segment, t, addr)| {
                t.effects
                    .claimed_nullifiers
                    .iter()
                    .map(|n| (**segment, *(*n).deref(), *addr))
            })
            .collect();
        // All contract-associated nullifiers must be claimed by exactly one
        // instance of the same contract in the same segment.
        if nullifiers != claimed_nullifiers {
            return Err(MalformedTransaction::EffectsCheckFailure(
                EffectsCheckError::NullifiersNEClaimedNullifiers {
                    nullifiers: nullifiers.into_iter().map(|(v, _)| v).collect(),
                    claimed_nullifiers: claimed_nullifiers.into_iter().map(|(v, _)| v).collect(),
                },
            ));
        }
        let claimed_shielded_receives: MultiSet<(u16, Commitment, ContractAddress)> = transcripts
            .iter()
            .flat_map(|(_, segment, t, addr)| {
                (*t).deref()
                    .effects
                    .claimed_shielded_receives
                    .iter()
                    .map(|c| (**segment, *(*c).deref(), *addr))
            })
            .collect();
        // All contract-associated commitments must be claimed by exactly one
        // instance of the same contract in the same segment.
        if commitments != claimed_shielded_receives {
            return Err(MalformedTransaction::EffectsCheckFailure(
                EffectsCheckError::CommitmentsNEClaimedShieldedReceives {
                    commitments: commitments.into_iter().map(|(v, _)| v).collect(),
                    claimed_shielded_receives: claimed_shielded_receives
                        .into_iter()
                        .map(|(v, _)| v)
                        .collect(),
                },
            ));
        }
        let claimed_shielded_spends: MultiSet<(u16, Commitment)> = transcripts
            .iter()
            .flat_map(|(_, segment, t, _addr)| {
                (*t).deref()
                    .effects
                    .claimed_shielded_spends
                    .iter()
                    .map(|c| (**segment, *(*c).deref()))
            })
            .collect();

        let duplicate_spends: Vec<((u16, Commitment), usize)> = claimed_shielded_spends
            .clone()
            .into_iter()
            .filter(|(_, count)| count > &1)
            .collect();

        if !(duplicate_spends.is_empty()) {
            return Err(MalformedTransaction::EffectsCheckFailure(
                EffectsCheckError::ClaimedUnshieldedSpendsUniquenessFailure(duplicate_spends),
            ));
        }

        let all_commitments: MultiSet<(u16, Commitment)> = offers
            .iter()
            .flat_map(|(segment, offer)| {
                offer
                    .outputs
                    .iter()
                    .map(|o| o.coin_com)
                    .chain(offer.transient.iter().map(|t| t.coin_com))
                    .map(|c| (*segment, c))
            })
            .collect();
        // Any claimed shielded outputs must exist, and may not be claimed by
        // another contract.
        // WG: @Thomas: Is it actually a multi-set subset check that we want here?
        // TK: This is probably fine. `claimed_shielded_spends` is known to be a plain set at this
        // point, `all_commitments` technically isn't, but that gets checked during application, so
        // it's not too big of a deal.
        if !(all_commitments.has_subset(&claimed_shielded_spends)) {
            return Err(MalformedTransaction::EffectsCheckFailure(
                EffectsCheckError::AllCommitmentsSubsetCheckFailure(SubsetCheckFailure {
                    subset: claimed_shielded_spends
                        .into_iter()
                        .map(|(v, _len)| v)
                        .collect(),
                    superset: all_commitments.into_iter().map(|(v, _len)| v).collect(),
                }),
            ));
        }
        let claimed_calls: MultiSet<(u16, (ContractAddress, HashOutput, Fr))> = transcripts
            .iter()
            .flat_map(|(segment, _, t, _)| {
                t.effects.claimed_contract_calls.iter().map(|call| {
                    let (_seq, addr, hash, fr) = &call.into_inner();
                    (**segment, (*addr, *hash, *fr))
                })
            })
            .collect();

        let duplicate_claimed_calls: Vec<((u16, (ContractAddress, HashOutput, Fr)), usize)> =
            claimed_calls
                .clone()
                .into_iter()
                .filter(|(_, count)| count > &1)
                .collect();

        if !(duplicate_claimed_calls.is_empty()) {
            return Err(MalformedTransaction::EffectsCheckFailure(
                EffectsCheckError::ClaimedCallsUniquenessFailure(duplicate_claimed_calls),
            ));
        }

        let real_calls: MultiSet<(u16, (ContractAddress, HashOutput, Fr))> = calls
            .iter()
            .map(|(segment, call)| {
                (
                    *segment,
                    (
                        call.address,
                        call.entry_point.ep_hash(),
                        call.communication_commitment,
                    ),
                )
            })
            .collect();
        // Any claimed call must also exist within the same segment
        if !(real_calls.has_subset(&claimed_calls)) {
            return Err(MalformedTransaction::EffectsCheckFailure(
                EffectsCheckError::RealCallsSubsetCheckFailure(SubsetCheckFailure {
                    subset: claimed_calls.into_iter().map(|(v, _len)| v).collect(),
                    superset: real_calls.into_iter().map(|(v, _len)| v).collect(),
                }),
            ));
        }
        let claimed_unshielded_spends: MultiSet<((u16, bool), ((TokenType, PublicAddress), u128))> =
            transcripts
                .iter()
                .flat_map(|(intent_seg, logical_seg, t, _)| {
                    t.effects.claimed_unshielded_spends.iter().map(|sp| {
                        let (sp, i) = sp.deref();
                        (
                            (**intent_seg, **logical_seg == 0),
                            ((sp.deref().0, sp.deref().1), *(*i).deref()),
                        )
                    })
                })
                .collect();
        let real_unshielded_spends: MultiSet<((u16, bool), ((TokenType, PublicAddress), u128))> =
            transcripts
                .iter()
                .flat_map(|(intent_seg, logical_seg, t, addr)| {
                    t.effects.unshielded_inputs.iter().map(move |sp| {
                        (
                            (**intent_seg, **logical_seg == 0),
                            (
                                (*sp.deref().0.deref(), PublicAddress::Contract(*addr)),
                                *sp.deref().1.deref(),
                            ),
                        )
                    })
                })
                .chain(self.intents.iter().flat_map(|sp| {
                    let intent = &sp.clone().1;
                    let segment = *sp.0;
                    let guaranteed_outputs = intent.guaranteed_outputs();
                    let fallible_outputs = intent.fallible_outputs();
                    guaranteed_outputs
                        .iter()
                        .map(|o| (true, o))
                        .chain(fallible_outputs.iter().map(|o| (false, o)))
                        .map(move |(guaranteed, output)| {
                            (
                                (segment, guaranteed),
                                (
                                    (
                                        TokenType::Unshielded(output.type_),
                                        PublicAddress::User(output.owner),
                                    ),
                                    output.value,
                                ),
                            )
                        })
                        .collect::<Vec<_>>()
                }))
                .collect();

        if !(real_unshielded_spends.has_subset(&claimed_unshielded_spends)) {
            return Err(MalformedTransaction::EffectsCheckFailure(
                EffectsCheckError::RealUnshieldedSpendsSubsetCheckFailure(SubsetCheckFailure {
                    subset: claimed_unshielded_spends
                        .into_iter()
                        .map(|(v, _)| v)
                        .collect(),
                    superset: real_unshielded_spends.into_iter().map(|(v, _)| v).collect(),
                }),
            ));
        }
        Ok(())
    }

    fn ttl_check_weak(
        &self,
        tblock: Timestamp,
        global_ttl: Duration,
    ) -> Result<(), TransactionApplicationError> {
        for (_, intent) in self.intents() {
            if intent.ttl < tblock {
                Err(TransactionApplicationError::IntentTtlExpired(
                    intent.ttl, tblock,
                ))?;
            }

            if intent.ttl > tblock + global_ttl {
                Err(TransactionApplicationError::IntentTtlTooFarInFuture(
                    intent.ttl,
                    tblock + global_ttl,
                ))?;
            }
        }

        Ok(())
    }
}

fn as_indexed<I, V>(v: I) -> std::collections::BTreeMap<u32, V>
where
    I: Iterator<Item = V>,
{
    v.enumerate()
        .map(|(i, v)| (i as u32, v))
        .collect::<std::collections::BTreeMap<u32, V>>()
}

impl<D: DB> ContractDeploy<D> {
    pub(crate) fn well_formed(&self) -> Result<(), MalformedTransaction<D>> {
        self.initial_state.well_formed(self.address())?;

        // Or we could change the types. Current (pre-this-ticket) ContractState could be renamed to
        // `DeployContractState`, and the new version with `balance` could be called ContractState
        // `ContractDeploy` would contain `DeployContractState`. Then, this check isn't required.
        if self.initial_state.balance.iter().any(|bal| *bal.1 > 0) {
            let mut err_data = std::collections::BTreeMap::new();
            for val in self.initial_state.balance.clone().iter() {
                err_data.insert(*val.0, *val.1);
            }
            return Err(MalformedTransaction::<D>::MalformedContractDeploy(
                MalformedContractDeploy::NonZeroBalance(err_data),
            ));
        }
        let rechecked_state = ChargedState::new((*self.initial_state.data.get()).clone());
        if rechecked_state != self.initial_state.data {
            return Err(MalformedTransaction::<D>::MalformedContractDeploy(
                MalformedContractDeploy::IncorrectChargedState,
            ));
        }
        Ok(())
    }
}

impl<P: ProofKind<D>, D: DB> ContractAction<P, D> {
    fn well_formed(
        &self,
        ref_state: &impl StateReference<D>,
        strictness: WellFormedStrictness,
        parent: &ErasedIntent<D>,
    ) -> Result<(), MalformedTransaction<D>> {
        match self {
            ContractAction::Call(call) => call.well_formed(ref_state, strictness, parent),
            ContractAction::Deploy(deploy) => deploy.well_formed(),
            ContractAction::Maintain(upd) => upd.well_formed(ref_state, strictness),
        }
    }
}

impl<D: DB> MaintenanceUpdate<D> {
    pub(crate) fn well_formed(
        &self,
        ref_state: &impl StateReference<D>,
        strictness: WellFormedStrictness,
    ) -> Result<(), MalformedTransaction<D>> {
        ref_state.maintenance_check(self.address, |authority| {
            let data = self.data_to_sign();
            // Ensure ordering, and that no party signed two votes
            if !Vec::from(&self.signatures)
                .windows(2)
                .all(|xs| xs[0].0 < xs[1].0)
            {
                return Err(MalformedTransaction::NotNormalized);
            }
            // Ensure that any new committee uses an incremented counter
            if self
                .updates
                .iter_deref()
                .filter_map(|up| match up {
                    SingleUpdate::ReplaceAuthority(new_auth) => Some(new_auth),
                    _ => None,
                })
                .any(|auth| auth.counter != self.counter.saturating_add(1))
            {
                return Err(MalformedTransaction::NotNormalized);
            }
            for (idx, sig) in self.signatures.iter().map(|x| x.into_inner()) {
                let vk = authority.committee.get(idx as usize).ok_or(
                    MalformedTransaction::KeyNotInCommittee {
                        address: self.address,
                        key_id: idx as usize,
                    },
                )?;
                if strictness.verify_signatures && !vk.verify(&data, &sig) {
                    return Err(MalformedTransaction::InvalidCommitteeSignature {
                        address: self.address,
                        key_id: idx as usize,
                    });
                }
            }
            if self.signatures.len() < authority.threshold as usize {
                return Err(MalformedTransaction::ThresholdMissed {
                    address: self.address,
                    signatures: self.signatures.len(),
                    threshold: authority.threshold as usize,
                });
            }
            Ok(())
        })
    }
}

impl<P: ProofKind<D>, D: DB> ContractCall<P, D> {
    pub(crate) fn well_formed(
        &self,
        ref_state: &impl StateReference<D>,
        strictness: WellFormedStrictness,
        parent: &ErasedIntent<D>,
    ) -> Result<(), MalformedTransaction<D>> {
        if let Some(fallible) = &self.fallible_transcript
            && fallible.program.get(0) != Some(&Op::Ckpt)
            && self.guaranteed_transcript.is_some()
        {
            return Err(MalformedTransaction::FallibleWithoutCheckpoint);
        }
        for transcript in [
            self.guaranteed_transcript.as_ref(),
            self.fallible_transcript.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            for window in Vec::from(&transcript.program).windows(2) {
                if let (Op::Noop { .. }, Op::Noop { .. }) = (&window[0], &window[1]) {
                    return Err(MalformedTransaction::NotNormalized);
                }
            }
        }

        if strictness.verify_contract_proofs {
            ref_state.op_check(self.address, &self.entry_point, |op| {
                let gt = self
                    .guaranteed_transcript
                    .clone()
                    .map(|x| x.deref().clone());

                if op.v2.is_some() {
                    if gt.is_some()
                        && !matches!(&gt, Some(Transcript { version: Some(version), ..}) if version.major == 2 && version.minor <= 3)
                    {
                        return Err(MalformedTransaction::GuaranteedTranscriptVersion {
                            op_version: "V2".to_string(),
                        });
                    }

                    let ft = self.fallible_transcript.clone().map(|x| x.deref().clone());

                    if ft.is_some()
                        && !matches!(&ft, Some(Transcript { version: Some(version), ..}) if version.major == 2 && version.minor <= 3)
                    {
                        return Err(MalformedTransaction::FallibleTranscriptVersion {
                            op_version: "V2".to_string(),
                        });
                    }
                }
                P::proof_verify(
                    op,
                    &self.proof,
                    self.public_inputs(parent.binding_commitment),
                    self,
                    strictness.proof_verification_mode
                )?;
                trace!("call valid");
                Ok(())
            })?;
        }
        Ok(())
    }

    // NOTE: The proof should receive the following inputs for binding purposes:
    //  - The contract address
    //  - The contract entry point
    //  - Both transcript parts's declared gas costs, and effects
    //  - The count of instructions in the guaranteed transcript
    //  - The parent ContractCalls's `binding_commitment.commitment`.
    // These need to be *truely* binding (see note at the bottom of this file!)
    // In addition, it should receive
    //  - The communication commitment
    //  - The transcript
    pub fn public_inputs(&self, binding_com: Pedersen) -> Vec<Fr> {
        let mut res = vec![self.binding_input(binding_com)];
        res.push(self.communication_commitment);
        if let Some(guaranteed) = self.guaranteed_transcript.as_ref() {
            for op in guaranteed.program.iter() {
                op.field_repr(&mut res);
            }
        }
        if let Some(fallible) = self.fallible_transcript.as_ref() {
            for op in fallible.program.iter() {
                op.field_repr(&mut res);
            }
        }
        res
    }

    pub(crate) fn binding_input(&self, binding_com: Pedersen) -> Fr {
        let mut binding_input = Vec::new();

        binding_input.extend(b"midnight:binding-input[v1]");
        let _ = Serializable::serialize(&self.address, &mut binding_input);
        let _ = Serializable::serialize(&self.entry_point, &mut binding_input);
        let _ = Serializable::serialize(
            &self
                .guaranteed_transcript
                .as_ref()
                .map(|t| t.gas)
                .unwrap_or_default(),
            &mut binding_input,
        );

        if let Some(t) = self.guaranteed_transcript.as_ref() {
            let _ = Serializable::serialize(&t.effects, &mut binding_input);
        } else {
            // Backwards-compatible with `Effects::default` serialization, as this may be unstable.
            binding_input.extend(vec![0u8; 20]);
        }
        if let Some(t) = self.fallible_transcript.as_ref() {
            let _ = Serializable::serialize(&Some(t.gas), &mut binding_input);
            // Discriminant for `Some`.
            binding_input.push(1);

            let _ = Serializable::serialize(&t.effects, &mut binding_input);
        } else {
            // (None, None)
            binding_input.extend(&[0, 0]);
        }
        let len = self
            .guaranteed_transcript
            .as_ref()
            .map(|t| t.program.len() as u64)
            .unwrap_or_default();
        let _ = Serializable::serialize(&len, &mut binding_input);
        let _ = Serializable::serialize(&Into::<Pedersen>::into(binding_com), &mut binding_input);
        let mut hasher = Sha256::new();
        hasher.update(&binding_input[..]);
        Fr::from_le_bytes(&hasher.finalize()[..31])
            .expect("Trimmed persistent hash should fall in Fr")
    }
}

#[cfg(feature = "proving")]
#[cfg(test)]
mod tests {
    use super::*;
    use base_crypto::hash::HashOutput;
    use coin_structure::contract::ContractAddress;
    use onchain_runtime::state::EntryPointBuf;
    use onchain_runtime::{
        context::{ClaimedContractCallsValue, Effects},
        transcript::Transcript,
    };
    use rand::Rng;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use storage::{arena::Sp, db::InMemoryDB};
    use transient_crypto::curve::Fr;

    #[cfg(feature = "proving")]
    #[tokio::test]
    async fn causality_check_failure() {
        let mut rng = StdRng::seed_from_u64(0x42);

        let addr_1 = rng.r#gen();
        let addr_2 = rng.r#gen();

        let cn_1 = CallNode {
            segment_id: 1,
            addr: addr_1,
            call_index: 1,
        };
        let cn_2 = CallNode {
            segment_id: 1,
            addr: addr_2,
            call_index: 2,
        };

        let mut guaranteed_calls = std::collections::BTreeSet::new();
        guaranteed_calls.insert(cn_1);

        let mut fallible_calls = std::collections::BTreeSet::new();
        fallible_calls.insert(cn_2);

        let mut adjacencies = std::collections::BTreeMap::new();
        adjacencies.insert(cn_2, vec![cn_1]);

        let res = causality_check::<InMemoryDB>(&guaranteed_calls, &fallible_calls, adjacencies);

        match res {
            Ok(_) => panic!("Unexpected success"),
            Err(MalformedTransaction::SequencingCheckFailure(
                SequencingCheckError::CausalityConstraintViolation { .. },
            )) => (),
            Err(e) => panic!(
                "Test failed as expected, but the error was unexpected: {:?}",
                e.to_string()
            ),
        }
    }

    #[cfg(feature = "proving")]
    #[tokio::test]
    async fn sequencing_check_failure() {
        let mut rng = StdRng::seed_from_u64(0x42);

        let addr_1 = rng.r#gen();
        let addr_2 = rng.r#gen();

        let mut claimed_contract_calls: storage::storage::HashSet<
            ClaimedContractCallsValue,
            InMemoryDB,
        > = storage::storage::HashSet::new();

        let claimed_call_pos = 1;
        let claimed_call_addr = addr_1;
        let claimed_call_hash = rng.r#gen();
        let claimed_call_rng = rng.r#gen();

        claimed_contract_calls = claimed_contract_calls.insert(ClaimedContractCallsValue(
            claimed_call_pos,
            claimed_call_addr,
            claimed_call_hash,
            claimed_call_rng,
        ));

        let eff = Effects {
            claimed_nullifiers: rng.r#gen(),
            claimed_shielded_receives: rng.r#gen(),
            claimed_shielded_spends: rng.r#gen(),
            claimed_contract_calls,
            shielded_mints: rng.r#gen(),
            unshielded_mints: rng.r#gen(),
            unshielded_inputs: rng.r#gen(),
            unshielded_outputs: rng.r#gen(),
            claimed_unshielded_spends: rng.r#gen(),
        };

        let transcript = Transcript {
            gas: rng.r#gen(),
            effects: eff,
            program: storage::storage::Array::new(),
            version: None,
        };

        let cc_2: ContractCall<(), InMemoryDB> = ContractCall {
            address: addr_2,
            entry_point: rng.r#gen(),
            guaranteed_transcript: Some(Sp::new(transcript)),
            fallible_transcript: None,
            communication_commitment: rng.r#gen(),
            proof: (),
        };

        let mut call_lookup: std::collections::BTreeMap<
            (ContractAddress, HashOutput, Fr),
            Vec<u32>,
        > = std::collections::BTreeMap::new();

        call_lookup.insert((addr_1, claimed_call_hash, claimed_call_rng), vec![1]);
        call_lookup.insert((addr_2, rng.r#gen(), rng.r#gen()), vec![2]);

        let res = call_sequencing_check(2, &cc_2, &call_lookup);

        match res {
            Ok(_) => panic!("Unexpected success"),
            Err(MalformedTransaction::SequencingCheckFailure(
                SequencingCheckError::CallSequencingViolation { .. },
            )) => (),
            Err(e) => panic!(
                "Test failed as expected, but the error was unexpected: {:?}",
                e.to_string()
            ),
        }
    }

    #[cfg(feature = "proving")]
    #[tokio::test]
    async fn sequencing_correlation_check_failure() {
        let mut rng = StdRng::seed_from_u64(0x42);

        let addr_1 = rng.r#gen();

        let sequenced_sub_calls = vec![(1, 2, addr_1), (2, 1, rng.r#gen())];

        let res = sequencing_correlation_check::<InMemoryDB>(addr_1, sequenced_sub_calls);

        match res {
            Ok(_) => panic!("Unexpected success"),
            Err(MalformedTransaction::SequencingCheckFailure(
                SequencingCheckError::SequencingCorrelationViolation { .. },
            )) => (),
            Err(e) => panic!(
                "Test failed as expected, but the error was unexpected: {:?}",
                e.to_string()
            ),
        }
    }

    #[cfg(feature = "proving")]
    #[tokio::test]
    async fn sequencing_context_check_failure_guaranteed_in_fallible() {
        let mut rng = StdRng::seed_from_u64(0x42);

        let addr_1 = rng.r#gen();
        let addr_2 = rng.r#gen();

        let cn_1 = CallNode {
            segment_id: 1,
            addr: addr_1,
            call_index: 1,
        };
        let cn_1_ep: EntryPointBuf = rng.r#gen();
        let cn_1_commitment = rng.r#gen();
        let cn_2_ep: EntryPointBuf = rng.r#gen();
        let cn_2_commitment = rng.r#gen();
        let ck_2 = CallKey {
            addr: addr_2,
            ep_hash: cn_2_ep.ep_hash(),
            commitment: cn_2_commitment,
        };
        let cn_2 = CallNode {
            segment_id: 1,
            addr: addr_2,
            call_index: 2,
        };

        let mut claimed_contract_calls: storage::storage::HashSet<
            ClaimedContractCallsValue,
            InMemoryDB,
        > = storage::storage::HashSet::new();
        claimed_contract_calls = claimed_contract_calls.insert(ClaimedContractCallsValue(
            2,
            addr_2,
            cn_2_ep.ep_hash(),
            cn_2_commitment,
        ));

        let eff_1 = Effects {
            claimed_nullifiers: rng.r#gen(),
            claimed_shielded_receives: rng.r#gen(),
            claimed_shielded_spends: rng.r#gen(),
            claimed_contract_calls,
            shielded_mints: rng.r#gen(),
            unshielded_mints: rng.r#gen(),
            unshielded_inputs: rng.r#gen(),
            unshielded_outputs: rng.r#gen(),
            claimed_unshielded_spends: rng.r#gen(),
        };

        let transcript_1 = Transcript {
            gas: rng.r#gen(),
            effects: eff_1,
            program: storage::storage::Array::new(),
            version: None,
        };

        let cc_1: ContractCall<(), InMemoryDB> = ContractCall {
            address: addr_1,
            entry_point: cn_1_ep.clone(),
            guaranteed_transcript: None,
            fallible_transcript: Some(Sp::new(transcript_1)),
            communication_commitment: rng.r#gen(),
            proof: (),
        };

        let eff_2 = Effects {
            claimed_nullifiers: rng.r#gen(),
            claimed_shielded_receives: rng.r#gen(),
            claimed_shielded_spends: rng.r#gen(),
            claimed_contract_calls: storage::storage::HashSet::new(),
            shielded_mints: rng.r#gen(),
            unshielded_mints: rng.r#gen(),
            unshielded_inputs: rng.r#gen(),
            unshielded_outputs: rng.r#gen(),
            claimed_unshielded_spends: rng.r#gen(),
        };

        let transcript_2 = Transcript {
            gas: rng.r#gen(),
            effects: eff_2,
            program: storage::storage::Array::new(),
            version: None,
        };

        let cc_2: ContractCall<(), InMemoryDB> = ContractCall {
            address: addr_2,
            entry_point: cn_2_ep.clone(),
            guaranteed_transcript: Some(Sp::new(transcript_2)),
            fallible_transcript: None,
            communication_commitment: cn_2_commitment,
            proof: (),
        };

        let mut call_lookup: std::collections::BTreeMap<
            (ContractAddress, HashOutput, Fr),
            Vec<i128>,
        > = std::collections::BTreeMap::new();

        call_lookup.insert((addr_1, cn_1_ep.ep_hash(), cn_1_commitment), vec![1]);
        call_lookup.insert((addr_2, cn_2_ep.ep_hash(), cn_2_commitment), vec![2]);

        let mut adjacencies = std::collections::BTreeMap::new();
        adjacencies.insert(cn_1, vec![cn_2]);

        let mut calls_in_intent = std::collections::BTreeMap::new();
        calls_in_intent.insert(1, &cc_1);
        calls_in_intent.insert(2, &cc_2);
        let mut callers_for_addr = std::collections::BTreeMap::new();
        callers_for_addr.insert(ck_2, vec![(cn_1, false)]);

        let res = sequencing_context_check::<(), InMemoryDB>(
            &mut adjacencies,
            1,
            calls_in_intent,
            callers_for_addr,
        );

        match res {
            Ok(_) => panic!("Unexpected success"),
            Err(MalformedTransaction::SequencingCheckFailure(
                SequencingCheckError::GuaranteedInFallibleContextViolation { .. },
            )) => (),
            Err(e) => panic!(
                "Test failed as expected, but the error was unexpected: {:?}",
                e.to_string()
            ),
        }
    }

    #[cfg(feature = "proving")]
    #[tokio::test]
    async fn sequencing_context_check_failure_fallible_in_guaranteed() {
        let mut rng = StdRng::seed_from_u64(0x42);

        let addr_1 = rng.r#gen();
        let addr_2 = rng.r#gen();

        let cn_1 = CallNode {
            segment_id: 1,
            addr: addr_1,
            call_index: 1,
        };
        let cn_1_ep: EntryPointBuf = rng.r#gen();
        let cn_1_commitment = rng.r#gen();
        let cn_2_ep: EntryPointBuf = rng.r#gen();
        let cn_2_commitment = rng.r#gen();
        let ck_2 = CallKey {
            addr: addr_2,
            ep_hash: cn_2_ep.ep_hash(),
            commitment: cn_2_commitment,
        };
        let cn_2 = CallNode {
            segment_id: 1,
            addr: addr_2,
            call_index: 2,
        };

        let mut claimed_contract_calls: storage::storage::HashSet<
            ClaimedContractCallsValue,
            InMemoryDB,
        > = storage::storage::HashSet::new();
        claimed_contract_calls = claimed_contract_calls.insert(ClaimedContractCallsValue(
            2,
            addr_2,
            cn_2_ep.ep_hash(),
            cn_2_commitment,
        ));

        let eff_1 = Effects {
            claimed_nullifiers: rng.r#gen(),
            claimed_shielded_receives: rng.r#gen(),
            claimed_shielded_spends: rng.r#gen(),
            claimed_contract_calls,
            shielded_mints: rng.r#gen(),
            unshielded_mints: rng.r#gen(),
            unshielded_inputs: rng.r#gen(),
            unshielded_outputs: rng.r#gen(),
            claimed_unshielded_spends: rng.r#gen(),
        };

        let transcript_1 = Transcript {
            gas: rng.r#gen(),
            effects: eff_1,
            program: storage::storage::Array::new(),
            version: None,
        };

        let cc_1: ContractCall<(), InMemoryDB> = ContractCall {
            address: addr_1,
            entry_point: cn_1_ep.clone(),
            guaranteed_transcript: Some(Sp::new(transcript_1)),
            fallible_transcript: None,
            communication_commitment: rng.r#gen(),
            proof: (),
        };

        let eff_2 = Effects {
            claimed_nullifiers: rng.r#gen(),
            claimed_shielded_receives: rng.r#gen(),
            claimed_shielded_spends: rng.r#gen(),
            claimed_contract_calls: storage::storage::HashSet::new(),
            shielded_mints: rng.r#gen(),
            unshielded_mints: rng.r#gen(),
            unshielded_inputs: rng.r#gen(),
            unshielded_outputs: rng.r#gen(),
            claimed_unshielded_spends: rng.r#gen(),
        };

        let transcript_2 = Transcript {
            gas: rng.r#gen(),
            effects: eff_2,
            program: storage::storage::Array::new(),
            version: None,
        };

        let cc_2: ContractCall<(), InMemoryDB> = ContractCall {
            address: addr_2,
            entry_point: cn_2_ep.clone(),
            guaranteed_transcript: None,
            fallible_transcript: Some(Sp::new(transcript_2)),
            communication_commitment: cn_2_commitment,
            proof: (),
        };

        let mut call_lookup: std::collections::BTreeMap<
            (ContractAddress, HashOutput, Fr),
            Vec<i128>,
        > = std::collections::BTreeMap::new();

        call_lookup.insert((addr_1, cn_1_ep.ep_hash(), cn_1_commitment), vec![1]);
        call_lookup.insert((addr_2, cn_2_ep.ep_hash(), cn_2_commitment), vec![2]);

        let mut adjacencies = std::collections::BTreeMap::new();
        adjacencies.insert(cn_1, vec![cn_2]);

        let mut calls_in_intent = std::collections::BTreeMap::new();
        calls_in_intent.insert(1, &cc_1);
        calls_in_intent.insert(2, &cc_2);
        let mut callers_for_addr = std::collections::BTreeMap::new();
        callers_for_addr.insert(ck_2, vec![(cn_1, true)]);

        let res = sequencing_context_check::<(), InMemoryDB>(
            &mut adjacencies,
            1,
            calls_in_intent,
            callers_for_addr,
        );

        match res {
            Ok(_) => panic!("Unexpected success"),
            Err(MalformedTransaction::SequencingCheckFailure(
                SequencingCheckError::FallibleInGuaranteedContextViolation { .. },
            )) => (),
            Err(e) => panic!(
                "Test failed as expected, but the error was unexpected: {:?}",
                e.to_string()
            ),
        }
    }

    /// Regression: as_indexed() previously returned HashMap<u32, V>, so
    /// relate_nodes could visit same-address calls in arbitrary order and
    /// produce backward precedence edges, triggering spurious
    /// CausalityConstraintViolation when guaranteed/fallible calls were mixed.
    #[cfg(feature = "proving")]
    #[tokio::test]
    async fn relate_nodes_same_address_ordering() {
        let mut rng = StdRng::seed_from_u64(0x42);
        let addr: ContractAddress = rng.r#gen();

        let make_transcript = |rng: &mut StdRng| -> Sp<Transcript<InMemoryDB>, InMemoryDB> {
            let eff = Effects {
                claimed_nullifiers: rng.r#gen(),
                claimed_shielded_receives: rng.r#gen(),
                claimed_shielded_spends: rng.r#gen(),
                claimed_contract_calls: storage::storage::HashSet::new(),
                shielded_mints: rng.r#gen(),
                unshielded_mints: rng.r#gen(),
                unshielded_inputs: rng.r#gen(),
                unshielded_outputs: rng.r#gen(),
                claimed_unshielded_spends: rng.r#gen(),
            };
            Sp::new(Transcript {
                gas: rng.r#gen(),
                effects: eff,
                program: storage::storage::Array::new(),
                version: None,
            })
        };

        // 6 calls to the same address: 0-3 guaranteed-only, 4-5 fallible-only.
        let calls: Vec<ContractCall<(), InMemoryDB>> = (0..6u32)
            .map(|i| {
                let (gt, ft) = if i < 4 {
                    (Some(make_transcript(&mut rng)), None)
                } else {
                    (None, Some(make_transcript(&mut rng)))
                };
                ContractCall {
                    address: addr,
                    entry_point: rng.r#gen(),
                    guaranteed_transcript: gt,
                    fallible_transcript: ft,
                    communication_commitment: rng.r#gen(),
                    proof: (),
                }
            })
            .collect();

        // Iterate enough times to catch nondeterministic HashMap ordering
        let mut backward_edge_found = false;
        let mut causality_violation_found = false;

        for _ in 0..200 {
            let calls_in_intent: std::collections::BTreeMap<u32, &ContractCall<(), InMemoryDB>> =
                calls
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (i as u32, c))
                    .collect();

            let mut guaranteed_calls = BTreeSet::new();
            let mut fallible_calls = BTreeSet::new();
            let mut calls_by_address = std::collections::BTreeMap::new();
            let mut adjacencies = std::collections::BTreeMap::new();

            let _callers = relate_nodes::<(), InMemoryDB>(
                &mut guaranteed_calls,
                &mut fallible_calls,
                &mut calls_by_address,
                &mut adjacencies,
                1,
                &calls_in_intent,
            )
            .unwrap();

            // All precedence edges should be forward (lower → higher index)
            for (from_node, to_nodes) in &adjacencies {
                for to_node in to_nodes {
                    if from_node.call_index > to_node.call_index {
                        backward_edge_found = true;
                    }
                }
            }

            // causality_check should not reject this valid ordering
            if causality_check::<InMemoryDB>(&guaranteed_calls, &fallible_calls, adjacencies)
                .is_err()
            {
                causality_violation_found = true;
            }

            if backward_edge_found && causality_violation_found {
                break;
            }
        }

        assert!(
            !backward_edge_found,
            "'related_nodes' produced backwards edges"
        );
        assert!(
            !causality_violation_found,
            "Spurious CausalityConstraintViolation for a valid call ordering"
        );
    }
}
