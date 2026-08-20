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

//! # Unshielded Token Tests with Full ZKIR Proving
//!
//! Integration tests for unshielded token operations using real ZK proof generation.
//!
//! ## Test Coverage
//!
//! - `test_unshielded_contract_deposit`: User deposits UTXO to contract
//! - `test_unshielded_contract_withdraw`: Contract sends tokens back to user
//!
//! ## Key Features
//!
//! - **Real ZKIR Proving**: Uses `tx_prove_bind()` to validate against compiled circuits
//! - **Signature Verification**: All UTXO spends are properly signed
//! - **Transcript Matching**: Operations match circuit expectations exactly
//!
//! ## Setup Required
//!
//! ```bash
//! # Set path to contract proving keys
//! export MIDNIGHT_LEDGER_TEST_STATIC_DIR="/home/ricardo/.dev/tmp/ledger-tokens/midnight-ledger/ledger/static"
//!
//! # Ensure token-vault contract is compiled and linked at:
//! # $MIDNIGHT_LEDGER_TEST_STATIC_DIR/token-vault/keys/*.{prover,verifier}
//! # $MIDNIGHT_LEDGER_TEST_STATIC_DIR/token-vault/zkir/*.bzkir
//!
//! # Run tests
//! cargo test --test token_vault_unshielded
//! ```

#![deny(warnings)]
#![allow(unused_imports)]
#![allow(unused_variables)]

mod token_vault_common;

use token_vault_common::*;

// ════════════════════════════════════════════════════════════════════════════
//  UNSHIELDED TOKEN TESTS - FULL ZKIR PROVING
// ════════════════════════════════════════════════════════════════════════════
//
// These tests use production-grade ZKIR proving via `tx_prove_bind()`.
//
// Requirements:
//  - Transcripts must match compiled circuit operations exactly
//  - `public_transcript_results` must match Popeq{cached:true} operations
//  - Private witnesses (e.g., owner_sk) must be provided
//  - Use `context_with_balance()` when circuits read contract balance
//  - UTXO spends must be signed with proper keys
//
// Strictness:
//  - `verify_contract_proofs`: true (real ZKIR proving)
//  - `verify_signatures`: true (UTXO signatures verified)
//  - `enforce_balancing`: false (unshielded uses UTXO model, not ZSwap)
// ════════════════════════════════════════════════════════════════════════════

