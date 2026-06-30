use midnight_node_e2e::e2e_test;

// ============================================================================
// DDoS Mitigation E2E Tests (PR367)
// Tests for ADR-0003: Pre-Dispatch Validation of Guaranteed Transaction Part
//
// TODO(#1792): These tests previously submitted static `local`-network contract
// fixtures (STORE_TX / DEPLOY_TX from `midnight_node_res::local::transactions`).
// Those fixtures were removed together with the unfunded `local` genesis: the
// `local` network now funds wallets at runtime via the cNIGHT->DUST bridge
// rather than at genesis, so the transactions must be generated dynamically
// against the live, bridge-funded chain (see
// `contract_state_distinguishes_historical_and_current_blocks` for the deploy
// pattern; the store-rejection tests need a deploy built locally but not
// submitted, then the store submitted so the contract is absent on-chain).
// Re-enable once wallet seeds are funded via the bridge in the local-env.
// ============================================================================

/// PR367-TC-0003-06: DDoS Attack Prevention - Single Transaction
///
/// A store for a non-deployed contract must be rejected at pre_dispatch
/// (ContractNotPresent), so attackers can't fill blocks with failing,
/// fee-less transactions.
#[e2e_test]
#[ignore = "TODO(#1792): rebuild STORE_TX dynamically against the bridge-funded local chain"]
async fn ddos_attack_transaction_rejected_at_rpc() {
    todo!("rebuild store-without-deploy tx dynamically once wallet seeds are bridge-funded");
}

/// PR367-TC-0003-06: DDoS Attack Prevention - Batch Attack
///
/// Multiple attack transactions must all be rejected.
#[e2e_test]
#[ignore = "TODO(#1792): rebuild STORE_TX dynamically against the bridge-funded local chain"]
async fn ddos_batch_attack_all_rejected() {
    todo!("rebuild store-without-deploy tx dynamically once wallet seeds are bridge-funded");
}

/// PR367-TC-0003-02 E2E: Replay Attack Prevention
///
/// Submitting the same deploy transaction twice must reject the duplicate at
/// pre_dispatch (replay protection / ContractAlreadyDeployed).
#[e2e_test]
#[ignore = "TODO(#1792): rebuild DEPLOY_TX dynamically against the bridge-funded local chain"]
async fn replay_attack_rejected_via_rpc() {
    todo!("rebuild deploy tx dynamically once wallet seeds are bridge-funded");
}
