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

use crate::construct::ContractCallPrototype;
use crate::dust::{
    DustActions, DustLocalState, DustOutput, DustPublicKey, DustRegistration, DustResolver,
    DustSecretKey,
};
use crate::error::{MalformedTransaction, SystemTransactionError, TransactionProvingError};
use crate::events::Event;
pub use crate::prove::Resolver;
use crate::semantics::{TransactionContext, TransactionResult};
use crate::structure::INITIAL_PARAMETERS;
use crate::structure::{
    BindingKind, ClaimKind, ClaimRewardsTransaction, ContractDeploy, Intent, LedgerState,
    MaintenanceUpdate, OutputInstructionUnshielded, PedersenDowngradeable, ProofKind,
    ProofPreimageMarker, SignatureKind, SystemTransaction, Transaction, UnshieldedOffer, Utxo,
    UtxoOutput,
};
#[cfg(feature = "proving")]
use crate::structure::{INITIAL_LIMITS, SPECKS_PER_DUST};
#[cfg(feature = "proving")]
use crate::structure::{ProofMarker, ProofPreimageVersioned, ProofVersioned};
use crate::verify::WellFormedStrictness;
use base_crypto::cost_model::{FixedPoint, NormalizedCost};
use base_crypto::data_provider::{self, MidnightDataProvider};
use base_crypto::rng::SplittableRng;
use base_crypto::signatures::{Signature, SigningKey};
use base_crypto::time::{Duration, Timestamp};
use coin_structure::coin::{
    Info as CoinInfo, NIGHT, ShieldedTokenType, TokenType, UnshieldedTokenType, UserAddress,
};
use derive_where::derive_where;
use lazy_static::lazy_static;
use onchain_runtime::context::BlockContext;
#[cfg(feature = "proving")]
use onchain_runtime::cost_model::INITIAL_COST_MODEL;
use rand::{CryptoRng, Rng, seq::SliceRandom};
#[cfg(feature = "proving")]
use reqwest::Client;
use serialize::{Serializable, Tagged};
#[cfg(feature = "proving")]
use serialize::{tagged_deserialize, tagged_serialize};
use std::collections::VecDeque;
use std::env;
use std::io;
use storage::Storable;
use storage::arena::{ArenaHash, Sp};
use storage::db::DB;
use storage::storage::{HashMap, HashSet, default_storage};
#[cfg(feature = "proving")]
use transient_crypto::commitment::PureGeneratorPedersen;
use transient_crypto::commitment::{Pedersen, PedersenRandomness};
#[cfg(feature = "proving")]
use transient_crypto::curve::Fr;
use transient_crypto::proofs::KeyLocation;
use transient_crypto::proofs::VerifierKey;
#[cfg(feature = "proving")]
use transient_crypto::proofs::{ProverKey, ProvingProvider, Resolver as ResolverT, WrappedIr};
#[cfg(feature = "proving")]
use zkir_v2::{IrSource, LocalProvingProvider};
use zswap::keys::SecretKeys;
use zswap::local::State as ZswapLocalState;
use zswap::prove::ZswapResolver;
use zswap::{Delta, Offer as ZswapOffer, Output as ZswapOutput};

#[cfg(feature = "proving")]
pub type Pk = ProverKey<IrSource>;
#[cfg(not(feature = "proving"))]
pub type Pk = ();

#[cfg(feature = "proving")]
pub type Tx<S, D> = Transaction<S, ProofMarker, PedersenRandomness, D>;
#[cfg(not(any(feature = "proving")))]
pub type Tx<S, D> = Transaction<S, (), Pedersen, D>;

#[cfg(feature = "proving")]
pub type TxBound<S, D> = Transaction<S, ProofMarker, PureGeneratorPedersen, D>;
#[cfg(not(any(feature = "proving")))]
pub type TxBound<S, D> = Transaction<S, (), Pedersen, D>;

lazy_static! {
    pub static ref PUBLIC_PARAMS: ZswapResolver = ZswapResolver(
        MidnightDataProvider::new(
            data_provider::FetchMode::OnDemand,
            data_provider::OutputMode::Log,
            zswap::ZSWAP_EXPECTED_FILES.to_owned(),
        )
        .unwrap()
    );
}

#[non_exhaustive]
#[derive(PartialEq, Eq, Clone)]
pub enum TestProcessingMode {
    Regular,
    /// Disables some processing around UTXO and Dust processing that may, due to tracking local
    /// state, lead to non-linear transaction processing. Primarily for use during setup of large
    /// test states, and not 'normal' tests.
    ForceConstantTime,
}

#[derive_where(Clone)]
pub struct TestState<D: DB> {
    pub ledger: LedgerState<D>,
    pub zswap: ZswapLocalState<D>,
    pub utxos: HashSet<Utxo, D>,
    pub dust: DustLocalState<D>,
    pub events: Vec<Event<D>>,