/// Test: User deposits unshielded NIGHT tokens to contract.
///
/// Flow: User UTXO → Contract.receiveUnshielded() → Contract balance increases
///
/// Requirements:
/// - UTXO must be signed by owner
/// - UnshieldedOffer.inputs value matches transcript receiveUnshielded amount
/// - Ledger verifies: UnshieldedOffer.inputs >= effects[6].unshielded_inputs
#[tokio::test]
async fn test_unshielded_contract_deposit() {
    use base_crypto::signatures::SigningKey;

    //midnight_ledger::init_logger(midnight_ledger::LogLevel::Trace);
    let mut rng = StdRng::seed_from_u64(0x43);

    // Initialize crypto parameters
    lazy_static::initialize(&PARAMS_VERIFIER);
    SPEND_VK.init().ok();
    OUTPUT_VK.init().ok();
    SIGN_VK.init().ok();

    println!(":: Unshielded Contract Deposit Test");

    // Create test state and generate NIGHT UTXOs
    let mut state: TestState<InMemoryDB> = TestState::new(&mut rng);
    const DEPOSIT_AMOUNT: u128 = 500_000;

    // Give fee tokens (register for dust generation and create NIGHT UTXOs)
    state.give_fee_token(&mut rng, 10).await;

    // Create additional NIGHT tokens for the deposit
    state.reward_night(&mut rng, DEPOSIT_AMOUNT).await;
    state.fast_forward(state.ledger.parameters.dust.time_to_cap());

    println!("   Created {} NIGHT tokens", DEPOSIT_AMOUNT);

    // Get user verifying key (UTXO info will be obtained after contract deployment)
    let user_verifying_key = state.night_key.verifying_key();
    let user_address = UserAddress::from(user_verifying_key.clone());

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

    // Deploy contract
    let owner_sk: HashOutput = rng.r#gen();
    let owner_pk = derive_public_key(owner_sk);

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

    let deploy = ContractDeploy::new(&mut rng, contract);
    let addr = deploy.address();

    let mut unbalanced_strictness = WellFormedStrictness::default();
    unbalanced_strictness.enforce_balancing = false;

    let balanced_strictness = WellFormedStrictness::default();

    // Deploy the contract
    let deploy_tx = Transaction::from_intents(
        "local-test",
        test_intents(&mut rng, Vec::new(), Vec::new(), vec![deploy], state.time),
    );
    deploy_tx
        .well_formed(&state.ledger, unbalanced_strictness, state.time)
        .unwrap();

    let deploy_tx = tx_prove_bind(rng.split(), &deploy_tx, &RESOLVER)
        .await
        .unwrap();
    let balanced = state
        .balance_tx(rng.split(), deploy_tx, &RESOLVER)
        .await
        .unwrap();
    state.assert_apply(&balanced, balanced_strictness);

    println!("   Contract deployed at {:?}", addr);

    // Now get UTXO info (after contract deployment so it's not spent by fee balancing)
    // Find a UTXO owned by the user with enough balance
    let utxo_ref = state
        .ledger
        .utxo
        .utxos
        .iter()
        .find(|utxo_ref| utxo_ref.0.owner == user_address && utxo_ref.0.value >= DEPOSIT_AMOUNT)
        .expect("User should have a UTXO with sufficient balance");
    let utxo_ih = utxo_ref.0.intent_hash;
    let utxo_out_no = utxo_ref.0.output_no;
    let utxo_value = utxo_ref.0.value;

    println!(
        "   Using UTXO: intent_hash={:?}, value={}",
        hex::encode(&utxo_ih.0.0[..8]),
        utxo_value
    );

    // Build the transcript for depositUnshielded
    // The circuit calls: receiveUnshielded(color, amount) + Counter_increment
    // Use the actual UTXO value for the deposit
    let token_type = TokenType::Unshielded(NIGHT);
    let deposit_amount = utxo_value; // Use actual UTXO value

    let deposit_transcript: Vec<Op<ResultModeGather, InMemoryDB>> = [
        // receiveUnshielded increments unshielded_inputs (effects index 6)
        &receive_unshielded_ops::<InMemoryDB>(token_type, deposit_amount)[..],
        // Counter_increment for totalUnshieldedDeposits (state index 6)
        &Counter_increment!([key!(6u8)], false, 1u64)[..],
    ]
    .into_iter()
    .flat_map(|x| x.iter())
    .cloned()
    .collect();

    let transcripts = partition_transcripts(
        &[PreTranscript {
            context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
            program: program_with_results(&deposit_transcript, &[]),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap();

    // Build contract call
    // Input: (color: Bytes<32>, amount: Uint<128>)
    let input_av: AlignedValue = AlignedValue::concat(
        [
            AlignedValue::from(NIGHT.0),        // color
            AlignedValue::from(deposit_amount), // amount (use actual UTXO value)
        ]
        .iter(),
    );

    let call = ContractCallPrototype {
        address: addr,
        entry_point: b"depositUnshielded"[..].into(),
        op: deposit_unshielded_op.clone(),
        input: input_av,
        output: ().into(),
        guaranteed_public_transcript: transcripts[0].0.clone(),
        fallible_public_transcript: transcripts[0].1.clone(),
        private_transcript_outputs: vec![],
        communication_commitment_rand: rng.r#gen(),
        key_location: KeyLocation(Cow::Borrowed("depositUnshielded")),
    };

    // Build UnshieldedOffer: user provides NIGHT tokens as input
    // ========================================================================
    // Build UnshieldedOffer: This is how unshielded tokens flow into contracts
    // ========================================================================
    //
    // For DEPOSITS (user → contract):
    // - UnshieldedOffer.inputs: The UTXOs being spent (user's tokens)
    // - UnshieldedOffer.outputs: Empty (tokens go to contract, not another UTXO)
    //
    // The ledger verifies: sum(inputs) >= contract's unshielded_inputs effect
    // This ensures the user actually has the tokens they claim to deposit.
    let uso: UnshieldedOffer<(), InMemoryDB> = UnshieldedOffer {
        inputs: vec![UtxoSpend {
            intent_hash: utxo_ih,
            output_no: utxo_out_no,
            owner: user_verifying_key,
            type_: NIGHT,
            value: deposit_amount, // must match the UTXO value
        }]
        .into(),
        outputs: vec![].into(), // No outputs - tokens go to contract
        signatures: vec![].into(),
    };

    // Create intent with contract call and unshielded offer
    use midnight_ledger::structure::StandardTransaction;

    let fallible_unshielded_offer: Option<UnshieldedOffer<(), InMemoryDB>> = Some(uso);

    // Create the intent and sign it for UTXO spending
    let intent = Intent::new(
        &mut rng,
        None,
        fallible_unshielded_offer,
        vec![call],
        Vec::new(),
        Vec::new(),
        None,
        state.time + base_crypto::time::Duration::from_secs(3600),
    )
    .sign(
        &mut rng,
        1,                          // segment_id
        &[],                        // No guaranteed signing keys
        &[state.night_key.clone()], // Sign the UTXO spend
        &[],                        // No dust registration signing keys
    )
    .unwrap();

    let mut intents: storage::storage::HashMap<
        u16,
        Intent<(), ProofPreimageMarker, transient_crypto::curve::EmbeddedFr, InMemoryDB>,
    > = storage::storage::HashMap::new();

    intents = intents.insert(1, intent);

    let tx = Transaction::Standard(StandardTransaction::new(
        "local-test",
        intents,
        None,
        HashMap::new(),
    ));

    // Prove the contract call against the ZKIR
    let tx = tx_prove_bind(rng.split(), &tx, &RESOLVER).await.unwrap();

    // Apply with full verification (only skip ZSwap balancing since we use UTXO)
    let mut strictness = WellFormedStrictness::default();
    strictness.enforce_balancing = false; // Unshielded uses UTXO, not ZSwap balancing

    state.assert_apply(&tx, strictness);

    println!("   Deposit transaction applied");

    // ========================================================================
    // Verify Results: Confirm the ledger state changed correctly
    // ========================================================================
    //
    // After applying a transaction, we verify two things:
    // 1. Contract balance increased (tokens were deposited)
    // 2. Original UTXO was consumed (can't double-spend)
    //
    // These checks prove the unshielded deposit flow works correctly.

    // Verify contract received the tokens
    let cstate = state.ledger.contract.get(&addr).unwrap();
    let final_balance = cstate.balance.get(&token_type).map(|v| *v).unwrap_or(0);
    assert_eq!(
        final_balance, deposit_amount,
        "Contract should have received tokens"
    );

    // Verify UTXO was spent
    let utxo_spent = !state
        .ledger
        .utxo
        .utxos
        .iter()
        .any(|r| r.0.intent_hash == utxo_ih && r.0.output_no == utxo_out_no);
    assert!(utxo_spent, "Original UTXO should be spent");

    println!("   Contract balance: {} NIGHT", final_balance);
    println!("   UTXO spent");
    println!("\nUnshielded contract deposit test PASSED!");
}

/// Test: Contract withdraws unshielded NIGHT tokens to user.
///
/// Flow: Contract.withdrawUnshielded() → New UTXO created for user
///
/// This test does a full deposit+withdraw cycle with real ZKIR proving:
/// 1. Deposit: User UTXO → Contract (via receiveUnshielded)
/// 2. Withdraw: Contract → User UTXO (via withdrawUnshielded with authorization)
///
/// Circuit operations for withdrawUnshielded:
/// - isAuthorized(): Set_member + Cell_read (checks pk == owner)
/// - unshieldedBalanceGte(): Balance check via unshieldedBalanceLt negation
/// - sendUnshielded(): Increments effects[7] and effects[8]
/// - Counter_increment: Tracks totalUnshieldedWithdrawals
///
/// Key type matching:
/// - Recipient::User(CoinPublicKey(hash)) must match UtxoOutput.owner(UserAddress(hash))
/// - Both wrap same HashOutput, extract with: CoinPublicKey(user_address.0)
#[tokio::test]
async fn test_unshielded_contract_withdraw() {
    use base_crypto::signatures::SigningKey;

    //midnight_ledger::init_logger(midnight_ledger::LogLevel::Trace);
    let mut rng = StdRng::seed_from_u64(0x44);

    // Initialize crypto parameters
    lazy_static::initialize(&PARAMS_VERIFIER);
    SPEND_VK.init().ok();
    OUTPUT_VK.init().ok();
    SIGN_VK.init().ok();

    println!(":: Unshielded Contract Withdraw Test");

    // Create test state and fund with fee tokens
    let mut state: TestState<InMemoryDB> = TestState::new(&mut rng);
    state.give_fee_token(&mut rng, 10).await;

    // Get user keys
    let user_verifying_key = state.night_key.verifying_key();
    let user_address = UserAddress::from(user_verifying_key.clone());

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

    // Deploy contract with initial balance
    let owner_sk: HashOutput = rng.r#gen();
    let owner_pk = derive_public_key(owner_sk);

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

    let deploy = ContractDeploy::new(&mut rng, contract);
    let addr = deploy.address();

    let mut unbalanced_strictness = WellFormedStrictness::default();
    unbalanced_strictness.enforce_balancing = false;

    let balanced_strictness = WellFormedStrictness::default();

    // Deploy the contract
    let deploy_tx = Transaction::from_intents(
        "local-test",
        test_intents(&mut rng, Vec::new(), Vec::new(), vec![deploy], state.time),
    );
    deploy_tx
        .well_formed(&state.ledger, unbalanced_strictness, state.time)
        .unwrap();

    let deploy_tx = tx_prove_bind(rng.split(), &deploy_tx, &RESOLVER)
        .await
        .unwrap();
    let balanced = state
        .balance_tx(rng.split(), deploy_tx, &RESOLVER)
        .await
        .unwrap();
    state.assert_apply(&balanced, balanced_strictness);

    println!("   Contract deployed at {:?}", addr);

    // ========================================================================
    // Step 1: Deposit tokens to contract first
    // ========================================================================

    // Find a UTXO for deposit
    let deposit_utxo = state
        .ledger
        .utxo
        .utxos
        .iter()
        .find(|utxo_ref| utxo_ref.0.owner == user_address)
        .expect("User should have a UTXO");
    let deposit_utxo_ih = deposit_utxo.0.intent_hash;
    let deposit_utxo_out_no = deposit_utxo.0.output_no;
    let deposit_amount = deposit_utxo.0.value;

    let token_type = TokenType::Unshielded(NIGHT);

    // Build deposit transcript
    let deposit_transcript: Vec<Op<ResultModeGather, InMemoryDB>> = [
        &receive_unshielded_ops::<InMemoryDB>(token_type, deposit_amount)[..],
        &Counter_increment!([key!(STATE_IDX_TOTAL_UNSHIELDED_DEPOSITS)], false, 1u64)[..],
    ]
    .into_iter()
    .flat_map(|x| x.iter())
    .cloned()
    .collect();

    let deposit_transcripts = partition_transcripts(
        &[PreTranscript {
            context: QueryContext::new(state.ledger.index(addr).unwrap().data, addr),
            program: program_with_results(&deposit_transcript, &[]),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap();

    let deposit_input_av: AlignedValue = AlignedValue::concat(
        [
            AlignedValue::from(NIGHT.0),
            AlignedValue::from(deposit_amount),
        ]
        .iter(),
    );

    let deposit_call = ContractCallPrototype {
        address: addr,
        entry_point: b"depositUnshielded"[..].into(),
        op: deposit_unshielded_op.clone(),
        input: deposit_input_av,
        output: ().into(),
        guaranteed_public_transcript: deposit_transcripts[0].0.clone(),
        fallible_public_transcript: deposit_transcripts[0].1.clone(),
        private_transcript_outputs: vec![],
        communication_commitment_rand: rng.r#gen(),
        key_location: KeyLocation(Cow::Borrowed("depositUnshielded")),
    };

    let deposit_uso: UnshieldedOffer<(), InMemoryDB> = UnshieldedOffer {
        inputs: vec![UtxoSpend {
            intent_hash: deposit_utxo_ih,
            output_no: deposit_utxo_out_no,
            owner: user_verifying_key.clone(),
            type_: NIGHT,
            value: deposit_amount,
        }]
        .into(),
        outputs: vec![].into(),
        signatures: vec![].into(),
    };

    use midnight_ledger::structure::StandardTransaction;

    // Create and sign intent (signing required because we're spending a UTXO)
    let deposit_intent = Intent::new(
        &mut rng,
        None,
        Some(deposit_uso),
        vec![deposit_call],
        Vec::new(),
        Vec::new(),
        None,
        state.time + base_crypto::time::Duration::from_secs(3600),
    )
    .sign(&mut rng, 1, &[], &[state.night_key.clone()], &[])
    .unwrap();

    let mut deposit_intents: storage::storage::HashMap<
        u16,
        Intent<(), ProofPreimageMarker, transient_crypto::curve::EmbeddedFr, InMemoryDB>,
    > = storage::storage::HashMap::new();

    deposit_intents = deposit_intents.insert(1, deposit_intent);

    let deposit_tx = Transaction::Standard(StandardTransaction::new(
        "local-test",
        deposit_intents,
        None,
        HashMap::new(),
    ));

    // Prove the transaction against ZKIR
    let deposit_tx = tx_prove_bind(rng.split(), &deposit_tx, &RESOLVER)
        .await
        .unwrap();

    let mut deposit_strictness = WellFormedStrictness::default();
    deposit_strictness.enforce_balancing = false;

    state.assert_apply(&deposit_tx, deposit_strictness);

    println!("   Deposited {} NIGHT to contract", deposit_amount);

    // Verify contract balance
    let cstate_after_deposit = state.ledger.contract.get(&addr).unwrap();
    let balance_after_deposit = cstate_after_deposit
        .balance
        .get(&token_type)
        .map(|v| *v)
        .unwrap_or(0);
    assert_eq!(
        balance_after_deposit, deposit_amount,
        "Contract should have deposited tokens"
    );

    // ========================================================================
    // Step 2: Withdraw tokens from contract
    // ========================================================================
    //
    // Withdrawal flow is the reverse of deposit:
    // - Contract calls sendUnshielded (effects[7]) to indicate outgoing tokens
    // - Contract claims the spend (effects[8]) to specify the recipient
    // - User receives a NEW UTXO in UnshieldedOffer.outputs
    //
    // The ledger verifies:
    //   claimed_unshielded_spends[(token_type, recipient)] ⊆ UnshieldedOffer.outputs
    // This ensures the recipient specified by the contract actually gets the UTXO.

    let withdraw_amount = deposit_amount / 2; // Withdraw half

    // ========================================================================
    // Critical: Key Type Matching
    // ========================================================================
    //
    // The `Recipient::User(PublicKey)` in our transcript MUST match the
    // `UserAddress` in `UnshieldedOffer.outputs.owner`.
    //
    // Both types wrap the same `HashOutput` internally:
    // - coin::PublicKey(HashOutput)
    // - UserAddress(HashOutput)
    //
    // We extract the HashOutput from UserAddress and wrap it in PublicKey:
    use coin_structure::coin::PublicKey as CoinPublicKey;
    let recipient_pk = CoinPublicKey(user_address.0); // Same HashOutput as UserAddress

    // Build the withdrawal transcript
    //
    // The withdrawUnshielded circuit performs these operations in order:
    // 1. isAuthorized(): Set_member check on authorized set + Cell_read of owner
    // 2. unshieldedBalanceGte(): Balance check (via unshieldedBalanceLt negated)
    // 3. sendUnshielded(): Increments effects[7] and effects[8]
    // 4. Counter_increment: Tracks total withdrawals in contract state
    //
    // We must match this exact order in our transcript.
    let withdraw_transcript: Vec<Op<ResultModeGather, InMemoryDB>> = [
        // === isAuthorized() ===
        // Check if public key is in authorized set (state index 3)
        &Set_member!([key!(STATE_IDX_AUTHORIZED)], false, [u8; 32], owner_pk.0)[..],
        // Read owner from state (state index 2)
        &Cell_read!([key!(STATE_IDX_OWNER)], false, [u8; 32])[..],
        // === unshieldedBalanceGte() ===
        // Check if balance >= amount (implemented as !(balance < amount))
        &unshielded_balance_lt_ops::<InMemoryDB>(token_type, withdraw_amount)[..],
        // === sendUnshielded() ===
        // sendUnshielded increments unshielded_outputs (effects index 7)
        &send_unshielded_ops::<InMemoryDB>(token_type, withdraw_amount)[..],
        // Also claim the unshielded spend (effects index 8)
        // This specifies: "User with public key X should receive these tokens"
        &claim_unshielded_spend_ops::<InMemoryDB>(
            token_type,
            Recipient::User(recipient_pk),
            withdraw_amount,
        )[..],
        // Counter_increment for totalUnshieldedWithdrawals (state index 7)
        &Counter_increment!([key!(STATE_IDX_TOTAL_UNSHIELDED_WITHDRAWALS)], false, 1u64)[..],
    ]
    .into_iter()
    .flat_map(|x| x.iter())
    .cloned()
    .collect();

    // Results for operations that have Popeq { cached: true }
    // 1. Set_member! returns false (pk is NOT in the authorized set)
    // 2. Cell_read! returns owner_pk (the owner's public key - pk == owner succeeds)
    // 3. unshielded_balance_lt_ops returns false (balance >= amount, NOT less than)
    let withdraw_transcript_results: Vec<AlignedValue> = vec![
        false.into(),    // authorized.member(pk) - pk not in set
        owner_pk.into(), // owner value - pk == owner succeeds authorization
        false.into(),    // balance_lt result - balance >= amount (NOT less than)
    ];

    let withdraw_transcripts = partition_transcripts(
        &[PreTranscript {
            // Use context_with_balance to include the contract's current unshielded balance
            // This is necessary because unshieldedBalanceGte reads from CallContext.balance
            context: context_with_balance(&state.ledger, addr),
            program: program_with_results(&withdraw_transcript, &withdraw_transcript_results),
            comm_comm: None,
        }],
        &INITIAL_PARAMETERS,
    )
    .unwrap();

    // Build contract call input for withdrawUnshielded
    // Input: (color: Bytes<32>, amount: Uint<128>, recipient: Either<ContractAddress, UserAddress>)
    // For Either::Right(UserAddress), encoding is: [false, (), user_addr]
    use coin_structure::coin::PublicAddress;
    let recipient_either = PublicAddress::User(user_address);
    let withdraw_input_av: AlignedValue = AlignedValue::concat(
        [
            AlignedValue::from(NIGHT.0),          // color
            AlignedValue::from(withdraw_amount),  // amount
            AlignedValue::from(recipient_either), // recipient as Either<ContractAddress, UserAddress>
        ]
        .iter(),
    );

    let withdraw_call = ContractCallPrototype {
        address: addr,
        entry_point: b"withdrawUnshielded"[..].into(),
        op: withdraw_unshielded_op.clone(),
        input: withdraw_input_av,
        output: ().into(),
        guaranteed_public_transcript: withdraw_transcripts[0].0.clone(),
        fallible_public_transcript: withdraw_transcripts[0].1.clone(),
        // Private witness: localSecretKey() needs owner_sk to prove authorization
        private_transcript_outputs: vec![owner_sk.into()],
        communication_commitment_rand: rng.r#gen(),
        key_location: KeyLocation(Cow::Borrowed("withdrawUnshielded")),
    };

    // ========================================================================
    // Build UnshieldedOffer for Withdrawal
    // ========================================================================
    //
    // For WITHDRAWALS (contract → user):
    // - UnshieldedOffer.inputs: Empty (no UTXOs being spent - tokens come from contract)
    // - UnshieldedOffer.outputs: The NEW UTXO being created for the user
    //
    // The owner field MUST match the recipient in claimed_unshielded_spends!
    let withdraw_uso: UnshieldedOffer<(), InMemoryDB> = UnshieldedOffer {
        inputs: vec![].into(), // No UTXO inputs - tokens come from contract
        outputs: vec![UtxoOutput {
            owner: user_address, // MUST match recipient_pk's inner HashOutput
            type_: NIGHT,
            value: withdraw_amount,
        }]
        .into(),
        signatures: vec![].into(),
    };

    // Create intent - no signing needed since no UTXOs are being spent
    let withdraw_intent = Intent::new(
        &mut rng,
        None,
        Some(withdraw_uso),
        vec![withdraw_call],
        Vec::new(),
        Vec::new(),
        None,
        state.time + base_crypto::time::Duration::from_secs(3600),
    );

    let mut withdraw_intents: storage::storage::HashMap<
        u16,
        Intent<(), ProofPreimageMarker, transient_crypto::curve::EmbeddedFr, InMemoryDB>,
    > = storage::storage::HashMap::new();

    withdraw_intents = withdraw_intents.insert(1, withdraw_intent);

    let withdraw_tx = Transaction::Standard(StandardTransaction::new(
        "local-test",
        withdraw_intents,
        None,
        HashMap::new(),
    ));

    // Prove the transaction against ZKIR
    let withdraw_tx = tx_prove_bind(rng.split(), &withdraw_tx, &RESOLVER)
        .await
        .unwrap();

    let mut withdraw_strictness = WellFormedStrictness::default();
    withdraw_strictness.enforce_balancing = false;

    state.assert_apply(&withdraw_tx, withdraw_strictness);

    println!("   Withdrew {} NIGHT from contract", withdraw_amount);

    // ========================================================================
    // Step 3: Verify results
    // ========================================================================

    // Verify contract balance decreased
    let cstate_after_withdraw = state.ledger.contract.get(&addr).unwrap();
    let balance_after_withdraw = cstate_after_withdraw
        .balance
        .get(&token_type)
        .map(|v| *v)
        .unwrap_or(0);
    let expected_balance = deposit_amount - withdraw_amount;
    assert_eq!(
        balance_after_withdraw, expected_balance,
        "Contract balance should have decreased by withdraw amount"
    );

    println!(
        "   Contract balance: {} NIGHT (was {}, withdrew {})",
        balance_after_withdraw, deposit_amount, withdraw_amount
    );

    // Verify user received new UTXO
    let user_utxos: Vec<_> = state
        .ledger
        .utxo
        .utxos
        .iter()
        .filter(|r| r.0.owner == user_address && r.0.value == withdraw_amount)
        .collect();
    assert!(
        !user_utxos.is_empty(),
        "User should have received a new UTXO with withdrawn tokens"
    );

    println!("   User received UTXO with {} NIGHT", withdraw_amount);

    println!("\nUnshielded contract withdraw test PASSED!");
}
