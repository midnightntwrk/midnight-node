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

use crate::error::EventReplayError;
use crate::error::{
    BalanceOperation, BlockLimitExceeded, InvariantViolation, SystemTransactionError,
    TransactionApplicationError, TransactionInvalid,
};
use crate::events::{Event, EventDetails, EventSource, ZswapPreimageEvidence};
use crate::structure::TransactionHash;
use crate::structure::UtxoMeta;
use crate::structure::{
    ClaimKind, ClaimRewardsTransaction, ContractAction, ErasedIntent, Intent, IntentHash,
    LedgerParameters, LedgerState, MAX_SUPPLY, OutputInstructionUnshielded, PedersenDowngradeable,
    ProofKind, ReplayProtectionState, SignatureKind, SingleUpdate, StandardTransaction,
    SystemTransaction, Transaction, UnshieldedOffer, Utxo, UtxoState, VerifiedTransaction,
};
use crate::utils::{KeySortedIter, SortedIter};
use crate::zswap::{WithZswapStateChanges, ZswapStateChanges};
use base_crypto::cost_model::{FixedPoint, NormalizedCost};
use base_crypto::hash::HashOutput;
use base_crypto::rng::SplittableRng;
use base_crypto::time::{Duration, Timestamp};
use coin_structure::coin::UserAddress;
use coin_structure::coin::{Commitment, NIGHT, ShieldedTokenType, TokenType};
use coin_structure::contract::ContractAddress;
use itertools::Either;
use onchain_runtime::context::{BlockContext, QueryContext};
use onchain_runtime::state::ContractOperation;
use rand::{CryptoRng, Rng};
use serialize::{Deserializable, Serializable, Tagged};
use std::borrow::Cow;
use std::fmt::Debug;
use std::ops::Deref;
use storage::Storable;
use storage::arena::Sp;
use storage::db::DB;
use storage::storage::HashSet;
use storage::storage::Map;
use transient_crypto::commitment::Pedersen;
use transient_crypto::commitment::{PedersenRandomness, PureGeneratorPedersen};
use transient_crypto::merkle_tree::InvalidUpdate;
use zswap::Offer as ZswapOffer;
use zswap::keys::SecretKeys;
use zswap::local::State as ZswapLocalState;

pub(crate) fn whitelist_matches(
    whitelist: &Option<Map<ContractAddress, ()>>,
    addr: &ContractAddress,
) -> bool {
    match whitelist {
        Some(wl) => wl.contains_key(addr),
        None => true,
    }
}

pub struct TransactionContext<D: DB> {
    pub ref_state: LedgerState<D>,
    pub block_context: BlockContext,
    pub whitelist: Option<Map<ContractAddress, ()>>,
}

impl<D: DB> Debug for TransactionContext<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransactionContext")
            .field("ref_state", &self.ref_state.state_hash())
            .field("block_context", &self.block_context)
            .field("whitelist", &self.whitelist)
            .finish()
    }
}

#[derive(Debug)]
pub enum TransactionResult<D: DB> {
    Success(Vec<Event<D>>),
    PartialSuccess(
        std::collections::BTreeMap<u16, Result<(), TransactionInvalid<D>>>,
        Vec<Event<D>>,
    ),
    Failure(TransactionInvalid<D>),
}

impl<D: DB> TransactionResult<D> {
    pub fn events(&self) -> &[Event<D>] {
        match self {
            TransactionResult::Success(events) => events,
            TransactionResult::PartialSuccess(_, events) => events,
            TransactionResult::Failure(..) => &[],
        }
    }
}

impl<D: DB> From<&TransactionResult<D>> for ErasedTransactionResult {
    fn from(res: &TransactionResult<D>) -> ErasedTransactionResult {
        match res {
            TransactionResult::Success(_) => ErasedTransactionResult::Success,
            TransactionResult::PartialSuccess(segments, _) => {
                ErasedTransactionResult::PartialSuccess(
                    segments.iter().map(|(k, v)| (*k, v.is_ok())).collect(),
                )
            }
            TransactionResult::Failure(_) => ErasedTransactionResult::Failure,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ErasedTransactionResult {
    Success,
    PartialSuccess(Vec<(u16, bool)>),
    Failure,
}

pub trait ZswapLocalStateExt<D: DB>: Sized {
    #[must_use]
    #[deprecated = "deprecated in favour of `replay_events`"]
    fn apply_system_tx(&self, secret_keys: &SecretKeys, tx: &SystemTransaction) -> Self;
    #[must_use = "return value must be used"]
    #[deprecated = "deprecated in favour of `replay_events`"]
    fn apply_tx<S: SignatureKind<D>, P: ProofKind<D>, B: Storable<D>>(
        &self,
        secret_keys: &SecretKeys,
        tx: &Transaction<S, P, B, D>,
        res: ErasedTransactionResult,
    ) -> Result<Self, InvalidUpdate>;
    #[must_use = "return value must be used"]
    fn replay_events<'a>(
        &self,
        secret_keys: &SecretKeys,
        events: impl IntoIterator<Item = &'a Event<D>>,
    ) -> Result<Self, EventReplayError>;
    #[must_use = "return value must be used"]
    fn replay_events_with_changes<'a>(
        &self,
        secret_keys: &SecretKeys,
        events: impl IntoIterator<Item = &'a Event<D>>,
    ) -> Result<WithZswapStateChanges<Self>, EventReplayError>;
}

impl<D: DB> ZswapLocalStateExt<D> for ZswapLocalState<D> {
    fn apply_system_tx(&self, _secret_keys: &SecretKeys, tx: &SystemTransaction) -> Self {
        #[allow(clippy::match_single_binding, reason = "to re-enable later")]
        match tx {
            // NOTE: Treasury payments have been completely disabled until governance for the
            // treasury has been agreed. All code has been commented out to maximize clarity.
            //
            // SystemTransaction::PayFromTreasuryShielded {
            //     outputs,
            //     nonce,
            //     token_type,
            // } => {
            //     let mut res = self.clone();
            //     for OutputInstructionShielded {
            //         amount,
            //         target_key: target_address,
            //     } in outputs
            //     {
            //         let coin = Info {
            //             value: *amount,
            //             type_: *token_type,
            //             nonce: Nonce(*nonce),
            //         };
            //         res = self.apply_claim(
            //             secret_keys,
            //             &AuthorizedClaim {
            //                 coin,
            //                 recipient: *target_address,
            //                 proof: std::sync::Arc::new(()),
            //             },
            //         );
            //     }
            //     res
            // }
            _ => self.clone(),
        }
    }

    fn apply_tx<S: SignatureKind<D>, P: ProofKind<D>, B: Storable<D>>(
        &self,
        secret_keys: &SecretKeys,
        tx: &Transaction<S, P, B, D>,
        res: ErasedTransactionResult,
    ) -> Result<Self, InvalidUpdate> {
        Ok(match (tx, res) {
            (_, ErasedTransactionResult::Failure) => self.clone(),
            (Transaction::Standard(stx), ErasedTransactionResult::PartialSuccess(segments, ..)) => {
                let post_guaranteed = if let Some(gc) = &stx.guaranteed_coins {
                    self.apply(secret_keys, gc)?
                } else {
                    self.clone()
                };

                segments
                    .iter()
                    .filter(|(segment, success)| *segment != 0 && *success)
                    .map(|(segment, _)| segment)
                    .try_fold(post_guaranteed, |st, segment| {
                        if let Some(fc) = stx.fallible_coins.get(segment) {
                            st.apply(secret_keys, &fc)
                        } else {
                            Ok(st)
                        }
                    })?
            }
            (Transaction::Standard(stx), ErasedTransactionResult::Success) => {
                let post_guaranteed = if let Some(gc) = &stx.guaranteed_coins {
                    self.apply(secret_keys, gc)?
                } else {
                    self.clone()
                };

                stx.fallible_coins
                    .sorted_iter()
                    .try_fold(post_guaranteed, |st, sp| {
                        st.apply(secret_keys, sp.1.deref())
                    })?
            }
            (Transaction::ClaimRewards(_), ErasedTransactionResult::Success) => self.clone(),
            (Transaction::ClaimRewards(rewards), ErasedTransactionResult::PartialSuccess(..)) => {
                // NOTE: Can only be reached through incorrect usage! Rewards can't partially
                // succeed
                error!(
                    ?rewards,
                    "processing partial success of rewards, that isn't possible!"
                );
                self.clone()
            }
        })
    }

    fn replay_events<'a>(
        &self,
        secret_keys: &SecretKeys,
        events: impl IntoIterator<Item = &'a Event<D>>,
    ) -> Result<Self, EventReplayError> {
        self.replay_events_with_changes(secret_keys, events)
            .map(|w| w.result)
    }