    pub time: Timestamp,

    pub zswap_keys: SecretKeys,
    pub night_key: SigningKey,
    pub dust_key: DustSecretKey,

    pub mode: TestProcessingMode,
    pub past_state_window: VecDeque<ArenaHash<D::Hasher>>,
    pub past_state_window_len: usize,
    pub debug_print: bool,
}

impl<D: DB> TestState<D> {
    pub fn new(rng: &mut (impl Rng + CryptoRng)) -> Self {
        TestState {
            ledger: LedgerState::new("local-test"),
            zswap: ZswapLocalState::new(),
            utxos: HashSet::new(),
            dust: DustLocalState::new(INITIAL_PARAMETERS.dust),
            events: Vec::new(),

            time: Timestamp::from_secs(0),

            zswap_keys: SecretKeys::from_rng_seed(&mut *rng),
            night_key: SigningKey::sample(&mut *rng),
            dust_key: DustSecretKey::sample(&mut *rng),

            mode: TestProcessingMode::Regular,
            past_state_window: VecDeque::new(),
            past_state_window_len: 20,
            debug_print: false,
        }
    }

    pub fn swizzle_to_db(&mut self) {
        use std::ops::Deref;
        let mut lstate = Sp::new(self.ledger.clone());
        let mut zstate = Sp::new(self.zswap.clone());
        let mut dstate = Sp::new(self.dust.clone());
        lstate.persist();
        zstate.persist();
        dstate.persist();
        let new_roots = [lstate.hash(), zstate.hash(), dstate.hash()];
        self.past_state_window.extend(new_roots);
        lstate.unload();
        zstate.unload();
        dstate.unload();
        self.ledger = lstate.deref().clone();
        self.zswap = zstate.deref().clone();
        self.dust = dstate.deref().clone();
        default_storage::<D>().with_backend(|b| {
            while self.past_state_window.len() > self.past_state_window_len {
                b.unpersist(&self.past_state_window.pop_front().unwrap());
            }
            b.flush_all_changes_to_db();
        });
    }

    pub fn context(&self) -> TransactionContext<D> {
        let block = BlockContext {
            tblock: self.time,
            ..BlockContext::default()
        };
        TransactionContext {
            ref_state: self.ledger.clone(),
            block_context: block,
            whitelist: None,
        }
    }

    pub async fn reward_night(&mut self, rng: &mut (impl CryptoRng + SplittableRng), amount: u128) {
        let amount = u128::max(amount, self.ledger.parameters.min_claimable_rewards());
        let address = UserAddress::from(self.night_key.verifying_key());

        // Distribute reserve
        let sys_tx_distribute = SystemTransaction::DistributeReserve(amount);

        let (ledger, _) = self
            .ledger
            .apply_system_tx(&sys_tx_distribute, self.time)
            .unwrap();

        self.ledger = ledger;

        // Rewards
        let sys_tx_rewards = SystemTransaction::DistributeNight(
            ClaimKind::Reward,
            vec![OutputInstructionUnshielded {
                amount,
                target_address: address,
                nonce: rng.r#gen(),
            }],
        );

        let new_ledger = self
            .ledger
            .apply_system_tx(&sys_tx_rewards, self.time)
            .unwrap()
            .0;

        self.ledger = new_ledger;

        let nonce = rng.r#gen();
        let tx = tx_prove(
            rng.split(),
            &Transaction::<(), _, _, _>::ClaimRewards(ClaimRewardsTransaction {
                network_id: "local-test".into(),
                value: amount,
                owner: self.night_key.verifying_key(),
                nonce,
                signature: (),
                kind: ClaimKind::Reward,
            }),
            &test_resolver(""),
        )
        .await
        .unwrap();
        let strictness = WellFormedStrictness::default();
        self.assert_apply(&tx, strictness);
    }

    pub async fn rewards_unshielded(
        &mut self,
        rng: &mut (impl CryptoRng + SplittableRng),
        token: UnshieldedTokenType,
        amount: u128,
    ) {
        if token == NIGHT {
            return self.reward_night(rng, amount).await;
        }

        let utxo = UtxoOutput {
            owner: UserAddress::from(self.night_key.verifying_key()),
            type_: token,
            value: amount,
        };
        let offer: UnshieldedOffer<(), D> = UnshieldedOffer {
            inputs: vec![].into(),
            outputs: vec![utxo].into(),
            signatures: vec![].into(),
        };
        let mut intent = Intent::empty(rng, self.time);
        intent.guaranteed_unshielded_offer = Some(Sp::new(offer));
        let tx = Transaction::from_intents("local-test", HashMap::new().insert(1u16, intent));
        let strictness = WellFormedStrictness {
            enforce_balancing: false,
            ..WellFormedStrictness::default()
        };
        self.assert_apply(&tx, strictness);
    }

