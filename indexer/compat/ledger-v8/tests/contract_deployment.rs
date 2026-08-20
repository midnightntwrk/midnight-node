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

use base_crypto::hash::{HashOutput, PERSISTENT_HASH_BYTES};
use base_crypto::rng::SplittableRng;
use base_crypto::time::Timestamp;
use coin_structure::coin::{ShieldedTokenType, TokenType};
use lazy_static::lazy_static;
use midnight_ledger::error::MalformedContractDeploy;
use midnight_ledger::structure::{ContractDeploy, LedgerState, Transaction};
use midnight_ledger::test_utilities::{
    Resolver, test_intents, test_resolver, tx_prove, verifier_key,
};
use midnight_ledger::verify::WellFormedStrictness;
use onchain_runtime::state::{ContractOperation, ContractState, StateValue, stval};
use rand::SeedableRng;
use rand::rngs::StdRng;
use storage::arena::Sp;
use storage::db::InMemoryDB;
use storage::storage::HashMap;

lazy_static! {
    static ref RESOLVER: Resolver = test_resolver("fallible");
}

#[tokio::test]
async fn zero_contract_deploy_balance() {
    let mut rng = StdRng::seed_from_u64(0x42);
    // Initial states
    let ledger_state: LedgerState<InMemoryDB> = LedgerState::new("local-test");

    // Part 1: Deploy
    println!(":: Part 1: Deploy");
    let count_op = ContractOperation::new(verifier_key(&RESOLVER, "count").await);
    let mut contract = ContractState::new(
        stval!([(0u64), (false), (0u64)]),
        HashMap::new().insert(b"count"[..].into(), count_op.clone()),
        Default::default(),
    );
    contract.balance = HashMap::new().insert(
        TokenType::Shielded(ShieldedTokenType(HashOutput([0; PERSISTENT_HASH_BYTES]))),
        0,
    );

    let deploy = ContractDeploy::new(&mut rng, contract.clone());
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

    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = false;
    tx.well_formed(&ledger_state, strictness, Timestamp::from_secs(0))
        .unwrap();
}

#[tokio::test]
async fn non_zero_contract_deploy_balance() {
    let mut rng = StdRng::seed_from_u64(0x42);
    // Initial states
    let ledger_state: LedgerState<InMemoryDB> = LedgerState::new("local-test");

    // Part 1: Deploy
    println!(":: Part 1: Deploy");
    let count_op = ContractOperation::new(verifier_key(&RESOLVER, "count").await);
    let expected_balance = HashMap::new()
        .insert(
            TokenType::Shielded(ShieldedTokenType(HashOutput([0; PERSISTENT_HASH_BYTES]))),
            0,
        )
        .insert(
            TokenType::Shielded(ShieldedTokenType(HashOutput([1; PERSISTENT_HASH_BYTES]))),
            10,
        );
    let mut contract = ContractState::new(
        stval!([(0u64), (false), (0u64)]),
        HashMap::new().insert(b"count"[..].into(), count_op.clone()),
        Default::default(),
    );
    contract.balance = expected_balance.clone();

    let deploy = ContractDeploy::new(&mut rng, contract.clone());
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

    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = false;
    let res: Result<_, midnight_ledger::error::MalformedTransaction<InMemoryDB>> =
        tx.well_formed(&ledger_state, strictness, Timestamp::from_secs(0));

    match res {
        Err(midnight_ledger::error::MalformedTransaction::MalformedContractDeploy(
            MalformedContractDeploy::NonZeroBalance(actual_balance),
        )) => assert_eq!(expected_balance, actual_balance.into_iter().collect()),
        Err(e) => panic!("Unexpected error: {:?}", e),
        _ => panic!(
            "Succeeded unexpectedly, balance was: {:?}",
            expected_balance
        ),
    }
}