    fn replay_events_with_changes<'a>(
        &self,
        secret_keys: &SecretKeys,
        events: impl IntoIterator<Item = &'a Event<D>>,
    ) -> Result<WithZswapStateChanges<Self>, EventReplayError> {
        use coin_structure::transfer::SenderEvidence;

        let mut res = events.into_iter().try_fold(
            WithZswapStateChanges::new(self.clone()),
            |mut acc, event| match &event.content {
                EventDetails::ZswapInput {
                    nullifier,
                    contract: None,
                } => {
                    let maybe_change = if acc.result.coins.contains_key(nullifier) {
                        acc.result
                            .coins
                            .get(nullifier)
                            .map(|qci| ZswapStateChanges {
                                received_coins: vec![],
                                spent_coins: vec![*qci],
                                source: event.source.transaction_hash,
                            })
                    } else {
                        None
                    };
                    acc.result.coins = acc.result.coins.remove(nullifier);
                    acc.result.pending_spends = acc.result.pending_spends.remove(nullifier);

                    Ok(acc.maybe_add_change(maybe_change))
                }
                EventDetails::ZswapOutput {
                    commitment,
                    preimage_evidence,
                    mt_index,
                    ..
                } => {
                    if *mt_index != acc.result.first_free {
                        return Err(EventReplayError::NonLinearInsertion {
                            expected_next: acc.result.first_free,
                            received: *mt_index,
                            tree_name: "zswap commitment",
                        });
                    }
                    acc.result.merkle_tree =
                        acc.result
                            .merkle_tree
                            .try_update_hash(*mt_index, commitment.0, ())?;
                    acc.result.first_free += 1;
                    let maybe_change = if let Some(ci) = acc.result.pending_outputs.get(commitment)
                    {
                        let nullifier = ci.nullifier(&SenderEvidence::User(Cow::Borrowed(
                            &secret_keys.coin_secret_key,
                        )));
                        let qci = ci.qualify(*mt_index);
                        acc.result.pending_outputs = acc.result.pending_outputs.remove(commitment);
                        acc.result.coins = acc.result.coins.insert(nullifier, qci);
                        Some(ZswapStateChanges {
                            received_coins: vec![qci],
                            spent_coins: vec![],
                            source: event.source.transaction_hash,
                        })
                    } else if let Some(ci) = preimage_evidence.try_with_keys(secret_keys) {
                        let nullifier = ci.nullifier(&SenderEvidence::User(Cow::Borrowed(
                            &secret_keys.coin_secret_key,
                        )));
                        let qci = ci.qualify(*mt_index);
                        acc.result.coins = acc.result.coins.insert(nullifier, qci);
                        Some(ZswapStateChanges {
                            received_coins: vec![qci],
                            spent_coins: vec![],
                            source: event.source.transaction_hash,
                        })
                    } else {
                        acc.result.merkle_tree =
                            acc.result.merkle_tree.collapse(*mt_index, *mt_index);
                        None
                    };
                    Ok(acc.maybe_add_change(maybe_change))
                }
                #[allow(unreachable_patterns)]
                _ => Ok(acc),
            },
        )?;
        res.result.merkle_tree = res.result.merkle_tree.rehash();

        Ok(res)
    }
}

type ApplySectionResult<D> = (LedgerState<D>, Vec<Event<D>>);

pub type MaybeEvents<D> = (LedgerState<D>, Vec<Event<D>>);

impl<D: DB> LedgerState<D> {
    #[allow(unused_variables)]
    fn apply_zswap<P: Storable<D> + Deserializable>(
        &self,
        offer: &ZswapOffer<P, D>,
        whitelist: Option<Map<ContractAddress, ()>>,
        com_indices: &mut Map<Commitment, u64>,
        transaction_hash: TransactionHash,
        segment: u16,
        mut event_push: impl FnMut(Event<D>),
    ) -> Result<Self, TransactionInvalid<D>> {
        let mut state = self.clone();
        let (new_zswap, new_com_indices) = state.zswap.try_apply(offer, whitelist)?;

        state.zswap = Sp::new(new_zswap);
        *com_indices = new_com_indices;
        for input in offer
            .inputs
            .iter_deref()
            .cloned()
            .chain(offer.transient.iter().map(|t| t.as_input()))
        {
            event_push(Event {
                source: EventSource {
                    transaction_hash,
                    logical_segment: segment,
                    physical_segment: segment,
                },
                content: EventDetails::ZswapInput {
                    nullifier: input.nullifier,
                    contract: input.contract_address.clone(),
                },
            });
        }
        for output in offer
            .outputs
            .iter_deref()
            .cloned()
            .chain(offer.transient.iter().map(|t| t.as_output()))
        {
            event_push(Event {
                source: EventSource {
                    transaction_hash,
                    logical_segment: segment,
                    physical_segment: segment,
                },
                content: EventDetails::ZswapOutput {
                    commitment: output.coin_com,
                    preimage_evidence: match &output.ciphertext {
                        Some(ciph) => ZswapPreimageEvidence::Ciphertext(Box::new((**ciph).clone())),
                        None => ZswapPreimageEvidence::None,
                    },
                    contract: output.contract_address.clone(),
                    mt_index: *com_indices
                        .get(&output.coin_com)
                        .expect("processed coin must be in com_indices"),
                },
            });
        }
        Ok(state)
    }

    #[instrument(skip(self))]
    #[allow(dead_code, reason = "used by disabled treasury interactions")]
    fn native_issue_unbalanced(
        &self,
        target: coin_structure::coin::PublicKey,
        token_type: ShieldedTokenType,
        nonce: HashOutput,
        value: u128,
        event_source: EventSource,
    ) -> Result<MaybeEvents<D>, SystemTransactionError> {
        let coin = coin_structure::coin::Info {
            value,
            nonce: coin_structure::coin::Nonce(nonce),
            type_: token_type,
        };
        let recipient = coin_structure::transfer::Recipient::User(target);
        let cm = coin.commitment(&recipient);
        if self.zswap.coin_coms_set.contains_key(&cm) {
            Err(SystemTransactionError::CommitmentAlreadyPresent(cm))
        } else {
            let zswap = zswap::ledger::State {
                coin_coms: self
                    .zswap
                    .coin_coms
                    .try_update(self.zswap.first_free, &cm, None)
                    .map_err(SystemTransactionError::MerkleTreeError)?,
                coin_coms_set: self.zswap.coin_coms_set.insert(cm, ()),
                first_free: self.zswap.first_free + 1,
                ..(*self.zswap).clone()
            };
            let state = LedgerState {
                zswap: Sp::new(zswap),
                ..self.clone()
            };
            let res = (
                state,
                vec![Event {
                    source: event_source,
                    content: EventDetails::ZswapOutput {
                        commitment: cm,
                        preimage_evidence: ZswapPreimageEvidence::PublicPreimage {
                            coin,
                            recipient,
                        },
                        contract: None,
                        mt_index: self.zswap.first_free,
                    },
                }],
            );
            Ok(res)
        }
    }

    pub fn check_night_balance_invariant(&self) -> Result<(), InvariantViolation> {
        let utxo_ann = self.utxo.utxos.ann();
        let treasury_night = self
            .treasury
            .get(&TokenType::Unshielded(NIGHT))
            .copied()
            .unwrap_or(0);
        let unclaimed_rewards = self.unclaimed_block_rewards.ann().value;
        let contract_value = self.contract.ann().value;

        // Ensure the total supply of NIGHT is conserved.
        let total_night = utxo_ann.value
            + self.locked_pool
            + self.reserve_pool
            + self.block_reward_pool
            + treasury_night
            + unclaimed_rewards
            + contract_value;

        if total_night != MAX_SUPPLY {
            Err(InvariantViolation::NightBalance(total_night))
        } else {
            Ok(())
        }
    }