    pub fn rewards_shielded(
        &mut self,
        rng: &mut (impl Rng + CryptoRng),
        token: ShieldedTokenType,
        amount: u128,
    ) {
        let coin = CoinInfo {
            nonce: rng.r#gen(),
            value: amount,
            type_: token,
        };
        let output = zswap::Output::<_, D>::new(
            rng,
            &coin,
            Some(0u16),
            &self.zswap_keys.coin_public_key(),
            Some(self.zswap_keys.enc_public_key()),
        )
        .expect("output creation must succeed");
        let offer = zswap::Offer {
            inputs: vec![].into(),
            outputs: vec![output].into(),
            transient: vec![].into(),
            deltas: vec![Delta {
                token_type: token,
                value: -(amount as i128),
            }]
            .into(),
        };
        let tx = Transaction::<(), _, _, D>::new(
            "local-test",
            HashMap::new(),
            Some(offer),
            HashMap::new(),
        );
        let strictness = WellFormedStrictness {
            enforce_balancing: false,
            ..WellFormedStrictness::default()
        };
        self.assert_apply(&tx, strictness);
    }

    pub fn fast_forward(&mut self, dur: Duration) {
        assert!(dur.as_seconds() > 0);
        self.time += dur;
        self.ledger = self
            .ledger
            .post_block_update(
                self.time,
                NormalizedCost::ZERO,
                FixedPoint::from_u64_div(1, 2),
            )
            .unwrap();
        if self.mode != TestProcessingMode::ForceConstantTime {
            self.dust = self.dust.process_ttls(self.time);
        }
    }

    pub fn step(&mut self) {
        self.fast_forward(Duration::from_secs(10))
    }

    fn dust_generation_register(&mut self, rng: &mut (impl Rng + CryptoRng)) {
        let reg: DustRegistration<(), D> = DustRegistration {
            allow_fee_payment: 0,
            dust_address: Some(Sp::new(DustPublicKey::from(self.dust_key.clone()))),
            night_key: self.night_key.verifying_key(),
            signature: None,
        };
        let actions = DustActions {
            spends: vec![].into(),
            registrations: vec![reg].into(),
            ctime: self.time,
        };
        let mut intent = Intent::empty(rng, self.time);
        intent.dust_actions = Some(Sp::new(actions));
        let tx = Transaction::from_intents("local-test", HashMap::new().insert(1, intent));
        let strictness = WellFormedStrictness {
            enforce_balancing: false,
            verify_signatures: false,
            ..WellFormedStrictness::default()
        };
        self.assert_apply(&tx, strictness);
    }

    pub async fn give_fee_token(
        &mut self,
        rng: &mut (impl CryptoRng + SplittableRng),
        utxos: usize,
    ) {
        use crate::structure::STARS_PER_NIGHT;

        self.dust_generation_register(rng);
        for _ in 0..utxos {
            self.reward_night(rng, 5 * STARS_PER_NIGHT).await
        }
        self.fast_forward(self.ledger.parameters.dust.time_to_cap());
    }

    pub fn apply_system_tx(
        &mut self,
        tx: &SystemTransaction,
    ) -> Result<(), SystemTransactionError> {
        use crate::semantics::ZswapLocalStateExt;

        let res = self.ledger.apply_system_tx(tx, self.time)?;
        let (res, events) = res;
        // Reapply to test determinism
        let cmp = self.ledger.apply_system_tx(tx, self.time)?.0;
        assert_eq!(res.state_hash(), cmp.state_hash());
        self.ledger = res;
        self.zswap = self
            .zswap
            .replay_events(&self.zswap_keys, events.iter())
            .expect("just applied transaction should replay");
        self.dust = self
            .dust
            .replay_events(&self.dust_key, events.iter())
            .expect("just applied transaction should replay");
        let pk = UserAddress::from(self.night_key.verifying_key());
        let utxo_iter = self
            .ledger
            .utxo
            .utxos
            .iter()
            .map(|kv| (*kv.0).clone())
            .filter(|utxo| utxo.owner == pk);

        self.utxos = match self.mode {
            TestProcessingMode::ForceConstantTime => utxo_iter.take(10).collect(),
            _ => utxo_iter.collect(),
        };
        self.step();
        Ok(())
    }

