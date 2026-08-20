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

// As this test relies on a ZK check of the segment ID.
#![cfg(feature = "proving")]

use base_crypto::rng::SplittableRng;
use base_crypto::signatures::Signature;
use coin_structure::coin::Info as CoinInfo;
use lazy_static::lazy_static;
use midnight_ledger::structure::{ContractDeploy, ProofPreimageMarker, Transaction};
use midnight_ledger::test_utilities::{Resolver, TestState, test_intents, test_resolver, tx_prove};
use midnight_ledger::verify::WellFormedStrictness;
use onchain_runtime::state::ContractState;
use rand::{Rng, SeedableRng, rngs::StdRng};
use storage::db::InMemoryDB;
use transient_crypto::commitment::PedersenRandomness;
use zswap::Delta;
use zswap::{Offer, Output};

lazy_static! {
    static ref RESOLVER: Resolver = test_resolver("");
}

#[tokio::test]
async fn malicious_imbalance() {
    let mut rng = StdRng::seed_from_u64(0x42);
    // Initial states
    let mut state: TestState<InMemoryDB> = TestState::new(&mut rng);

    // Lets start by giving ourself 100 million tokens. We just need something to start.
    let token = Default::default();
    state.give_fee_token(&mut rng, 10).await;
    state.rewards_shielded(&mut rng, token, 100_000_000);
    let deploy = ContractDeploy::new(&mut rng, ContractState::default());
    // Bypass some machinery to directly deploy an existing contract.
    state.ledger.contract = state
        .ledger
        .contract
        .insert(deploy.address(), deploy.initial_state.clone());
    let coin = *state.zswap.coins.iter().next().unwrap().1;

    // We are now at our genesis state. The goal of the attack is to create 95 million tokens from
    // thin air. The 5 million difference is just a generous buffer for transaction fees.
    // (It turns out even attackers pay taxes)

    // Let's create the coin that we want to create maliciously.
    let bad_coin = CoinInfo {
        nonce: rng.r#gen(),
        value: 95_000_000,
        type_: token,
    };
    // And a corresponding Zswap output
    let bad_output = Output::new(
        &mut rng,
        &bad_coin,
        Some(0),
        &state.zswap_keys.coin_public_key(),
        Some(state.zswap_keys.enc_public_key()),
    )
    .unwrap();
    // We create an offer that gives it to ourselves -- but we claim the offer has a net *input* of
    // 5 million (to cover the fees)
    let bad_offer_guaranteed = Offer {
        inputs: vec![].into(),
        outputs: vec![bad_output].into(),
        transient: vec![].into(),
        deltas: vec![Delta {
            token_type: token,
            value: 5_000_000,
        }]
        .into(),
    };
    // In order to "balance" the transaction, we create a fallible offer that spends our 100m
    // tokens. But we declare that it has *no* net input.
    let bad_offer_fallible_1 = Offer {
        inputs: vec![
            state
                .zswap
                .spend(&mut rng, &state.zswap_keys, &coin, Some(1))
                .unwrap()
                .1,
        ]
        .into(),
        outputs: vec![].into(),
        transient: vec![].into(),
        deltas: vec![].into(),
    };
    // We also test with (incorrectly) using a guaranteed offer in a fallible context
    let bad_offer_fallible_2 = Offer {
        inputs: vec![
            state
                .zswap
                .spend(&mut rng, &state.zswap_keys, &coin, Some(0))
                .unwrap()
                .1,
        ]
        .into(),
        outputs: vec![].into(),
        transient: vec![].into(),
        deltas: vec![].into(),
    };
    // We create a transaction with these two offers, and attempting to deploy the already deployed
    // contract.
    // let mut hm = std::collections::HashMap::new();
    // hm.insert(1, bad_offer_fallible_1.clone());
    // hm.insert(2, bad_offer_fallible_2.clone());

    for bad_offer_fallible in [bad_offer_fallible_1, bad_offer_fallible_2] {
        let hm2 = storage::storage::HashMap::new().insert(1, bad_offer_fallible);

        let mut bad_tx: Transaction<
            Signature,
            ProofPreimageMarker,
            PedersenRandomness,
            InMemoryDB,
        > = Transaction::new(
            "local-test",
            test_intents(
                &mut rng,
                Vec::new(),
                Vec::new(),
                vec![deploy.clone()],
                state.time,
            ),
            Some(bad_offer_guaranteed.clone()),
            Default::default(),
        );
        if let Transaction::Standard(stx) = &mut bad_tx {
            // Insert fallible coins manually to ensure the incorrect segment declaration survives
            stx.fallible_coins = hm2;
            stx.recompute_binding_randomness();
        }
        let bad_tx = tx_prove(rng.split(), &bad_tx, &RESOLVER).await.unwrap();
        // This transaction should partially succeed application, meaning that that 95m output is
        // applied, but the 100m input is not spent, leaving us with 195m left over... from 100m start.
        // Because of this, this transaction had damn well better not be considered a well-formed one!
        if state
            .clone()
            .apply(&bad_tx, WellFormedStrictness::default())
            .is_ok()
        {
            panic!("unexpected success")
        }
    }
}

