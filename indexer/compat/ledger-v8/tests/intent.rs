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

#![cfg(feature = "proving")]
use base_crypto::fab::AlignedValue;
use base_crypto::time::{Duration, Timestamp};
use base_crypto::{
    rng::SplittableRng,
    signatures::{Signature, SigningKey},
};
use coin_structure::coin::TokenType;
use lazy_static::lazy_static;
use midnight_ledger::construct::{ContractCallPrototype, PreTranscript, partition_transcripts};
use midnight_ledger::error::MalformedTransaction;
use midnight_ledger::error::TransactionApplicationError;
use midnight_ledger::prove::Resolver;
use midnight_ledger::structure::ContractAction;
use midnight_ledger::structure::ReplayProtectionState;
use midnight_ledger::structure::{
    ContractDeploy, INITIAL_PARAMETERS, Intent, LedgerState, Transaction, UnshieldedOffer,
    UtxoOutput, UtxoSpend,
};
use midnight_ledger::test_utilities::{test_intents, test_resolver, tx_prove, verifier_key};
use midnight_ledger::verify::WellFormedStrictness;
use midnight_ledger::{structure::StandardTransaction, test_utilities::TestState};
use onchain_runtime::cost_model::INITIAL_COST_MODEL;
use onchain_runtime::ops::Key;
use onchain_runtime::result_mode::{ResultModeGather, ResultModeVerify};
use onchain_runtime::state::StateValue;
use onchain_runtime::{
    Cell_read, Cell_write, Counter_increment,
    context::QueryContext,
    kernel_checkpoint,
    ops::{Op, key},
    state::{ContractOperation, ContractState, stval},
};
use rand::Rng;
use rand::{SeedableRng, rngs::StdRng};
use std::borrow::Cow;
use std::slice;
use storage::arena::Sp;
use storage::db::DB;
use storage::db::InMemoryDB;
use storage::storage::HashMap;
use transient_crypto::commitment::PureGeneratorPedersen;
use transient_crypto::proofs::KeyLocation;
use zkir_v2::LocalProvingProvider;

lazy_static! {
    static ref RESOLVER: Resolver = test_resolver("fallible");
}

fn program_with_results<D: DB>(
    prog: &[Op<ResultModeGather, D>],
    results: &[AlignedValue],
) -> Vec<Op<ResultModeVerify, D>> {
    let mut res_iter = results.iter();

    prog.iter()
        .map(|op| op.clone().translate(|()| res_iter.next().unwrap().clone()))
        .filter(|op| match op {
            Op::Idx { path, .. } => !path.is_empty(),
            Op::Ins { n, .. } => *n != 0,
            _ => true,
        })
        .collect::<Vec<_>>()
}

#[cfg(feature = "proving")]
#[tokio::test]
async fn well_formed() {
    let (mut rng, prover, call, ledger_state) = setup().await;
    let segment_id = rng.r#gen();

    let (signing_key_g, uso_g): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);
    let (signing_key_f, uso_f): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);

    let guaranteed_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_g);
    let fallible_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_f);

    let ttl = Timestamp::from_secs(0) + Duration::from_secs(3600);

    let intent = Intent::new(
        &mut rng,
        guaranteed_unshielded_offer,
        fallible_unshielded_offer,
        vec![call],
        Vec::new(),
        Vec::new(),
        None,
        ttl,
    );

    let (_, intent_proven) = intent
        .prove(segment_id, prover, &INITIAL_COST_MODEL)
        .await
        .unwrap();

    let intent_signed = intent_proven
        .sign(
            &mut rng,
            segment_id,
            &[signing_key_g],
            &[signing_key_f],
            &[],
        )
        .unwrap();

    let strictness = WellFormedStrictness::default();
    let res = intent_signed.well_formed(
        segment_id,
        &ledger_state,
        strictness,
        Timestamp::from_secs(0),
    );

    match res {
        Ok(_) => (),
        Err(e) => panic!("{:?}", e.to_string()),
    }
}

#[cfg(feature = "proving")]
#[tokio::test]
async fn well_formed_sign_wrong_key() {
    let (mut rng, prover, call, ledger_state) = setup().await;
    let segment_id = rng.r#gen();

    let (_, uso_g): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);
    let (signing_key_f, uso_f): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);

    let guaranteed_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_g);
    let fallible_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_f);

    let ttl = Timestamp::from_secs(0) + Duration::from_secs(3600);

    let intent = Intent::new(
        &mut rng,
        guaranteed_unshielded_offer,
        fallible_unshielded_offer,
        vec![call],
        Vec::new(),
        Vec::new(),
        None,
        ttl,
    );

    let (_, intent_proven) = intent
        .prove(segment_id, prover, &INITIAL_COST_MODEL)
        .await
        .unwrap();

    let strictness = WellFormedStrictness::default();
    let res = intent_proven
        .sign(
            &mut rng,
            segment_id,
            slice::from_ref(&signing_key_f.clone()),
            &[signing_key_f],
            &[],
        )
        .map(|x| {
            x.well_formed(
                segment_id,
                &ledger_state,
                strictness,
                Timestamp::from_secs(0),
            )
        });

    match res {
        Ok(_) => panic!(
            "Test succeeded unexpectedly. Did you change the way the signature key is checked against the verification key?"
        ),
        Err(MalformedTransaction::IntentSignatureKeyMismatch) => (),
        Err(e) => panic!(
            "Test failed as expected, but the error was unexpected: {:?}",
            e.to_string()
        ),
    }
}

#[cfg(feature = "proving")]
#[tokio::test]
async fn well_formed_signature_verification_failure_all() {
    let (mut rng, prover, call, ledger_state) = setup().await;
    let segment_id = rng.r#gen();

    let (signing_key_g, uso_g): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);
    let (signing_key_f, uso_f): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);

    let guaranteed_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_g);
    let fallible_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_f);

    let ttl = Timestamp::from_secs(0) + Duration::from_secs(3600);

    let intent = Intent::new(
        &mut rng,
        guaranteed_unshielded_offer,
        fallible_unshielded_offer,
        vec![call],
        Vec::new(),
        Vec::new(),
        None,
        ttl,
    );

    let (_, intent_proven) = intent
        .prove(segment_id, prover, &INITIAL_COST_MODEL)
        .await
        .unwrap();

    let intent_signed = intent_proven
        .sign(
            &mut rng,
            segment_id,
            slice::from_ref(&signing_key_g),
            &[signing_key_f],
            &[],
        )
        .unwrap()
        .seal(rng.clone(), 1);

    let mut intent_guar = intent_signed.clone();
    intent_guar.guaranteed_unshielded_offer = None;

    let mut intent_fall = intent_signed.clone();
    intent_fall.fallible_unshielded_offer = None;

    let mut intent_acts = intent_signed.clone();
    intent_acts.actions = vec![].into();

    let mut intent_ttl = intent_signed.clone();
    intent_ttl.ttl += Duration::from_secs(1);

    let mut intent_bc = intent_signed.clone();
    intent_bc.binding_commitment = PureGeneratorPedersen::new_from(
        &mut rng.clone(),
        &rng.r#gen(),
        &ContractAction::challenge_pre_for(&Vec::from(&intent_bc.actions)),
    );

    let strictness = WellFormedStrictness::default();
    let res_segment = intent_signed.well_formed(
        segment_id + 1,
        &ledger_state,
        strictness,
        Timestamp::from_secs(0),
    );
    let res_guar = intent_guar.well_formed(
        segment_id,
        &ledger_state,
        strictness,
        Timestamp::from_secs(0),
    );
    let res_fall = intent_fall.well_formed(
        segment_id,
        &ledger_state,
        strictness,
        Timestamp::from_secs(0),
    );
    let res_acts = intent_acts.well_formed(
        segment_id,
        &ledger_state,
        strictness,
        Timestamp::from_secs(0),
    );
    let res_ttl = intent_ttl.well_formed(
        segment_id,
        &ledger_state,
        strictness,
        Timestamp::from_secs(0),
    );
    let res_bc = intent_bc.well_formed(
        segment_id,
        &ledger_state,
        strictness,
        Timestamp::from_secs(0),
    );

    assert_failure(res_segment);
    assert_failure(res_guar);
    assert_failure(res_fall);
    assert_failure(res_acts);
    assert_failure(res_ttl);
    assert_failure(res_bc);
}