    pub fn apply<
        S: SignatureKind<D>,
        P: ProofKind<D>,
        B: PedersenDowngradeable<D> + Serializable + Storable<D> + BindingKind<S, P, D> + Tagged,
    >(
        &mut self,
        tx: &Transaction<S, P, B, D>,
        strictness: WellFormedStrictness,
    ) -> Result<TransactionResult<D>, MalformedTransaction<D>> {
        use crate::semantics::ZswapLocalStateExt;

        let context = self.context();
        let vtx = tx.well_formed(&self.ledger, strictness, self.time)?;
        let (new_st, result) = self.ledger.apply(&vtx, &context);
        // Reapply to test determinism
        let cmp = self.ledger.apply(&vtx, &context).0;
        assert_eq!(new_st.state_hash(), cmp.state_hash());
        self.ledger = new_st;
        self.zswap = self
            .zswap
            .replay_events(&self.zswap_keys, result.events())
            .expect("just applied transaction should replay");
        self.dust = self
            .dust
            .replay_events(&self.dust_key, result.events())
            .expect("just applied transaction should replay");
        let pk = UserAddress::from(self.night_key.verifying_key());
        let utxo_iter = self
            .ledger
            .utxo
            .utxos
            .iter()
            .map(|kv| (*kv.0).clone())
            .filter(|utxo| utxo.owner == pk);

        self.utxos = match self.mode {
            TestProcessingMode::ForceConstantTime => utxo_iter.take(10).collect(),
            _ => utxo_iter.collect(),
        };
        self.step();
        Ok(result)
    }

    pub fn assert_apply<
        S: SignatureKind<D>,
        P: ProofKind<D>,
        B: PedersenDowngradeable<D> + Serializable + Storable<D> + BindingKind<S, P, D> + Tagged,
    >(
        &mut self,
        tx: &Transaction<S, P, B, D>,
        strictness: WellFormedStrictness,
    ) {
        if self.debug_print {
            dbg!(tx.cost(&self.ledger.parameters, false)).ok();
            dbg!(tx.validation_cost(&self.ledger.parameters.cost_model));
            dbg!(tx.application_cost(&self.ledger.parameters.cost_model));
            dbg!(
                tx.cost(&self.ledger.parameters, false)
                    .ok()
                    .and_then(|cost| cost.normalize(self.ledger.parameters.limits.block_limits))
            );
            dbg!(&tx.erase_proofs());
        }
        let res = self
            .apply(tx, strictness)
            .expect("transaction should be well-formed");
        let success = matches!(res, TransactionResult::Success(..));
        if !success {
            panic!("transaction application failure: {res:?}");
        }
    }

    pub async fn balance_tx<
        S: SignatureKind<D> + Tagged,
        P: ProofKindExt<B, D>,
        B: Serializable + Clone + PedersenDowngradeable<D> + Storable<D>,
    >(
        &mut self,
        mut rng: impl CryptoRng + SplittableRng,
        mut tx: Transaction<S, P, B, D>,
        resolver: &Resolver,
    ) -> Result<Transaction<S, P, B, D>, MalformedTransaction<D>> {
        let fees = None;
        let balance = tx.balance(fees)?;
        let zswap_to_balance = balance
            .iter()
            .filter_map(|((tt, seg), val)| match tt {
                TokenType::Shielded(tt) if *val < 0 => Some(((*tt, *seg), *val)),
                _ => None,
            })
            .map(|((tt, seg), val)| {
                let mut total_inp = 0;
                let input_coins = self
                    .zswap
                    .coins
                    .iter()
                    .filter(|(_, qci)| qci.type_ == tt)
                    .take_while(|(_, qci)| {
                        let res = total_inp < (-val) as u128;
                        if res {
                            total_inp += qci.value;
                        }
                        res
                    })
                    .collect::<Vec<_>>();
                let output_val = total_inp.saturating_sub((-val) as u128);
                let inputs = input_coins
                    .into_iter()
                    .map(|(_, qci)| {
                        let (next_state, inp) = self
                            .zswap
                            .spend(&mut rng, &self.zswap_keys, &qci, Some(seg))
                            .unwrap(); // TODO: unwrap
                        self.zswap = next_state;
                        inp
                    })
                    .collect::<Vec<_>>();
                let output_coin = CoinInfo {
                    nonce: rng.r#gen(),
                    type_: tt,
                    value: output_val,
                };
                let output = ZswapOutput::new(
                    &mut rng,
                    &output_coin,
                    Some(seg),
                    &self.zswap_keys.coin_public_key(),
                    Some(self.zswap_keys.enc_public_key()),
                )
                .unwrap(); // TODO: unwrap
                let offer = ZswapOffer {
                    inputs: inputs.into_iter().collect(),
                    outputs: if output_val > 0 {
                        vec![output].into()
                    } else {
                        vec![].into()
                    },
                    transient: vec![].into(),
                    deltas: vec![Delta {
                        token_type: tt,
                        value: (total_inp - output_val) as i128,
                    }]
                    .into(),
                };
                if seg == 0 {
                    Transaction::<S, ProofPreimageMarker, PedersenRandomness, D>::new(
                        "local-test",
                        HashMap::new(),
                        Some(offer),
                        HashMap::new(),
                    )
                } else {
                    let mut fc = HashMap::new();
                    fc = fc.insert(seg, offer);
                    Transaction::new("local-test", HashMap::new(), None, fc)
                }
            })
            .collect::<Vec<_>>();
        for txb in zswap_to_balance.into_iter() {
            let txb = P::from_unproven(rng.split(), resolver, txb).await;
            tx = tx.merge(&txb)?;
        }
        let mut merged_tx = tx.clone();
        let mut unproven_bal = None;
        let old_dust = self.dust.clone();
        let mut last_dust = 0;
        while let Some(mut dust) = merged_tx
            .balance(Some(merged_tx.fees(&self.ledger.parameters, false)?))?
            .get(&(TokenType::Dust, 0))
            .and_then(|bal| (*bal < 0).then_some((-*bal) as u128))
        {
            dust += last_dust;
            last_dust = dust;
            if self.mode != TestProcessingMode::ForceConstantTime && self.debug_print {
                eprintln!(
                    "balancing {dust} Dust atomic units / wallet balance: {} Dust atomic units",
                    self.dust.wallet_balance(self.time)
                );
            }
            let mut spends = storage::storage::Array::new();
            let mut utxos = old_dust.utxos().collect::<Vec<_>>();
            utxos.shuffle(&mut rng);
            for qdo in utxos.into_iter() {
                if dust == 0 {
                    break;
                }
                let gen_info = old_dust.generation_info(&qdo).unwrap(); // TODO unwrap
                let value = DustOutput::from(qdo).updated_value(
                    &gen_info,
                    self.time,
                    &self.ledger.parameters.dust,
                );
                let v_fee = u128::min(value, dust);
                if self.debug_print {
                    eprintln!("adding utxo of {v_fee} Dust atomic units");
                }
                dust = dust.saturating_sub(value);
                let (new_dust, spend) = self
                    .dust
                    .spend(&self.dust_key, &qdo, v_fee, self.time)
                    .unwrap(); // TODO unwrap
                self.dust = new_dust;
                spends = spends.push(spend);
            }
            if dust > 0 {
                panic!("failed to balance testing transaction's dust");
            }
            let mut intent = Intent::empty(&mut rng, self.time);
            intent.dust_actions = Some(Sp::new(DustActions {
                spends,
                registrations: vec![].into(),
                ctime: self.time,
            }));
            let hm = HashMap::new().insert(0xFEED, intent);
            let tx2_unproven = Transaction::from_intents("local-test", hm);
            let tx2_mock = P::mock_from_unproven(tx2_unproven.clone());
            merged_tx = tx.merge(&tx2_mock)?;
            unproven_bal = Some(tx2_unproven);
        }
        if let Some(unproven) = unproven_bal {
            let proven = P::from_unproven(rng, resolver, unproven).await;
            merged_tx = tx.merge(&proven)?;
        }
        // TODO: Balance unshielded
        Ok(merged_tx)
    }
}

