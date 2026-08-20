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
use base_crypto::rng::SplittableRng;
use base_crypto::time::Timestamp;
use lazy_static::lazy_static;
use midnight_ledger::construct::{ContractCallPrototype, PreTranscript, partition_transcripts};
use midnight_ledger::structure::{ContractDeploy, INITIAL_PARAMETERS, Transaction};
use midnight_ledger::test_utilities::{Resolver, TestState, test_resolver, verifier_key};
use midnight_ledger::test_utilities::{test_intents, tx_prove};
use midnight_ledger::verify::WellFormedStrictness;
use onchain_runtime::context::QueryContext;
use onchain_runtime::ops::{Key, Op, key};
use onchain_runtime::program_fragments::{
    HistoricMerkleTree_check_root, HistoricMerkleTree_insert,
};
use onchain_runtime::result_mode::{ResultModeGather, ResultModeVerify};
use onchain_runtime::state::{ContractOperation, ContractState, StateValue, stval};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::borrow::Cow;
use storage::arena::Sp;
use storage::db::{DB, InMemoryDB};
use storage::storage::HashMap;
use transient_crypto::fab::ValueReprAlignedValue;
use transient_crypto::merkle_tree::{MerkleTree, leaf_hash};
use transient_crypto::proofs::KeyLocation;

lazy_static! {
    static ref RESOLVER: Resolver = test_resolver("simple-merkle-tree");
}

fn program_with_results<D: DB>(
    prog: &[Op<ResultModeGather, D>],
    results: &[AlignedValue],
) -> Vec<Op<ResultModeVerify, D>> {
    let mut res_iter = results.iter();
    prog.iter()
        .map(|op| op.clone().translate(|()| res_iter.next().unwrap().clone()))
        .collect()
}

