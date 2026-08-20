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

use std::ops::Deref;

use coin_structure::coin::{Info as CoinInfo, ShieldedTokenType};
use coin_structure::contract::ContractAddress;
use midnight_zswap::keys::SecretKeys;
use midnight_zswap::ledger::State;
use midnight_zswap::local;
use midnight_zswap::{Offer, Output as ZswapOutput};
use rand::rngs::{OsRng, StdRng};
use rand::{Rng, SeedableRng};
use storage::db::{DB, InMemoryDB};
use transient_crypto::proofs::ProofPreimage;

#[test]
fn coin_receiving() {
    let mut rng = StdRng::seed_from_u64(0x42);
    let mut state = local::State::<InMemoryDB>::new();
    let keys = SecretKeys::from_rng_seed(&mut rng);
    let coin = CoinInfo {
        nonce: OsRng.r#gen(),
        type_: ShieldedTokenType(OsRng.r#gen()),
        value: OsRng.r#gen(),
    };
    let output = ZswapOutput::new(
        &mut rng,
        &coin,
        None,
        &keys.coin_public_key(),
        Some(keys.enc_public_key()),
    )
    .unwrap();
    let offer = Offer {
        inputs: vec![].into(),
        outputs: vec![output].into(),
        transient: vec![].into(),
        deltas: vec![].into(),
    };
    state = state.apply(&keys, &offer).unwrap();
    assert_eq!(
        state
            .coins
            .iter()
            .map(|(_, c)| CoinInfo::from(&*c))
            .collect::<std::vec::Vec<_>>(),
        vec![coin]
    );
}

#[test]
fn state_filtering() {
    let mut rng = StdRng::seed_from_u64(0x42);
    let mut state: State<InMemoryDB> = State::new();

    fn apply_random_offer<D: DB>(
        state: &mut State<D>,
        rng: &mut StdRng,
    ) -> ZswapOutput<ProofPreimage, D> {
        let coin = CoinInfo {
            nonce: OsRng.r#gen(),
            type_: ShieldedTokenType(OsRng.r#gen()),
            value: OsRng.r#gen(),
        };
        let address = ContractAddress(OsRng.r#gen());
        let output = ZswapOutput::new_contract_owned(rng, &coin, None, address).unwrap();
        let offer = Offer {
            inputs: vec![].into(),
            outputs: vec![output.clone()].into(),
            transient: vec![].into(),
            deltas: vec![].into(),
        };

        *state = state.try_apply(&offer, None).unwrap().0;
        output
    }

    for _ in 0..2 {
        apply_random_offer(&mut state, &mut rng);
    }
    let output = apply_random_offer(&mut state, &mut rng);

    let reference_tree = state.coin_coms.collapse(0, 1);

    assert_eq!(
        state.filter(&[*(output.contract_address.clone()).unwrap().deref()]),
        reference_tree
    );

    for _ in 0..2 {
        apply_random_offer(&mut state, &mut rng);
    }

    let mut reference_tree = state.coin_coms.collapse(0, 1);
    reference_tree = reference_tree.collapse(3, 4);

    assert_eq!(
        state.filter(&[*(output.contract_address.clone()).unwrap().deref()]),
        reference_tree
    );
}
