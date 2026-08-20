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

use base_crypto::rng::SplittableRng;
use base_crypto::time::Timestamp;
use lazy_static::lazy_static;
use midnight_ledger::construct::{ContractCallPrototype, PreTranscript, partition_transcripts};
use midnight_ledger::semantics::TransactionResult;
use midnight_ledger::structure::{ContractDeploy, INITIAL_PARAMETERS, Transaction};
use midnight_ledger::test_utilities::{Resolver, TestState, test_resolver, verifier_key};
use midnight_ledger::test_utilities::{test_intents, tx_prove};
use midnight_ledger::verify::WellFormedStrictness;
use onchain_runtime::context::QueryContext;
use onchain_runtime::ops::{Key, Op, key};
use onchain_runtime::program_fragments::*;
use onchain_runtime::result_mode::{ResultModeGather, ResultModeVerify};
use onchain_runtime::state::{ContractOperation, ContractState, StateValue, stval};
use storage::storage::HashMap;
use transient_crypto::proofs::KeyLocation;
//use onchain_runtime::{key, stval};
use base_crypto::fab::AlignedValue;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::borrow::Cow;
use storage::arena::Sp;
use storage::db::{DB, InMemoryDB};

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
async fn fallible() {
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
    let tx = {
        let call = ContractCallPrototype {
            address: addr,
            entry_point: b"count"[..].into(),
            op: count_op.clone(),
            input: ().into(),
            output: ().into(),
            guaranteed_public_transcript: Some(guaranteed_public_transcript.clone()),
            fallible_public_transcript: Some(fallible_public_transcript.clone()),
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
    //dbg!(&tx);
    state.assert_apply(&tx, strictness);
    assert_eq!(
        state.ledger.index(addr).unwrap().data.get_ref(),
        &stval!([(1u64), (true), (1u64)])
    );

    // Part 3: Duplicate count
    println!(":: Part 3: Duplicate count");
    let tx = {
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

    let res = state.apply(&tx, strictness);
    assert!(matches!(res, Ok(TransactionResult::PartialSuccess(..))));
    assert_eq!(
        state.ledger.index(addr).unwrap().data.get_ref(),
        &stval!([(2u64), (true), (1u64)])
    );
}