pub async fn verifier_key(resolver: &Resolver, name: &'static str) -> Option<VerifierKey> {
    use serialize::tagged_deserialize;
    use transient_crypto::proofs::Resolver;
    let proof_data = resolver
        .resolve_key(KeyLocation(std::borrow::Cow::Borrowed(name)))
        .await
        .ok()??;
    tagged_deserialize(&mut &proof_data.verifier_key[..]).ok()
}

pub fn test_resolver(test_name: &'static str) -> Resolver {
    use transient_crypto::proofs::ProvingKeyMaterial;

    let test_dir = env::var("MIDNIGHT_LEDGER_TEST_STATIC_DIR")
        .expect("MIDNIGHT_LEDGER_TEST_STATIC_DIR should be set as env variable");

    Resolver::new(
        PUBLIC_PARAMS.clone(),
        DustResolver(
            MidnightDataProvider::new(
                data_provider::FetchMode::OnDemand,
                data_provider::OutputMode::Log,
                crate::dust::DUST_EXPECTED_FILES.to_owned(),
            )
            .unwrap(),
        ),
        Box::new(move |KeyLocation(loc)| {
            let sync_block = || {
                let read_file = |dir, ext| {
                    let path = format!("{test_dir}/{test_name}/{dir}/{loc}.{ext}");
                    let res = std::fs::read(path);
                    match res {
                        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
                        Err(e) => Err(e),
                        Ok(v) => Ok(Some(v)),
                    }
                };
                let Some(prover_key) = read_file("keys", "prover")? else {
                    return Ok(None);
                };
                let Some(verifier_key) = read_file("keys", "verifier")? else {
                    return Ok(None);
                };
                let Some(ir_source) = read_file("zkir", "bzkir")? else {
                    return Ok(None);
                };
                Ok(Some(ProvingKeyMaterial {
                    prover_key,
                    verifier_key,
                    ir_source,
                }))
            };
            let res = sync_block();
            Box::pin(std::future::ready(res))
        }),
    )
}

#[derive(Debug)]
pub enum ClientProvingError<D: DB> {
    Io(io::Error),
    Local(TransactionProvingError<D>),
    Reqwest(reqwest::Error),
}