#[tokio::test]
// This is a bad name and it probably shouldn't even be in this file
async fn malicious_imbalance_duplicate_nullifier() {
    let mut rng = StdRng::seed_from_u64(0x42);
    // Initial states
    let mut state: TestState<InMemoryDB> = TestState::new(&mut rng);

    // Lets start by giving ourself 100 million tokens. We just need something to start.
    let token = Default::default();
    state.give_fee_token(&mut rng, 10).await;
    state.rewards_shielded(&mut rng, token, 100_000_000);
    // Bypass some machinery to directly deploy an existing contract.
    let deploy = ContractDeploy::new(&mut rng, Default::default());
    state.ledger.contract = state
        .ledger
        .contract
        .insert(deploy.address(), deploy.initial_state.clone());
    let coin = *state.zswap.coins.iter().next().unwrap().1;

    // We are now at our genesis state. The goal of the attack is to create 95 million tokens from
    // thin air. The 5 million difference is just a generous buffer for transaction fees.
    // (It turns out even attackers pay taxes)

    // Let's create the coin that we want to create maliciously.
    let bad_coin = CoinInfo {
        nonce: rng.r#gen(),
        value: 95_000_000,
        type_: token,
    };
    // And a corresponding Zswap output
    let bad_output = Output::new(
        &mut rng,
        &bad_coin,
        Some(0),
        &state.zswap_keys.coin_public_key(),
        Some(state.zswap_keys.enc_public_key()),
    )
    .unwrap();
    // We create an offer that gives it to ourselves -- but we claim the offer has a net *input* of
    // 5 million (to cover the fees)
    let bad_offer_guaranteed = Offer {
        inputs: vec![].into(),
        outputs: vec![bad_output].into(),
        transient: vec![].into(),
        deltas: vec![Delta {
            token_type: token,
            value: 5_000_000,
        }]
        .into(),
    };
    // In order to "balance" the transaction, we create a fallible offer that spends our 100m
    // tokens. But we declare that it has *no* net input.
    let bad_offer_fallible_1 = Offer {
        inputs: vec![
            state
                .zswap
                .spend(&mut rng, &state.zswap_keys, &coin, Some(1))
                .unwrap()
                .1,
        ]
        .into(),
        outputs: vec![].into(),
        transient: vec![].into(),
        deltas: vec![].into(),
    };
    // We also test with (incorrectly) using a guaranteed offer in a fallible context
    let bad_offer_fallible_2 = Offer {
        inputs: vec![
            state
                .zswap
                .spend(&mut rng, &state.zswap_keys, &coin, Some(0))
                .unwrap()
                .1,
        ]
        .into(),
        outputs: vec![].into(),
        transient: vec![].into(),
        deltas: vec![].into(),
    };
    // We create a transaction with these two offers, and attempting to deploy the already deployed
    // contract.
    let hm = storage::storage::HashMap::new()
        .insert(1, bad_offer_fallible_1.clone())
        .insert(2, bad_offer_fallible_2.clone());

    let mut bad_tx: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB> =
        Transaction::new(
            "local-test",
            test_intents(&mut rng, Vec::new(), Vec::new(), vec![deploy], state.time),
            Some(bad_offer_guaranteed),
            Default::default(),
        );
    if let Transaction::Standard(stx) = &mut bad_tx {
        // Insert fallible coins manually to ensure the incorrect segment declaration survives
        stx.fallible_coins = hm;
        stx.recompute_binding_randomness();
    }
    let bad_tx = tx_prove(rng.split(), &bad_tx, &RESOLVER).await.unwrap();
    // This transaction should partially succeed application, meaning that that 95m output is
    // applied, but the 100m input is not spent, leaving us with 195m left over... from 100m start.
    // Because of this, this transaction had damn well better not be considered a well-formed one!
    if state
        .clone()
        .apply(&bad_tx, WellFormedStrictness::default())
        .is_ok()
    {
        panic!("unexpected success")
    }
}
