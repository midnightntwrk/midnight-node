//! Local-env wallet seeding.
//!
//! Funds the well-known dev wallet (seed `0x..01`) with NIGHT by driving the
//! real cNIGHT→NIGHT bridge end-to-end against a running local-env stack. This
//! is the "Part B" companion to the #1778 genesis cNIGHT seeding: the `local`
//! network ships an *unfunded* genesis, so without this every wallet starts
//! empty.
//!
//! Unlike the c2m_bridge tests it does NOT mint fresh cNIGHT — it **spends the
//! seeded faucet cNIGHT** (the circulating pool minted by the `cnight-seeder`
//! step). Each transfer only moves cNIGHT faucet→ICS (increasing `C.L`), so
//! #1778's pools and the `M.U ≤ C.L` invariant are preserved.
//!
//! The bridge transfer's Cardano tx hash is pre-approved through the local-env
//! governance flow (council + technical-committee `root_call`) before it is
//! submitted, so the pallet treats it as an approved user transfer (not an
//! `UnapprovedTransfer` swept to Treasury). The final `ClaimRewards(CardanoBridge)`
//! is feeless and self-signed by the seed, so the otherwise-empty wallet can
//! claim its bridged NIGHT with no pre-existing balance or DUST.
//!
//! Gated behind the `local-env-seed` feature (built alongside `local`) so it
//! never joins the normal `cargo test` sweep; run as a one-shot compose job.

use midnight_node_e2e::api::cardano::{BridgeTransferRecipient, CardanoClient};
use midnight_node_e2e::api::midnight::MidnightClient;
use midnight_node_e2e::config::{self, Settings};
use midnight_node_e2e::e2e_test;
use midnight_node_ledger_helpers::{UnshieldedWallet, WalletSeed, extract_tx_with_context};
use midnight_node_toolkit::commands::generate_txs::{self, GenerateTxsArgs};
use midnight_node_toolkit::tx_generator::builder::{
    Builder, ClaimKindArg, ClaimRewardsArgs, RegisterDustAddressArgs,
};
use midnight_node_toolkit::tx_generator::destination::Destination;
use midnight_node_toolkit::tx_generator::source::Source;
use ogmios_client::types::OgmiosUtxo;
use std::time::Duration;
use whisky::Asset;

use crate::c2m_bridge::{
    approve_mc_tx_hash_via_governance, claimable_amount, wait_for_bridge_calls,
};
use crate::global_faucet_manager;

/// Dev wallet seed to fund. `UnshieldedWallet::default(seed).user_address` is the
/// bridge recipient; the same seed self-claims the feeless `ClaimRewards`.
const SEED_HEX: &str = "0000000000000000000000000000000000000000000000000000000000000001";

/// Amount to bridge to the wallet: 1,000,000,000 NIGHT = 1e15 STARS
/// (1 NIGHT = 1_000_000 STARS). Comfortably above `c_to_m_bridge_min_amount`
/// and a small fraction of the faucet's ~1.68e16-STARS circulating cNIGHT.
const AMOUNT_STARS: u64 = 1_000_000_000_000_000;

/// Minimum lovelace a faucet UTxO must hold to serve as the tx payment +
/// collateral input.
const MIN_PAYMENT_LOVELACE: u64 = 5_000_000;

/// Lovelace bundled with the cNIGHT UTxO moved to the spender wallet (min-UTxO
/// plus headroom for the bridge tx it feeds).
const SPENDER_CNIGHT_LOVELACE: u64 = 3_000_000;

/// Blocks to let the just-claimed NIGHT age before the self-funded DUST
/// registration, so its retroactive-DUST budget (`age * rate`) covers the reg
/// fee. Right after the claim the finalized head *is* the claim block (age 0).
const DUST_AGING_BLOCKS: u64 = 3;

/// Sum of the cNIGHT (`policy_hex`, any asset name) held by a UTxO, in STARS.
fn cnight_balance(utxo: &OgmiosUtxo, policy_hex: &str) -> u128 {
    utxo.value
        .native_tokens
        .iter()
        .filter(|(policy, _)| hex::encode(policy) == policy_hex)
        .flat_map(|(_, assets)| assets.iter())
        .map(|a| a.amount as u128)
        .sum()
}