pub async fn tx_prove_bind<
    S: SignatureKind<D> + Tagged,
    R: Rng + CryptoRng + SplittableRng,
    D: DB,
>(
    #[allow(unused_mut)] mut rng: R,
    tx: &Transaction<S, ProofPreimageMarker, PedersenRandomness, D>,
    #[cfg_attr(not(feature = "proving"), allow(unused_variables))] resolver: &Resolver,
) -> Result<TxBound<S, D>, ClientProvingError<D>> {
    #[cfg(feature = "proving")]
    {
        Ok(tx_prove(rng.split(), tx, resolver).await?.seal(rng))
    }
    #[cfg(not(feature = "proving"))]
    {
        tx_prove(rng, tx, resolver).await
    }
}

pub async fn tx_prove<S: SignatureKind<D> + Tagged, R: Rng + CryptoRng + SplittableRng, D: DB>(
    #[cfg_attr(feature = "proving", allow(unused_mut))] mut _rng: R,
    tx: &Transaction<S, ProofPreimageMarker, PedersenRandomness, D>,
    #[cfg_attr(not(feature = "proving"), allow(unused_variables))] resolver: &Resolver,
) -> Result<Tx<S, D>, ClientProvingError<D>> {
    #[cfg(feature = "proving")]
    {
        if let Ok(addr) = env::var("MIDNIGHT_PROOF_SERVER") {
            let provider = ProofServerProvider {
                base_url: addr,
                resolver,
            };
            tx.prove(provider, &INITIAL_COST_MODEL)
                .await
                .map_err(ClientProvingError::Local)
        } else {
            let provider = LocalProvingProvider {
                rng: _rng.split(),
                params: resolver,
                resolver,
            };
            // Duplication because ProvingProvider isn't dyn-compatible :(
            let proven = tx
                .prove(provider, &INITIAL_COST_MODEL)
                .await
                .map_err(ClientProvingError::Local)?;
            // Test mocked proofs while we're here
            let mocked = tx.mock_prove();
            // Mocked proof should succeed iff there are no contract calls
            assert_eq!(mocked.is_ok(), tx.calls().next().is_none());
            // If it does, the fees associated with the mocked proof should
            // match the real one.
            if let Ok(mocked) = tx.mock_prove() {
                let allowed_error_margin: u128 = base_crypto::cost_model::SyntheticCost {
                    block_usage: 5,
                    ..Default::default()
                }
                .normalize(INITIAL_LIMITS.block_limits)
                .map(|norm| {
                    INITIAL_PARAMETERS
                        .fee_prices
                        .overall_cost(&norm)
                        .into_atomic_units(SPECKS_PER_DUST)
                })
                .unwrap_or(u128::MAX);
                let mocked_fees = mocked.fees(&INITIAL_PARAMETERS, false);
                let real_fees = proven.seal(_rng.split()).fees(&INITIAL_PARAMETERS, false);
                if let (Ok(real_fees), Ok(mocked_fees)) = (real_fees, mocked_fees) {
                    assert!(real_fees <= mocked_fees);
                    assert!(mocked_fees <= real_fees + allowed_error_margin);
                }
            }
            Ok(proven)
        }
    }
    #[cfg(not(feature = "proving"))]
    Ok(tx.erase_proofs())
}

#[cfg(feature = "proving")]
#[derive(Clone)]
pub struct ProofServerProvider<'a> {
    pub base_url: String,
    pub resolver: &'a Resolver,
}

#[cfg(feature = "proving")]
impl ProofServerProvider<'_> {
    fn is_builtin_key(loc: &KeyLocation) -> bool {
        [
            "midnight/zswap/spend",
            "midnight/zswap/output",
            "midnight/zswap/sign",
            "midnight/dust/spend",
        ]
        .contains(&loc.0.as_ref())
    }
    pub async fn check_request_body(
        &self,
        preimage: &ProofPreimageVersioned,
    ) -> Result<Vec<u8>, anyhow::Error> {
        let ir = if Self::is_builtin_key(preimage.key_location()) {
            None
        } else {
            let data = self
                .resolver
                .resolve_key(preimage.key_location().clone())
                .await?
                .ok_or_else(|| {
                    anyhow::anyhow!("failed to find key '{}'", &preimage.key_location().0)
                })?;
            Some(WrappedIr(data.ir_source))
        };
        let mut res = Vec::new();
        tagged_serialize(&(preimage.clone(), ir), &mut res)?;
        Ok(res)
    }

    pub async fn proving_request_body(
        &self,
        preimage: &ProofPreimageVersioned,
        overwrite_binding_input: Option<Fr>,
    ) -> Result<Vec<u8>, anyhow::Error> {
        let data = if Self::is_builtin_key(preimage.key_location()) {
            None
        } else {
            self.resolver
                .resolve_key(preimage.key_location().clone())
                .await?
        };
        let mut res = Vec::new();
        tagged_serialize(&(preimage.clone(), data, overwrite_binding_input), &mut res)?;
        Ok(res)
    }
}