#[tokio::test]
#[allow(unused_assignments, clippy::redundant_clone)]
async fn simple_merkle_tree() {
    let mut rng = StdRng::seed_from_u64(0x42);
    // Initial states
    let mut state: TestState<InMemoryDB> = TestState::new(&mut rng);

    // Part 1: Deploy
    let root = MerkleTree::<()>::blank(10).root();
    let store_op = ContractOperation::new(verifier_key(&RESOLVER, "store").await);
    let check_op = ContractOperation::new(verifier_key(&RESOLVER, "check").await);
    let contract = ContractState::new(
        stval!([[{MT(10) {}}, (0u64), {root => null}]]),
        HashMap::new()
            .insert(b"store"[..].into(), store_op.clone())
            .insert(b"check"[..].into(), check_op.clone()),
        Default::default(),
    );
    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = false;
    let (tx, addr) = {
        // Create partial deploy tx
        let deploy = ContractDeploy::new(&mut rng, contract);
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
    state.assert_apply(&tx, strictness);
    assert!(state.ledger.index(addr).is_some());

    // Part 2: Store 1
    let entry1 = 12u32;
    let tx = {
        let transcripts = partition_transcripts(
            &[PreTranscript {
                context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
                program: HistoricMerkleTree_insert!([key!(0u8)], false, 10, u32, entry1.clone())
                    .into(),
                comm_comm: None,
            }],
            &INITIAL_PARAMETERS,
        )
        .unwrap();
        let call = ContractCallPrototype {
            address: addr,
            entry_point: b"store"[..].into(),
            op: store_op.clone(),
            guaranteed_public_transcript: transcripts[0].0.clone(),
            fallible_public_transcript: transcripts[0].1.clone(),
            private_transcript_outputs: vec![],
            input: entry1.into(),
            output: ().into(),
            communication_commitment_rand: rng.r#gen(),
            key_location: KeyLocation(Cow::Borrowed("store")),
        };
        let pre_tx = Transaction::from_intents(
            "local-test",
            test_intents(
                &mut rng,
                vec![call],
                Vec::new(),
                Vec::new(),
                Timestamp::from_secs(0),
            ),
        );
        let tx = tx_prove(rng.split(), &pre_tx, &RESOLVER).await.unwrap();
        tx.well_formed(&state.ledger, strictness, Timestamp::from_secs(0))
            .unwrap();
        tx
    };
    // dbg!(&tx);
    state.assert_apply(&tx, strictness);

    // Part 2: Check the path.
    let contract_state = state.ledger.index(addr).unwrap();
    let composite_tree_var = if let StateValue::Array(arr) = contract_state.data.get_ref() {
        arr.get(0).unwrap()
    } else {
        unreachable!()
    };
    let real_tree_var = if let StateValue::Array(arr) = composite_tree_var {
        arr.get(0).unwrap()
    } else {
        unreachable!()
    };
    let path = if let StateValue::BoundedMerkleTree(tree) = real_tree_var {
        tree.find_path_for_leaf(entry1).unwrap()
    } else {
        unreachable!()
    };
    let tx_check = {
        let mut transcripts = partition_transcripts(
            &[PreTranscript {
                context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
                program: program_with_results(
                    &HistoricMerkleTree_check_root!([key!(0u8)], false, 10, u32, path.root()),
                    &[true.into()],
                ),
                comm_comm: None,
            }],
            &INITIAL_PARAMETERS,
        )
        .unwrap();
        if let Some(ref mut transcript) = transcripts[0].0 {
            transcript.gas = transcript.gas * 1.2;
        }
        let call = ContractCallPrototype {
            address: addr,
            entry_point: b"check"[..].into(),
            op: check_op.clone(),
            guaranteed_public_transcript: transcripts[0].0.clone(),
            fallible_public_transcript: transcripts[0].1.clone(),
            private_transcript_outputs: vec![path.into()],
            input: entry1.into(),
            output: ().into(),
            communication_commitment_rand: rng.r#gen(),
            key_location: KeyLocation(Cow::Borrowed("check")),
        };
        let pre_tx = Transaction::from_intents(
            "local-test",
            test_intents(
                &mut rng,
                vec![call],
                Vec::new(),
                Vec::new(),
                Timestamp::from_secs(0),
            ),
        );
        let tx = tx_prove(rng.split(), &pre_tx, &RESOLVER).await.unwrap();
        tx.well_formed(&state.ledger, strictness, Timestamp::from_secs(0))
            .unwrap();
        tx
    };
    // dbg!(&tx_check);
    let mut pre_tx_check = state.clone();
    state.assert_apply(&tx_check, strictness);

    // Part 3: Another insert
    let entry2 = 42u32;
    let transcripts = partition_transcripts(
        &[PreTranscript {
            context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
            program: HistoricMerkleTree_insert!([key!(0u8)], false, 10, u32, entry2.clone()).into(),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap();
    let tx = {
        let call = ContractCallPrototype {
            address: addr,
            entry_point: b"store"[..].into(),
            op: store_op.clone(),
            guaranteed_public_transcript: transcripts[0].0.clone(),
            fallible_public_transcript: transcripts[0].1.clone(),
            private_transcript_outputs: vec![],
            input: entry2.into(),
            output: ().into(),
            communication_commitment_rand: rng.r#gen(),
            key_location: KeyLocation(Cow::Borrowed("store")),
        };
        let pre_tx = Transaction::from_intents(
            "local-test",
            test_intents(
                &mut rng,
                vec![call],
                Vec::new(),
                Vec::new(),
                Timestamp::from_secs(0),
            ),
        );
        let tx = tx_prove(rng.split(), &pre_tx, &RESOLVER).await.unwrap();
        tx.well_formed(&state.ledger, strictness, Timestamp::from_secs(0))
            .unwrap();
        tx
    };
    // dbg!(&tx);
    state.assert_apply(&tx, strictness);

    // Part 4: Old check (on the old ledger state) should work.
    pre_tx_check.assert_apply(&tx_check, strictness);
}
