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
use coin_structure::coin::{SecretKey, TokenType, UserAddress};
use lazy_static::lazy_static;
use midnight_ledger::structure::{
    ClaimKind, ClaimRewardsTransaction, LedgerState, MAX_SUPPLY, OutputInstructionShielded,
    OutputInstructionUnshielded, SystemTransaction, Transaction,
};
use midnight_ledger::test_utilities::tx_prove;
use midnight_ledger::test_utilities::{Resolver, TestState, test_resolver};
use midnight_ledger::verify::WellFormedStrictness;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use storage::db::InMemoryDB;
use storage::storage::Map;

lazy_static! {
    static ref RESOLVER: Resolver = test_resolver("");
}

#[tokio::test]
async fn system_tx_pay_from_unshielded() {
    let mut rng = StdRng::seed_from_u64(0x42);
    // Initial states
    let mut state: TestState<InMemoryDB> = TestState::new(&mut rng);
    let verifying_key = state.night_key.verifying_key();
    let address = UserAddress::from(verifying_key.clone());

    // Distribute reserve
    let sys_tx_distribute = SystemTransaction::DistributeReserve(500_000);

    let (ledger, _) = state
        .ledger
        .apply_system_tx(&sys_tx_distribute, state.time)
        .unwrap();

    state.ledger = ledger;

    assert_eq!(state.ledger.reserve_pool, MAX_SUPPLY - 500_000);

    // Rewards
    let sys_tx_rewards = SystemTransaction::DistributeNight(
        ClaimKind::Reward,
        vec![OutputInstructionUnshielded {
            amount: 500_000,
            target_address: address,
            nonce: rng.r#gen(),
        }],
    );
    let res = state
        .ledger
        .apply_system_tx(&sys_tx_rewards, state.time)
        .unwrap();
    state.ledger = res.0;
    let nonce = rng.r#gen();
    let tx = tx_prove(
        rng.split(),
        &Transaction::<(), _, _, _>::ClaimRewards(ClaimRewardsTransaction {
            network_id: "local-test".into(),
            value: 500_000,
            owner: verifying_key,
            nonce,
            signature: (),
            kind: ClaimKind::Reward,
        }),
        &RESOLVER,
    )
    .await
    .unwrap();
    state.assert_apply(&tx, WellFormedStrictness::default());

    assert_eq!(state.ledger.block_reward_pool, 0);

    assert_eq!(
        state
            .ledger
            .utxo
            .utxos
            .iter()
            .map(|a| a.0.value)
            .sum::<u128>(),
        500_000
    );
}

#[tokio::test]
#[ignore = "disabled due to treasury payments being disabled"]
async fn system_tx_pay_from_shielded() {
    let mut rng = StdRng::seed_from_u64(0x42);
    // Initial states
    let mut ledger_state: LedgerState<InMemoryDB> = LedgerState::new("local-test");
    let mut treasury: Map<TokenType, u128, InMemoryDB> = Map::new();
    let amount = 500_000;
    let token = Default::default();
    treasury = treasury.insert(TokenType::Shielded(token), amount);
    ledger_state.treasury = treasury;
    let secret_key: SecretKey = SecretKey(rng.r#gen());
    let target_key = secret_key.public_key();

    // Treasury transfer
    let sys_tx = SystemTransaction::PayFromTreasuryShielded {
        nonce: rng.r#gen(),
        outputs: vec![OutputInstructionShielded { target_key, amount }],
        token_type: token,
    };
    assert_eq!(
        ledger_state
            .treasury
            .get(&coin_structure::coin::TokenType::Shielded(token))
            .copied()
            .unwrap_or(0),
        500_000
    );
    let res = ledger_state
        .apply_system_tx(&sys_tx, Timestamp::from_secs(0))
        .unwrap();
    let ledger_state = res.0;
    assert_eq!(
        ledger_state
            .treasury
            .get(&coin_structure::coin::TokenType::Shielded(token))
            .copied()
            .unwrap_or(0),
        0
    );
    // Would be nice to add an assert to confirm the tokens went to the right place
}
