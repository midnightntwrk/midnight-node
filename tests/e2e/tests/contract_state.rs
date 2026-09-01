use midnight_node_e2e::api::midnight::{AlignedValue, MidnightClient, PathKey, RpcStateQuery};
use midnight_node_e2e::config::Settings;
use midnight_node_e2e::e2e_test;
use tokio::time::{Duration, sleep, timeout};

use crate::{PreDeployGuard, wait_before_deploying};

// ============================================================================
// Audit Issue AD (#1166): Return ContractNotPresent Instead of Default State
//
// The RPC `midnight_contractState` must surface a `ContractNotPresent` error
// when queried for a contract that has never been deployed, so that callers
// can distinguish "deployed contract with empty state" from "no such contract".
// ============================================================================

fn assert_contract_not_present_error(err: &(dyn std::error::Error + 'static)) {
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("not present") || msg.contains("notpresent"),
        "expected ContractNotPresent error, got: {err}"
    );
}

/// #1166: a well-formed but undeployed contract address must return
/// ContractNotPresent — not an empty string and not a generic decode error.
/// Uses a well-formed but never-deployed address so we know the address itself
/// parses; the only reason for failure is "no contract here".
/// Pre-deploy gated so it runs before any deploy submission.
#[e2e_test]
async fn contract_state_for_undeployed_address_returns_not_present() {
    let _pre_deploy_guard = PreDeployGuard::new();

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    // A well-formed 32-byte contract address that is never deployed. Contract
    // addresses are raw hex (network-independent), so no fixture is needed.
    let addr = "0000000000000000000000000000000000000000000000000000000000000001";

    let result = client.get_contract_state(addr).await;

    let err = result.expect_err("expected ContractNotPresent for undeployed contract, got Ok");
    assert_contract_not_present_error(err.as_ref());
}

/// #1166: an unparseable (non-hex) address must be rejected at the RPC layer
/// with BadContractAddress, distinct from ContractNotPresent. This protects
/// the new error variant from being conflated with input-validation failures.
#[e2e_test]
async fn contract_state_rejects_unparseable_address() {
    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    let result = client.get_contract_state("zz_not_hex").await;

    let err = result.expect_err("expected BadContractAddress, got Ok");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("decode") && msg.contains("contract address"),
        "expected BadContractAddress (\"Unable to decode contract address\"), got: {err}"
    );
    assert!(
        !msg.contains("not present"),
        "BadContractAddress and ContractNotPresent must be distinct errors, got: {err}"
    );
}

/// #1166: the same address must return ContractNotPresent at a pre-deploy
/// block hash and the deployed state at a post-deploy block hash. This is
/// the strongest demonstration that the RPC now lets callers distinguish
/// "missing contract" from "contract with empty state".
///
/// Block 1 (the first block after genesis) is the pre-deploy reference —
/// no user transaction can have been included yet.
#[e2e_test]
async fn contract_state_distinguishes_historical_and_current_blocks() {
    // The deploy is funded by dev wallet 0x..01, which the local-env funds at runtime
    // over the cNIGHT bridge (init-mnight-faucet). Wait until it can fund + pay fees.
    crate::ensure_dev_wallet_funded().await;

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client.clone()).await;

    // AURA produces block 1 ~6s after genesis. On a freshly-started CI runner
    // this test can race the first block, so poll briefly rather than failing
    // outright.
    let pre_deploy_hash = timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(hash) = client.get_block_hash_at_height(1).await {
                return hash;
            }
            sleep(Duration::from_secs(1)).await;
        }
    })
    .await
    .expect("block 1 not produced within 30s");

    let _deploy_guard = wait_before_deploying().await;

    // Generate a fresh DEPLOY_TX dynamically against the live chain. The
    // static fixture in res/test-contract has its intent_ttl baked in at
    // generation time and expires once chain time advances past it (~14 days
    // after fixture regeneration), so we can't rely on it for CI/live envs.
    // The toolkit's local prover (via MIDNIGHT_LEDGER_TEST_STATIC_DIR, set in
    // .envrc) handles ZK proof generation in-process; no external proof
    // server is required.
    let url = settings.node_client.base_url.clone();

    // Build + submit a fresh deploy (retrying past transient shared-wallet DUST
    // contention) and return its contract address. Dynamic generation keeps the
    // intent TTL valid against current chain time; the toolkit's in-process local
    // prover handles ZK proving (MIDNIGHT_LEDGER_TEST_STATIC_DIR, set in .envrc).
    let (_deploy_tx, addr) = crate::deploy_and_confirm(&client, &url).await;
    tracing::info!("Contract deployed at address: {addr}");

    // The deploy is now included; the contract must be present at the current head.
    let post_deploy_state = client
        .get_contract_state(&addr)
        .await
        .expect("expected deployed state at current head, got Err");
    assert!(
        !post_deploy_state.is_empty(),
        "deployed contract state should be non-empty ({} hex chars)",
        post_deploy_state.len()
    );

    // At block 1 the contract cannot exist — must error with ContractNotPresent.
    let pre_result = client
        .get_contract_state_at(&addr, Some(pre_deploy_hash))
        .await;
    let err = pre_result.expect_err("expected ContractNotPresent at pre-deploy block 1, got Ok");
    assert_contract_not_present_error(err.as_ref());

    tracing::info!("✓ block 1 → ContractNotPresent; current head → deployed state");
}

// ============================================================================
// midnight_queryContractState RPC: lazy path-based contract state access
// ============================================================================