    #[instrument(skip(self, tx), fields(tx = ?tx.transaction_hash().0))]
    pub fn apply_system_tx(
        &self,
        tx: &SystemTransaction,
        tblock: Timestamp,
    ) -> Result<MaybeEvents<D>, SystemTransactionError> {
        let res = match tx {
            SystemTransaction::OverwriteParameters(new_params) => {
                if new_params.cardano_to_midnight_bridge_fee_basis_points > 10_000 {
                    return Err(SystemTransactionError::InvalidBasisPoints(
                        new_params.cardano_to_midnight_bridge_fee_basis_points,
                    ));
                }
                let state = LedgerState {
                    parameters: Sp::new(new_params.clone()),
                    ..self.clone()
                };
                let res = (
                    state,
                    vec![Event {
                        source: tx.event_source(),
                        content: EventDetails::ParamChange(Sp::new(new_params.clone())),
                    }],
                );
                Ok(res)
            }
            SystemTransaction::DistributeNight(kind, outputs) => {
                let total_sum = outputs
                    .iter()
                    .try_fold(0u128, |sum, o| sum.checked_add(o.amount));

                let totals = total_sum.map(|sum| match kind {
                    ClaimKind::Reward => (sum, 0),
                    ClaimKind::CardanoBridge => (0, sum),
                });
                let (claimed_total, bridged_total) = match totals {
                    Some((claim_total, cardano_bridge_total))
                        if claim_total <= self.block_reward_pool
                            && cardano_bridge_total <= self.locked_pool =>
                    {
                        (claim_total, cardano_bridge_total)
                    }
                    _ => {
                        error!(?kind, ?totals, ?outputs, supply = ?self.block_reward_pool, locked = ?self.locked_pool, "[privileged] DistributeNight rejected: insufficient pool(s)");
                        return Err(SystemTransactionError::IllegalPayout {
                            claimed_amount: totals.map(|(claimed, _)| claimed),
                            supply: self.block_reward_pool,
                            bridged_amount: totals.map(|(_, bridged)| bridged),
                            locked: self.locked_pool,
                        });
                    }
                };
                info!(?outputs, kind = ?kind, supply_before = ?self.block_reward_pool, locked_before = ?self.locked_pool, "[privileged] DistributeNight");
                let block_reward_pool = self.block_reward_pool - claimed_total;
                let locked_pool = self.locked_pool - bridged_total;
                let mut state = self.clone();
                state.block_reward_pool = block_reward_pool;
                state.locked_pool = locked_pool;

                for o @ OutputInstructionUnshielded {
                    amount,
                    target_address,
                    nonce: _,
                } in outputs
                {
                    let hash = o.clone().mk_intent_hash(NIGHT);
                    let replay_protection = state
                        .replay_protection
                        .clone()
                        .apply_member(
                            hash,
                            tblock + self.parameters.global_ttl,
                            tblock,
                            self.parameters.global_ttl,
                        )
                        .map_err(SystemTransactionError::ReplayProtectionFailure)?;

                    match kind {
                        ClaimKind::Reward => {
                            let curr_value = state
                                .unclaimed_block_rewards
                                .get(target_address)
                                .copied()
                                .unwrap_or(0);
                            let unclaimed_block_rewards = state
                                .unclaimed_block_rewards
                                .insert(*target_address, curr_value.saturating_add(*amount));

                            state = LedgerState {
                                unclaimed_block_rewards,
                                replay_protection: Sp::new(replay_protection),
                                ..state
                            };
                        }
                        ClaimKind::CardanoBridge => {
                            let (fees, post_fee_amount) = if *amount
                                < state.parameters.c_to_m_bridge_min_amount
                            {
                                (*amount, 0)
                            } else {
                                let fee = basis_points_of(
                                    state.parameters.cardano_to_midnight_bridge_fee_basis_points,
                                    *amount,
                                );
                                (fee, amount - fee)
                            };

                            let treasury = state.treasury.insert(
                                TokenType::Unshielded(NIGHT),
                                state
                                    .treasury
                                    .get(&TokenType::Unshielded(NIGHT))
                                    .copied()
                                    .unwrap_or(0)
                                    .saturating_add(fees),
                            );

                            let curr_value = state
                                .bridge_receiving
                                .get(target_address)
                                .copied()
                                .unwrap_or(0);
                            let bridge_receiving = state.bridge_receiving.insert(
                                *target_address,
                                curr_value.saturating_add(post_fee_amount),
                            );

                            state = LedgerState {
                                bridge_receiving,
                                treasury,
                                replay_protection: Sp::new(replay_protection),
                                ..state
                            };
                        }
                    }
                }

                state.check_night_balance_invariant()?;

                Ok((state, vec![]))
            }
            SystemTransaction::PayBlockRewardsToTreasury { amount } => {
                if *amount > self.block_reward_pool {
                    error!(?amount, supply = ?self.block_reward_pool, "[privileged] rewards to treasury rejected due to insufficient block reward pool");
                    return Err(SystemTransactionError::IllegalPayout {
                        claimed_amount: Some(*amount),
                        supply: self.block_reward_pool,
                        bridged_amount: None,
                        locked: self.locked_pool,
                    });
                }
                info!(?amount, supply_before = ?self.block_reward_pool, "[privileged] native token rewards to treasury");
                let mut treasury = self.treasury.clone();
                let native_token = treasury
                    .get(&TokenType::Unshielded(NIGHT))
                    .copied()
                    .unwrap_or(0)
                    .saturating_add(*amount);
                treasury = treasury.insert(TokenType::Unshielded(NIGHT), native_token);
                let state = LedgerState {
                    block_reward_pool: self.block_reward_pool - *amount,
                    treasury,
                    ..self.clone()
                };
                let res = (state, vec![]);
                Ok(res)
            }
            // NOTE: Treasury payments have been completely disabled until governance for the
            // treasury has been agreed. All code has been commented out to maximize clarity.
            SystemTransaction::PayFromTreasuryShielded { .. }
            | SystemTransaction::PayFromTreasuryUnshielded { .. } => {
                error!(
                    ?tx,
                    "[privileged] denied attempt to access treasury. The treasury is disabled until governance for it has been agreed."
                );
                Err(SystemTransactionError::TreasuryDisabled)
            }
            // SystemTransaction::PayFromTreasuryShielded {
            //     outputs,
            //     token_type,
            //     nonce,
            // } => {
            //     let mut treasury = self.treasury.clone();
            //     let tt_amount = treasury
            //         .get(&TokenType::Shielded(*token_type))
            //         .copied()
            //         .unwrap_or(0);
            //     let req_total = outputs
            //         .iter()
            //         .map(|o| o.amount)
            //         .try_fold(0u128, |acc, a| acc.checked_add(a));
            //     let req_total = match req_total {
            //         Some(v) if v <= tt_amount => v,
            //         _ => {
            //             error!(?req_total, ?token_type, ?outputs, supply = ?tt_amount, "[privileged] treasury payout rejected due to insufficient funds");
            //             return Err(SystemTransactionError::InsufficientTreasuryFunds {
            //                 requested: req_total,
            //                 actual: tt_amount,
            //                 token_type: TokenType::Shielded(*token_type),
            //             });
            //         }
            //     };
            //     info!(
            //         ?req_total,
            //         ?token_type,
            //         ?outputs,
            //         supply_before = tt_amount,
            //         "[privileged] authorized treasury payout"
            //     );
            //     treasury = treasury.insert(TokenType::Shielded(*token_type), tt_amount - req_total);
            //     let mut state = LedgerState {
            //         treasury,
            //         ..self.clone()
            //     };
            //     let mut events = vec![];
            //     for output in outputs {
            //         let res = state.native_issue_unbalanced(
            //             output.target_key,
            //             *token_type,
            //             *nonce,
            //             output.amount,
            //             tx.event_source(),
            //         )?;
            //         {
            //             state = res.0;
            //             events.extend(res.1);
            //         }
            //     }

            //     state.check_night_balance_invariant()?;

            //     let res = (state, events);
            //     Ok(res)
            // }
            // SystemTransaction::PayFromTreasuryUnshielded {
            //     outputs,
            //     token_type,
            // } => {
            //     let mut treasury = self.treasury.clone();
            //     let tt_amount = treasury
            //         .get(&TokenType::Unshielded(*token_type))
            //         .copied()
            //         .unwrap_or(0);
            //     let req_total = outputs
            //         .iter()
            //         .map(|o| o.amount)
            //         .try_fold(0u128, |acc, a| acc.checked_add(a));
            //     let req_total = match req_total {
            //         Some(v) if v <= tt_amount => v,
            //         _ => {
            //             error!(?req_total, ?token_type, ?outputs, supply = ?tt_amount, "[privileged] treasury payout rejected due to insufficient funds");
            //             return Err(SystemTransactionError::InsufficientTreasuryFunds {
            //                 requested: req_total,
            //                 actual: tt_amount,
            //                 token_type: TokenType::Unshielded(*token_type),
            //             });
            //         }
            //     };
            //     info!(
            //         ?req_total,
            //         ?token_type,
            //         ?outputs,
            //         supply_before = tt_amount,
            //         "[privileged] authorized treasury payout"
            //     );

            //     treasury =
            //         treasury.insert(TokenType::Unshielded(*token_type), tt_amount - req_total);

            //     let mut state = LedgerState {
            //         treasury,
            //         ..self.clone()
            //     };

            //     let mut events = vec![];
            //     for (i, output) in outputs.iter().enumerate() {
            //         let hash = output.clone().mk_intent_hash(*token_type);
            //         let replay_protection = self
            //             .replay_protection
            //             .clone()
            //             .apply_member(
            //                 hash,
            //                 tblock + self.parameters.global_ttl,
            //                 tblock,
            //                 self.parameters.global_ttl,
            //             )
            //             .map_err(SystemTransactionError::ReplayProtectionFailure)?;

            //         let utxo = Utxo {
            //             value: output.amount,
            //             owner: output.target_address,
            //             type_: *token_type,
            //             intent_hash: hash,
            //             output_no: i as u32,
            //         };
            //         let meta = UtxoMeta { ctime: tblock };

            //         state.utxo = Sp::new(state.utxo.clone().insert(utxo.clone(), meta));
            //         state.replay_protection = Sp::new(replay_protection);

            //         if let Some(dust_addr) = state
            //             .dust
            //             .generation
            //             .address_delegation
            //             .get(&output.target_address)
            //             && *token_type == NIGHT
            //         {
            //             let mut event_push = |content| {
            //                 events.push(Event {
            //                     source: tx.event_source(),
            //                     content,
            //                 })
            //             };
            //             state.dust = Sp::new(state.dust.fresh_dust_output(
            //                 crate::dust::initial_nonce(i as u32, hash),
            //                 0,
            //                 utxo.value,
            //                 *dust_addr,
            //                 tblock,
            //                 tblock,
            //                 &mut event_push,
            //             )?);
            //         }
            //     }

            //     state.check_night_balance_invariant()?;

            //     let res = (state, events);
            //     Ok(res)
            // }
            SystemTransaction::CNightGeneratesDustUpdate { events: actions } => {
                let mut state = self.clone();
                let mut dust_state = (*self.dust).clone();
                let mut events = vec![];
                let mut event_push = |content| {
                    events.push(Event {
                        source: tx.event_source(),
                        content,
                    })
                };
                for action in actions.iter() {
                    use crate::structure::CNightGeneratesDustActionType::*;
                    match action.action {
                        Create => {
                            dust_state = dust_state.fresh_dust_output(
                                action.nonce,
                                0,
                                action.value,
                                action.owner,
                                action.time,
                                tblock,
                                &mut event_push,
                            )?;
                        }
                        Destroy => {
                            let Some(idx) = dust_state.generation.night_indices.get(&action.nonce)
                            else {
                                error!(?action, "system transaction destroying non-tracked night");
                                debug_assert!(false);
                                continue;
                            };
                            let Some(mut gen_info) = dust_state
                                .generation
                                .generating_tree
                                .index(*idx)
                                .map(|gen_info| *gen_info.1)
                            else {
                                error!(
                                    ?action,
                                    ?idx,
                                    "invariant violated: `night_indices` reference not backed in `generating_tree`"
                                );
                                debug_assert!(false);
                                continue;
                            };
                            gen_info.dtime = action.time;
                            dust_state.generation.generating_tree = dust_state
                                .generation
                                .generating_tree
                                .try_update_hash(*idx, gen_info.merkle_hash(), gen_info)
                                .map_err(SystemTransactionError::MerkleTreeError)?
                                .rehash();
                            event_push(EventDetails::DustGenerationDtimeUpdate {
                                update: dust_state
                                    .generation
                                    .generating_tree
                                    .insertion_evidence(*idx)
                                    .expect("must be able to produce evidence for updated path"),
                                block_time: tblock,
                            });
                        }
                    }
                }
                state.dust = Sp::new(dust_state);
                Ok((state, events))
            }
            SystemTransaction::DistributeReserve(amount) => {
                if *amount > self.reserve_pool {
                    error!(?amount, reserve_supply = ?self.reserve_pool, "[privileged] reserve distribution rejected due to insufficient reserve supply");
                    return Err(SystemTransactionError::IllegalReserveDistribution {
                        distributed_amount: *amount,
                        reserve_supply: self.reserve_pool,
                    });
                }

                let reserve_pool = self.reserve_pool - *amount;
                let block_reward_pool = self.block_reward_pool + *amount;

                let new_st = LedgerState {
                    reserve_pool,
                    block_reward_pool,
                    ..self.clone()
                };

                new_st.check_night_balance_invariant()?;

                Ok((new_st, vec![]))
            }
        };
        match &res {
            Ok(res) => {
                debug!(
                    "state transition: {:?} => {:?} [system transaction {:?}]",
                    self.state_hash(),
                    res.0.state_hash(),
                    tx.transaction_hash().0
                );
                trace!(?tx, "system transaction details");
            }
            Err(e) => {
                debug!(
                    "system transaction rejection from state {:?} (system transaction {:?}): {e}",
                    self.state_hash(),
                    tx.transaction_hash().0
                );
                trace!(?tx, "system transaction details");
            }
        }
        res
    }

