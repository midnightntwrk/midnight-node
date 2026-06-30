use midnight_node_e2e::api::midnight::MidnightClient;
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
/// Uses CONTRACT_ADDR (the address DEPLOY_TX deploys to) so we know the
/// address itself parses; the only reason for failure is "no contract here".
/// Pre-deploy gated so it runs before any DEPLOY_TX submission.
#[e2e_test]
async fn contract_state_for_undeployed_address_returns_not_present() {
    let _pre_deploy_guard = PreDeployGuard::new();
    use midnight_node_res::undeployed::transactions::CONTRACT_ADDR;

    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    let addr = std::str::from_utf8(CONTRACT_ADDR)
        .expect("CONTRACT_ADDR is ASCII hex")
        .trim();

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
    use midnight_node_ledger_helpers::extract_tx_with_context;
    use midnight_node_toolkit::commands::contract_address::{self, ContractAddressArgs};
    use midnight_node_toolkit::commands::generate_txs::{self, GenerateTxsArgs};
    use midnight_node_toolkit::tx_generator::builder::{Builder, ContractCall, ContractDeployArgs};
    use midnight_node_toolkit::tx_generator::destination::Destination;
    use midnight_node_toolkit::tx_generator::source::Source;

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
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let deploy_file = tempdir.path().join("contract_deploy.mn");
    let deploy_file_str = deploy_file.to_string_lossy().to_string();

    tracing::info!("Generating fresh DEPLOY_TX against live chain at {url}...");
    let gen_args = GenerateTxsArgs {
        builder: Builder::ContractSimple(ContractCall::Deploy(ContractDeployArgs {
            funding_seed: "0000000000000000000000000000000000000000000000000000000000000001"
                .to_string(),
            authority_seeds: vec![],
            authority_threshold: None,
            rng_seed: None,
        })),
        source: Source {
            src_url: Some(url.clone()),
            fetch_concurrency: crate::fetch_concurrency(),
            fetch_compute_concurrency: None,
            src_files: None,
            dust_warp: false,
            ignore_block_context: false,
            fetch_only_cached: false,
            fetch_cache: crate::fetch_cache_config(),
            ledger_state_db: String::new(),
        },
        destination: Destination {
            dest_urls: vec![],
            rate: 1.0,
            dest_file: Some(deploy_file_str.clone()),
            no_watch_progress: true,
        },
        proof_server: None,
        dry_run: false,
    };
    generate_txs::execute(gen_args)
        .await
        .expect("generate-txs contract-simple deploy failed");

    let addr = contract_address::execute(ContractAddressArgs {
        src_file: deploy_file_str.clone(),
        tagged: false,
        untagged: false,
    })
    .expect("extract contract address from deploy tx");
    tracing::info!("Contract address (dynamic): {addr}");

    let deploy_bytes = std::fs::read(&deploy_file).expect("read generated deploy tx file");
    let (deploy_tx_bytes, _block_context) = extract_tx_with_context(&deploy_bytes);

    tracing::info!("Submitting DEPLOY_TX...");
    let mut progress = client
        .submit_midnight_tx(deploy_tx_bytes)
        .await
        .expect("DEPLOY_TX submission rejected by RPC");

    let post_deploy_hash = timeout(Duration::from_secs(60), async {
        while let Some(status) = progress.next().await {
            match status {
                Ok(subxt::tx::TransactionStatus::InBestBlock(info)) => {
                    tracing::info!("  DEPLOY_TX in best block: {:?}", info.block_hash());
                    return Ok::<_, String>(info.block_hash());
                }
                Ok(subxt::tx::TransactionStatus::InFinalizedBlock(info)) => {
                    tracing::info!("  DEPLOY_TX finalized: {:?}", info.block_hash());
                    return Ok(info.block_hash());
                }
                Ok(subxt::tx::TransactionStatus::Invalid { message })
                | Ok(subxt::tx::TransactionStatus::Dropped { message })
                | Ok(subxt::tx::TransactionStatus::Error { message }) => {
                    return Err(format!("DEPLOY_TX terminated without inclusion: {message}"));
                }
                Ok(other) => tracing::info!("  status: {other:?}"),
                Err(e) => return Err(format!("progress error: {e}")),
            }
        }
        Err("progress stream ended without confirmation".to_string())
    })
    .await
    .expect("DEPLOY_TX did not reach a terminal status within 60s")
    .expect("DEPLOY_TX failed to land in a block");

    let post_deploy_state = client
        .get_contract_state_at(&addr, Some(post_deploy_hash))
        .await
        .expect("expected deployed state at post-deploy block, got Err");
    assert!(
        !post_deploy_state.is_empty(),
        "deployed contract state should be non-empty"
    );
    tracing::info!(
        "Contract present at post-deploy block {:?} ({} hex chars of state)",
        post_deploy_hash,
        post_deploy_state.len()
    );

    // At block 1 the contract cannot exist — must error with ContractNotPresent.
    let pre_result = client
        .get_contract_state_at(&addr, Some(pre_deploy_hash))
        .await;
    let err = pre_result.expect_err("expected ContractNotPresent at pre-deploy block 1, got Ok");
    assert_contract_not_present_error(err.as_ref());

    tracing::info!(
        "✓ block 1 → ContractNotPresent; block {:?} → deployed state",
        post_deploy_hash
    );
}