#[cfg(feature = "proving")]
#[tokio::test]
async fn unsealed_no_signature_check() {
    let (mut rng, prover, call, ledger_state) = setup().await;
    let segment_id = rng.r#gen();

    let (signing_key_g, uso_g): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);
    let (signing_key_f, uso_f): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);

    let guaranteed_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_g);
    let fallible_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_f);

    let ttl = Timestamp::from_secs(0) + Duration::from_secs(3600);

    let intent = Intent::new(
        &mut rng,
        guaranteed_unshielded_offer,
        fallible_unshielded_offer,
        vec![call],
        Vec::new(),
        Vec::new(),
        None,
        ttl,
    );

    let (_, intent_proven) = intent
        .prove(segment_id, prover, &INITIAL_COST_MODEL)
        .await
        .unwrap();

    let intent_signed = intent_proven
        .sign(
            &mut rng,
            segment_id,
            slice::from_ref(&signing_key_g),
            &[signing_key_f],
            &[],
        )
        .unwrap();

    let mut intent_guar = intent_signed.clone();
    intent_guar.guaranteed_unshielded_offer = None;

    let mut intent_fall = intent_signed.clone();
    intent_fall.fallible_unshielded_offer = None;

    let mut intent_acts = intent_signed.clone();
    intent_acts.actions = vec![].into();

    let mut intent_ttl = intent_signed.clone();
    intent_ttl.ttl += Duration::from_secs(1);

    let mut intent_bc = intent_signed.clone();
    intent_bc.binding_commitment = rng.r#gen();

    let strictness = WellFormedStrictness::default();
    let res_segment = intent_signed.well_formed(
        segment_id + 1,
        &ledger_state,
        strictness,
        Timestamp::from_secs(0),
    );
    let res_guar = intent_guar.well_formed(
        segment_id,
        &ledger_state,
        strictness,
        Timestamp::from_secs(0),
    );
    let res_fall = intent_fall.well_formed(
        segment_id,
        &ledger_state,
        strictness,
        Timestamp::from_secs(0),
    );
    let res_acts = intent_acts.well_formed(
        segment_id,
        &ledger_state,
        strictness,
        Timestamp::from_secs(0),
    );
    let res_ttl = intent_ttl.well_formed(
        segment_id,
        &ledger_state,
        strictness,
        Timestamp::from_secs(0),
    );

    assert_success(res_segment);
    assert_success(res_guar);
    assert_success(res_fall);
    assert_success(res_acts);
    assert_success(res_ttl);
}

#[cfg(feature = "proving")]
#[tokio::test]
async fn well_formed_failure_segment_guaranteed() {
    let (mut rng, prover, call, ledger_state) = setup().await;
    let segment_id = 0;

    let (signing_key_g, uso_g): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);
    let (signing_key_f, uso_f): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);

    let guaranteed_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_g);
    let fallible_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_f);

    let ttl = Timestamp::from_secs(0) + Duration::from_secs(3600);

    let intent = Intent::new(
        &mut rng,
        guaranteed_unshielded_offer,
        fallible_unshielded_offer,
        vec![call],
        Vec::new(),
        Vec::new(),
        None,
        ttl,
    );

    let (_, intent_proven) = intent
        .prove(segment_id, prover, &INITIAL_COST_MODEL)
        .await
        .unwrap();

    let intent_signed = intent_proven
        .sign(
            &mut rng,
            segment_id,
            &[signing_key_g],
            &[signing_key_f],
            &[],
        )
        .unwrap();

    let strictness = WellFormedStrictness::default();
    let res = intent_signed.well_formed(
        segment_id,
        &ledger_state,
        strictness,
        Timestamp::from_secs(0),
    );

    match res {
        Ok(_) => panic!(
            "Test succeeded unexpectedly. Did you change or move the check for `segment_id != guaranteed_segment` in `Intent::well_formed`?"
        ),
        Err(MalformedTransaction::IntentAtGuaranteedSegmentId) => (),
        Err(e) => panic!(
            "Test failed as expected, but the error was unexpected: {:?}",
            e.to_string()
        ),
    }
}

#[cfg(feature = "proving")]
#[tokio::test]
async fn merge_failure_segment_clash() {
    use std::iter::once;

    let (mut rng, _prover, call, _ledger_state) = setup().await;
    let segment_id = 1;

    let (_signing_key_g, uso_g): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);
    let (_signing_key_f, uso_f): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);

    let guaranteed_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_g);
    let fallible_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_f);

    let ttl = Timestamp::from_secs(0) + Duration::from_secs(3600);

    let intent = Intent::new(
        &mut rng,
        guaranteed_unshielded_offer,
        fallible_unshielded_offer,
        vec![call],
        Vec::new(),
        Vec::new(),
        None,
        ttl,
    );

    let tx = Transaction::new(
        "local-test",
        storage::storage::HashMap::from_iter(once((segment_id, intent.clone()))),
        None,
        storage::storage::HashMap::new(),
    );

    let res = tx.clone().merge(&tx);

    match res {
        Ok(_) => panic!(
            "Test succeeded unexpectedly. Did you change or move the segment_id collision check in `Transaction::merge`?"
        ),
        Err(MalformedTransaction::IntentSegmentIdCollision(_)) => (),
        Err(e) => panic!(
            "Test failed as expected, but the error was unexpected: {:?}",
            e.to_string()
        ),
    }
}