    #[instrument(skip(self, tx, context))]
    fn apply_section<
        S: SignatureKind<D>,
        P: ProofKind<D>,
        B: Storable<D> + PedersenDowngradeable<D> + Serializable + Tagged,
    >(
        &self,
        tx: &StandardTransaction<S, P, B, D>,
        transaction_hash: TransactionHash,
        segment: u16,
        context: &TransactionContext<D>,
    ) -> Result<ApplySectionResult<D>, TransactionInvalid<D>> {
        let mut events: Vec<Event<D>> = vec![];
        let mut state: LedgerState<D> = self.clone();
        if segment == 0 {
            // Apply replay protection
            state.replay_protection = Sp::new(
                state
                    .replay_protection
                    .apply_tx(tx, context.block_context.tblock, self.parameters.global_ttl)
                    .map_err(|e| TransactionInvalid::ReplayProtectionViolation(e))?,
            );

            let mut com_indices = Map::new();
            if let Some(offer) = &tx.guaranteed_coins {
                state = state.apply_zswap(
                    offer,
                    context.whitelist.clone(),
                    &mut com_indices,
                    transaction_hash,
                    segment,
                    |event| events.push(event),
                )?;
            }

            // Make sure all fallible offers *can* be applied
            let _ = tx.fallible_coins.sorted_values_by_key().try_fold(
                (*state.zswap).clone(),
                |st, offer| {
                    st.try_apply(&offer, context.whitelist.clone())
                        .map(|(x, _)| x)
                },
            )?;

            #[allow(unused_variables)]
            for (phys_seg, intent) in tx.intents.sorted_iter() {
                let erased = intent.erase_proofs().erase_signatures();
                if let Some(offer) = &intent.guaranteed_unshielded_offer {
                    state.utxo = Sp::new(state.utxo.apply_offer(offer, &erased, segment, context)?);
                    {
                        state.dust = Sp::new(state.dust.apply_offer(
                            offer,
                            &erased,
                            segment,
                            context,
                            |content| {
                                events.push(Event {
                                    source: EventSource {
                                        transaction_hash,
                                        logical_segment: segment,
                                        physical_segment: *phys_seg,
                                    },
                                    content,
                                })
                            },
                        )?);
                    }
                }

                let res = state.apply_actions(
                    &Vec::from(&intent.actions),
                    true,
                    context,
                    erased,
                    &com_indices,
                    EventSource {
                        transaction_hash,
                        logical_segment: segment,
                        physical_segment: *phys_seg,
                    },
                )?;
                state = res.0;
                events.extend(res.1);
            }
            {
                // process fees and dust actions first. This is not in the segment part of
                // `apply_segment`, as they are not processed segment-by-segment.
                //
                // NOTE: The `unwrap_or` is safe here, as fees have already been
                // checked during well-formedness.
                let mut fees_remaining = Transaction::Standard(tx.clone())
                    .fees(&self.parameters, true)
                    .unwrap_or(0);
                // apply spends first, to make sure registration outputs get the maximum dust they can.
                let intents = tx.intents.sorted_iter().collect::<Vec<_>>();
                for (phys_seg, time, dust_spend) in intents.iter().flat_map(|(phys_seg, i)| {
                    i.dust_actions.iter().flat_map(move |da| {
                        da.spends.iter_deref().map(move |s| (phys_seg, da.ctime, s))
                    })
                }) {
                    state.dust = Sp::new(state.dust.apply_spend(
                        dust_spend,
                        time,
                        context,
                        &state.parameters.dust,
                        |content| {
                            events.push(Event {
                                source: EventSource {
                                    transaction_hash,
                                    physical_segment: **phys_seg,
                                    logical_segment: segment,
                                },
                                content,
                            })
                        },
                    )?);
                    fees_remaining = fees_remaining.saturating_sub(dust_spend.v_fee);
                }
                // then apply registrations
                for (phys_seg, intent) in tx.intents.sorted_iter() {
                    let erased = intent.erase_proofs().erase_signatures();
                    if let Some(da) = intent.dust_actions.as_ref() {
                        for reg in da.registrations.iter_deref() {
                            let (new_dust, new_fees_remaining) = state.dust.apply_registration(
                                &context.ref_state.utxo,
                                fees_remaining,
                                &erased,
                                reg,
                                &state.parameters.dust,
                                da.ctime,
                                context,
                                |content| {
                                    events.push(Event {
                                        source: EventSource {
                                            transaction_hash,
                                            physical_segment: *phys_seg,
                                            logical_segment: segment,
                                        },
                                        content,
                                    })
                                },
                            )?;
                            state.dust = Sp::new(new_dust);
                            fees_remaining = new_fees_remaining;
                        }
                    }
                }
                if fees_remaining != 0 {
                    error!(
                        "Reached end of fee accounting stage with {fees_remaining} SPECKs of Dust not paid. This is either a ledger accounting bug, `well_formed` was not checked correctly, or the fee parameters changed after it was checked."
                    );
                }
            }
        } else {
            let mut com_indices = Map::new();
            if let Some(offer) = tx.fallible_coins.get(&segment) {
                state = state.apply_zswap(
                    &offer,
                    context.whitelist.clone(),
                    &mut com_indices,
                    transaction_hash,
                    segment,
                    |event| events.push(event),
                )?;
            }

            if let Some(intent) = tx.intents.get(&segment) {
                let erased = intent.erase_proofs().erase_signatures();
                if let Some(offer) = &intent.fallible_unshielded_offer {
                    state.utxo = Sp::new(state.utxo.apply_offer(offer, &erased, segment, context)?);
                    {
                        state.dust = Sp::new(state.dust.apply_offer(
                            offer,
                            &erased,
                            segment,
                            context,
                            |content| {
                                events.push(Event {
                                    source: EventSource {
                                        transaction_hash,
                                        logical_segment: 0,
                                        physical_segment: segment,
                                    },
                                    content,
                                })
                            },
                        )?);
                    }
                }

                let res = state.apply_actions(
                    &Vec::from(&intent.actions),
                    false,
                    context,
                    erased,
                    &com_indices,
                    EventSource {
                        transaction_hash,
                        logical_segment: 0,
                        physical_segment: segment,
                    },
                )?;
                state = res.0;
                events.extend(res.1);
            }
        }

        state.check_night_balance_invariant()?;

        trace!("transaction phase {segment} successfully applied");
        Ok((state, events))
    }

