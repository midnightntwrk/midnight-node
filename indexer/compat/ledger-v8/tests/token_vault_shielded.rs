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

//! Token Vault Shielded Token Tests
//!
//! Integration tests for shielded token operations in the token-vault contract.
//! Shielded tokens use ZSwap for privacy-preserving transfers with commitments,
//! nullifiers, and zero-knowledge proofs.
//!
//! ## Shielded Token Flow
//!
//! ### Deposit (User → Contract):
//! 1. User creates ZswapOutput with contract ownership
//! 2. Contract claims coin via `kernel.claimZswapCoinReceive()`
//! 3. Contract stores coin info in shieldedVault
//!
//! ### Merge (Contract Coin + New Deposit):
//! 1. Contract nullifies existing vault coin
//! 2. Contract receives new coin via transient
//! 3. Contract creates merged coin with combined value
//! 4. Contract stores merged coin in shieldedVault
//!
//! ### Withdrawal (Contract → User):
//! 1. Contract nullifies vault coin
//! 2. Contract creates user output (withdrawn amount)
//! 3. Contract creates change output (remaining in vault)
//!
//! ## Important Implementation Notes
//!
//! - **Transcript matching**: Ops must match circuit execution order exactly
//! - **Double reads**: If circuit reads state twice, transcript needs two Cell_read ops
//! - **Nonce evolution**: Multiple coins from one input need different domain separators:
//!   - Primary coin: `midnight:kernel:nonce_evolve`
//!   - Change coin: `midnight:kernel:nonce_evolve/2`
//! - **Results**: Only Cell_read and kernel_self produce results; kernel_claim_* ops don't
//! - **Private witnesses**: localSecretKey() gets secret key, ownPublicKey() gets public key

#![deny(warnings)]
#![allow(unused_imports)]
#![allow(unused_variables)]

mod token_vault_common;

use token_vault_common::*;

// ============================================================================
// Main Shielded Integration Test
// ============================================================================