#[e2e_test]
async fn seed_wallet() {
    let settings = Settings::default();
    // The funded faucet wallet holds the seeded cNIGHT (the cnight-seeder mints to
    // its payment key). We move the amount to bridge into a fresh spender wallet and
    // bridge from there: the funded wallet is the local-env governance authority (an
    // enterprise address), and whisky drops its witness when signing the metadata +
    // collateral bridge tx from it — whereas a plain asset `send` from the funded
    // wallet, and `make_bridge_transfer` from a fresh wallet, are exactly the paths
    // the faucet and c2m_bridge e2e tests already exercise.
    let funded =
        CardanoClient::new_from_funded(settings.ogmios_client.clone(), settings.constants.clone())
            .await;
    let spender =
        CardanoClient::new(settings.ogmios_client.clone(), settings.constants.clone()).await;
    let midnight = MidnightClient::new(settings.node_client.clone()).await;

    let seed = WalletSeed::try_from_hex_str(SEED_HEX).expect("dev seed parses");
    let recipient: [u8; 32] = UnshieldedWallet::default(seed).user_address.0.0;
    tracing::info!(recipient = %hex::encode(recipient), amount_stars = AMOUNT_STARS, "seeding dev wallet via c2m bridge");

    // Move `AMOUNT_STARS` cNIGHT from the funded faucet to the fresh spender wallet.
    let cnight_policy = config::cnight_token_policy_id();
    let funded_addr = funded.address_as_bech32();
    let utxos = funded.query_utxos(&funded_addr).await;

    let faucet_cnight = utxos
        .iter()
        .find(|u| cnight_balance(u, &cnight_policy) >= AMOUNT_STARS as u128)
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "no faucet cNIGHT UTxO with >= {AMOUNT_STARS} STARS under policy {cnight_policy} \
                 at {funded_addr} — is the cnight-seeder step done?"
            )
        });
    let funding_ada = utxos
        .iter()
        .filter(|u| u.value.native_tokens.is_empty() && u.value.lovelace >= MIN_PAYMENT_LOVELACE)
        .max_by_key(|u| u.value.lovelace)
        .cloned()
        .expect("no pure-ADA faucet UTxO to fund the cNIGHT transfer");

    let spender_addr = spender.address_as_bech32();
    let cnight_utxo = funded
        .fund_wallet(
            &[faucet_cnight, funding_ada],
            &spender_addr,
            vec![
                Asset::new_from_str("lovelace", &SPENDER_CNIGHT_LOVELACE.to_string()),
                Asset::new_from_str(&cnight_policy, &AMOUNT_STARS.to_string()),
            ],
        )
        .await
        .expect("cNIGHT transfer to the spender produced no UTxO");
    tracing::info!(spender = %spender_addr, "moved cNIGHT to fresh spender wallet");

    // A pure-ADA UTxO at the spender for the bridge tx payment + collateral.
    let payment_utxo = global_faucet_manager()
        .await
        .request_tokens(&spender_addr, MIN_PAYMENT_LOVELACE)
        .await;

    let ics_address = midnight
        .ics_validator_address()
        .await
        .expect("read ICS validator address from Bridge pallet storage");

    // Build + sign (not submit) the bridge transfer from the spender wallet.
    let prepared = spender
        .make_bridge_transfer(
            &cnight_utxo,
            &payment_utxo,
            &ics_address,
            AMOUNT_STARS,
            BridgeTransferRecipient::Address(recipient),
        )
        .await
        .expect("build bridge transfer");
    let bridge_tx = prepared.tx_id;
    tracing::info!(bridge_tx = %hex::encode(bridge_tx), "bridge transfer signed (not yet submitted)");

    // Pre-approve the Cardano tx hash via governance so the transfer is treated
    // as approved (not swept to Treasury) once observed.
    approve_mc_tx_hash_via_governance(&midnight, bridge_tx)
        .await
        .expect("pre-approve bridge tx hash via governance");
    tracing::info!("bridge tx hash pre-approved on Midnight");

    let min_block = midnight
        .get_finalized_block_number()
        .await
        .expect("read finalized head before submitting Cardano tx");
    spender
        .submit_tx(prepared.signed_tx_bytes)
        .await
        .expect("submit bridge transfer to Cardano");
    tracing::info!(
        min_block,
        "bridge transfer submitted on Cardano; awaiting observation"
    );

    wait_for_bridge_calls(&midnight, min_block).await;

    // Compute the post-fee claimable amount, then claim it.
    let params = midnight
        .get_ledger_parameters()
        .await
        .expect("read ledger parameters for bridge-fee computation");
    let claimable = claimable_amount(
        AMOUNT_STARS as u128,
        params.cardano_to_midnight_bridge_fee_basis_points,
        params.c_to_m_bridge_min_amount,
    );
    assert!(
        claimable > 0,
        "post-fee claimable must be positive (amount={AMOUNT_STARS}, fee_bps={}, min={})",
        params.cardano_to_midnight_bridge_fee_basis_points,
        params.c_to_m_bridge_min_amount,
    );

    // ClaimRewards(CardanoBridge) is feeless and signed by the recipient seed,
    // so the empty wallet can claim its first bridged transfer directly. The
    // toolkit's in-process local prover only needs the zswap public params
    // (cached under MIDNIGHT_PP), no prover keys.
    let url = midnight.base_url().to_string();
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let claim_file = tempdir.path().join("bridge_claim.mn");
    let claim_file_str = claim_file.to_string_lossy().to_string();
    let claim_args = GenerateTxsArgs {
        builder: Builder::ClaimRewards(ClaimRewardsArgs {
            funding_seed: SEED_HEX.to_string(),
            rng_seed: None,
            amount: claimable,
            claim_kind: ClaimKindArg::CardanoBridge,
        }),
        source: Source {
            src_url: Some(url),
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
            dest_file: Some(claim_file_str),
            no_watch_progress: true,
        },
        proof_server: None,
        dry_run: false,
    };
    generate_txs::execute(claim_args)
        .await
        .expect("generate-txs claim-rewards (cardano-bridge) failed");

    let claim_bytes = std::fs::read(&claim_file).expect("read generated claim tx file");
    let (claim_tx_bytes, _block_context) = extract_tx_with_context(&claim_bytes);
    midnight
        .submit_midnight_tx(claim_tx_bytes)
        .await
        .expect("claim tx rejected by RPC at submission")
        .wait_for_finalized_success()
        .await
        .expect("ClaimRewards(CardanoBridge) extrinsic should finalize successfully");

    tracing::info!(recipient = %hex::encode(recipient), claimable, "dev wallet funded: bridged NIGHT claimed");

    // Register the wallet's DUST address so its NIGHT starts generating DUST (the
    // fee token) — otherwise the freshly-funded wallet still can't transact. On
    // this unfunded network the registration self-funds from the NIGHT's
    // retroactive DUST (`funding_seed = None`), a budget of `age * rate`. Right
    // after the claim the finalized head is the claim block (age 0, budget 0), so
    // first let the NIGHT age a few blocks. `dust_warp` stays off: the tx is
    // submitted to the live node, which validates the fee at its own clock.
    let claim_block = midnight
        .get_finalized_block_number()
        .await
        .expect("read finalized head after claim");
    loop {
        let finalized = midnight
            .get_finalized_block_number()
            .await
            .expect("poll finalized head while aging claimed NIGHT");
        if finalized >= claim_block + DUST_AGING_BLOCKS {
            break;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }

    let reg_file = tempdir.path().join("register_dust.mn");
    let reg_file_str = reg_file.to_string_lossy().to_string();
    let reg_args = GenerateTxsArgs {
        builder: Builder::RegisterDustAddress(RegisterDustAddressArgs {
            wallet_seed: SEED_HEX.to_string(),
            funding_seed: None,
            destination_dust: None,
            rng_seed: None,
        }),
        source: Source {
            src_url: Some(midnight.base_url().to_string()),
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
            dest_file: Some(reg_file_str),
            no_watch_progress: true,
        },
        proof_server: None,
        dry_run: false,
    };
    generate_txs::execute(reg_args)
        .await
        .expect("generate-txs register-dust-address failed");

    let reg_bytes = std::fs::read(&reg_file).expect("read generated register-dust tx file");
    let (reg_tx_bytes, _block_context) = extract_tx_with_context(&reg_bytes);
    midnight
        .submit_midnight_tx(reg_tx_bytes)
        .await
        .expect("register-dust tx rejected by RPC at submission")
        .wait_for_finalized_success()
        .await
        .expect("RegisterDustAddress extrinsic should finalize successfully");

    tracing::info!(
        recipient = %hex::encode(recipient),
        claimable,
        "dev wallet funded + registered for DUST generation"
    );
}