    pub fn batch_apply_independant(
        &self,
        txs: &[VerifiedTransaction<D>],
        context: &TransactionContext<D>,
    ) -> Vec<TransactionResult<D>> {
        let st: LedgerState<D> = self.clone();
        let mut res = Vec::new();
        for tx in txs.iter() {
            res.push(st.apply(tx, context).1);
        }
        res
    }

    pub fn batch_apply_all_or_nothing(
        &self,
        txs: &[VerifiedTransaction<D>],
        context: &TransactionContext<D>,
    ) -> Result<(Self, Vec<TransactionResult<D>>), TransactionInvalid<D>> {
        let mut state = self.clone();
        let mut res = Vec::with_capacity(txs.len());
        for tx in txs {
            let (state2, txres) = state.apply(tx, context);
            if let TransactionResult::Failure(err) = txres {
                return Err(err);
            } else {
                res.push(txres);
            }
            state = state2;
        }
        Ok((state, res))
    }

    #[allow(clippy::type_complexity)]
    pub fn batch_apply_until_first_failure<'a>(
        &self,
        txs: &'a [VerifiedTransaction<D>],
        context: &TransactionContext<D>,
    ) -> Result<
        (Self, Vec<TransactionResult<D>>),
        Box<(
            // The state before the first failure
            Self,
            // The results up to the failure
            Vec<TransactionResult<D>>,
            // The failure itself
            TransactionInvalid<D>,
            // The remaining transactions, including the failing one
            &'a [VerifiedTransaction<D>],
        )>,
    > {
        let mut state = self.clone();
        let mut res = Vec::with_capacity(txs.len());
        for (i, tx) in txs.iter().enumerate() {
            let (state2, txres) = state.apply(tx, context);
            if let TransactionResult::Failure(err) = txres {
                return Err(Box::new((state, res, err, &txs[i..])));
            } else {
                res.push(txres);
            }
            state = state2;
        }
        Ok((state, res))
    }

    #[instrument(skip(self, tx, context), fields(tx = ?tx.hash))]
    pub fn apply(
        &self,
        tx: &VerifiedTransaction<D>,
        context: &TransactionContext<D>,
    ) -> (Self, TransactionResult<D>) {
        let res = match &tx.inner {
            Transaction::Standard(stx) => {
                let cloned_stx = stx.clone();
                let segments = cloned_stx.segments();
                let mut segment_success = std::collections::BTreeMap::new();
                let mut events = Vec::new();
                let mut total_success = true;
                let mut new_st = self.clone();
                for &segment in segments.iter() {
                    match new_st.apply_section(stx, tx.hash, segment, context) {
                        Ok(state) => {
                            new_st = state.0;
                            events.extend(state.1);
                            segment_success.insert(segment, Ok(()));
                        }
                        Err(e) => {
                            if segment == 0 {
                                return (self.clone(), TransactionResult::Failure(e));
                            } else {
                                segment_success.insert(segment, Err(e));
                                total_success = false;
                            }
                        }
                    }
                }

                (
                    new_st,
                    if total_success {
                        TransactionResult::Success(events)
                    } else {
                        TransactionResult::PartialSuccess(segment_success, events)
                    },
                )
            }
            Transaction::ClaimRewards(rewards) => claim_unshielded::<D>(
                self,
                rewards,
                &context.block_context,
                EventSource {
                    transaction_hash: tx.hash,
                    logical_segment: 0,
                    physical_segment: 0,
                },
            ),
        };
        debug!(
            "state transition: {:?} => {:?} [transaction {:?}]",
            self.state_hash(),
            res.0.state_hash(),
            tx.hash,
        );
        debug!(
            "transaction result: {:?}",
            ErasedTransactionResult::from(&res.1)
        );
        trace!(?tx, "transaction details");
        res
    }

    fn apply_actions<P: ProofKind<D>>(
        &self,
        calls: &[ContractAction<P, D>],
        guaranteed: bool,
        context: &TransactionContext<D>,
        parent_intent: Intent<(), (), Pedersen, D>,
        com_indices: &Map<Commitment, u64>,
        event_source: EventSource,
    ) -> Result<MaybeEvents<D>, TransactionInvalid<D>> {
        let mut res = self.clone();
        let mut events = vec![];
        for call in calls.iter() {
            match call {
                ContractAction::Call(call) => {
                    if !whitelist_matches(&context.whitelist, &call.address) {
                        continue;
                    } else if let Some(cstate) = res.index(call.address) {
                        let mut qcontext = QueryContext::new(cstate.data.clone(), call.address);
                        let call_context = (**call).clone().context(
                            &context.block_context,
                            &parent_intent,
                            cstate.clone(),
                            com_indices,
                        );
                        qcontext.call_context = call_context;
                        let transcript = if guaranteed {
                            call.guaranteed_transcript.as_ref()
                        } else {
                            call.fallible_transcript.as_ref()
                        };
                        if let Some(transcript) = transcript {
                            let results = qcontext.run_transcript(
                                transcript,
                                &self.parameters.cost_model.runtime_cost_model,
                            )?;
                            if results.context.effects != transcript.effects {
                                return Err(TransactionInvalid::EffectsMismatch {
                                    declared: Box::new(transcript.effects.clone()),
                                    actual: Box::new(results.context.effects),
                                });
                            }
                            let mut new_balance = cstate.balance.clone();
                            for (token_type, val) in transcript.effects.unshielded_inputs.clone() {
                                let bal = new_balance.get(&token_type).map(|x| *x).unwrap_or(0);
                                new_balance = new_balance.insert(
                                    token_type,
                                    bal.checked_add(val).ok_or(
                                        TransactionInvalid::BalanceCheckOutOfBounds {
                                            token_type,
                                            current_balance: bal,
                                            operation_value: val,
                                            operation: BalanceOperation::Addition,
                                        },
                                    )?,
                                );
                            }
                            for (token_type, val) in transcript.effects.unshielded_outputs.clone() {
                                let bal = new_balance.get(&token_type).map(|x| *x).unwrap_or(0);
                                new_balance = new_balance.insert(
                                    token_type,
                                    bal.checked_sub(val).ok_or(
                                        TransactionInvalid::BalanceCheckOutOfBounds {
                                            token_type,
                                            current_balance: bal,
                                            operation_value: val,
                                            operation: BalanceOperation::Subtraction,
                                        },
                                    )?,
                                );
                            }
                            for event in results.events {
                                events.push(Event {
                                    source: event_source.clone(),
                                    content: EventDetails::ContractLog {
                                        address: call.address,
                                        entry_point: call.entry_point.clone(),
                                        logged_item: event,
                                    },
                                });
                            }
                            res =
                                res.update_index(call.address, results.context.state, new_balance);
                        }
                    } else {
                        warn!(?call.address, "contract not present");
                        return Err(TransactionInvalid::ContractNotPresent(call.address));
                    }
                }
                ContractAction::Deploy(deploy) => {
                    let addr = deploy.address();
                    if !whitelist_matches(&context.whitelist, &addr) || guaranteed {
                        continue;
                    } else {
                        if res.contract.contains_key(&addr) {
                            return Err(TransactionInvalid::ContractAlreadyDeployed(addr));
                        }
                        res.contract = res.contract.insert(addr, deploy.initial_state.clone());
                        events.push(Event {
                            source: event_source.clone(),
                            content: EventDetails::ContractDeploy {
                                address: addr,
                                initial_state: deploy.initial_state.clone(),
                            },
                        });
                    }
                }
                ContractAction::Maintain(upd) => {
                    let addr = upd.address;
                    if !whitelist_matches(&context.whitelist, &addr) || guaranteed {
                        continue;
                    } else {
                        let mut cstate = match res.contract.get(&addr) {
                            Some(st) => st.clone(),
                            None => return Err(TransactionInvalid::ContractNotPresent(addr)),
                        };
                        if cstate.maintenance_authority.counter != upd.counter {
                            return Err(TransactionInvalid::ReplayCounterMismatch(addr));
                        }
                        cstate.maintenance_authority.counter =
                            cstate.maintenance_authority.counter.saturating_add(1);
                        for op in upd.updates.iter_deref() {
                            match op {
                                SingleUpdate::ReplaceAuthority(auth) => {
                                    cstate.maintenance_authority = auth.clone()
                                }
                                SingleUpdate::VerifierKeyRemove(ep, ver) => {
                                    let mut op = match cstate.operations.get(ep) {
                                        Some(op) => op.deref().clone(),
                                        None => {
                                            return Err(TransactionInvalid::VerifierKeyNotFound(
                                                ep.clone(),
                                                ver.clone(),
                                            ));
                                        }
                                    };
                                    ver.rm_from(&mut op);
                                    if op == ContractOperation::new(None) {
                                        cstate.operations = cstate.operations.remove(ep);
                                    } else {
                                        cstate.operations =
                                            cstate.operations.insert(ep.clone(), op);
                                    }
                                }
                                SingleUpdate::VerifierKeyInsert(ep, vk) => {
                                    let mut op = match cstate.operations.get(ep) {
                                        Some(op) => (*op).clone(),
                                        None => ContractOperation::new(None),
                                    };
                                    if vk.as_version().has(&op) {
                                        return Err(TransactionInvalid::VerifierKeyAlreadyPresent(
                                            ep.clone(),
                                            vk.as_version(),
                                        ));
                                    }
                                    vk.insert_into(&mut op);
                                    cstate.operations = cstate.operations.insert(ep.clone(), op);
                                }
                            }
                        }
                        res.contract = res.contract.insert(addr, cstate);
                    }
                }
            }
        }
        let res = (res, events);
        Ok(res)
    }

    #[instrument(skip(self))]
    pub fn post_block_update(
        &self,
        tblock: Timestamp,
        detailed_block_fullness: NormalizedCost,
        overall_block_fullness: FixedPoint,
    ) -> Result<Self, BlockLimitExceeded> {
        let mut new_st = self.clone();
        let values = [
            &detailed_block_fullness.read_time,
            &detailed_block_fullness.compute_time,
            &detailed_block_fullness.block_usage,
            &detailed_block_fullness.bytes_written,
            &detailed_block_fullness.bytes_churned,
            &overall_block_fullness,
        ];
        if values
            .iter()
            .any(|v| **v < FixedPoint::ZERO || **v > FixedPoint::ONE)
        {
            debug!(
                "post block update rejection at state {:?}: {}",
                self.state_hash(),
                BlockLimitExceeded
            );
            return Err(BlockLimitExceeded);
        }
        let fee_prices = self.parameters.fee_prices.update_from_fullness(
            detailed_block_fullness,
            overall_block_fullness,
            self.parameters.cost_dimension_min_ratio,
            self.parameters.price_adjustment_a_parameter,
        );
        new_st.parameters = Sp::new(LedgerParameters {
            fee_prices,
            ..(*self.parameters).clone()
        });
        new_st.replay_protection = Sp::new(new_st.replay_protection.post_block_update(tblock));
        new_st.zswap = Sp::new(new_st.zswap.post_block_update(tblock));
        new_st.dust = Sp::new(
            new_st
                .dust
                .post_block_update(tblock, self.parameters.global_ttl),
        );
        debug!(
            "state transition: {:?} => {:?} [post block update]",
            self.state_hash(),
            new_st.state_hash()
        );
        Ok(new_st)
    }
}

