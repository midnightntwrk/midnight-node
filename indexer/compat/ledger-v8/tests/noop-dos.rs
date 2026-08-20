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
use base_crypto::rng::SplittableRng;
use base_crypto::time::Timestamp;
use lazy_static::lazy_static;
use midnight_ledger::construct::{ContractCallPrototype, PreTranscript, partition_transcripts};
use midnight_ledger::structure::{ContractAction, ContractDeploy, INITIAL_PARAMETERS, Transaction};
use midnight_ledger::test_utilities::{
    Resolver, TestState, test_intents, test_resolver, tx_prove, verifier_key,
};
use midnight_ledger::verify::WellFormedStrictness;
use onchain_runtime::context::QueryContext;
use onchain_runtime::ops::{Key, Op, key};
use onchain_runtime::program_fragments::*;
use onchain_runtime::result_mode::{ResultModeGather, ResultModeVerify};
use onchain_runtime::state::{ContractOperation, ContractState, StateValue, stval};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serialize::Serializable;
use std::borrow::Cow;
use std::time::{Duration, Instant};
use storage::arena::Sp;
use storage::db::{DB, InMemoryDB};
use storage::storage::HashMap;
use transient_crypto::proofs::KeyLocation;

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

#[tokio::test]
async fn noop_dos() {
    let mut rng = StdRng::seed_from_u64(0x42);
    // Initial states
    let mut state: TestState<InMemoryDB> = TestState::new(&mut rng);

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

    // Part 2: First application
    println!(":: Part 2: First count");
    let guaranteed_public_transcript = partition_transcripts(
        &[PreTranscript {
            context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
            program: program_with_results::<InMemoryDB>(
                &Counter_increment!([key!(0u8)], false, 1u64),
                &[],
            ),
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
    let mut tx = {
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
                    vec![call],
                    Vec::new(),
                    Vec::new(),
                    Timestamp::from_secs(0),
                ),
            ),
            &RESOLVER,
        )
        .await
        .unwrap()
    };
    let mut noop_dos: Vec<Op<ResultModeVerify, InMemoryDB>> = Vec::new();
    for _ in 0..1_000 {
        noop_dos.push(Op::Noop { n: 0x1ffff });
        noop_dos.push(Op::Dup { n: 1 });
        noop_dos.push(Op::Pop);
    }
    match &mut tx {
        Transaction::Standard(stx) => {
            let mut call_action = (*stx
                .intents
                .clone()
                .into_iter()
                .collect::<std::collections::HashMap<_, _>>()
                .iter()
                .flat_map(|i| i.1.actions.iter_deref().cloned())
                .find_map(|action| {
                    if let ContractAction::Call(call) = action {
                        Some(call)
                    } else {
                        None
                    }
                })
                .unwrap())
            .clone();

            call_action.fallible_transcript = Some(Sp::new(
                partition_transcripts(
                    &[PreTranscript {
                        context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
                        program: noop_dos,
                        comm_comm: None,
                    }],
                    &INITIAL_PARAMETERS,
                )
                .unwrap()[0]
                    .1
                    .clone()
                    .unwrap(),
            ));
            dbg!(call_action);
        }
        _ => unreachable!(),
    }
    dbg!(Serializable::serialized_size(&tx));
    let t0 = Instant::now();
    dbg!(tx.well_formed(&state.ledger, strictness, state.time)).ok();
    let t1 = Instant::now();
    assert!(t1 - t0 < Duration::from_millis(100));
}