/// Full lifecycle test: deploy, deposit, merge, withdraw.
///
/// Parts:
/// 1. Deploy contract
/// 2. First deposit (empty vault)
/// 3. Merge deposit (combine with existing)
/// 4. Partial withdrawal (split into user + change coins)
#[tokio::test]
async fn test_shielded_full_lifecycle() {
    //midnight_ledger::init_logger(midnight_ledger::LogLevel::Trace);
    let mut rng = StdRng::seed_from_u64(0x42);

    // Initialize crypto parameters
    init_crypto();

    // Generate owner keys
    let owner_sk: HashOutput = rng.r#gen();
    let owner_pk = derive_public_key(owner_sk);

    // Load contract operations
    let deposit_shielded_op =
        ContractOperation::new(verifier_key(&RESOLVER, "depositShielded").await);
    let withdraw_shielded_op =
        ContractOperation::new(verifier_key(&RESOLVER, "withdrawShielded").await);
    let deposit_unshielded_op =
        ContractOperation::new(verifier_key(&RESOLVER, "depositUnshielded").await);
    let withdraw_unshielded_op =
        ContractOperation::new(verifier_key(&RESOLVER, "withdrawUnshielded").await);
    let get_shielded_balance_op =
        ContractOperation::new(verifier_key(&RESOLVER, "getShieldedBalance").await);
    let get_unshielded_balance_op =
        ContractOperation::new(verifier_key(&RESOLVER, "getUnshieldedBalance").await);

    println!(":: Shielded Token Vault Test Suite");
    println!("   Owner PK: {:?}", hex::encode(&owner_pk.0[..8]));

    // Initial test state with shielded rewards
    let mut state: TestState<InMemoryDB> = TestState::new(&mut rng);
    const REWARDS_AMOUNT: u128 = 10_000_000_000;
    let token = Default::default();
    state.rewards_shielded(&mut rng, token, REWARDS_AMOUNT);
    state.give_fee_token(&mut rng, 100).await;

    let mut unbalanced_strictness = WellFormedStrictness::default();
    unbalanced_strictness.enforce_balancing = false;
    let balanced_strictness = WellFormedStrictness::default();

    // ========================================================================
    // Part 1: Deploy Contract
    // ========================================================================
    println!("\n:: Part 1: Deploy Token Vault Contract");

    let contract: ContractState<InMemoryDB> = ContractState::new(
        stval!([
            (QualifiedCoinInfo::default()), // 0: shieldedVault
            (false),                        // 1: hasShieldedTokens
            (owner_pk),                     // 2: owner
            {},                             // 3: authorized (empty set)
            (0u64),                         // 4: totalShieldedDeposits
            (0u64),                         // 5: totalShieldedWithdrawals
            (0u64),                         // 6: totalUnshieldedDeposits
            (0u64)                          // 7: totalUnshieldedWithdrawals
        ]),
        HashMap::new()
            .insert(b"depositShielded"[..].into(), deposit_shielded_op.clone())
            .insert(b"withdrawShielded"[..].into(), withdraw_shielded_op.clone())
            .insert(
                b"depositUnshielded"[..].into(),
                deposit_unshielded_op.clone(),
            )
            .insert(
                b"withdrawUnshielded"[..].into(),
                withdraw_unshielded_op.clone(),
            )
            .insert(
                b"getShieldedBalance"[..].into(),
                get_shielded_balance_op.clone(),
            )
            .insert(
                b"getUnshieldedBalance"[..].into(),
                get_unshielded_balance_op.clone(),
            ),
        Default::default(),
    );

    let deploy = ContractDeploy::new(&mut rng, contract.clone());
    let addr = deploy.address();
    println!("   Contract address: {:?}", addr);

    let tx = Transaction::from_intents(
        "local-test",
        test_intents(&mut rng, Vec::new(), Vec::new(), vec![deploy], state.time),
    );
    tx.well_formed(&state.ledger, unbalanced_strictness, state.time)
        .unwrap();

    let tx = tx_prove_bind(rng.split(), &tx, &RESOLVER).await.unwrap();
    let balanced = state.balance_tx(rng.split(), tx, &RESOLVER).await.unwrap();
    state.assert_apply(&balanced, balanced_strictness);

    println!("   Contract deployed successfully");

    // ========================================================================
    // Part 2: First Shielded Deposit (empty vault)
    // ========================================================================
    println!("\n:: Part 2: First Shielded Deposit");
    const FIRST_DEPOSIT: u128 = 1_000_000;

    let coin = CoinInfo::new(&mut rng, FIRST_DEPOSIT, token);
    let out = ZswapOutput::new_contract_owned(&mut rng, &coin, None, addr).unwrap();
    let coin_com = coin.commitment(&Recipient::Contract(addr));

    // Transcript must match circuit execution order exactly
    let public_transcript: Vec<Op<ResultModeGather, InMemoryDB>> = [
        &kernel_self!((), ())[..],
        &kernel_claim_zswap_coin_receive!((), (), coin_com),
        &Cell_read!([key!(STATE_IDX_HAS_SHIELDED_TOKENS)], false, bool)[..], // Read hasShieldedTokens
        &kernel_self!((), ())[..],
        &Cell_write_coin!(
            [key!(STATE_IDX_SHIELDED_VAULT)],
            true,
            QualifiedCoinInfo,
            coin.clone(),
            Recipient::Contract(addr)
        )[..],
        &Cell_write!([key!(STATE_IDX_HAS_SHIELDED_TOKENS)], true, bool, true)[..],
        &Counter_increment!([key!(STATE_IDX_TOTAL_SHIELDED_DEPOSITS)], false, 1u64)[..],
    ]
    .into_iter()
    .flat_map(|x| x.iter())
    .cloned()
    .collect();

    // Only reads and kernel_self produce results
    let public_transcript_results: Vec<AlignedValue> = vec![
        addr.into(),  // First kernel_self returns contract address
        false.into(), // hasShieldedTokens was false
        addr.into(),  // Second kernel_self returns contract address
    ];

    // ZSwap offer: user sends coins (negative delta), contract receives output
    let offer = ZswapOffer {
        inputs: vec![].into(),
        outputs: vec![out].into(),
        transient: vec![].into(),
        deltas: vec![Delta {
            token_type: token,
            value: -(FIRST_DEPOSIT as i128), // Negative = user spends
        }]
        .into(),
    };

    let transcripts = partition_transcripts(
        &[PreTranscript {
            context: context_with_offer(&state.ledger, addr, Some(&offer)),
            program: program_with_results(&public_transcript, &public_transcript_results),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap();

    let call = ContractCallPrototype {
        address: addr,
        entry_point: b"depositShielded"[..].into(),
        op: deposit_shielded_op.clone(),
        input: coin.into(),
        output: ().into(),
        guaranteed_public_transcript: transcripts[0].0.clone(),
        fallible_public_transcript: transcripts[0].1.clone(),
        private_transcript_outputs: vec![],
        communication_commitment_rand: rng.r#gen(),
        key_location: KeyLocation(Cow::Borrowed("depositShielded")),
    };

    let tx = Transaction::new(
        "local-test",
        test_intents(&mut rng, vec![call], Vec::new(), Vec::new(), state.time),
        Some(offer),
        HashMap::new(),
    );

    let tx = tx_prove_bind(rng.split(), &tx, &RESOLVER).await.unwrap();
    tx.well_formed(&state.ledger, unbalanced_strictness, state.time)
        .unwrap();

    let balanced = state.balance_tx(rng.split(), tx, &RESOLVER).await.unwrap();
    state.assert_apply(&balanced, balanced_strictness);

    println!("   First deposit: {} tokens locked", FIRST_DEPOSIT);

    // ========================================================================
    // Part 3: Second Shielded Deposit (merge with existing via transient)
    // ========================================================================
    println!("\n:: Part 3: Second Shielded Deposit (Merge with Existing)");
    const SECOND_DEPOSIT: u128 = 500_000;

    // Get current pot from contract state
    let cstate = state.ledger.contract.get(&addr).unwrap();
    let pot = if let StateValue::Array(arr) = &cstate.data.get_ref() {
        if let StateValue::Cell(pot) = &arr.get(0).unwrap() {
            QualifiedCoinInfo::try_from(&*pot.value).unwrap()
        } else {
            unreachable!()
        }
    } else {
        unreachable!()
    };

    let new_coin = CoinInfo::new(&mut rng, SECOND_DEPOSIT, token);
    let out = ZswapOutput::new_contract_owned(&mut rng, &new_coin, None, addr).unwrap();
    let new_coin_com = new_coin.commitment(&Recipient::Contract(addr));

    // Merged coin derives nonce from pot
    let merged_coin = CoinInfo::from(&pot).evolve_from(
        b"midnight:kernel:nonce_evolve",
        pot.value + new_coin.value,
        pot.type_,
    );
    let merged_coin_com = merged_coin.commitment(&Recipient::Contract(addr));

    // Create nullifiers for both coins being consumed
    let pot_nul = CoinInfo::from(&pot).nullifier(&SenderEvidence::Contract(addr));
    let coin_nul = new_coin.nullifier(&SenderEvidence::Contract(addr));

    // Create ZSwap input for existing pot
    let pot_in =
        ZswapInput::new_contract_owned(&mut rng, &pot, None, addr, &state.ledger.zswap.coin_coms)
            .unwrap();

    // Create transient for new deposit (created and nullified in same tx)
    let transient =
        ZswapTransient::new_from_contract_owned_output(&mut rng, &new_coin.qualify(0), None, out)
            .unwrap();

    // Output for merged coin
    let merged_out = ZswapOutput::new_contract_owned(&mut rng, &merged_coin, None, addr).unwrap();

    let offer = ZswapOffer {
        inputs: vec![pot_in].into(),
        outputs: vec![merged_out].into(),
        transient: vec![transient].into(),
        deltas: vec![Delta {
            token_type: token,
            value: -(SECOND_DEPOSIT as i128),
        }]
        .into(),
    };

    // Merge transcript: receive, read existing, nullify both, create merged
    let public_transcript: Vec<Op<ResultModeGather, InMemoryDB>> = [
        &kernel_self!((), ())[..],
        &kernel_claim_zswap_coin_receive!((), (), new_coin_com)[..],
        &Cell_read!([key!(STATE_IDX_HAS_SHIELDED_TOKENS)], false, bool)[..],
        &Cell_read!([key!(STATE_IDX_SHIELDED_VAULT)], false, QualifiedCoinInfo)[..],
        &kernel_self!((), ())[..],
        &kernel_claim_zswap_nullifier!((), (), pot_nul)[..],
        &kernel_claim_zswap_nullifier!((), (), coin_nul)[..],
        &kernel_claim_zswap_coin_spend!((), (), merged_coin_com)[..],
        &kernel_claim_zswap_coin_receive!((), (), merged_coin_com)[..],
        &kernel_self!((), ())[..],
        &Cell_write_coin!(
            [key!(STATE_IDX_SHIELDED_VAULT)],
            true,
            QualifiedCoinInfo,
            merged_coin.clone(),
            Recipient::Contract(addr)
        )[..],
        &Counter_increment!([key!(STATE_IDX_TOTAL_SHIELDED_DEPOSITS)], false, 1u64)[..],
    ]
    .into_iter()
    .flat_map(|x| x.iter())
    .cloned()
    .collect();

    let public_transcript_results: Vec<AlignedValue> = vec![
        addr.into(), // First kernel_self
        true.into(), // hasShieldedTokens is now true
        pot.into(),  // Read pot for merge
        addr.into(), // Second kernel_self (for nullifiers/coin ops)
        addr.into(), // Third kernel_self (for write)
    ];

    let transcripts = partition_transcripts(
        &[PreTranscript {
            context: context_with_offer(&state.ledger, addr, Some(&offer)),
            program: program_with_results(&public_transcript, &public_transcript_results),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap();

    let call = ContractCallPrototype {
        address: addr,
        entry_point: b"depositShielded"[..].into(),
        op: deposit_shielded_op.clone(),
        input: new_coin.into(),
        output: ().into(),
        guaranteed_public_transcript: transcripts[0].0.clone(),
        fallible_public_transcript: transcripts[0].1.clone(),
        private_transcript_outputs: vec![],
        communication_commitment_rand: rng.r#gen(),
        key_location: KeyLocation(Cow::Borrowed("depositShielded")),
    };

    let tx = Transaction::new(
        "local-test",
        test_intents(&mut rng, vec![call], Vec::new(), Vec::new(), state.time),
        Some(offer),
        HashMap::new(),
    );

    let tx = tx_prove_bind(rng.split(), &tx, &RESOLVER).await.unwrap();
    tx.well_formed(&state.ledger, unbalanced_strictness, state.time)
        .unwrap();

    let balanced = state.balance_tx(rng.split(), tx, &RESOLVER).await.unwrap();
    state.assert_apply(&balanced, balanced_strictness);

    println!(
        "   Merge deposit: {} + {} = {} tokens",
        FIRST_DEPOSIT,
        SECOND_DEPOSIT,
        FIRST_DEPOSIT + SECOND_DEPOSIT
    );

    // ========================================================================
    // Part 4: Partial Shielded Withdrawal (split into user + change coins)
    // ========================================================================
    println!("\n:: Part 4: Partial Shielded Withdrawal");
    const WITHDRAW_AMOUNT: u128 = 300_000;

    // Get current pot and check hasShieldedTokens
    let cstate = state.ledger.contract.get(&addr).unwrap();
    let (pot, has_shielded_tokens) = if let StateValue::Array(arr) = &cstate.data.get_ref() {
        let pot = if let StateValue::Cell(pot) = &arr.get(0).unwrap() {
            QualifiedCoinInfo::try_from(&*pot.value).unwrap()
        } else {
            unreachable!()
        };
        let has_tokens = if let StateValue::Cell(cell) = &arr.get(1).unwrap() {
            bool::try_from(&*cell.value).unwrap_or(false)
        } else {
            false
        };
        (pot, has_tokens)
    } else {
        unreachable!()
    };

    println!(
        "   Contract state: hasShieldedTokens={}, pot.value={}",
        has_shielded_tokens, pot.value
    );

    // Create withdrawal coin (goes to user) and change coin (stays in contract)
    // Note: Different domain separators are required to derive unique nonces
    let withdraw_coin = CoinInfo::from(&pot).evolve_from(
        b"midnight:kernel:nonce_evolve",
        WITHDRAW_AMOUNT,
        pot.type_,
    );

    let change_coin = CoinInfo::from(&pot).evolve_from(
        b"midnight:kernel:nonce_evolve/2", // Different domain separator for change
        pot.value - WITHDRAW_AMOUNT,
        pot.type_,
    );

    let pot_nul = CoinInfo::from(&pot).nullifier(&SenderEvidence::Contract(addr));
    let withdraw_com =
        withdraw_coin.commitment(&Recipient::User(state.zswap_keys.coin_public_key()));
    let change_com = change_coin.commitment(&Recipient::Contract(addr));

    let pot_in =
        ZswapInput::new_contract_owned(&mut rng, &pot, None, addr, &state.ledger.zswap.coin_coms)
            .unwrap();

    // User output (not contract-owned)
    let withdraw_out = ZswapOutput::new(
        &mut rng,
        &withdraw_coin,
        None,
        &state.zswap_keys.coin_public_key(),
        Some(state.zswap_keys.enc_public_key()),
    )
    .unwrap();

    // Change output (contract-owned)
    let change_out = ZswapOutput::new_contract_owned(&mut rng, &change_coin, None, addr).unwrap();

    // Outputs must be sorted for ZSwap offer normalization
    let mut outputs = vec![withdraw_out, change_out];
    outputs.sort();

    let offer = ZswapOffer {
        inputs: vec![pot_in].into(),
        outputs: outputs.into(),
        transient: vec![].into(),
        deltas: vec![].into(), // No delta - pure internal transfer
    };

    // Track the withdrawn coin so user can spend it later
    state.zswap = state
        .zswap
        .watch_for(&state.zswap_keys.coin_public_key(), &withdraw_coin);

    // Withdrawal transcript: isAuthorized checks, then sendShielded
    // Note: shieldedVault is read twice (value check + sendShielded)
    let public_transcript: Vec<Op<ResultModeGather, InMemoryDB>> = [
        &Set_member!([key!(STATE_IDX_AUTHORIZED)], false, [u8; 32], owner_pk.0)[..], // Check authorized.member(pk)
        &Cell_read!([key!(STATE_IDX_OWNER)], false, [u8; 32])[..], // Read owner for pk == owner
        &Cell_read!([key!(STATE_IDX_HAS_SHIELDED_TOKENS)], false, bool)[..], // Check hasShieldedTokens
        &Cell_read!([key!(STATE_IDX_SHIELDED_VAULT)], false, QualifiedCoinInfo)[..], // Read vault for value check
        &Cell_read!([key!(STATE_IDX_SHIELDED_VAULT)], false, QualifiedCoinInfo)[..], // Read vault for sendShielded
        &kernel_self!((), ())[..],
        &kernel_claim_zswap_nullifier!((), (), pot_nul)[..],
        &kernel_claim_zswap_coin_spend!((), (), withdraw_com)[..],
        &kernel_claim_zswap_coin_spend!((), (), change_com)[..],
        &kernel_claim_zswap_coin_receive!((), (), change_com)[..],
        &kernel_self!((), ())[..],
        &Cell_write_coin!(
            [key!(STATE_IDX_SHIELDED_VAULT)],
            true,
            QualifiedCoinInfo,
            change_coin.clone(),
            Recipient::Contract(addr)
        )[..],
        &Counter_increment!([key!(STATE_IDX_TOTAL_SHIELDED_WITHDRAWALS)], false, 1u64)[..],
    ]
    .into_iter()
    .flat_map(|x| x.iter())
    .cloned()
    .collect();

    let public_transcript_results: Vec<AlignedValue> = vec![
        false.into(),    // authorized.member(pk) result - pk is NOT in authorized set
        owner_pk.into(), // owner value - this equals pk, so pk == owner is true
        true.into(),     // hasShieldedTokens
        pot.into(),      // First vault read (for value check)
        pot.into(),      // Second vault read (for sendShielded)
        addr.into(),
        addr.into(),
    ];

    let transcripts = partition_transcripts(
        &[PreTranscript {
            context: context_with_offer(&state.ledger, addr, Some(&offer)),
            program: program_with_results(&public_transcript, &public_transcript_results),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap();

    let call = ContractCallPrototype {
        address: addr,
        entry_point: b"withdrawShielded"[..].into(),
        op: withdraw_shielded_op.clone(),
        input: (WITHDRAW_AMOUNT as u64).into(),
        output: withdraw_coin.into(),
        guaranteed_public_transcript: transcripts[0].0.clone(),
        fallible_public_transcript: transcripts[0].1.clone(),
        // Private witnesses: localSecretKey() gets owner_sk, ownPublicKey() gets public key
        private_transcript_outputs: vec![
            owner_sk.into(),
            state.zswap_keys.coin_public_key().into(),
        ],
        communication_commitment_rand: rng.r#gen(),
        key_location: KeyLocation(Cow::Borrowed("withdrawShielded")),
    };

    let tx = Transaction::new(
        "local-test",
        test_intents(&mut rng, vec![call], Vec::new(), Vec::new(), state.time),
        Some(offer),
        HashMap::new(),
    );

    let tx = tx_prove_bind(rng.split(), &tx, &RESOLVER).await.unwrap();
    tx.well_formed(&state.ledger, unbalanced_strictness, state.time)
        .unwrap();

    let balanced = state.balance_tx(rng.split(), tx, &RESOLVER).await.unwrap();
    state.assert_apply(&balanced, balanced_strictness);

    let remaining = FIRST_DEPOSIT + SECOND_DEPOSIT - WITHDRAW_AMOUNT;
    println!(
        "   Partial withdrawal: {} tokens withdrawn, {} remaining in vault",
        WITHDRAW_AMOUNT, remaining
    );

    // ========================================================================
    // Summary
    // ========================================================================
    println!("\n:: Test Summary");
    println!("   Initial funds: {} tokens", REWARDS_AMOUNT);
    println!("   First deposit: {} tokens", FIRST_DEPOSIT);
    println!("   Second deposit (merge): {} tokens", SECOND_DEPOSIT);
    println!(
        "   Total deposited: {} tokens",
        FIRST_DEPOSIT + SECOND_DEPOSIT
    );
    println!("   Withdrawn: {} tokens", WITHDRAW_AMOUNT);
    println!("   Remaining in vault: {} tokens", remaining);
    println!("\n   All shielded operations completed successfully!");
}

// ============================================================================
// Unit Tests
// ============================================================================

/// Test contract deployment with shielded operations
#[tokio::test]
async fn test_deploy_only() {
    let mut rng = StdRng::seed_from_u64(0x42);
    let state: TestState<InMemoryDB> = TestState::new(&mut rng);
    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = false;

    let owner_sk: HashOutput = rng.r#gen();
    let owner_pk = derive_public_key(owner_sk);

    let deposit_shielded_op =
        ContractOperation::new(verifier_key(&RESOLVER, "depositShielded").await);

    let contract: ContractState<InMemoryDB> = ContractState::new(
        stval!([
            (QualifiedCoinInfo::default()),
            (false),
            (owner_pk),
            {},
            (0u64),
            (0u64),
            (0u64),
            (0u64)
        ]),
        HashMap::new().insert(b"depositShielded"[..].into(), deposit_shielded_op),
        Default::default(),
    );

    let deploy = ContractDeploy::new(&mut rng, contract);
    let addr = deploy.address();

    let tx = Transaction::from_intents(
        "local-test",
        test_intents(&mut rng, Vec::new(), Vec::new(), vec![deploy], state.time),
    );
    tx.well_formed(&state.ledger, strictness, state.time)
        .unwrap();

    println!("Contract deployment test passed");
    println!("   Address: {:?}", addr);
}