const fn basis_points_of(points: u32, val: u128) -> u128 {
    assert!(
        points <= 10_000,
        "cardano_to_midnight_bridge_fee_basis_points must not exceed 10_000"
    );

    // `val` should never be high enough to overflow but let's do it safely regardless
    let quotient = val / 10_000;
    let remainder = val % 10_000;

    quotient * points as u128 + (remainder * points as u128) / 10_000
}

fn claim_unshielded<D: DB>(
    state: &LedgerState<D>,
    tx: &ClaimRewardsTransaction<(), D>,
    context: &BlockContext,
    event_source: EventSource,
) -> (LedgerState<D>, TransactionResult<D>) {
    let old_state = state.clone();
    let address = UserAddress::from(tx.owner.clone());

    let claimable = match tx.kind {
        ClaimKind::CardanoBridge => {
            // We can use the full amount since bridge fees are taken at-source
            old_state
                .bridge_receiving
                .get(&address)
                .copied()
                .unwrap_or(0)
        }
        ClaimKind::Reward => old_state
            .unclaimed_block_rewards
            .get(&address)
            .copied()
            .unwrap_or(0),
    };

    if tx.value > claimable {
        return (
            old_state,
            TransactionResult::Failure(TransactionInvalid::InsufficientClaimable {
                requested: tx.value,
                claimable,
                claimant: address,
                kind: tx.kind,
            }),
        );
    }

    if tx.kind == ClaimKind::Reward && tx.value < state.parameters.min_claimable_rewards() {
        return (
            old_state,
            TransactionResult::Failure(TransactionInvalid::RewardTooSmall {
                claimed: tx.value,
                minimum: state.parameters.min_claimable_rewards(),
            }),
        );
    }

    let remaining = claimable - tx.value;

    let res = match tx.kind {
        ClaimKind::CardanoBridge => Either::Left(if remaining == 0 {
            state.bridge_receiving.remove(&address)
        } else {
            state.bridge_receiving.insert(address, remaining)
        }),
        ClaimKind::Reward => Either::Right(if remaining == 0 {
            state.unclaimed_block_rewards.remove(&address)
        } else {
            state.unclaimed_block_rewards.insert(address, remaining)
        }),
    };

    // NOTE: There are no fees payable; this is either covered by the bridge fees,
    // or fees are explicitly not taken for block rewards over a minimum threshold.
    let hash = OutputInstructionUnshielded {
        amount: tx.value,
        target_address: address,
        nonce: tx.nonce,
    }
    .clone()
    .mk_intent_hash(NIGHT);
    let replay_protection = match state.replay_protection.clone().apply_member(
        hash,
        context.tblock + state.parameters.global_ttl,
        context.tblock,
        state.parameters.global_ttl,
    ) {
        Ok(v) => v,
        Err(e) => {
            return (
                old_state,
                TransactionResult::Failure(TransactionInvalid::ReplayProtectionViolation(e)),
            );
        }
    };
    let output = Utxo {
        value: tx.value, // Note that the fees have already been taken from the `unclaimed_block_rewards` entry (or taken from the bridge transfer at-source)
        owner: address,
        type_: NIGHT,
        intent_hash: hash,
        output_no: 0,
    };
    let meta = UtxoMeta {
        ctime: context.tblock,
    };
    let utxo = state.utxo.clone().insert(output.clone(), meta);
    #[allow(unused_mut)]
    let mut state = match res {
        Either::Left(bridge_receiving) => LedgerState {
            bridge_receiving,
            utxo: Sp::new(utxo),
            replay_protection: Sp::new(replay_protection),
            ..state.clone()
        },
        Either::Right(unclaimed_block_rewards) => LedgerState {
            unclaimed_block_rewards,
            utxo: Sp::new(utxo),
            replay_protection: Sp::new(replay_protection),
            ..state.clone()
        },
    };
    let mut events = vec![];
    if let Some(dust_addr) = state.dust.generation.address_delegation.get(&address) {
        let mut event_push = |content| {
            events.push(Event {
                source: event_source.clone(),
                content,
            })
        };
        let new_dust = match state.dust.fresh_dust_output(
            crate::dust::initial_nonce(0, hash),
            0,
            output.value,
            *dust_addr,
            context.tblock,
            context.tblock,
            &mut event_push,
        ) {
            Ok(v) => v,
            Err(e) => return (old_state, TransactionResult::Failure(e.into())),
        };
        state.dust = Sp::new(new_dust);
    }

    if let Err(e) = state.check_night_balance_invariant() {
        return (old_state, TransactionResult::Failure(e.into()));
    }

    let res = TransactionResult::Success(events);
    (state, res)
}