#[cfg(feature = "proving")]
impl ProvingProvider for ProofServerProvider<'_> {
    async fn check(
        &self,
        preimage: &transient_crypto::proofs::ProofPreimage,
    ) -> Result<Vec<Option<usize>>, anyhow::Error> {
        let ser = self
            .check_request_body(&ProofPreimageVersioned::V2(std::sync::Arc::new(
                preimage.clone(),
            )))
            .await?;
        println!("    Check request: {} bytes", ser.len());
        let resp = Client::new()
            .post(format!("{}/check", &self.base_url))
            .body(ser)
            .send()
            .await?;
        if resp.status().is_success() {
            let bytes = resp.bytes().await?;
            println!("    Check response: {} bytes", bytes.len());
            let res: Vec<Option<u64>> = tagged_deserialize(&mut bytes.to_vec().as_slice())?;
            Ok(res.into_iter().map(|i| i.map(|i| i as usize)).collect())
        } else {
            anyhow::bail!(
                "proving server error: {}",
                resp.text().await.expect("error retrieving error")
            )
        }
    }
    async fn prove(
        self,
        preimage: &transient_crypto::proofs::ProofPreimage,
        overwrite_binding_input: Option<transient_crypto::curve::Fr>,
    ) -> Result<transient_crypto::proofs::Proof, anyhow::Error> {
        let ser = self
            .proving_request_body(
                &ProofPreimageVersioned::V2(std::sync::Arc::new(preimage.clone())),
                overwrite_binding_input,
            )
            .await?;
        println!("    Proving request: {} bytes", ser.len());
        let resp = Client::new()
            .post(format!("{}/prove", &self.base_url))
            .body(ser)
            .send()
            .await?;
        if resp.status().is_success() {
            let bytes = resp.bytes().await?;
            println!("    Proving response: {} bytes", bytes.len());
            let proof: ProofVersioned = tagged_deserialize(&mut bytes.to_vec().as_slice())?;
            match proof {
                ProofVersioned::V2(proof) => Ok(proof),
            }
        } else {
            anyhow::bail!(
                "proving server error: {}",
                resp.text().await.expect("error retrieving error")
            )
        }
    }
    fn split(&mut self) -> Self {
        self.clone()
    }
}

#[cfg(feature = "proving")]
#[deprecated = "/prove-tx is deprecated, consider using the ProofServerProvider instead"]
pub async fn serialize_request_body<S: SignatureKind<D> + Tagged, D: DB>(
    tx: &Transaction<S, ProofPreimageMarker, PedersenRandomness, D>,
    resolver: &Resolver,
) -> Result<Vec<u8>, ClientProvingError<D>> {
    use serialize::tagged_serialize;
    use transient_crypto::proofs::Resolver;

    let circuits_used = tx
        .calls()
        .map(|(_, c)| String::from_utf8_lossy(&c.entry_point).into_owned())
        .collect::<Vec<_>>();
    let mut keys = std::collections::HashMap::new();
    for k in circuits_used.into_iter() {
        let k = KeyLocation(std::borrow::Cow::Owned(k));
        let data = resolver
            .resolve_key(k.clone())
            .await
            .map_err(ClientProvingError::Io)?;
        if let Some(data) = data {
            keys.insert(k, data);
        }
    }
    let mut ser = Vec::new();
    tagged_serialize(&(tx, keys), &mut ser).map_err(ClientProvingError::Io)?;
    Ok(ser)
}

pub trait ProofKindExt<B: Storable<D>, D: DB>: ProofKind<D> {
    // Allowed as this is testing-only API
    #[allow(async_fn_in_trait)]
    async fn from_unproven<S: SignatureKind<D> + Tagged>(
        rng: impl CryptoRng + SplittableRng,
        resolver: &Resolver,
        tx: Transaction<S, ProofPreimageMarker, PedersenRandomness, D>,
    ) -> Transaction<S, Self, B, D>;
    fn mock_from_unproven<S: SignatureKind<D> + Tagged>(
        tx: Transaction<S, ProofPreimageMarker, PedersenRandomness, D>,
    ) -> Transaction<S, Self, B, D>;
}

impl<D: DB> ProofKindExt<PedersenRandomness, D> for ProofPreimageMarker {
    async fn from_unproven<S: SignatureKind<D> + Tagged>(
        _rng: impl CryptoRng + SplittableRng,
        _resolver: &Resolver,
        tx: Transaction<S, ProofPreimageMarker, PedersenRandomness, D>,
    ) -> Transaction<S, Self, PedersenRandomness, D> {
        tx
    }
    fn mock_from_unproven<S: SignatureKind<D> + Tagged>(
        tx: Transaction<S, ProofPreimageMarker, PedersenRandomness, D>,
    ) -> Transaction<S, Self, PedersenRandomness, D> {
        tx
    }
}