#[cfg(feature = "proving")]
#[tokio::test]
async fn balanced_utxos_1_intent() {
    let mut rng = StdRng::seed_from_u64(0x42);
    // Initial states
    let mut state: TestState<InMemoryDB> = TestState::new(&mut rng);
    const REWARDS_AMOUNT: u128 = 5_000_000_000;
    let token = Default::default();
    state.rewards_shielded(&mut rng, token, REWARDS_AMOUNT);
    state.give_fee_token(&mut rng, 25).await;

    // Part 1: Deploy
    println!(":: Part 1: Deploy");
    let count_op = ContractOperation::new(verifier_key(&RESOLVER, "count").await);
    let contract = ContractState::new(
        stval!([(0u64), (false), (0u64)]),
        HashMap::new().insert(b"count"[..].into(), count_op.clone()),
        Default::default(),
    );

    let deploy = ContractDeploy::new(&mut rng, contract.clone());
    let addr = deploy.address();
    let tx = tx_prove(
        rng.split(),
        &Transaction::from_intents(
            "local-test",
            test_intents(&mut rng, Vec::new(), Vec::new(), vec![deploy], state.time),
        ),
        &RESOLVER,
    )
    .await
    .unwrap();

    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = false;
    state.assert_apply(&tx, strictness);

    // Part 2: First application
    println!(":: Part 2: First count");
    let guaranteed_public_transcript = partition_transcripts(
        &[PreTranscript {
            context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
            program: program_with_results(&Counter_increment!([key!(0u8)], false, 1u64), &[]),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap()[0]
        .0
        .clone()
        .unwrap();
    let fallible_public_transcript = partition_transcripts(
        &[PreTranscript {
            // Playing fast and loose with state here, this should be the state after applying
            // the guaranteed part, not that it matters here.
            context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
            program: program_with_results(
                &[
                    &kernel_checkpoint!((), ())[..],
                    &Cell_read!([key!(1u8)], false, bool),
                    &Cell_write!([key!(1u8)], false, bool, true),
                    &Counter_increment!([key!(2u8)], false, 1u64),
                ]
                .into_iter()
                .flat_map(|x| x.iter())
                .cloned()
                .collect::<Vec<_>>(),
                &[false.into()],
            ),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap()[0]
        .0
        .clone()
        .unwrap();
    let call = ContractCallPrototype {
        address: addr,
        entry_point: b"count"[..].into(),
        op: count_op.clone(),
        input: ().into(),
        output: ().into(),
        guaranteed_public_transcript: Some(guaranteed_public_transcript),
        fallible_public_transcript: Some(fallible_public_transcript),
        private_transcript_outputs: vec![],
        communication_commitment_rand: rng.r#gen(),
        key_location: KeyLocation(Cow::Borrowed("count")),
    };

    let signing_key_g: SigningKey = SigningKey::sample(rng.clone());

    let owner = signing_key_g.verifying_key();
    let type_ = rng.r#gen();
    let intent_hash = rng.r#gen();
    let output_no = rng.r#gen();

    let spend = UtxoSpend {
        value: 500,
        owner,
        type_,
        intent_hash,
        output_no,
    };

    let addr = rng.r#gen();

    let out = UtxoOutput {
        value: 500,
        owner: addr,
        type_,
    };

    let uso_g = UnshieldedOffer {
        inputs: vec![spend].into(),
        outputs: vec![out].into(),
        signatures: vec![].into(),
    };

    let signing_key_f: SigningKey = SigningKey::sample(rng.clone());

    let spend = UtxoSpend {
        value: 200,
        owner: signing_key_f.verifying_key(),
        type_,
        intent_hash: rng.r#gen(),
        output_no: rng.r#gen(),
    };

    let out = UtxoOutput {
        value: 200,
        owner: rng.r#gen(),
        type_,
    };

    let uso_f = UnshieldedOffer {
        inputs: vec![spend].into(),
        outputs: vec![out].into(),
        signatures: vec![].into(),
    };

    let guaranteed_unshielded_offer: Option<UnshieldedOffer<(), InMemoryDB>> = Some(uso_g);
    let fallible_unshielded_offer: Option<UnshieldedOffer<(), InMemoryDB>> = Some(uso_f);

    let mut intents: storage::storage::HashMap<
        u16,
        Intent<
            (),
            midnight_ledger::structure::ProofPreimageMarker,
            transient_crypto::curve::EmbeddedFr,
            InMemoryDB,
        >,
    > = storage::storage::HashMap::new();
    intents = intents.insert(
        1,
        Intent::new(
            &mut rng,
            guaranteed_unshielded_offer,
            fallible_unshielded_offer,
            vec![call],
            Vec::new(),
            Vec::new(),
            None,
            state.time,
        ),
    );

    let tx = Transaction::Standard(StandardTransaction::new(
        "local-test",
        intents,
        None,
        storage::storage::HashMap::new(),
    ));

    let balanced_tx = state
        .balance_tx(rng.split(), tx.clone(), &RESOLVER)
        .await
        .unwrap();

    let proven_tx = tx_prove(rng.split(), &balanced_tx, &RESOLVER)
        .await
        .unwrap();
    let proven_unbalanced_tx = tx_prove(rng.split(), &tx, &RESOLVER).await.unwrap();

    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = true;
    let res: Result<_, midnight_ledger::error::MalformedTransaction<InMemoryDB>> =
        proven_tx.well_formed(&state.ledger, strictness, state.time);
    let res_unbalanced: Result<_, midnight_ledger::error::MalformedTransaction<InMemoryDB>> =
        proven_unbalanced_tx.well_formed(&state.ledger, strictness, state.time);

    let fees = proven_unbalanced_tx
        .fees(&state.ledger.parameters, false)
        .unwrap();

    match res_unbalanced {
        Ok(_) => panic!(
            "Test succeeded unexpectedly. Did you change the way unshielded tokens balancing is calculated?"
        ),
        Err(MalformedTransaction::BalanceCheckOverspend {
            token_type: TokenType::Shielded(_) | TokenType::Dust,
            segment: 0,
            overspent_value,
        }) => {
            if overspent_value != -(fees as i128) {
                panic!(
                    "Transaction is unbalanced as expected, but the imbalance was unexpected: {:?}",
                    overspent_value
                )
            }
        }
        Err(e) => panic!(
            "Test failed as expected, but the error was unexpected: {:?}",
            e.to_string()
        ),
    }

    match res {
        Ok(_) => (),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[cfg(feature = "proving")]
#[tokio::test]
async fn intents_cannot_balance_across_segments() {
    let mut rng = StdRng::seed_from_u64(0x42);

    // Initial states
    let mut state: TestState<InMemoryDB> = TestState::new(&mut rng);
    const REWARDS_AMOUNT: u128 = 5_000_000_000;
    let token = Default::default();
    state.rewards_shielded(&mut rng, token, REWARDS_AMOUNT);
    state.give_fee_token(&mut rng, 25).await;

    // Part 1: Deploy
    println!(":: Part 1: Deploy");
    let count_op = ContractOperation::new(verifier_key(&RESOLVER, "count").await);
    let contract = ContractState::new(
        stval!([(0u64), (false), (0u64)]),
        HashMap::new().insert(b"count"[..].into(), count_op.clone()),
        Default::default(),
    );

    let deploy = ContractDeploy::new(&mut rng, contract.clone());
    let addr = deploy.address();
    let tx = tx_prove(
        rng.split(),
        &Transaction::from_intents(
            "local-test",
            test_intents(&mut rng, Vec::new(), Vec::new(), vec![deploy], state.time),
        ),
        &RESOLVER,
    )
    .await
    .unwrap();

    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = false;
    state.assert_apply(&tx, strictness);

    ////////////////////////////////////////////////////////
    // First segment
    ////////////////////////////////////////////////////////
    let transcripts_1 = partition_transcripts(
        &[PreTranscript {
            context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
            program: program_with_results(
                &[
                    &Counter_increment!([key!(0u8)], false, 1u64)[..],
                    &kernel_checkpoint!((), ()),
                    &Cell_read!([key!(1u8)], false, bool),
                    &Cell_write!([key!(1u8)], false, bool, true),
                    &Counter_increment!([key!(2u8)], false, 1u64),
                ]
                .into_iter()
                .flat_map(|x| x.iter())
                .cloned()
                .collect::<Vec<_>>(),
                &[false.into()],
            ),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap();
    assert!(transcripts_1[0].1.is_none());
    let call_1 = ContractCallPrototype {
        address: addr,
        entry_point: b"count"[..].into(),
        op: count_op.clone(),
        input: ().into(),
        output: ().into(),
        guaranteed_public_transcript: transcripts_1[0].0.clone(),
        fallible_public_transcript: transcripts_1[0].1.clone(),
        private_transcript_outputs: Vec::new(),
        communication_commitment_rand: rng.r#gen(),
        key_location: KeyLocation(Cow::Borrowed("count")),
    };

    let type_ = rng.r#gen();

    let signing_key_g_1: SigningKey = SigningKey::sample(rng.clone());

    let owner_1 = signing_key_g_1.verifying_key();
    let intent_hash = rng.r#gen();
    let output_no = rng.r#gen();

    let spend_1 = UtxoSpend {
        value: 0,
        owner: owner_1,
        type_,
        intent_hash,
        output_no,
    };

    let addr_1 = rng.r#gen();

    let out_1 = UtxoOutput {
        value: 100,
        owner: addr_1,
        type_,
    };

    let uso_g_1 = UnshieldedOffer {
        inputs: vec![spend_1].into(),
        outputs: vec![out_1].into(),
        signatures: vec![].into(),
    };

    let signing_key_f_1: SigningKey = SigningKey::sample(rng.clone());

    let spend_1 = UtxoSpend {
        value: 0,
        owner: signing_key_f_1.verifying_key(),
        type_,
        intent_hash: rng.r#gen(),
        output_no: rng.r#gen(),
    };

    let out_1 = UtxoOutput {
        value: 0,
        owner: rng.r#gen(),
        type_,
    };

    let uso_f_1 = UnshieldedOffer {
        inputs: vec![spend_1].into(),
        outputs: vec![out_1].into(),
        signatures: vec![].into(),
    };

    let guaranteed_unshielded_offer_1: Option<UnshieldedOffer<(), InMemoryDB>> = Some(uso_g_1);
    let fallible_unshielded_offer_1: Option<UnshieldedOffer<(), InMemoryDB>> = Some(uso_f_1);

    ////////////////////////////////////////////////////////
    // Second segment
    ////////////////////////////////////////////////////////
    let transcripts_2 = partition_transcripts(
        &[PreTranscript {
            context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
            program: program_with_results(
                &[
                    &Counter_increment!([key!(0u8)], false, 1u64)[..],
                    &kernel_checkpoint!((), ()),
                    &Cell_read!([key!(1u8)], false, bool),
                    &Cell_write!([key!(1u8)], false, bool, true),
                    &Counter_increment!([key!(2u8)], false, 1u64),
                ]
                .into_iter()
                .flat_map(|x| x.iter())
                .cloned()
                .collect::<Vec<_>>(),
                &[false.into()],
            ),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap();
    assert!(transcripts_2[0].1.is_none());
    let call_2 = ContractCallPrototype {
        address: addr,
        entry_point: b"count"[..].into(),
        op: count_op.clone(),
        input: ().into(),
        output: ().into(),
        guaranteed_public_transcript: transcripts_2[0].0.clone(),
        fallible_public_transcript: transcripts_2[0].1.clone(),
        private_transcript_outputs: vec![],
        communication_commitment_rand: rng.r#gen(),
        key_location: KeyLocation(Cow::Borrowed("count")),
    };

    let signing_key_f_2: SigningKey = SigningKey::sample(rng.clone());

    let spend_2 = UtxoSpend {
        value: 0,
        owner: signing_key_f_2.verifying_key(),
        type_,
        intent_hash: rng.r#gen(),
        output_no: rng.r#gen(),
    };

    let out_2 = UtxoOutput {
        value: 0,
        owner: rng.r#gen(),
        type_,
    };

    let uso_f_2 = UnshieldedOffer {
        inputs: vec![spend_2].into(),
        outputs: vec![out_2].into(),
        signatures: vec![].into(),
    };

    let fallible_unshielded_offer_2: Option<UnshieldedOffer<(), InMemoryDB>> = Some(uso_f_2);

    ////////////////////////////////////////////////////////

    let ttl = state.time + Duration::from_secs(3600);

    let mut intents: storage::storage::HashMap<
        u16,
        Intent<
            (),
            midnight_ledger::structure::ProofPreimageMarker,
            transient_crypto::curve::EmbeddedFr,
            InMemoryDB,
        >,
    > = storage::storage::HashMap::new();
    intents = intents.insert(
        1,
        Intent::new(
            &mut rng,
            guaranteed_unshielded_offer_1,
            fallible_unshielded_offer_1,
            vec![call_1],
            Vec::new(),
            Vec::new(),
            None,
            ttl,
        ),
    );

    intents = intents.insert(
        2,
        Intent::new(
            &mut rng,
            None,
            fallible_unshielded_offer_2,
            vec![call_2],
            Vec::new(),
            Vec::new(),
            None,
            ttl,
        ),
    );

    let tx = Transaction::Standard(StandardTransaction::new(
        "local-test",
        intents,
        None,
        storage::storage::HashMap::new(),
    ));

    let balanced_tx = state
        .balance_tx(rng.split(), tx.clone(), &RESOLVER)
        .await
        .unwrap();
    let proven_tx = tx_prove(rng.split(), &balanced_tx, &RESOLVER)
        .await
        .unwrap();

    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = true;
    let res: Result<_, midnight_ledger::error::MalformedTransaction<InMemoryDB>> =
        proven_tx.well_formed(&state.ledger, strictness, state.time);

    match res {
        Ok(_) => panic!(
            "Test succeeded unexpectedly. Have you adjusted the way balancing works, specifically across segment_ids for intents?"
        ),
        Err(MalformedTransaction::BalanceCheckOverspend {
            token_type: _,
            segment: _,
            overspent_value: -100,
        }) => (),
        Err(MalformedTransaction::Unbalanced(_, -100)) => (),
        Err(e) => panic!(
            "Test failed as expected, but the error was unexpected: {:?}",
            e.to_string()
        ),
    }
}

#[cfg(feature = "proving")]
#[tokio::test]
// This test is useful but should be streamlined and the messages made more sensible
async fn causality_check_sanity_check() {
    use midnight_ledger::error::SequencingCheckError;

    let mut rng = StdRng::seed_from_u64(0x42);

    // Initial states
    let mut state: TestState<InMemoryDB> = TestState::new(&mut rng);
    const REWARDS_AMOUNT: u128 = 5_000_000_000;
    let token = Default::default();
    state.rewards_shielded(&mut rng, token, REWARDS_AMOUNT);
    state.give_fee_token(&mut rng, 25).await;

    // Part 1: Deploy
    println!(":: Part 1: Deploy");
    let count_op = ContractOperation::new(verifier_key(&RESOLVER, "count").await);
    let contract = ContractState::new(
        stval!([(0u64), (false), (0u64)]),
        HashMap::new().insert(b"count"[..].into(), count_op.clone()),
        Default::default(),
    );

    let deploy = ContractDeploy::new(&mut rng, contract.clone());
    let addr = deploy.address();
    let tx = tx_prove(
        rng.split(),
        &Transaction::from_intents(
            "local-test",
            test_intents(&mut rng, Vec::new(), Vec::new(), vec![deploy], state.time),
        ),
        &RESOLVER,
    )
    .await
    .unwrap();

    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = false;
    state.assert_apply(&tx, strictness);

    ////////////////////////////////////////////////////////
    // First segment
    ////////////////////////////////////////////////////////
    let guaranteed_public_transcript_1 = partition_transcripts(
        &[PreTranscript {
            context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
            program: program_with_results(&Counter_increment!([key!(0u8)], false, 1u64), &[]),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap()[0]
        .0
        .clone()
        .unwrap();
    let fallible_public_transcript_1 = partition_transcripts(
        &[PreTranscript {
            // Playing fast and loose with state here, this should be the state after applying
            // the guaranteed part, not that it matters here.
            context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
            program: program_with_results(
                &[
                    &kernel_checkpoint!((), ())[..],
                    &Cell_read!([key!(1u8)], false, bool),
                    &Cell_write!([key!(1u8)], false, bool, true),
                    &Counter_increment!([key!(2u8)], false, 1u64),
                ]
                .into_iter()
                .flat_map(|x| x.iter())
                .cloned()
                .collect::<Vec<_>>(),
                &[false.into()],
            ),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap()[0]
        .0
        .clone()
        .unwrap();
    let call_1 = ContractCallPrototype {
        address: addr,
        entry_point: b"count"[..].into(),
        op: count_op.clone(),
        input: ().into(),
        output: ().into(),
        guaranteed_public_transcript: Some(guaranteed_public_transcript_1),
        fallible_public_transcript: Some(fallible_public_transcript_1),
        private_transcript_outputs: Vec::new(),
        communication_commitment_rand: rng.r#gen(),
        key_location: KeyLocation(Cow::Borrowed("count")),
    };

    let type_ = rng.r#gen();

    let signing_key_g_1: SigningKey = SigningKey::sample(rng.clone());

    let owner_1 = signing_key_g_1.verifying_key();
    let intent_hash = rng.r#gen();
    let output_no = rng.r#gen();

    let spend_1 = UtxoSpend {
        value: 0,
        owner: owner_1,
        type_,
        intent_hash,
        output_no,
    };

    let addr_1 = rng.r#gen();

    let out_1 = UtxoOutput {
        value: 100,
        owner: addr_1,
        type_,
    };

    let uso_g_1 = UnshieldedOffer {
        inputs: vec![spend_1].into(),
        outputs: vec![out_1].into(),
        signatures: vec![].into(),
    };

    let signing_key_f_1: SigningKey = SigningKey::sample(rng.clone());

    let spend_1 = UtxoSpend {
        value: 0,
        owner: signing_key_f_1.verifying_key(),
        type_,
        intent_hash: rng.r#gen(),
        output_no: rng.r#gen(),
    };

    let out_1 = UtxoOutput {
        value: 0,
        owner: rng.r#gen(),
        type_,
    };

    let uso_f_1 = UnshieldedOffer {
        inputs: vec![spend_1].into(),
        outputs: vec![out_1].into(),
        signatures: vec![].into(),
    };

    let guaranteed_unshielded_offer_1: Option<UnshieldedOffer<(), InMemoryDB>> = Some(uso_g_1);
    let fallible_unshielded_offer_1: Option<UnshieldedOffer<(), InMemoryDB>> = Some(uso_f_1);

    ////////////////////////////////////////////////////////
    // Second segment
    ////////////////////////////////////////////////////////
    let guaranteed_public_transcript_2 = partition_transcripts(
        &[PreTranscript {
            context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
            program: program_with_results(&Counter_increment!([key!(0u8)], false, 1u64), &[]),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap()[0]
        .0
        .clone()
        .unwrap();
    let fallible_public_transcript_2 = partition_transcripts(
        &[PreTranscript {
            // Playing fast and loose with state here, this should be the state after applying
            // the guaranteed part, not that it matters here.
            context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
            program: program_with_results(
                &[
                    &kernel_checkpoint!((), ())[..],
                    &Cell_read!([key!(1u8)], false, bool),
                    &Cell_write!([key!(1u8)], false, bool, true),
                    &Counter_increment!([key!(2u8)], false, 1u64),
                ]
                .into_iter()
                .flat_map(|x| x.iter())
                .cloned()
                .collect::<Vec<_>>(),
                &[false.into()],
            ),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap()[0]
        .0
        .clone()
        .unwrap();
    let call_2 = ContractCallPrototype {
        address: addr,
        entry_point: b"count"[..].into(),
        op: count_op.clone(),
        input: ().into(),
        output: ().into(),
        guaranteed_public_transcript: Some(guaranteed_public_transcript_2),
        fallible_public_transcript: Some(fallible_public_transcript_2),
        private_transcript_outputs: vec![],
        communication_commitment_rand: rng.r#gen(),
        key_location: KeyLocation(Cow::Borrowed("count")),
    };

    let signing_key_f_2: SigningKey = SigningKey::sample(rng.clone());

    let spend_2 = UtxoSpend {
        value: 0,
        owner: signing_key_f_2.verifying_key(),
        type_,
        intent_hash: rng.r#gen(),
        output_no: rng.r#gen(),
    };

    let out_2 = UtxoOutput {
        value: 0,
        owner: rng.r#gen(),
        type_,
    };

    let uso_f_2 = UnshieldedOffer {
        inputs: vec![spend_2].into(),
        outputs: vec![out_2].into(),
        signatures: vec![].into(),
    };

    let fallible_unshielded_offer_2: Option<UnshieldedOffer<(), InMemoryDB>> = Some(uso_f_2);

    ////////////////////////////////////////////////////////

    let ttl = state.time + Duration::from_secs(3600);

    let mut intents: storage::storage::HashMap<
        u16,
        Intent<
            (),
            midnight_ledger::structure::ProofPreimageMarker,
            transient_crypto::curve::EmbeddedFr,
            InMemoryDB,
        >,
    > = storage::storage::HashMap::new();
    intents = intents.insert(
        1,
        Intent::new(
            &mut rng,
            guaranteed_unshielded_offer_1,
            fallible_unshielded_offer_1,
            vec![call_1],
            Vec::new(),
            Vec::new(),
            None,
            ttl,
        ),
    );

    intents = intents.insert(
        2,
        Intent::new(
            &mut rng,
            None,
            fallible_unshielded_offer_2,
            vec![call_2],
            Vec::new(),
            Vec::new(),
            None,
            ttl,
        ),
    );

    let tx = Transaction::Standard(StandardTransaction::new(
        "local-test",
        intents,
        None,
        storage::storage::HashMap::new(),
    ));

    let balanced_tx = state
        .balance_tx(rng.split(), tx.clone(), &RESOLVER)
        .await
        .unwrap();
    let proven_tx = tx_prove(rng.split(), &balanced_tx, &RESOLVER)
        .await
        .unwrap();

    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = true;
    let res: Result<_, midnight_ledger::error::MalformedTransaction<InMemoryDB>> =
        proven_tx.well_formed(&state.ledger, strictness, state.time);

    match res {
        Ok(_) => panic!(
            "Test succeeded unexpectedly. Have you adjusted the way balancing works, specifically across segment_ids for intents?"
        ),
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
async fn imbalanced_utxos_1_intent() {
    use midnight_ledger::structure::StandardTransaction;

    let mut rng = StdRng::seed_from_u64(0x42);

    // Initial states
    let mut state: TestState<InMemoryDB> = TestState::new(&mut rng);
    const REWARDS_AMOUNT: u128 = 5_000_000_000;
    let token = Default::default();
    state.rewards_shielded(&mut rng, token, REWARDS_AMOUNT);
    state.give_fee_token(&mut rng, 25).await;

    // Part 1: Deploy
    println!(":: Part 1: Deploy");
    let count_op = ContractOperation::new(verifier_key(&RESOLVER, "count").await);
    let contract = ContractState::new(
        stval!([(0u64), (false), (0u64)]),
        HashMap::new().insert(b"count"[..].into(), count_op.clone()),
        Default::default(),
    );

    let deploy = ContractDeploy::new(&mut rng, contract.clone());
    let addr = deploy.address();
    let tx = tx_prove(
        rng.split(),
        &Transaction::from_intents(
            "local-test",
            test_intents(&mut rng, Vec::new(), Vec::new(), vec![deploy], state.time),
        ),
        &RESOLVER,
    )
    .await
    .unwrap();

    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = false;
    state.assert_apply(&tx, strictness);

    // Part 2: First application
    println!(":: Part 2: First count");
    let guaranteed_public_transcript = partition_transcripts(
        &[PreTranscript {
            context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
            program: program_with_results(&Counter_increment!([key!(0u8)], false, 1u64), &[]),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap()[0]
        .0
        .clone()
        .unwrap();
    let fallible_public_transcript = partition_transcripts(
        &[PreTranscript {
            // Playing fast and loose with state here, this should be the state after applying
            // the guaranteed part, not that it matters here.
            context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
            program: program_with_results(
                &[
                    &kernel_checkpoint!((), ())[..],
                    &Cell_read!([key!(1u8)], false, bool),
                    &Cell_write!([key!(1u8)], false, bool, true),
                    &Counter_increment!([key!(2u8)], false, 1u64),
                ]
                .into_iter()
                .flat_map(|x| x.iter())
                .cloned()
                .collect::<Vec<_>>(),
                &[false.into()],
            ),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap()[0]
        .0
        .clone()
        .unwrap();
    let call = ContractCallPrototype {
        address: addr,
        entry_point: b"count"[..].into(),
        op: count_op.clone(),
        input: ().into(),
        output: ().into(),
        guaranteed_public_transcript: Some(guaranteed_public_transcript),
        fallible_public_transcript: Some(fallible_public_transcript),
        private_transcript_outputs: vec![],
        communication_commitment_rand: rng.r#gen(),
        key_location: KeyLocation(Cow::Borrowed("count")),
    };

    let signing_key_g: SigningKey = SigningKey::sample(rng.clone());

    let owner = signing_key_g.verifying_key();
    let type_ = rng.r#gen();
    let intent_hash = rng.r#gen();
    let output_no = rng.r#gen();

    let spend = UtxoSpend {
        value: 500,
        owner,
        type_,
        intent_hash,
        output_no,
    };

    let addr = rng.r#gen();

    let out = UtxoOutput {
        value: 600,
        owner: addr,
        type_,
    };

    let uso_g = UnshieldedOffer {
        inputs: vec![spend].into(),
        outputs: vec![out].into(),
        signatures: vec![].into(),
    };

    let guaranteed_unshielded_offer: Option<UnshieldedOffer<(), InMemoryDB>> = Some(uso_g);

    let ttl = state.time + Duration::from_secs(3600);

    let mut intents: storage::storage::HashMap<
        u16,
        Intent<
            (),
            midnight_ledger::structure::ProofPreimageMarker,
            transient_crypto::curve::EmbeddedFr,
            InMemoryDB,
        >,
    > = storage::storage::HashMap::new();
    intents = intents.insert(
        1,
        Intent::new(
            &mut rng,
            guaranteed_unshielded_offer,
            None,
            vec![call],
            Vec::new(),
            vec![],
            None,
            ttl,
        ),
    );

    let tx = Transaction::Standard(StandardTransaction::new(
        "local-test",
        intents,
        None,
        storage::storage::HashMap::new(),
    ));

    // Balance only the Shielded, to handle fees
    let balanced_tx = state
        .balance_tx(rng.split(), tx.clone(), &RESOLVER)
        .await
        .unwrap();
    let proven_tx = tx_prove(rng.split(), &balanced_tx, &RESOLVER)
        .await
        .unwrap();

    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = true;
    let res: Result<_, midnight_ledger::error::MalformedTransaction<InMemoryDB>> =
        proven_tx.well_formed(&state.ledger, strictness, state.time);

    match res {
        Ok(_) => panic!(
            "Test succeeded unexpectedly. Did you change the way unshielded tokens balancing is calculated?"
        ),
        Err(MalformedTransaction::BalanceCheckOverspend {
            token_type: _,
            segment: _,
            overspent_value,
        }) => {
            if overspent_value != -100 {
                panic!("unbalanced by incorrect amount")
            }
        }
        Err(e) => panic!(
            "Test failed as expected, but the error was unexpected: {:?}",
            e.to_string()
        ),
    }
}

#[cfg(feature = "proving")]
#[tokio::test]
async fn imbalanced_utxos_1_intent_fallible() {
    use midnight_ledger::structure::StandardTransaction;

    let mut rng = StdRng::seed_from_u64(0x42);

    // Initial states
    let mut state: TestState<InMemoryDB> = TestState::new(&mut rng);
    const REWARDS_AMOUNT: u128 = 5_000_000_000;
    let token = Default::default();
    state.rewards_shielded(&mut rng, token, REWARDS_AMOUNT);
    state.give_fee_token(&mut rng, 25).await;

    // Part 1: Deploy
    println!(":: Part 1: Deploy");
    let count_op = ContractOperation::new(verifier_key(&RESOLVER, "count").await);
    let contract = ContractState::new(
        stval!([(0u64), (false), (0u64)]),
        HashMap::new().insert(b"count"[..].into(), count_op.clone()),
        Default::default(),
    );

    let deploy = ContractDeploy::new(&mut rng, contract.clone());
    let addr = deploy.address();
    let tx = tx_prove(
        rng.split(),
        &Transaction::from_intents(
            "local-test",
            test_intents(&mut rng, Vec::new(), Vec::new(), vec![deploy], state.time),
        ),
        &RESOLVER,
    )
    .await
    .unwrap();

    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = false;
    state.assert_apply(&tx, strictness);

    // Part 2: First application
    println!(":: Part 2: First count");
    let guaranteed_public_transcript = partition_transcripts(
        &[PreTranscript {
            context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
            program: program_with_results(&Counter_increment!([key!(0u8)], false, 1u64), &[]),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap()[0]
        .0
        .clone()
        .unwrap();
    let fallible_public_transcript = partition_transcripts(
        &[PreTranscript {
            // Playing fast and loose with state here, this should be the state after applying
            // the guaranteed part, not that it matters here.
            context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
            program: program_with_results(
                &[
                    &kernel_checkpoint!((), ())[..],
                    &Cell_read!([key!(1u8)], false, bool),
                    &Cell_write!([key!(1u8)], false, bool, true),
                    &Counter_increment!([key!(2u8)], false, 1u64),
                ]
                .into_iter()
                .flat_map(|x| x.iter())
                .cloned()
                .collect::<Vec<_>>(),
                &[false.into()],
            ),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap()[0]
        .0
        .clone()
        .unwrap();
    let call = ContractCallPrototype {
        address: addr,
        entry_point: b"count"[..].into(),
        op: count_op.clone(),
        input: ().into(),
        output: ().into(),
        guaranteed_public_transcript: Some(guaranteed_public_transcript),
        fallible_public_transcript: Some(fallible_public_transcript),
        private_transcript_outputs: vec![],
        communication_commitment_rand: rng.r#gen(),
        key_location: KeyLocation(Cow::Borrowed("count")),
    };

    let type_ = rng.r#gen();

    let signing_key_f: SigningKey = SigningKey::sample(rng.clone());

    let spend = UtxoSpend {
        value: 200,
        owner: signing_key_f.verifying_key(),
        type_,
        intent_hash: rng.r#gen(),
        output_no: rng.r#gen(),
    };

    let out = UtxoOutput {
        value: 300,
        owner: rng.r#gen(),
        type_,
    };

    let uso_f = UnshieldedOffer {
        inputs: vec![spend].into(),
        outputs: vec![out].into(),
        signatures: vec![].into(),
    };

    let fallible_unshielded_offer: Option<UnshieldedOffer<(), InMemoryDB>> = Some(uso_f);
    let ttl = state.time + Duration::from_secs(3600);

    let mut intents: storage::storage::HashMap<
        u16,
        Intent<
            (),
            midnight_ledger::structure::ProofPreimageMarker,
            transient_crypto::curve::EmbeddedFr,
            InMemoryDB,
        >,
    > = storage::storage::HashMap::new();
    intents = intents.insert(
        1,
        Intent::new(
            &mut rng,
            None,
            fallible_unshielded_offer,
            vec![call],
            Vec::new(),
            vec![],
            None,
            ttl,
        ),
    );

    let tx = Transaction::Standard(StandardTransaction::new(
        "local-test",
        intents,
        None,
        storage::storage::HashMap::new(),
    ));

    // Balance only the Shielded, to handle fees
    let balanced_tx = state
        .balance_tx(rng.split(), tx.clone(), &RESOLVER)
        .await
        .unwrap();
    let proven_tx = tx_prove(rng.split(), &balanced_tx, &RESOLVER)
        .await
        .unwrap();

    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = true;
    let res: Result<_, midnight_ledger::error::MalformedTransaction<InMemoryDB>> =
        proven_tx.well_formed(&state.ledger, strictness, state.time);

    match res {
        Ok(_) => panic!(
            "Test succeeded unexpectedly. Did you change the way unshielded tokens balancing is calculated?"
        ),
        Err(MalformedTransaction::BalanceCheckOverspend {
            token_type: _,
            segment: _,
            overspent_value,
        }) => {
            if overspent_value != -100 {
                panic!("unbalanced by incorrect amount")
            }
        }
        Err(e) => panic!(
            "Test failed as expected, but the error was unexpected: {:?}",
            e.to_string()
        ),
    }
}

#[cfg(feature = "proving")]
#[tokio::test]
async fn apply() {
    let (mut rng, prover, call, ledger_state) = setup().await;
    let segment_id = rng.r#gen();

    let (signing_key_g, uso_g): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);
    let (signing_key_f, uso_f): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);

    let guaranteed_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_g);
    let fallible_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_f);

    let ttl = Timestamp::from_secs(0) + Duration::from_secs(3600);

    let intent = Intent::new(
        &mut rng,
        guaranteed_unshielded_offer,
        fallible_unshielded_offer,
        vec![call],
        Vec::new(),
        Vec::new(),
        None,
        ttl,
    );

    let (_, intent_proven) = intent
        .prove(segment_id, prover, &INITIAL_COST_MODEL)
        .await
        .unwrap();

    let intent_signed = intent_proven
        .sign(
            &mut rng,
            segment_id,
            &[signing_key_g],
            &[signing_key_f],
            &[],
        )
        .unwrap();

    let strictness = WellFormedStrictness::default();
    let res = intent_signed.clone().well_formed(
        segment_id,
        &ledger_state,
        strictness,
        Timestamp::from_secs(0),
    );

    match res {
        Ok(_) => {
            let replay_state = ReplayProtectionState::new();
            match replay_state.apply_intent(
                intent_signed.clone(),
                Timestamp::from_secs(0),
                ledger_state.parameters.global_ttl,
            ) {
                Ok(_) => (),
                Err(e) => panic!("{:?}", e),
            }
        }
        Err(e) => panic!("{:?}", e.to_string()),
    }
}

#[cfg(feature = "proving")]
#[tokio::test]
async fn apply_duplicate_failure() {
    let (mut rng, prover, call, ledger_state) = setup().await;
    let segment_id = rng.r#gen();

    let (signing_key_g, uso_g): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);
    let (signing_key_f, uso_f): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);

    let guaranteed_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_g);
    let fallible_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_f);

    let ttl = Timestamp::from_secs(0) + Duration::from_secs(3600);

    let intent = Intent::new(
        &mut rng,
        guaranteed_unshielded_offer,
        fallible_unshielded_offer,
        vec![call],
        Vec::new(),
        Vec::new(),
        None,
        ttl,
    );

    let (_, intent_proven) = intent
        .prove(segment_id, prover, &INITIAL_COST_MODEL)
        .await
        .unwrap();

    let intent_signed = intent_proven
        .sign(
            &mut rng,
            segment_id,
            &[signing_key_g],
            &[signing_key_f],
            &[],
        )
        .unwrap();

    let strictness = WellFormedStrictness::default();
    let res = intent_signed.clone().well_formed(
        segment_id,
        &ledger_state,
        strictness,
        Timestamp::from_secs(0),
    );

    match res {
        Ok(_) => {
            let replay_state = ReplayProtectionState::new();

            // Apply once (should be valid)
            let replay_state = replay_state
                .apply_intent(
                    intent.clone(),
                    Timestamp::from_secs(0),
                    ledger_state.parameters.global_ttl,
                )
                .unwrap();

            // Apply again (duplicate, so should fail)
            match replay_state.apply_intent(
                intent.clone(),
                Timestamp::from_secs(0),
                ledger_state.parameters.global_ttl,
            ) {
                Ok(_) => panic!("Test succeeded unexpectedly."),
                Err(TransactionApplicationError::IntentAlreadyExists) => (),
                Err(e) => panic!(
                    "Test failed as expected, but the error was unexpected: {:?}",
                    e.to_string()
                ),
            }
        }
        Err(e) => panic!("{:?}", e.to_string()),
    }
}

#[cfg(feature = "proving")]
#[tokio::test]
async fn apply_expired_ttl_failure() {
    use storage::db::InMemoryDB;

    let (mut rng, prover, call, ledger_state) = setup().await;
    let segment_id = rng.r#gen();

    let (signing_key_g, uso_g): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);
    let (signing_key_f, uso_f): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);

    let guaranteed_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_g);
    let fallible_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_f);

    let ttl = Timestamp::from_secs(3599);

    let intent = Intent::new(
        &mut rng,
        guaranteed_unshielded_offer,
        fallible_unshielded_offer,
        vec![call],
        Vec::new(),
        Vec::new(),
        None,
        ttl,
    );

    let (_, intent_proven) = intent
        .prove(segment_id, prover, &INITIAL_COST_MODEL)
        .await
        .unwrap();

    let intent_signed = intent_proven
        .sign(
            &mut rng,
            segment_id,
            &[signing_key_g],
            &[signing_key_f],
            &[],
        )
        .unwrap();

    let strictness = WellFormedStrictness::default();
    let res = intent_signed.clone().well_formed(
        segment_id,
        &ledger_state,
        strictness,
        Timestamp::from_secs(0),
    );

    match res {
        Ok(_) => {
            let replay_state = ReplayProtectionState::new();
            match replay_state.apply_intent(
                intent.clone(),
                Timestamp::from_secs(3600),
                ledger_state.parameters.global_ttl,
            ) {
                Ok(_) => panic!("Test succeeded unexpectedly."),
                Err(TransactionApplicationError::IntentTtlExpired(_, _)) => (),
                Err(e) => panic!(
                    "Test failed as expected, but the error was unexpected: {:?}",
                    e.to_string()
                ),
            }
        }
        Err(e) => panic!("{:?}", e.to_string()),
    }
}

#[cfg(feature = "proving")]
#[tokio::test]
async fn apply_ttl_in_future_failure() {
    use storage::db::InMemoryDB;

    let (mut rng, prover, call, ledger_state) = setup().await;

    let segment_id = rng.r#gen();

    let (signing_key_g, uso_g): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);
    let (signing_key_f, uso_f): (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) =
        gen_unshielded_offer(&mut rng);

    let guaranteed_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_g);
    let fallible_unshielded_offer: Option<UnshieldedOffer<Signature, InMemoryDB>> = Some(uso_f);

    let ttl = Timestamp::from_secs(3601);

    let intent = Intent::new(
        &mut rng,
        guaranteed_unshielded_offer,
        fallible_unshielded_offer,
        vec![call],
        Vec::new(),
        Vec::new(),
        None,
        ttl,
    );

    let (_, intent_proven) = intent.prove(0, prover, &INITIAL_COST_MODEL).await.unwrap();

    let intent_signed = intent_proven
        .sign(
            &mut rng,
            segment_id,
            &[signing_key_g],
            &[signing_key_f],
            &[],
        )
        .unwrap();

    let strictness = WellFormedStrictness::default();
    let res = intent_signed.clone().well_formed(
        segment_id,
        &ledger_state,
        strictness,
        Timestamp::from_secs(0),
    );

    match res {
        Ok(_) => {
            let replay_state = ReplayProtectionState::new();
            match replay_state.apply_intent(
                intent.clone(),
                Timestamp::from_secs(0),
                ledger_state.parameters.global_ttl,
            ) {
                Ok(_) => panic!("Test succeeded unexpectedly."),
                Err(TransactionApplicationError::IntentTtlTooFarInFuture(_, _)) => (),
                Err(e) => panic!(
                    "Test failed as expected, but the error was unexpected: {:?}",
                    e.to_string()
                ),
            }
        }
        Err(e) => panic!("{:?}", e.to_string()),
    }
}

#[cfg(feature = "proving")]
#[tokio::test]
async fn post_block_update() {
    use midnight_ledger::structure::IntentHash;

    let time1 = Timestamp::from_secs(10);

    // Time before which entries should be removed
    let tblock = Timestamp::from_secs(15);

    // Shouldn't be removed
    let time2 = Timestamp::from_secs(20);

    let mut rng = StdRng::seed_from_u64(0x42);

    let mut rps: ReplayProtectionState<InMemoryDB> = ReplayProtectionState::new();

    let hash1: IntentHash = rng.r#gen();
    let hash2: IntentHash = rng.r#gen();

    rps.time_filter_map = rps.time_filter_map.upsert_one(time1, hash1);
    rps.time_filter_map = rps.time_filter_map.upsert_one(time2, hash2);

    rps = rps.post_block_update(tblock);

    assert!(rps.time_filter_map.get(time1).is_none());
    assert!(rps.time_filter_map.get(time2).is_some());

    assert!(!rps.time_filter_map.contains(&hash1));
    assert!(rps.time_filter_map.contains(&hash2));
}

// TK: We don't actually have the primitives here yet until the compiler picks them up!
// We also don't have a test, so.... Come back when we do?
//
// #[cfg(feature = "proving")]
// #[tokio::test]
// async fn contract_to_contract_transfer() {
//     /*
//
//     @Will
//
//     The flow starts by deploying two contracts, A and B. Contract A mints some unshielded tokens by
//     executing the 'minter_program'. This mints a new unshielded token that belongs to A. Contract A then
//     sends some unshielded tokens by executing 'sender_program'. Contract B then receives them by executing
//     'receiver_program'. We need to check that the final balances of A and B are correct.
//
//     It is unclear to me whether the operations contained within each program are the necessary and
//     sufficient ones to do an unshielded transfer.
//
//     In the case below, we don't start with any initial token balances for A or B because A is minting
//     the tokens it will send to B. Other cases might start with non-zero balances for A and B and check
//     that things still work.
//
//     The below test should be one where the minting, sending, and receiving programs are all included
//     in the same transaction. We might also want a test where each program is run in a separate transaction.
//     */
//     let mut rng = StdRng::seed_from_u64(0x42);
//
//     // You would actually get these addresses from deployments
//     let contract_a_addr = ContractAddress(rng.gen());
//     let contract_b_addr = ContractAddress(rng.gen());
//
//     let domain_sep = HashOutput(*b"midnight:derive_token\0\0\0\0\0\0\0\0\0\0\0");
//     let unshielded_token_type =
//         TokenType::Unshielded(contract_a_addr.custom_unshielded_token_type(domain_sep));
//
//     let mint_amount = 100u64;
//     // We are minting the new unshielded token to contract A. There should also be tests that mint
//     // tokens directly to contract B, in which case we would use 'contract_b_addr'. The sending and
//     // receiving logic would have to be modified for consistency.
//     let mint_recipient = PublicAddress::Contract(contract_a_addr);
//     let _minter_program: &[&[Op<ResultModeVerify, InMemoryDB>; 16]; 2] = &[
//         &kernel_mint_unshielded!(
//             (),
//             (),
//             AlignedValue::from(domain_sep),
//             AlignedValue::from(mint_amount)
//         ),
//         &kernel_claim_unshielded_coin_spend!(
//             (),
//             (),
//             AlignedValue::from(unshielded_token_type),
//             AlignedValue::from(mint_recipient),
//             AlignedValue::from(mint_amount)
//         ),
//     ];
//
//     let send_amount = 50u64;
//     let send_recipient = PublicAddress::Contract(contract_b_addr);
//     let _sender_program: &[&[Op<ResultModeVerify, InMemoryDB>; 16]; 2] = &[
//         &kernel_inc_unshielded_outputs!(
//             (),
//             (),
//             AlignedValue::from(unshielded_token_type),
//             AlignedValue::from(send_amount)
//         ),
//         &kernel_claim_unshielded_coin_spend!(
//             (),
//             (),
//             AlignedValue::from(unshielded_token_type),
//             AlignedValue::from(send_recipient),
//             AlignedValue::from(send_amount)
//         ),
//     ];
//
//     // For this simple case, we receive the exact amount the sender sent. Another case could be
//     // when A sends a different amount than B receives.
//     let receive_amount = 50u64;
//     let _receiver_program: &[&[Op<ResultModeVerify, InMemoryDB>; 16]; 1] =
//         &[&kernel_inc_unshielded_inputs!(
//             (),
//             (),
//             AlignedValue::from(unshielded_token_type),
//             AlignedValue::from(receive_amount)
//         )];
//
//     ()
// }

fn assert_failure(res: Result<(), MalformedTransaction<InMemoryDB>>) {
    match res {
        Ok(_) => panic!("Test succeeded unexpectedly."),
        Err(MalformedTransaction::IntentSignatureVerificationFailure) => (),
        Err(e) => panic!(
            "Test failed as expected, but the error was unexpected: {:?}",
            e.to_string()
        ),
    }
}

fn assert_success(res: Result<(), MalformedTransaction<InMemoryDB>>) {
    match res {
        Ok(_) => (),
        Err(e) => panic!("{:?}", e.to_string()),
    }
}

fn gen_unshielded_offer(rng: &mut StdRng) -> (SigningKey, UnshieldedOffer<Signature, InMemoryDB>) {
    let signing_key: SigningKey = SigningKey::sample(rng.clone());

    let spend = UtxoSpend {
        value: rng.r#gen(),
        owner: signing_key.verifying_key(),
        type_: rng.r#gen(),
        intent_hash: rng.r#gen(),
        output_no: rng.r#gen(),
    };

    let out = UtxoOutput {
        value: rng.r#gen(),
        owner: rng.r#gen(),
        type_: rng.r#gen(),
    };

    (
        signing_key,
        UnshieldedOffer {
            inputs: vec![spend].into(),
            outputs: vec![out].into(),
            signatures: vec![].into(),
        },
    )
}

async fn setup() -> (
    StdRng,
    LocalProvingProvider<'static, StdRng, Resolver, Resolver>,
    ContractCallPrototype<InMemoryDB>,
    LedgerState<InMemoryDB>,
) {
    let mut rng = StdRng::seed_from_u64(0x42);
    // Initial states
    let mut state: TestState<InMemoryDB> = TestState::new(&mut rng);

    let prover = LocalProvingProvider {
        rng: rng.split(),
        resolver: &*RESOLVER,
        params: &*RESOLVER,
    };

    // Part 1: Deploy
    println!(":: Part 1: Deploy");
    let count_op = ContractOperation::new(verifier_key(&RESOLVER, "count").await);
    let contract = ContractState::new(
        stval!([(0u64), (false), (0u64)]),
        HashMap::new().insert(b"count"[..].into(), count_op.clone()),
        Default::default(),
    );
    let (tx, addr) = {
        let deploy = ContractDeploy::new(&mut rng, contract.clone());
        let addr = deploy.address();
        let tx = tx_prove(
            rng.split(),
            &Transaction::from_intents(
                "local-test",
                test_intents(&mut rng, Vec::new(), Vec::new(), vec![deploy], state.time),
            ),
            &RESOLVER,
        )
        .await
        .unwrap();
        (tx, addr)
    };
    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = false;
    state.assert_apply(&tx, strictness);

    // Part 2: First application
    println!(":: Part 2: First count");
    let guaranteed_public_transcript = partition_transcripts(
        &[PreTranscript {
            context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
            program: program_with_results(&Counter_increment!([key!(0u8)], false, 1u64), &[]),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap()[0]
        .0
        .clone()
        .unwrap();
    let fallible_public_transcript = partition_transcripts(
        &[PreTranscript {
            // Playing fast and loose with state here, this should be the state after applying
            // the guaranteed part, not that it matters here.
            context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
            program: program_with_results(
                &[
                    &kernel_checkpoint!((), ())[..],
                    &Cell_read!([key!(1u8)], false, bool),
                    &Cell_write!([key!(1u8)], false, bool, true),
                    &Counter_increment!([key!(2u8)], false, 1u64),
                ]
                .into_iter()
                .flat_map(|x| x.iter())
                .cloned()
                .collect::<Vec<_>>(),
                &[false.into()],
            ),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap()[0]
        .0
        .clone()
        .unwrap();
    let call = ContractCallPrototype {
        address: addr,
        entry_point: b"count"[..].into(),
        op: count_op.clone(),
        input: ().into(),
        output: ().into(),
        guaranteed_public_transcript: Some(guaranteed_public_transcript),
        fallible_public_transcript: Some(fallible_public_transcript),
        private_transcript_outputs: vec![],
        communication_commitment_rand: rng.r#gen(),
        key_location: KeyLocation(Cow::Borrowed("count")),
    };

    tx_prove(
        rng.split(),
        &Transaction::from_intents(
            "local-test",
            test_intents(
                &mut rng,
                vec![call.clone()],
                Vec::new(),
                Vec::new(),
                Timestamp::from_secs(0),
            ),
        ),
        &RESOLVER,
    )
    .await
    .unwrap();

    tx.well_formed(&state.ledger, strictness, Timestamp::from_secs(0))
        .unwrap();

    (rng, prover, call, state.ledger)
}