impl<D: DB> UtxoState<D> {
    pub fn apply_offer<S: SignatureKind<D>>(
        &self,
        offer: &UnshieldedOffer<S, D>,
        parent: &ErasedIntent<D>,
        segment_id: u16,
        context: &TransactionContext<D>,
    ) -> Result<Self, TransactionInvalid<D>> {
        let mut res = self.clone();
        for input in offer.inputs.iter_deref() {
            let input_utxo = Utxo::from(input.clone());
            let is_member = res.utxos.contains_key(&input_utxo);
            if !is_member {
                return Err(TransactionInvalid::InputNotInUtxos(Box::new(
                    input_utxo.clone(),
                )));
            }

            // self.utxos -= inputs;
            res = res.remove(&input_utxo);
        }

        let intent_hash = parent.intent_hash(segment_id);
        let outputs: HashSet<Utxo, D> = offer
            .outputs
            .iter()
            .enumerate()
            .map(|(output_no, output)| Utxo {
                value: output.value,
                owner: output.owner,
                type_: output.type_,
                intent_hash,
                // Cast safe, as we assume transactions with less than 4B outputs.
                output_no: output_no as u32,
            })
            .collect();

        // The below is *not* needed, due to the uniqueness of outputs.
        // assert!(self.utxo.intersection(outputs).is_empty());

        for output in outputs.iter() {
            let meta = UtxoMeta {
                ctime: context.block_context.tblock,
            };
            res = res.insert((**output).clone(), meta);
        }
        Ok(res)
    }
}

impl<D: DB> ReplayProtectionState<D> {
    pub fn apply_member(
        &self,
        hash: IntentHash,
        ttl: Timestamp,
        tblock: Timestamp,
        global_ttl: Duration,
    ) -> Result<Self, TransactionApplicationError> {
        if self.time_filter_map.contains(&hash) {
            Err(TransactionApplicationError::IntentAlreadyExists)?
        }

        if ttl < tblock {
            Err(TransactionApplicationError::IntentTtlExpired(ttl, tblock))?
        }

        if ttl > tblock + global_ttl {
            Err(TransactionApplicationError::IntentTtlTooFarInFuture(
                ttl,
                tblock + global_ttl,
            ))?
        }

        Ok(ReplayProtectionState {
            time_filter_map: self.time_filter_map.upsert_one(ttl, hash),
        })
    }

    pub fn apply_intent<
        S: SignatureKind<D>,
        P: ProofKind<D>,
        B: Storable<D> + PedersenDowngradeable<D> + Serializable,
    >(
        &self,
        intent: Intent<S, P, B, D>,
        tblock: Timestamp,
        global_ttl: Duration,
    ) -> Result<Self, TransactionApplicationError> {
        let hash = intent
            .erase_proofs()
            .erase_signatures()
            // The spec uses a hash independent of the segment ID here to prevent
            // a malleable replay in a different segment. In most cases something else stops that,
            // but this is slightly more restrictive.
            .intent_hash(0);
        self.apply_member(hash, intent.ttl, tblock, global_ttl)
    }

    pub fn apply_tx<
        S: SignatureKind<D>,
        P: ProofKind<D>,
        B: Storable<D> + PedersenDowngradeable<D> + Serializable,
    >(
        &self,
        stx: &StandardTransaction<S, P, B, D>,
        tblock: Timestamp,
        global_ttl: Duration,
    ) -> Result<Self, TransactionApplicationError> {
        stx.intents
            .sorted_iter()
            .try_fold(self.clone(), |st, segment_id_x_intent| {
                st.apply_intent::<S, P, B>(
                    segment_id_x_intent.1.deref().clone(),
                    tblock,
                    global_ttl,
                )
            })
    }

    pub fn post_block_update(&self, tblock: Timestamp) -> Self {
        ReplayProtectionState {
            time_filter_map: self.time_filter_map.filter(tblock),
        }
    }
}

impl<S: SignatureKind<D>, P: ProofKind<D>, D: DB> Transaction<S, P, PedersenRandomness, D> {
    pub fn seal<R: Rng + CryptoRng + SplittableRng>(
        &self,
        mut rng: R,
    ) -> Transaction<S, P, PureGeneratorPedersen, D> {
        match self {
            Transaction::Standard(standard_transaction) => {
                let mut intents: storage::storage::HashMap<
                    u16,
                    Intent<S, P, PureGeneratorPedersen, D>,
                    D,
                > = storage::storage::HashMap::new();
                for (segment_id, x) in standard_transaction
                    .intents
                    .sorted_iter()
                    .map(|seg_x_intent| (*seg_x_intent.0.deref(), seg_x_intent.1.deref().clone()))
                {
                    intents = intents.insert(segment_id, x.seal(rng.split(), segment_id));
                }

                Transaction::Standard(StandardTransaction {
                    network_id: standard_transaction.network_id.clone(),
                    intents,
                    guaranteed_coins: standard_transaction.guaranteed_coins.clone(),
                    fallible_coins: standard_transaction.fallible_coins.clone(),
                    binding_randomness: standard_transaction.binding_randomness,
                })
            }
            Transaction::ClaimRewards(claim_rewards_transaction) => {
                Transaction::ClaimRewards(claim_rewards_transaction.clone())
            }
        }
    }
}

impl<S: SignatureKind<D>, P: ProofKind<D>, D: DB> Intent<S, P, PedersenRandomness, D> {
    pub fn seal<R: Rng + CryptoRng + SplittableRng>(
        &self,
        mut rng: R,
        segment_id: u16,
    ) -> Intent<S, P, PureGeneratorPedersen, D> {
        Intent {
            guaranteed_unshielded_offer: self.guaranteed_unshielded_offer.clone(),
            fallible_unshielded_offer: self.fallible_unshielded_offer.clone(),
            actions: self.actions.clone(),
            dust_actions: self.dust_actions.clone(),
            ttl: self.ttl,
            binding_commitment: PureGeneratorPedersen::new_from(
                &mut rng,
                &self.binding_commitment,
                &self.challenge_pre_for(segment_id),
            ),
        }
    }
}