/// Query path [0][1] on the deployed test contract — the counter Cell
/// initialised to `0u64` — and assert the deserialised value matches.
#[e2e_test]
async fn query_contract_state_returns_expected_value() {
    use midnight_node_ledger_helpers::{DefaultDB, StateValue, deserialize_untagged};
    use midnight_node_res::undeployed::transactions::{CONTRACT_ADDR, DEPLOY_TX};

    let _deploy_guard = wait_before_deploying().await;

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    // Deploy the test contract and wait for in-block inclusion before querying.
    // If a prior run already deployed it, the resubmit will surface a rejection;
    // log and proceed because the on-chain state we need is already there.
    if let Err(e) = client.submit_expecting_success(DEPLOY_TX.to_vec()).await {
        tracing::info!("DEPLOY_TX did not confirm (likely already deployed): {e}");
    }
    let contract_address =
        String::from_utf8(CONTRACT_ADDR.to_vec()).expect("CONTRACT_ADDR should be valid UTF-8");

    // The test contract's state after deployment is:
    //   Array(1) [ Array(3) [ MerkleTree(10), Cell(0u64), Map{...} ] ]
    //
    // Query path [0][1] to reach the counter Cell initialized to 0.
    let key_0 = PathKey(AlignedValue::from(0u8));
    let key_1 = PathKey(AlignedValue::from(1u8));

    let results = client
        .query_contract_state(
            &contract_address,
            vec![RpcStateQuery {
                path: vec![key_0, key_1],
            }],
        )
        .await
        .expect("RPC failed");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].error, None);

    let value_hex = results[0]
        .value
        .as_ref()
        .expect("field [0][1] should exist");
    let value_bytes = hex::decode(value_hex).expect("invalid hex in value");
    let state_value: StateValue<DefaultDB> =
        deserialize_untagged(&mut &value_bytes[..]).expect("failed to deserialize StateValue");

    let expected = StateValue::<DefaultDB>::from(0u64);
    assert_eq!(state_value, expected);
}

/// Three queries in one RPC call covering the three result kinds: a leaf
/// `Cell` ([0][1]), a map hit returning `Null` ([0][2][key]), and an
/// out-of-bounds error ([0][99]).
#[e2e_test]
async fn query_contract_state_batch_processes_all_queries() {
    use midnight_node_ledger_helpers::{DefaultDB, StateValue, deserialize_untagged};
    use midnight_node_res::undeployed::transactions::{CONTRACT_ADDR, DEPLOY_TX};

    let _deploy_guard = wait_before_deploying().await;

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    // See sibling test for rationale on tolerating an "already deployed" error.
    if let Err(e) = client.submit_expecting_success(DEPLOY_TX.to_vec()).await {
        tracing::info!("DEPLOY_TX did not confirm (likely already deployed): {e}");
    }
    let contract_address =
        String::from_utf8(CONTRACT_ADDR.to_vec()).expect("CONTRACT_ADDR should be valid UTF-8");

    let key = |v: u8| PathKey(AlignedValue::from(v));

    // The test contract's map at [0][2] has one entry with key "820140c20141"
    // (a compound AlignedValue: boolean(true) + field(0)) and value Null.
    // Reconstruct the AlignedValue from the hand-crafted untagged bytes; the
    // PathKey wrapper handles the tagged-hex wire encoding on serialize.
    let map_key = {
        use midnight_node_ledger_helpers::midnight_serialize::Deserializable;
        let bytes = hex::decode("820140c20141").unwrap();
        let mut reader: &[u8] = &bytes;
        PathKey(AlignedValue::deserialize(&mut reader, 0).expect("compound AlignedValue"))
    };

    let results = client
        .query_contract_state(
            &contract_address,
            vec![
                RpcStateQuery {
                    path: vec![key(0), key(1)],
                },
                RpcStateQuery {
                    path: vec![key(0), key(2), map_key],
                },
                RpcStateQuery {
                    path: vec![key(0), key(99)],
                },
            ],
        )
        .await
        .expect("RPC failed");

    assert_eq!(results.len(), 3);

    // [0][1]: counter Cell initialized to 0
    assert_eq!(results[0].error, None);
    let value_bytes = hex::decode(results[0].value.as_ref().unwrap()).unwrap();
    let cell: StateValue<DefaultDB> = deserialize_untagged(&mut &value_bytes[..]).unwrap();
    assert_eq!(cell, StateValue::from(0u64));

    // [0][2][map_key]: map hit → Null
    assert_eq!(results[1].error, None);
    let value_bytes = hex::decode(results[1].value.as_ref().unwrap()).unwrap();
    let map_value: StateValue<DefaultDB> = deserialize_untagged(&mut &value_bytes[..]).unwrap();
    assert_eq!(map_value, StateValue::Null);

    // [0][99]: array out of bounds → error
    assert_eq!(results[2].value, None);
    assert!(
        results[2].error.as_ref().unwrap().contains("out of bounds"),
        "expected out-of-bounds error, got: {:?}",
        results[2].error
    );
}

/// Queries a well-formed but never-deployed contract address (`"00" * 35`,
/// distinct from `CONTRACT_ADDR`) — the RPC must surface a descriptive
/// "Unable to query contract state" error rather than returning an empty
/// result. Independent of deploy ordering, so no guard is held.
#[e2e_test]
async fn query_contract_state_nonexistent_contract() {
    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    // 32-byte (64-hex-char) zero address: well-formed but no contract deployed.
    let fake_address = "00".repeat(32);
    let err = client
        .query_contract_state(
            &fake_address,
            vec![RpcStateQuery {
                path: vec![PathKey(AlignedValue::from(0u8))],
            }],
        )
        .await
        .expect_err("should fail for a non-existent contract");

    assert!(
        err.to_string().contains("Contract not present"),
        "unexpected error: {err}"
    );
}
