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

use base_crypto::fab::AlignedValue;
use base_crypto::hash::{HashOutput, persistent_commit};
use base_crypto::rng::SplittableRng;
use base_crypto::signatures::Signature;
use base_crypto::time::Timestamp;
use coin_structure::coin::Info as CoinInfo;
use coin_structure::transfer::{Recipient, SenderEvidence};
use lazy_static::lazy_static;
use midnight_ledger::construct::{ContractCallPrototype, PreTranscript, partition_transcripts};
use midnight_ledger::error::{
    EffectsCheckError, MalformedTransaction, SequencingCheckError, TransactionInvalid,
};
use midnight_ledger::semantics::TransactionResult;
use midnight_ledger::structure::{
    ContractDeploy, INITIAL_PARAMETERS, ProofPreimageMarker, Transaction,
};
use midnight_ledger::test_utilities::PUBLIC_PARAMS;
use midnight_ledger::test_utilities::{Resolver, TestState, test_resolver, verifier_key};
use midnight_ledger::test_utilities::{test_intents, tx_prove};
use midnight_ledger::verify::WellFormedStrictness;
use onchain_runtime::context::QueryContext;
use onchain_runtime::error::TranscriptRejected;
use onchain_runtime::ops::{Key, Op, key};
use onchain_runtime::program_fragments::*;
use onchain_runtime::result_mode::{ResultModeGather, ResultModeVerify};
use onchain_runtime::state::{ChargedState, ContractOperation, ContractState, StateValue, stval};
use onchain_runtime::vm_error::OnchainProgramError;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::borrow::Cow;
use storage::arena::Sp;
use storage::db::{DB, InMemoryDB};
use storage::storage::{Array, HashMap};
use transient_crypto::commitment::PedersenRandomness;
use transient_crypto::fab::ValueReprAlignedValue;
use transient_crypto::hash::transient_commit;
use transient_crypto::proofs::KeyLocation;
use transient_crypto::proofs::{ProvingKeyMaterial, Resolver as ResolverT};
use zswap::Delta;
use zswap::{Offer, Output, Transient};

lazy_static! {
    static ref RESOLVER_INNER: Resolver = test_resolver("composable-inner");
    static ref RESOLVER_OUTER: Resolver = test_resolver("composable-outer");
    static ref RESOLVER_RELAY: Resolver = test_resolver("composable-relay");
    static ref RESOLVER_BURN: Resolver = test_resolver("composable-burn");
}

async fn resolve_any(key: KeyLocation) -> std::io::Result<Option<ProvingKeyMaterial>> {
    let resolvers = [
        &*RESOLVER_INNER,
        &*RESOLVER_OUTER,
        &*RESOLVER_RELAY,
        &*RESOLVER_BURN,
    ];
    for resolver in resolvers.into_iter() {
        if let Some(res) = resolver.resolve_key(key.clone()).await? {
            return Ok(Some(res));
        }
    }
    Ok(None)
}