impl SystemTransaction {
    fn event_source(&self) -> EventSource {
        EventSource {
            transaction_hash: self.transaction_hash(),
            logical_segment: 0,
            physical_segment: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        annotation::NightAnn,
        structure::{INITIAL_PARAMETERS, LedgerParameters},
    };

    use super::*;
    use base_crypto::signatures::VerifyingKey;
    use rand::{SeedableRng, rngs::StdRng};
    use storage::db::InMemoryDB;
    use zswap::ledger::State;

    use crate::dust::DustState;

    #[test]
    fn simple_bridge_claim() {
        let mut rng = StdRng::seed_from_u64(0x42);

        let network_id = "a".to_string();
        let vk: VerifyingKey = rng.r#gen();
        let mut bridge_receiving = Map::new();
        bridge_receiving = bridge_receiving.insert(UserAddress::from(vk.clone()), MAX_SUPPLY);
        let state = LedgerState {
            network_id: network_id.clone(),
            parameters: Sp::<LedgerParameters, InMemoryDB>::new(INITIAL_PARAMETERS),
            locked_pool: 0,
            bridge_receiving,
            reserve_pool: 0,
            block_reward_pool: 0,
            unclaimed_block_rewards: Map::new(),
            treasury: Map::new(),
            zswap: Sp::new(State::default()),
            contract: Map::new(),
            utxo: Sp::new(UtxoState::default()),
            replay_protection: Sp::new(ReplayProtectionState::new()),
            dust: Sp::new(DustState::default()),
        };
        let rewards: ClaimRewardsTransaction<(), InMemoryDB> = ClaimRewardsTransaction {
            network_id,
            value: MAX_SUPPLY,
            owner: vk,
            nonce: rng.r#gen(),
            signature: (),
            kind: ClaimKind::CardanoBridge,
        };
        let context = BlockContext::default();
        match claim_unshielded(
            &state,
            &rewards,
            &context,
            EventSource {
                transaction_hash: TransactionHash(rng.r#gen()),
                logical_segment: 0,
                physical_segment: 0,
            },
        ) {
            (new_st, TransactionResult::Success(_)) => {
                if new_st.bridge_receiving != Map::new() {
                    panic!(
                        "bridge_receiving should now be empty, but is: {:?}",
                        new_st.bridge_receiving
                    )
                }
            }
            (_, e) => panic!("{:?}", e),
        }
    }

    #[test]
    fn bridge_claim_not_enough_available() {
        let mut rng = StdRng::seed_from_u64(0x42);

        let network_id = "a".to_string();
        let vk: VerifyingKey = rng.r#gen();
        let mut bridge_receiving = Map::new();
        bridge_receiving = bridge_receiving.insert(UserAddress::from(vk.clone()), 1000);
        let state = LedgerState {
            network_id: network_id.clone(),
            parameters: Sp::<LedgerParameters, InMemoryDB>::new(INITIAL_PARAMETERS),
            locked_pool: 10,
            bridge_receiving,
            reserve_pool: 10,
            block_reward_pool: 0,
            unclaimed_block_rewards: Map::new(),
            treasury: Map::new(),
            zswap: Sp::new(State::default()),
            contract: Map::new(),
            utxo: Sp::new(UtxoState::default()),
            replay_protection: Sp::new(ReplayProtectionState::new()),
            dust: Sp::new(DustState::default()),
        };
        let rewards: ClaimRewardsTransaction<(), InMemoryDB> = ClaimRewardsTransaction {
            network_id,
            value: 1001,
            owner: vk,
            nonce: rng.r#gen(),
            signature: (),
            kind: ClaimKind::CardanoBridge,
        };
        let context = BlockContext::default();
        match claim_unshielded(
            &state,
            &rewards,
            &context,
            EventSource {
                transaction_hash: TransactionHash(rng.r#gen()),
                logical_segment: 0,
                physical_segment: 0,
            },
        ) {
            (_, TransactionResult::Failure(TransactionInvalid::InsufficientClaimable { .. })) => (),
            (_, TransactionResult::Failure(e)) => {
                panic!("Expected InsufficientClaimable, but got {e}")
            }
            (_, TransactionResult::PartialSuccess(e, _)) => {
                panic!("Expected InsufficientClaimable, but got {:?}", e)
            }
            (_, TransactionResult::Success(_)) => {
                panic!("Expected InsufficientClaimable, succeeded unexpectedly")
            }
        }
    }

    #[test]
    fn simple_rewards_claim() {
        let mut rng = StdRng::seed_from_u64(0x42);

        let network_id = "a".to_string();
        let vk: VerifyingKey = rng.r#gen();
        let mut unclaimed_block_rewards: Map<UserAddress, u128, InMemoryDB, NightAnn> = Map::new();
        unclaimed_block_rewards =
            unclaimed_block_rewards.insert(UserAddress::from(vk.clone()), MAX_SUPPLY);
        let state = LedgerState {
            network_id: network_id.clone(),
            parameters: Sp::<LedgerParameters, InMemoryDB>::new(INITIAL_PARAMETERS),
            locked_pool: 0,
            bridge_receiving: Map::new(),
            reserve_pool: 0,
            block_reward_pool: 0,
            unclaimed_block_rewards,
            treasury: Map::new(),
            zswap: Sp::new(State::default()),
            contract: Map::new(),
            utxo: Sp::new(UtxoState::default()),
            replay_protection: Sp::new(ReplayProtectionState::new()),
            dust: Sp::new(DustState::default()),
        };
        let rewards: ClaimRewardsTransaction<(), InMemoryDB> = ClaimRewardsTransaction {
            network_id,
            value: MAX_SUPPLY,
            owner: vk,
            nonce: rng.r#gen(),
            signature: (),
            kind: ClaimKind::Reward,
        };
        let context = BlockContext::default();
        match claim_unshielded(
            &state,
            &rewards,
            &context,
            EventSource {
                transaction_hash: TransactionHash(rng.r#gen()),
                logical_segment: 0,
                physical_segment: 0,
            },
        ) {
            (new_st, TransactionResult::Success(_)) => {
                if new_st.unclaimed_block_rewards != Map::new() {
                    panic!(
                        "unclaimed_block_rewards should now be empty, but is: {:?}",
                        new_st.unclaimed_block_rewards
                    )
                }
            }
            (_, e) => panic!("{:?}", e),
        }
    }

    #[test]
    fn rewards_claim_not_enough_available() {
        let mut rng = StdRng::seed_from_u64(0x42);

        let network_id = "a".to_string();
        let vk: VerifyingKey = rng.r#gen();
        let mut unclaimed_block_rewards: Map<UserAddress, u128, InMemoryDB, NightAnn> = Map::new();
        unclaimed_block_rewards =
            unclaimed_block_rewards.insert(UserAddress::from(vk.clone()), 200000);
        let state = LedgerState {
            network_id: network_id.clone(),
            parameters: Sp::<LedgerParameters, InMemoryDB>::new(INITIAL_PARAMETERS),
            locked_pool: 10,
            bridge_receiving: Map::new(),
            reserve_pool: 10,
            block_reward_pool: 0,
            unclaimed_block_rewards,
            treasury: Map::new(),
            zswap: Sp::new(State::default()),
            contract: Map::new(),
            utxo: Sp::new(UtxoState::default()),
            replay_protection: Sp::new(ReplayProtectionState::new()),
            dust: Sp::new(DustState::default()),
        };
        let rewards: ClaimRewardsTransaction<(), InMemoryDB> = ClaimRewardsTransaction {
            network_id,
            value: 1000001,
            owner: vk,
            nonce: rng.r#gen(),
            signature: (),
            kind: ClaimKind::Reward,
        };
        let context = BlockContext::default();
        match claim_unshielded(
            &state,
            &rewards,
            &context,
            EventSource {
                transaction_hash: TransactionHash(rng.r#gen()),
                logical_segment: 0,
                physical_segment: 0,
            },
        ) {
            (_, TransactionResult::Failure(TransactionInvalid::InsufficientClaimable { .. })) => (),
            (_, TransactionResult::Failure(e)) => {
                panic!("Expected InsufficientClaimable, but got {e}")
            }
            (_, TransactionResult::PartialSuccess(e, _)) => {
                panic!("Expected InsufficientClaimable, but got {:?}", e)
            }
            (_, TransactionResult::Success(_)) => {
                panic!("Expected InsufficientClaimable, succeeded unexpectedly")
            }
        }
    }

    fn sample_utxo(n: u128) -> (Utxo, UtxoMeta) {
        (
            Utxo {
                value: n,
                owner: UserAddress(HashOutput([0; 32])),
                type_: NIGHT,
                intent_hash: IntentHash(HashOutput([0; 32])),
                output_no: 0,
            },
            UtxoMeta {
                ctime: Timestamp::from_secs(0),
            },
        )
    }

    #[test]
    fn insert_updates_annotation() {
        let state: UtxoState<InMemoryDB> = Default::default();
        let (utxo, meta) = sample_utxo(15);

        let new_state = state.insert(utxo.clone(), meta);

        assert!(new_state.utxos.contains_key(&utxo));

        let ann = new_state.utxos.ann();
        assert_eq!(ann.size, 1);
        assert_eq!(ann.value, 15);
    }

    #[test]
    fn remove_updates_annotation() {
        let (utxo, meta) = sample_utxo(100);
        let mut state: UtxoState<InMemoryDB> = Default::default();
        state = state.insert(utxo.clone(), meta);

        let new_state = state.remove(&utxo);
        assert!(!new_state.utxos.contains_key(&utxo));

        let ann = new_state.utxos.ann();
        assert_eq!(ann.size, 0);
        assert_eq!(ann.value, 0);
    }

    #[test]
    fn multiple_inserts_accumulate() {
        let state: UtxoState<InMemoryDB> = Default::default();
        let (a, ma) = sample_utxo(7);
        let (b, mb) = sample_utxo(11);

        let state = state.insert(a.clone(), ma);
        let state = state.insert(b.clone(), mb);

        let ann = state.utxos.ann();
        assert_eq!(ann.size, 2);
        assert_eq!(ann.value, 18);

        assert!(state.utxos.contains_key(&a));
        assert!(state.utxos.contains_key(&b));
    }

    #[test]
    fn remove_one_of_many() {
        let (a, ma) = sample_utxo(5);
        let (b, mb) = sample_utxo(9);

        let mut state: UtxoState<InMemoryDB> = Default::default();
        state = state.insert(a.clone(), ma);
        state = state.insert(b.clone(), mb);

        let new_utxos = state.remove(&a);
        state = new_utxos;

        let ann = state.utxos.ann();
        assert_eq!(ann.size, 1);
        assert_eq!(ann.value, 9);

        assert!(!state.utxos.contains_key(&a));
        assert!(state.utxos.contains_key(&b));
    }
}