impl<D: DB> ProofKindExt<Pedersen, D> for () {
    async fn from_unproven<S: SignatureKind<D> + Tagged>(
        mut _rng: impl CryptoRng + SplittableRng,
        _resolver: &Resolver,
        tx: Transaction<S, ProofPreimageMarker, PedersenRandomness, D>,
    ) -> Transaction<S, Self, Pedersen, D> {
        tx.erase_proofs()
    }
    fn mock_from_unproven<S: SignatureKind<D> + Tagged>(
        tx: Transaction<S, ProofPreimageMarker, PedersenRandomness, D>,
    ) -> Transaction<S, Self, Pedersen, D> {
        tx.erase_proofs()
    }
}

#[cfg(feature = "proving")]
impl<D: DB> ProofKindExt<PureGeneratorPedersen, D> for ProofMarker {
    async fn from_unproven<S: SignatureKind<D> + Tagged>(
        mut rng: impl CryptoRng + SplittableRng,
        resolver: &Resolver,
        tx: Transaction<S, ProofPreimageMarker, PedersenRandomness, D>,
    ) -> Transaction<S, Self, PureGeneratorPedersen, D> {
        tx_prove(rng.split(), &tx, resolver)
            .await
            .unwrap()
            .seal(rng)
    }
    fn mock_from_unproven<S: SignatureKind<D> + Tagged>(
        tx: Transaction<S, ProofPreimageMarker, PedersenRandomness, D>,
    ) -> Transaction<S, Self, PureGeneratorPedersen, D> {
        tx.mock_prove().unwrap()
    }
}

#[cfg(feature = "proving")]
#[allow(clippy::type_complexity, clippy::result_large_err)]
pub fn well_formed_tx_builder<
    R: Rng + CryptoRng + SplittableRng,
    S: SignatureKind<D> + Tagged,
    D: DB,
>(
    mut rng: R,
    secret_key: &SecretKeys,
    resolver: &Resolver,
) -> Result<Transaction<S, ProofMarker, PedersenRandomness, D>, ClientProvingError<D>> {
    use coin_structure::coin::ShieldedTokenType;

    const REWARDS_AMOUNT: u128 = 5000000000;
    let token = ShieldedTokenType(Default::default());
    let coin = CoinInfo::new(&mut rng, REWARDS_AMOUNT, token);
    let out = ZswapOutput::new(&mut rng, &coin, None, &secret_key.coin_public_key(), None).unwrap();
    let offer = ZswapOffer {
        inputs: vec![].into(),
        outputs: vec![out].into(),
        transient: vec![].into(),
        deltas: vec![Delta {
            token_type: token,
            value: -(REWARDS_AMOUNT as i128),
        }]
        .into(),
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        tx_prove(
            rng,
            &Transaction::new(
                "local-test",
                storage::storage::HashMap::new(),
                Some(offer),
                storage::storage::HashMap::new(),
            ),
            resolver,
        )
        .await
    })
}

pub fn test_intents<D: DB, R: Rng + CryptoRng + ?Sized>(
    rng: &mut R,
    calls: Vec<ContractCallPrototype<D>>,
    updates: Vec<MaintenanceUpdate<D>>,
    deploys: Vec<ContractDeploy<D>>,
    tblock: Timestamp,
) -> storage::storage::HashMap<u16, Intent<Signature, ProofPreimageMarker, PedersenRandomness, D>, D>
{
    let intents = storage::storage::HashMap::<
        u16,
        Intent<Signature, ProofPreimageMarker, PedersenRandomness, D>,
        D,
    >::new();
    intents.insert(
        1,
        Intent::new(
            rng,
            None,
            None,
            calls,
            updates,
            deploys,
            None,
            tblock + Duration::from_secs(3600),
        ),
    )
}

pub fn test_intents_adv<S: SignatureKind<D>, D: DB, R: Rng + CryptoRng + ?Sized>(
    orig_intents: storage::storage::HashMap<
        u16,
        Intent<S, ProofPreimageMarker, PedersenRandomness, D>,
        D,
    >,
    rng: &mut R,
    segment: u16,
    calls: Vec<ContractCallPrototype<D>>,
    updates: Vec<MaintenanceUpdate<D>>,
    deploys: Vec<ContractDeploy<D>>,
    tblock: Timestamp,
) -> storage::storage::HashMap<u16, Intent<S, ProofPreimageMarker, PedersenRandomness, D>, D> {
    let mut intents = storage::storage::HashMap::new();
    intents = intents.insert(
        segment,
        Intent::new(
            rng,
            None,
            None,
            calls,
            updates,
            deploys,
            None,
            tblock + Duration::from_secs(3600),
        ),
    );

    let mut new_intents = orig_intents.clone();
    for (k, v) in intents.into_iter() {
        new_intents = new_intents.insert(k, v);
    }
    new_intents
}