lazy_static! {
    static ref RESOLVER: Resolver = Resolver::new(
        PUBLIC_PARAMS.clone(),
        midnight_ledger::dust::DustResolver(
            base_crypto::data_provider::MidnightDataProvider::new(
                base_crypto::data_provider::FetchMode::OnDemand,
                base_crypto::data_provider::OutputMode::Log,
                midnight_ledger::dust::DUST_EXPECTED_FILES.to_owned(),
            )
            .unwrap()
        ),
        Box::new(|inp| Box::pin(resolve_any(inp))),
    );
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

#[tokio::test]
async fn composable() {
    let mut rng = StdRng::seed_from_u64(0x42);
    // Initial states
    let mut state: TestState<InMemoryDB> = TestState::new(&mut rng);

    // Part 1: Deploy inner
    println!(":: Part 1: Deploy inner");
    let get_op = ContractOperation::new(verifier_key(&RESOLVER, "get").await);
    let set_op = ContractOperation::new(verifier_key(&RESOLVER, "set").await);
    let auth_sk: HashOutput = rng.r#gen();
    let auth_pk = persistent_commit(b"mdn:ex:ci", auth_sk);
    let contract = ContractState::new(
        stval!([(auth_pk), (b"hello".to_vec())]),
        HashMap::new()
            .insert(b"get"[..].into(), get_op.clone())
            .insert(b"set"[..].into(), set_op.clone()),
        Default::default(),
    );
    let (tx, addr_inner) = {
        let deploy = ContractDeploy::new(&mut rng, contract.clone());
        let addr = deploy.address();
        let tx = tx_prove(
            rng.split(),
            &Transaction::from_intents(
                "local-test",
                test_intents(
                    &mut rng,
                    Vec::new(),
                    Vec::new(),
                    vec![deploy],
                    Timestamp::from_secs(0),
                ),
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

    // Part 2: Deploy outer
    println!(":: Part 2: Deploy outer");
    let ep_hash = persistent_commit(
        b"get",
        HashOutput(*b"midnight:entry-point\0\0\0\0\0\0\0\0\0\0\0\0"),
    );
    let update_op = ContractOperation::new(verifier_key(&RESOLVER, "update").await);
    let contract = ContractState::new(
        stval!([(b"".to_vec()), (addr_inner), (ep_hash)]),
        HashMap::new().insert(b"update"[..].into(), update_op.clone()),
        Default::default(),
    );
    let (tx, addr_outer) = {
        let deploy = ContractDeploy::new(&mut rng, contract.clone());
        let addr = deploy.address();
        let tx = tx_prove(
            rng.split(),
            &Transaction::from_intents(
                "local-test",
                test_intents(
                    &mut rng,
                    Vec::new(),
                    Vec::new(),
                    vec![deploy],
                    Timestamp::from_secs(0),
                ),
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

    // Part 3: (Golden) run update
    println!(":: Part 3: (Golden) run update");
    let tx = {
        let cc_rand = rng.r#gen();
        let cc = transient_commit(&ValueReprAlignedValue(b"hello".to_vec().into()), cc_rand);
        let transcripts = partition_transcripts(
            &[
                PreTranscript {
                    context: QueryContext::new(
                        state.ledger.index(addr_inner).unwrap().data,
                        addr_inner,
                    ),
                    program: program_with_results(
                        &Cell_read!([key!(1u8)], false, Vec<u8>),
                        &[b"hello".to_vec().into()],
                    ),
                    comm_comm: Some(cc),
                },
                PreTranscript {
                    context: QueryContext::new(
                        state.ledger.index(addr_outer).unwrap().data,
                        addr_outer,
                    ),
                    program: program_with_results(
                        &[
                            &Cell_read!([key!(1u8)], false, HashOutput)[..],
                            &Cell_read!([key!(2u8)], false, HashOutput),
                            &kernel_claim_contract_call!(
                                (),
                                (),
                                AlignedValue::from(addr_inner.0),
                                AlignedValue::from(ep_hash),
                                AlignedValue::from(cc)
                            ),
                            &Cell_write!([key!(0u8)], false, Vec<u8>, b"hello".to_vec()),
                        ]
                        .into_iter()
                        .flat_map(|x| x.iter())
                        .cloned()
                        .collect::<Vec<_>>(),
                        &[addr_inner.into(), ep_hash.into()],
                    ),
                    comm_comm: None,
                },
            ],
            &INITIAL_PARAMETERS,
        )
        .unwrap();
        let call_inner = ContractCallPrototype {
            address: addr_inner,
            entry_point: b"get"[..].into(),
            op: get_op.clone(),
            guaranteed_public_transcript: transcripts[0].0.clone(),
            fallible_public_transcript: transcripts[0].1.clone(),
            private_transcript_outputs: vec![],
            input: ().into(),
            output: b"hello".to_vec().into(),
            communication_commitment_rand: cc_rand,
            key_location: KeyLocation(Cow::Borrowed("get")),
        };
        let call_outer = ContractCallPrototype {
            address: addr_outer,
            entry_point: b"update"[..].into(),
            op: update_op.clone(),
            guaranteed_public_transcript: transcripts[1].0.clone(),
            fallible_public_transcript: transcripts[1].1.clone(),
            private_transcript_outputs: vec![b"hello".to_vec().into(), cc_rand.into()],
            input: ().into(),
            output: ().into(),
            communication_commitment_rand: rng.r#gen(),
            key_location: KeyLocation(Cow::Borrowed("update")),
        };
        let pre_tx = Transaction::from_intents(
            "local-test",
            test_intents(
                &mut rng,
                vec![call_inner, call_outer],
                Vec::new(),
                Vec::new(),
                Timestamp::from_secs(0),
            ),
        );
        let tx = tx_prove(rng.split(), &pre_tx, &RESOLVER).await.unwrap();
        tx.well_formed(&state.ledger, strictness, state.time)
            .unwrap();
        tx
    };
    state.assert_apply(&tx, strictness);
    assert_eq!(
        state.ledger.index(addr_outer).unwrap().data.get_ref(),
        &stval!([(b"hello".to_vec()), (addr_inner), (ep_hash)])
    );

    // Part 4: Rejected (call missing)
    println!(":: Part 4: Rejected (call missing)");
    {
        let cc_rand = rng.r#gen();
        let cc = transient_commit(
            &ValueReprAlignedValue(AlignedValue::from(b"malicious".to_vec())),
            cc_rand,
        );
        let transcripts = partition_transcripts(
            &[PreTranscript {
                context: QueryContext::new(
                    state.ledger.index(addr_outer).unwrap().data,
                    addr_outer,
                ),
                program: program_with_results(
                    &[
                        &Cell_read!([key!(1u8)], false, HashOutput)[..],
                        &Cell_read!([key!(2u8)], false, HashOutput),
                        &kernel_claim_contract_call!(
                            (),
                            (),
                            AlignedValue::from(addr_inner.0),
                            AlignedValue::from(ep_hash),
                            AlignedValue::from(cc)
                        ),
                        &Cell_write!([key!(0u8)], false, Vec<u8>, b"malicious".to_vec()),
                    ]
                    .into_iter()
                    .flat_map(|x| x.iter())
                    .cloned()
                    .collect::<Vec<_>>(),
                    &[addr_inner.into(), ep_hash.into()],
                ),
                comm_comm: None,
            }],
            &INITIAL_PARAMETERS,
        )
        .unwrap();
        let call_outer = ContractCallPrototype {
            address: addr_outer,
            entry_point: b"update"[..].into(),
            op: update_op.clone(),
            guaranteed_public_transcript: transcripts[0].0.clone(),
            fallible_public_transcript: transcripts[0].1.clone(),
            private_transcript_outputs: vec![b"malicious".to_vec().into(), cc_rand.into()],
            input: ().into(),
            output: ().into(),
            communication_commitment_rand: rng.r#gen(),
            key_location: KeyLocation(Cow::Borrowed("update")),
        };
        let pre_tx = Transaction::from_intents(
            "local-test",
            test_intents(
                &mut rng,
                vec![call_outer],
                Vec::new(),
                Vec::new(),
                Timestamp::from_secs(0),
            ),
        );
        let tx = tx_prove(rng.split(), &pre_tx, &RESOLVER).await.unwrap();

        match tx.well_formed(&state.ledger, strictness, state.time) {
            Err(MalformedTransaction::EffectsCheckFailure(
                EffectsCheckError::RealCallsSubsetCheckFailure(_),
            )) => (),
            Err(e) => panic!("{e:?}"),
            Ok(_) => panic!("succeeded unexpectedly"),
        };
    }

    // Part 5: Rejected (call mismatch)
    println!(":: Part 5: Rejected (call mismatch)");
    {
        let cc_rand = rng.r#gen();
        let cc = transient_commit(
            &ValueReprAlignedValue(AlignedValue::from(b"malicious".to_vec())),
            cc_rand,
        );
        let transcripts = partition_transcripts(
            &[
                PreTranscript {
                    context: QueryContext::new(
                        state.ledger.index(addr_inner).unwrap().data,
                        addr_inner,
                    ),
                    program: program_with_results(
                        &Cell_read!([key!(1u8)], false, Vec<u8>),
                        &[b"hello".to_vec().into()],
                    ),
                    comm_comm: Some(cc),
                },
                PreTranscript {
                    context: QueryContext::new(
                        state.ledger.index(addr_outer).unwrap().data,
                        addr_outer,
                    ),
                    program: program_with_results(
                        &[
                            &Cell_read!([key!(1u8)], false, HashOutput)[..],
                            &Cell_read!([key!(2u8)], false, HashOutput),
                            &kernel_claim_contract_call!(
                                (),
                                (),
                                AlignedValue::from(addr_inner.0),
                                AlignedValue::from(ep_hash),
                                AlignedValue::from(cc)
                            ),
                            &Cell_write!([key!(0u8)], false, Vec<u8>, b"malicious".to_vec()),
                        ]
                        .into_iter()
                        .flat_map(|x| x.iter())
                        .cloned()
                        .collect::<Vec<_>>(),
                        &[addr_inner.into(), ep_hash.into()],
                    ),
                    comm_comm: None,
                },
            ],
            &INITIAL_PARAMETERS,
        )
        .unwrap();
        let call_inner = ContractCallPrototype {
            address: addr_inner,
            entry_point: b"get"[..].into(),
            op: get_op.clone(),
            guaranteed_public_transcript: transcripts[0].0.clone(),
            fallible_public_transcript: transcripts[0].1.clone(),
            private_transcript_outputs: vec![],
            input: ().into(),
            output: b"hello".to_vec().into(),
            communication_commitment_rand: cc_rand,
            key_location: KeyLocation(Cow::Borrowed("get")),
        };
        let call_outer = ContractCallPrototype {
            address: addr_outer,
            entry_point: b"update"[..].into(),
            op: update_op.clone(),
            guaranteed_public_transcript: transcripts[1].0.clone(),
            fallible_public_transcript: transcripts[1].1.clone(),
            private_transcript_outputs: vec![b"malicious".to_vec().into(), cc_rand.into()],
            input: ().into(),
            output: ().into(),
            communication_commitment_rand: rng.r#gen(),
            key_location: KeyLocation(Cow::Borrowed("update")),
        };
        let pre_tx = Transaction::from_intents(
            "local-test",
            test_intents(
                &mut rng,
                vec![call_inner, call_outer],
                Vec::new(),
                Vec::new(),
                Timestamp::from_secs(0),
            ),
        );
        let tx = tx_prove(rng.split(), &pre_tx, &RESOLVER).await.unwrap();
        assert!(matches!(
            tx.well_formed(&state.ledger, strictness, state.time),
            Err(MalformedTransaction::EffectsCheckFailure(
                EffectsCheckError::RealCallsSubsetCheckFailure(_)
            ))
        ));
    }

    // Part 6: Rejected (read mismatch)
    println!(":: Part 6: Rejected (read mismatch)");
    let tx = {
        let cc_rand = rng.r#gen();
        let cc = transient_commit(
            &ValueReprAlignedValue(b"malicious".to_vec().into()),
            cc_rand,
        );
        let transcripts = partition_transcripts(
            &[
                PreTranscript {
                    context: QueryContext::new(
                        ChargedState::new(stval!([(auth_pk), (b"malicious".to_vec())])),
                        addr_inner,
                    ),
                    program: program_with_results(
                        &Cell_read!([key!(1u8)], false, Vec<u8>),
                        &[b"malicious".to_vec().into()],
                    ),
                    comm_comm: Some(cc),
                },
                PreTranscript {
                    context: QueryContext::new(
                        state.ledger.index(addr_outer).unwrap().data,
                        addr_outer,
                    ),
                    program: program_with_results(
                        &[
                            &Cell_read!([key!(1u8)], false, HashOutput)[..],
                            &Cell_read!([key!(2u8)], false, HashOutput),
                            &kernel_claim_contract_call!(
                                (),
                                (),
                                AlignedValue::from(addr_inner.0),
                                AlignedValue::from(ep_hash),
                                AlignedValue::from(cc)
                            ),
                            &Cell_write!([key!(0u8)], false, Vec<u8>, b"malicious".to_vec()),
                        ]
                        .into_iter()
                        .flat_map(|x| x.iter())
                        .cloned()
                        .collect::<Vec<_>>(),
                        &[addr_inner.into(), ep_hash.into()],
                    ),
                    comm_comm: None,
                },
            ],
            &INITIAL_PARAMETERS,
        )
        .unwrap();
        let call_inner = ContractCallPrototype {
            address: addr_inner,
            entry_point: b"get"[..].into(),
            op: get_op.clone(),
            guaranteed_public_transcript: transcripts[0].0.clone(),
            fallible_public_transcript: transcripts[0].1.clone(),
            private_transcript_outputs: vec![],
            input: ().into(),
            output: b"malicious".to_vec().into(),
            communication_commitment_rand: cc_rand,
            key_location: KeyLocation(Cow::Borrowed("get")),
        };
        let call_outer = ContractCallPrototype {
            address: addr_outer,
            entry_point: b"update"[..].into(),
            op: update_op.clone(),
            guaranteed_public_transcript: transcripts[1].0.clone(),
            fallible_public_transcript: transcripts[1].1.clone(),
            private_transcript_outputs: vec![b"malicious".to_vec().into(), cc_rand.into()],
            input: ().into(),
            output: ().into(),
            communication_commitment_rand: rng.r#gen(),
            key_location: KeyLocation(Cow::Borrowed("update")),
        };
        let pre_tx = Transaction::from_intents(
            "local-test",
            test_intents(
                &mut rng,
                vec![call_inner, call_outer],
                Vec::new(),
                Vec::new(),
                Timestamp::from_secs(0),
            ),
        );
        let tx = tx_prove(rng.split(), &pre_tx, &RESOLVER).await.unwrap();
        tx.well_formed(&state.ledger, strictness, state.time)
            .unwrap()
    };
    match state.ledger.apply(&tx, &state.context()).1 {
        TransactionResult::PartialSuccess(res, _) => assert!(matches!(
            res.get(&1),
            Some(Err(TransactionInvalid::Transcript(
                TranscriptRejected::Execution(OnchainProgramError::ReadMismatch { .. })
            )))
        )),
        r => panic!("unexpected transaction result: {r:?}"),
    }

    // Part 7: Rejected (read mismatch & fallibility hacking)
    println!(":: Part 7: Rejected (read mismatch & fallibility hacking)");
    {
        let cc_rand = rng.r#gen();
        let cc = transient_commit(
            &ValueReprAlignedValue(b"malicious".to_vec().into()),
            cc_rand,
        );
        let transcripts = partition_transcripts(
            &[
                PreTranscript {
                    context: QueryContext::new(
                        ChargedState::new(stval!([(auth_pk), (b"malicious".to_vec())])),
                        addr_inner,
                    ),
                    program: program_with_results(
                        &Cell_read!([key!(1u8)], false, Vec<u8>),
                        &[b"malicious".to_vec().into()],
                    ),
                    comm_comm: Some(cc),
                },
                PreTranscript {
                    context: QueryContext::new(
                        state.ledger.index(addr_outer).unwrap().data,
                        addr_outer,
                    ),
                    program: program_with_results(
                        &[
                            &Cell_read!([key!(1u8)], false, HashOutput)[..],
                            &Cell_read!([key!(2u8)], false, HashOutput),
                            &kernel_claim_contract_call!(
                                (),
                                (),
                                AlignedValue::from(addr_inner.0),
                                AlignedValue::from(ep_hash),
                                AlignedValue::from(cc)
                            ),
                            &Cell_write!([key!(0u8)], false, Vec<u8>, b"malicious".to_vec()),
                        ]
                        .into_iter()
                        .flat_map(|x| x.iter())
                        .cloned()
                        .collect::<Vec<_>>(),
                        &[addr_inner.into(), ep_hash.into()],
                    ),
                    comm_comm: None,
                },
            ],
            &INITIAL_PARAMETERS,
        )
        .unwrap();
        let call_inner = ContractCallPrototype {
            address: addr_inner,
            entry_point: b"get"[..].into(),
            op: get_op.clone(),
            guaranteed_public_transcript: transcripts[0].0.clone(),
            fallible_public_transcript: transcripts[0].1.clone(),
            private_transcript_outputs: vec![],
            input: ().into(),
            output: b"malicious".to_vec().into(),
            communication_commitment_rand: cc_rand,
            key_location: KeyLocation(Cow::Borrowed("get")),
        };
        dbg!(&transcripts);
        // Manually move things to the guaranteed section.
        let call_outer = ContractCallPrototype {
            address: addr_outer,
            entry_point: b"update"[..].into(),
            op: update_op.clone(),
            guaranteed_public_transcript: transcripts[1].1.clone(),
            fallible_public_transcript: None,
            private_transcript_outputs: vec![b"malicious".to_vec().into(), cc_rand.into()],
            input: ().into(),
            output: ().into(),
            communication_commitment_rand: rng.r#gen(),
            key_location: KeyLocation(Cow::Borrowed("update")),
        };
        let pre_tx = Transaction::from_intents(
            "local-test",
            test_intents(
                &mut rng,
                vec![call_inner, call_outer],
                Vec::new(),
                Vec::new(),
                Timestamp::from_secs(0),
            ),
        );
        let tx = tx_prove(rng.split(), &pre_tx, &RESOLVER).await.unwrap();

        match tx.well_formed(&state.ledger, strictness, state.time) {
            Err(MalformedTransaction::SequencingCheckFailure(
                SequencingCheckError::FallibleInGuaranteedContextViolation { .. },
            )) => (),
            Err(e) => panic!("{e:?}"),
            Ok(_) => panic!("succeeded unexpectedly"),
        };
    }
}

#[tokio::test]
// Quick and dirty modification of `composable`
async fn guaranteed_in_fallible() {
    let mut rng = StdRng::seed_from_u64(0x42);
    // Initial states
    let mut ledger_state: TestState<InMemoryDB> = TestState::new(&mut rng);

    // Part 1: Deploy inner
    println!(":: Part 1: Deploy inner");
    let get_op = ContractOperation::new(verifier_key(&RESOLVER, "get").await);
    let set_op = ContractOperation::new(verifier_key(&RESOLVER, "set").await);
    let auth_sk: HashOutput = rng.r#gen();
    let auth_pk = persistent_commit(b"mdn:ex:ci", auth_sk);
    let contract = ContractState::new(
        stval!([(auth_pk), (b"hello".to_vec())]),
        HashMap::new()
            .insert(b"get"[..].into(), get_op.clone())
            .insert(b"set"[..].into(), set_op.clone()),
        Default::default(),
    );
    let (tx, addr_inner) = {
        let deploy = ContractDeploy::new(&mut rng, contract.clone());
        let addr = deploy.address();
        let tx = tx_prove(
            rng.split(),
            &Transaction::from_intents(
                "local-test",
                test_intents(
                    &mut rng,
                    Vec::new(),
                    Vec::new(),
                    vec![deploy],
                    Timestamp::from_secs(0),
                ),
            ),
            &RESOLVER,
        )
        .await
        .unwrap();
        (tx, addr)
    };
    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = false;
    ledger_state.assert_apply(&tx, strictness);

    // Part 2: Deploy outer
    println!(":: Part 2: Deploy outer");
    let ep_hash = persistent_commit(
        b"get",
        HashOutput(*b"midnight:entry-point\0\0\0\0\0\0\0\0\0\0\0\0"),
    );
    let update_op = ContractOperation::new(verifier_key(&RESOLVER, "update").await);
    let contract = ContractState::new(
        stval!([(b"".to_vec()), (addr_inner), (ep_hash)]),
        HashMap::new().insert(b"update"[..].into(), update_op.clone()),
        Default::default(),
    );
    let (tx, addr_outer) = {
        let deploy = ContractDeploy::new(&mut rng, contract.clone());
        let addr = deploy.address();
        let tx = tx_prove(
            rng.split(),
            &Transaction::from_intents(
                "local-test",
                test_intents(
                    &mut rng,
                    Vec::new(),
                    Vec::new(),
                    vec![deploy],
                    Timestamp::from_secs(0),
                ),
            ),
            &RESOLVER,
        )
        .await
        .unwrap();
        (tx, addr)
    };
    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = false;
    ledger_state.assert_apply(&tx, strictness);

    // Part 3: (Golden) run update
    println!(":: Part 3: (Golden) run update");
    let tx = {
        let cc_rand = rng.r#gen();
        let cc = transient_commit(&ValueReprAlignedValue(b"hello".to_vec().into()), cc_rand);
        let transcripts = partition_transcripts(
            &[
                PreTranscript {
                    context: QueryContext::new(
                        ledger_state.ledger.index(addr_inner).unwrap().data,
                        addr_inner,
                    ),
                    program: program_with_results(
                        &Cell_read!([key!(1u8)], false, Vec<u8>),
                        &[b"hello".to_vec().into()],
                    ),
                    comm_comm: Some(cc),
                },
                PreTranscript {
                    context: QueryContext::new(
                        ledger_state.ledger.index(addr_outer).unwrap().data,
                        addr_outer,
                    ),
                    program: program_with_results(
                        &[
                            &Cell_read!([key!(1u8)], false, HashOutput)[..],
                            &Cell_read!([key!(2u8)], false, HashOutput),
                            &kernel_claim_contract_call!(
                                (),
                                (),
                                AlignedValue::from(addr_inner.0),
                                AlignedValue::from(ep_hash),
                                AlignedValue::from(cc)
                            ),
                            &Cell_write!([key!(0u8)], false, Vec<u8>, b"hello".to_vec()),
                        ]
                        .into_iter()
                        .flat_map(|x| x.iter())
                        .cloned()
                        .collect::<Vec<_>>(),
                        &[addr_inner.into(), ep_hash.into()],
                    ),
                    comm_comm: None,
                },
            ],
            &INITIAL_PARAMETERS,
        )
        .unwrap();
        let call_inner = ContractCallPrototype {
            address: addr_inner,
            entry_point: b"get"[..].into(),
            op: get_op.clone(),
            guaranteed_public_transcript: transcripts[0].0.clone(),
            fallible_public_transcript: transcripts[0].1.clone(),
            private_transcript_outputs: vec![],
            input: ().into(),
            output: b"hello".to_vec().into(),
            communication_commitment_rand: cc_rand,
            key_location: KeyLocation(Cow::Borrowed("get")),
        };
        let call_outer = ContractCallPrototype {
            address: addr_outer,
            entry_point: b"update"[..].into(),
            op: update_op.clone(),
            guaranteed_public_transcript: transcripts[1].0.clone(),
            fallible_public_transcript: transcripts[1].1.clone(),
            private_transcript_outputs: vec![b"hello".to_vec().into(), cc_rand.into()],
            input: ().into(),
            output: ().into(),
            communication_commitment_rand: rng.r#gen(),
            key_location: KeyLocation(Cow::Borrowed("update")),
        };
        let pre_tx = Transaction::from_intents(
            "local-test",
            test_intents(
                &mut rng,
                vec![call_inner, call_outer],
                Vec::new(),
                Vec::new(),
                Timestamp::from_secs(0),
            ),
        );
        tx_prove(rng.split(), &pre_tx, &RESOLVER).await.unwrap()
    };
    ledger_state.assert_apply(&tx, strictness);
    assert_eq!(
        ledger_state
            .ledger
            .index(addr_outer)
            .unwrap()
            .data
            .get_ref(),
        &stval!([(b"hello".to_vec()), (addr_inner), (ep_hash)])
    );

    // Part 4: Rejected (call missing)
    println!(":: Part 4: Rejected (call missing)");
    {
        let cc_rand = rng.r#gen();
        let cc = transient_commit(
            &ValueReprAlignedValue(AlignedValue::from(b"malicious".to_vec())),
            cc_rand,
        );
        let transcripts = partition_transcripts(
            &[PreTranscript {
                context: QueryContext::new(
                    ledger_state.ledger.index(addr_outer).unwrap().data,
                    addr_outer,
                ),
                program: program_with_results(
                    &[
                        &Cell_read!([key!(1u8)], false, HashOutput)[..],
                        &Cell_read!([key!(2u8)], false, HashOutput),
                        &kernel_claim_contract_call!(
                            (),
                            (),
                            AlignedValue::from(addr_inner.0),
                            AlignedValue::from(ep_hash),
                            AlignedValue::from(cc)
                        ),
                        &Cell_write!([key!(0u8)], false, Vec<u8>, b"malicious".to_vec()),
                    ]
                    .into_iter()
                    .flat_map(|x| x.iter())
                    .cloned()
                    .collect::<Vec<_>>(),
                    &[addr_inner.into(), ep_hash.into()],
                ),
                comm_comm: None,
            }],
            &INITIAL_PARAMETERS,
        )
        .unwrap();
        let call_outer = ContractCallPrototype {
            address: addr_outer,
            entry_point: b"update"[..].into(),
            op: update_op.clone(),
            guaranteed_public_transcript: transcripts[0].0.clone(),
            fallible_public_transcript: transcripts[0].1.clone(),
            private_transcript_outputs: vec![b"malicious".to_vec().into(), cc_rand.into()],
            input: ().into(),
            output: ().into(),
            communication_commitment_rand: rng.r#gen(),
            key_location: KeyLocation(Cow::Borrowed("update")),
        };
        let pre_tx = Transaction::from_intents(
            "local-test",
            test_intents(
                &mut rng,
                vec![call_outer],
                Vec::new(),
                Vec::new(),
                Timestamp::from_secs(0),
            ),
        );
        let tx = tx_prove(rng.split(), &pre_tx, &RESOLVER).await.unwrap();

        match tx.well_formed(&ledger_state.ledger, strictness, Timestamp::from_secs(0)) {
            Err(MalformedTransaction::EffectsCheckFailure(
                EffectsCheckError::RealCallsSubsetCheckFailure(_),
            )) => (),
            Err(e) => panic!("{e:?}"),
            Ok(_) => panic!("succeeded unexpectedly"),
        };
    }

    // Part 5: Rejected (call mismatch)
    println!(":: Part 5: Rejected (call mismatch)");
    {
        let cc_rand = rng.r#gen();
        let cc = transient_commit(
            &ValueReprAlignedValue(AlignedValue::from(b"malicious".to_vec())),
            cc_rand,
        );
        let transcripts = partition_transcripts(
            &[
                PreTranscript {
                    context: QueryContext::new(
                        ledger_state.ledger.index(addr_inner).unwrap().data,
                        addr_inner,
                    ),
                    program: program_with_results(
                        &Cell_read!([key!(1u8)], false, Vec<u8>),
                        &[b"hello".to_vec().into()],
                    ),
                    comm_comm: Some(cc),
                },
                PreTranscript {
                    context: QueryContext::new(
                        ledger_state.ledger.index(addr_outer).unwrap().data,
                        addr_outer,
                    ),
                    program: program_with_results(
                        &[
                            &Cell_read!([key!(1u8)], false, HashOutput)[..],
                            &Cell_read!([key!(2u8)], false, HashOutput),
                            &kernel_claim_contract_call!(
                                (),
                                (),
                                AlignedValue::from(addr_inner.0),
                                AlignedValue::from(ep_hash),
                                AlignedValue::from(cc)
                            ),
                            &Cell_write!([key!(0u8)], false, Vec<u8>, b"malicious".to_vec()),
                        ]
                        .into_iter()
                        .flat_map(|x| x.iter())
                        .cloned()
                        .collect::<Vec<_>>(),
                        &[addr_inner.into(), ep_hash.into()],
                    ),
                    comm_comm: None,
                },
            ],
            &INITIAL_PARAMETERS,
        )
        .unwrap();
        let call_inner = ContractCallPrototype {
            address: addr_inner,
            entry_point: b"get"[..].into(),
            op: get_op.clone(),
            guaranteed_public_transcript: transcripts[0].0.clone(),
            fallible_public_transcript: transcripts[0].1.clone(),
            private_transcript_outputs: vec![],
            input: ().into(),
            output: b"hello".to_vec().into(),
            communication_commitment_rand: cc_rand,
            key_location: KeyLocation(Cow::Borrowed("get")),
        };
        let call_outer = ContractCallPrototype {
            address: addr_outer,
            entry_point: b"update"[..].into(),
            op: update_op.clone(),
            guaranteed_public_transcript: transcripts[1].0.clone(),
            fallible_public_transcript: transcripts[1].1.clone(),
            private_transcript_outputs: vec![b"malicious".to_vec().into(), cc_rand.into()],
            input: ().into(),
            output: ().into(),
            communication_commitment_rand: rng.r#gen(),
            key_location: KeyLocation(Cow::Borrowed("update")),
        };
        let pre_tx = Transaction::from_intents(
            "local-test",
            test_intents(
                &mut rng,
                vec![call_inner, call_outer],
                Vec::new(),
                Vec::new(),
                Timestamp::from_secs(0),
            ),
        );
        let tx = tx_prove(rng.split(), &pre_tx, &RESOLVER).await.unwrap();
        assert!(matches!(
            tx.well_formed(&ledger_state.ledger, strictness, Timestamp::from_secs(0)),
            Err(MalformedTransaction::EffectsCheckFailure(
                EffectsCheckError::RealCallsSubsetCheckFailure(_)
            ))
        ));
    }

    // Part 6: Rejected (read mismatch)
    println!(":: Part 6: Rejected (read mismatch)");
    let tx = {
        let cc_rand = rng.r#gen();
        let cc = transient_commit(
            &ValueReprAlignedValue(b"malicious".to_vec().into()),
            cc_rand,
        );
        let transcripts = partition_transcripts(
            &[
                PreTranscript {
                    context: QueryContext::new(
                        ChargedState::new(stval!([(auth_pk), (b"malicious".to_vec())])),
                        addr_inner,
                    ),
                    program: program_with_results(
                        &Cell_read!([key!(1u8)], false, Vec<u8>),
                        &[b"malicious".to_vec().into()],
                    ),
                    comm_comm: Some(cc),
                },
                PreTranscript {
                    context: QueryContext::new(
                        ledger_state.ledger.index(addr_outer).unwrap().data,
                        addr_outer,
                    ),
                    program: program_with_results(
                        &[
                            &Cell_read!([key!(1u8)], false, HashOutput)[..],
                            &Cell_read!([key!(2u8)], false, HashOutput),
                            &kernel_claim_contract_call!(
                                (),
                                (),
                                AlignedValue::from(addr_inner.0),
                                AlignedValue::from(ep_hash),
                                AlignedValue::from(cc)
                            ),
                            &Cell_write!([key!(0u8)], false, Vec<u8>, b"malicious".to_vec()),
                        ]
                        .into_iter()
                        .flat_map(|x| x.iter())
                        .cloned()
                        .collect::<Vec<_>>(),
                        &[addr_inner.into(), ep_hash.into()],
                    ),
                    comm_comm: None,
                },
            ],
            &INITIAL_PARAMETERS,
        )
        .unwrap();
        let call_inner = ContractCallPrototype {
            address: addr_inner,
            entry_point: b"get"[..].into(),
            op: get_op.clone(),
            guaranteed_public_transcript: transcripts[0].0.clone(),
            fallible_public_transcript: transcripts[0].1.clone(),
            private_transcript_outputs: vec![],
            input: ().into(),
            output: b"malicious".to_vec().into(),
            communication_commitment_rand: cc_rand,
            key_location: KeyLocation(Cow::Borrowed("get")),
        };
        let call_outer = ContractCallPrototype {
            address: addr_outer,
            entry_point: b"update"[..].into(),
            op: update_op.clone(),
            guaranteed_public_transcript: transcripts[1].0.clone(),
            fallible_public_transcript: transcripts[1].1.clone(),
            private_transcript_outputs: vec![b"malicious".to_vec().into(), cc_rand.into()],
            input: ().into(),
            output: ().into(),
            communication_commitment_rand: rng.r#gen(),
            key_location: KeyLocation(Cow::Borrowed("update")),
        };
        let pre_tx = Transaction::from_intents(
            "local-test",
            test_intents(
                &mut rng,
                vec![call_inner, call_outer],
                Vec::new(),
                Vec::new(),
                Timestamp::from_secs(0),
            ),
        );
        let tx = tx_prove(rng.split(), &pre_tx, &RESOLVER).await.unwrap();
        tx.well_formed(&ledger_state.ledger, strictness, Timestamp::from_secs(0))
            .unwrap()
    };
    match ledger_state.ledger.apply(&tx, &ledger_state.context()).1 {
        TransactionResult::PartialSuccess(res, _) => assert!(matches!(
            res.get(&1),
            Some(Err(TransactionInvalid::Transcript(
                TranscriptRejected::Execution(OnchainProgramError::ReadMismatch { .. })
            )))
        )),
        r => panic!("unexpected transaction result: {r:?}"),
    }

    // Part 7: Rejected (read mismatch & fallibility hacking)
    println!(":: Part 7: Rejected (read mismatch & fallibility hacking)");
    {
        let cc_rand = rng.r#gen();
        let cc = transient_commit(
            &ValueReprAlignedValue(b"malicious".to_vec().into()),
            cc_rand,
        );
        let transcripts = partition_transcripts(
            &[
                PreTranscript {
                    context: QueryContext::new(
                        ChargedState::new(stval!([(auth_pk), (b"malicious".to_vec())])),
                        addr_inner,
                    ),
                    program: program_with_results(
                        &Cell_read!([key!(1u8)], false, Vec<u8>),
                        &[b"malicious".to_vec().into()],
                    ),
                    comm_comm: Some(cc),
                },
                PreTranscript {
                    context: QueryContext::new(
                        ledger_state.ledger.index(addr_outer).unwrap().data,
                        addr_outer,
                    ),
                    program: program_with_results(
                        &[
                            &Cell_read!([key!(1u8)], false, HashOutput)[..],
                            &Cell_read!([key!(2u8)], false, HashOutput),
                            &kernel_claim_contract_call!(
                                (),
                                (),
                                AlignedValue::from(addr_inner.0),
                                AlignedValue::from(ep_hash),
                                AlignedValue::from(cc)
                            ),
                            &Cell_write!([key!(0u8)], false, Vec<u8>, b"malicious".to_vec()),
                        ]
                        .into_iter()
                        .flat_map(|x| x.iter())
                        .cloned()
                        .collect::<Vec<_>>(),
                        &[addr_inner.into(), ep_hash.into()],
                    ),
                    comm_comm: None,
                },
            ],
            &INITIAL_PARAMETERS,
        )
        .unwrap();
        let call_inner = ContractCallPrototype {
            address: addr_inner,
            entry_point: b"get"[..].into(),
            op: get_op.clone(),
            guaranteed_public_transcript: transcripts[0].0.clone(),
            fallible_public_transcript: transcripts[0].1.clone(),
            private_transcript_outputs: vec![],
            input: ().into(),
            output: b"malicious".to_vec().into(),
            communication_commitment_rand: cc_rand,
            key_location: KeyLocation(Cow::Borrowed("get")),
        };
        dbg!(&transcripts);
        let call_outer = ContractCallPrototype {
            address: addr_outer,
            entry_point: b"update"[..].into(),
            op: update_op.clone(),
            guaranteed_public_transcript: transcripts[1].1.clone(),
            fallible_public_transcript: transcripts[1].0.clone(),
            private_transcript_outputs: vec![b"malicious".to_vec().into(), cc_rand.into()],
            input: ().into(),
            output: ().into(),
            communication_commitment_rand: rng.r#gen(),
            key_location: KeyLocation(Cow::Borrowed("update")),
        };
        let pre_tx = Transaction::from_intents(
            "local-test",
            test_intents(
                &mut rng,
                vec![call_inner, call_outer],
                Vec::new(),
                Vec::new(),
                Timestamp::from_secs(0),
            ),
        );
        let tx = tx_prove(rng.split(), &pre_tx, &RESOLVER).await.unwrap();

        match tx.well_formed(&ledger_state.ledger, strictness, Timestamp::from_secs(0)) {
            Err(MalformedTransaction::SequencingCheckFailure(
                SequencingCheckError::FallibleInGuaranteedContextViolation { .. },
            )) => (),
            Err(e) => panic!("{e:?}"),
            Ok(_) => panic!("succeeded unexpectedly"),
        };
    }
}

#[tokio::test]
async fn composable_funded() {
    let mut rng = StdRng::seed_from_u64(0x42);
    // Initial states
    let mut state: TestState<InMemoryDB> = TestState::new(&mut rng);

    // Part 1: Deploy burn
    println!(":: Part 1: Deploy burn");
    let burn_op = ContractOperation::new(verifier_key(&RESOLVER, "burn").await);
    let contract = ContractState::new(
        stval!([]),
        HashMap::new().insert(b"burn"[..].into(), burn_op.clone()),
        Default::default(),
    );
    let (tx, addr_burn) = {
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

    // Part 2: Deploy relay
    println!(":: Part 2: Deploy relay");
    let ep_hash = persistent_commit(
        b"burn",
        HashOutput(*b"midnight:entry-point\0\0\0\0\0\0\0\0\0\0\0\0"),
    );
    let send_to_burn_op = ContractOperation::new(verifier_key(&RESOLVER, "send_to_burn").await);
    let contract = ContractState::new(
        stval!([(addr_burn), (ep_hash)]),
        HashMap::new().insert(b"send_to_burn"[..].into(), send_to_burn_op.clone()),
        Default::default(),
    );
    let (tx, addr_relay) = {
        let deploy = ContractDeploy::new(&mut rng, contract.clone());
        let addr = deploy.address();
        let tx = tx_prove(
            rng.split(),
            &Transaction::from_intents(
                "local-test",
                test_intents(
                    &mut rng,
                    Vec::new(),
                    Vec::new(),
                    vec![deploy],
                    Timestamp::from_secs(0),
                ),
            ),
            &RESOLVER,
        )
        .await
        .unwrap();
        (tx, addr)
    };
    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = false;
    tx.well_formed(&state.ledger, strictness, Timestamp::from_secs(0))
        .unwrap();
    state.assert_apply(&tx, strictness);

    let token = Default::default();

    // Part 3: Burn
    println!(":: Part 3: Burn");
    let tx = {
        let coin_in = CoinInfo {
            nonce: rng.r#gen(),
            value: 10_001,
            type_: token,
        };
        let coin_xfer = coin_in.evolve_from(b"midnight:kernel:nonce_evolve", 10_000, token);
        let coin_change = coin_in.evolve_from(b"midnight:kernel:nonce_evolve/2", 1, token);
        let recipient_burn = Recipient::Contract(addr_burn);
        let recipient_relay = Recipient::Contract(addr_relay);
        let sender_relay = SenderEvidence::Contract(addr_relay);
        let cc_rand = rng.r#gen();
        let cc = transient_commit(&ValueReprAlignedValue(coin_xfer.into()), cc_rand);
        let transcripts = partition_transcripts(
            &[
                PreTranscript {
                    context: QueryContext::new(
                        state.ledger.index(addr_burn).unwrap().data,
                        addr_burn,
                    ),
                    program: program_with_results(
                        &[
                            &kernel_self!((), ())[..],
                            &kernel_claim_zswap_coin_receive!(
                                (),
                                (),
                                coin_xfer.commitment(&recipient_burn)
                            ),
                        ]
                        .into_iter()
                        .flatten()
                        .cloned()
                        .collect::<Vec<_>>()[..],
                        &[addr_burn.into()],
                    ),
                    comm_comm: Some(cc),
                },
                PreTranscript {
                    context: QueryContext::new(
                        state.ledger.index(addr_relay).unwrap().data,
                        addr_relay,
                    ),
                    program: program_with_results(
                        &[
                            &kernel_self!((), ())[..],
                            &kernel_claim_zswap_coin_receive!(
                                (),
                                (),
                                coin_in.commitment(&recipient_relay)
                            ),
                            &Cell_read!([key!(0u8)], false, HashOutput),
                            &kernel_self!((), ()),
                            &kernel_claim_zswap_nullifier!(
                                (),
                                (),
                                coin_in.nullifier(&sender_relay)
                            ),
                            &kernel_claim_zswap_coin_spend!(
                                (),
                                (),
                                coin_xfer.commitment(&recipient_burn)
                            ),
                            &kernel_claim_zswap_coin_spend!(
                                (),
                                (),
                                coin_change.commitment(&recipient_relay)
                            ),
                            &kernel_claim_zswap_coin_receive!(
                                (),
                                (),
                                coin_change.commitment(&recipient_relay)
                            ),
                            &Cell_read!([key!(0u8)], false, HashOutput),
                            &Cell_read!([key!(1u8)], false, HashOutput),
                            &kernel_claim_contract_call!(
                                (),
                                (),
                                AlignedValue::from(addr_burn),
                                AlignedValue::from(ep_hash),
                                AlignedValue::from(cc)
                            ),
                        ]
                        .into_iter()
                        .flat_map(|x| x.iter())
                        .cloned()
                        .collect::<Vec<_>>(),
                        &[
                            addr_relay.into(),
                            addr_burn.into(),
                            addr_relay.into(),
                            addr_burn.into(),
                            ep_hash.into(),
                        ],
                    ),
                    comm_comm: None,
                },
            ],
            &INITIAL_PARAMETERS,
        )
        .unwrap();
        let call_burn = ContractCallPrototype {
            address: addr_burn,
            entry_point: b"burn"[..].into(),
            op: burn_op.clone(),
            guaranteed_public_transcript: transcripts[0].0.clone(),
            fallible_public_transcript: transcripts[0].1.clone(),
            private_transcript_outputs: vec![],
            input: coin_xfer.into(),
            output: ().into(),
            communication_commitment_rand: cc_rand,
            key_location: KeyLocation(Cow::Borrowed("burn")),
        };

        let call_relay = ContractCallPrototype {
            address: addr_relay,
            entry_point: b"send_to_burn"[..].into(),
            op: send_to_burn_op.clone(),
            guaranteed_public_transcript: transcripts[1].0.clone(),
            fallible_public_transcript: transcripts[1].1.clone(),
            private_transcript_outputs: vec![().into(), cc_rand.into()],
            input: coin_in.into(),
            output: ().into(),
            communication_commitment_rand: rng.r#gen(),
            key_location: KeyLocation(Cow::Borrowed("send_to_burn")),
        };
        let coin_in_out = Output::new_contract_owned(&mut rng, &coin_in, None, addr_relay).unwrap();
        let mut offer = Offer {
            inputs: Array::new(),
            outputs: vec![
                Output::new_contract_owned(&mut rng, &coin_xfer, None, addr_burn).unwrap(),
                Output::new_contract_owned(&mut rng, &coin_change, None, addr_relay).unwrap(),
            ]
            .into(),
            transient: vec![
                Transient::new_from_contract_owned_output(
                    &mut rng,
                    &coin_in.qualify(0),
                    None,
                    coin_in_out,
                )
                .unwrap(),
            ]
            .into(),
            deltas: vec![Delta {
                token_type: token,
                value: -10_001,
            }]
            .into(),
        };
        offer.normalize();
        let pre_tx: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
            Transaction::new(
                "local-test",
                test_intents(
                    &mut rng,
                    vec![call_burn, call_relay],
                    Vec::new(),
                    Vec::new(),
                    state.time,
                ),
                None,
                [(1, offer)].into_iter().collect(),
            );
        dbg!(&pre_tx);
        tx_prove(rng.split(), &pre_tx, &RESOLVER).await.unwrap()
    };
    state.assert_apply(&tx, strictness);
}
