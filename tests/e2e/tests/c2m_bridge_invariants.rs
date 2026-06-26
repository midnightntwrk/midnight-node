//! E8 — cross-chain bridge pool invariants (#1773 / #1779).
//!
//! An independent **state monitor** + a **cooperative seeded flood** on local-env.
//! Invariants are state predicates over the aggregate pool balances, so the monitor
//! needs no per-transaction knowledge:
//!
//! * Midnight pools (`M.R` reserve / `M.L` locked / `M.U` unlocked) via the toolkit's
//!   `night_pools()` genesis replay (`show-night-pools`).
//! * Cardano pools: `C.L` = Σ cNIGHT at the ICS validator, `C.R` = Σ cNIGHT at the
//!   Reserve validator (both via ogmios), and `C.U = minted_total − C.L − C.R` where
//!   `minted_total` is read from kupo.
//!
//! Relations (from *Cross-Chain Token Invariants v2*), with the subminimal wait-pool
//! `W` (cNIGHT locked on Cardano but not yet reflected on Midnight):
//!   * continuous (Cardano leads, Midnight reflects after observation):
//!     `M.U ≤ C.L`, `C.U ≤ M.L`, `C.R ≤ M.R`, `M.U + C.U ≤ S`;
//!   * at quiescence (everything observed): `C.R == M.R`, `C.L == M.U + W`,
//!     `C.U == M.L − W`.
//!
//! **Requires a clean local-env** (the Part A cNIGHT genesis seeding, #1778, with no
//! prior cNIGHT *minting* activity): the mint-based functional flows in `c2m_bridge.rs`
//! inflate `C.U` past `M.L` under the infinite-mint test policy, which permanently
//! breaks the supply-bound invariants on a shared chain. Run this suite first / on a
//! dedicated local-env. It is serialized behind `C2M_BRIDGE_SERIAL` so it never races
//! the functional bridge tests. Local-env only (fast finality + fresh genesis).

use midnight_node_e2e::api::cardano::{BridgeTransferRecipient, CardanoClient};
use midnight_node_e2e::api::kupo::KupoClient;
use midnight_node_e2e::api::midnight::MidnightClient;
use midnight_node_e2e::config::{self, Settings};
use midnight_node_e2e::e2e_test;
use midnight_node_metadata::midnight_metadata_latest as mn_meta;
use midnight_node_toolkit::commands::show_night_pools::read_night_pools;
use midnight_node_toolkit::tx_generator::source::Source;
use ogmios_client::types::OgmiosUtxo;
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::time::Duration;
use tokio::time::sleep;

use crate::c2m_bridge::{
    approve_mc_tx_hash_via_governance, lock_c2m_bridge_serial, read_subminimal_flush_threshold,
    set_subminimal_threshold_via_governance,
};
use crate::global_faucet_manager;

/// Total NIGHT supply S, in STARS (1 NIGHT = 1_000_000 STARS).
const S_STARS: u128 = 24_000_000_000 * 1_000_000;

/// Subminimal flush threshold (STARS) we set for the flood, low enough that each
/// subminimal deposit flushes promptly so the wait-pool returns to 0 at quiescence.
const FLOOD_FLUSH_THRESHOLD_STARS: u64 = 500;

/// A single subminimal deposit (< `c_to_m_bridge_min_amount` = 1000 STARS).
const SUBMINIMAL_STARS: u64 = 999;

/// Deterministic RNG seed for the flood (reproducibility).
const FLOOD_RNG_SEED: u64 = 0xC2C_1779;

/// ADA funded to the flood wallet for fees + per-lock min-UTxO. Each lock sends
/// ~1.5 ADA to ICS plus a small fee, so this covers a few hundred locks.
const FLOOD_WALLET_ADA: u64 = 900_000_000;

/// The three pools of a single chain, in STARS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pools {
    /// Reserve.
    r: u128,
    /// Locked.
    l: u128,
    /// Unlocked / circulating.
    u: u128,
}

/// A cross-chain snapshot: both chains' pools plus the subminimal wait-pool.
#[derive(Debug, Clone, Copy)]
struct Snapshot {
    midnight: Pools,
    cardano: Pools,
    /// Subminimal cNIGHT locked on Cardano but not yet flushed/reflected on Midnight.
    waitpool: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Mid-flood: only the directional inequalities are guaranteed.
    Continuous,
    /// Settled: strict equalities (accounting for the wait-pool) also hold.
    Quiescent,
}

/// Pure invariant check — returns `Err(reason)` on the first violated relation.
/// Kept non-panicking so the quiescence poll can use it and the negative control
/// can assert it reports a violation.
fn check_cross_chain_invariants(s: &Snapshot, phase: Phase) -> Result<(), String> {
    let (m, c, w) = (s.midnight, s.cardano, s.waitpool);

    // Supply bound — always.
    if m.u + c.u > S_STARS {
        return Err(format!("M.U + C.U ({} + {}) > S ({})", m.u, c.u, S_STARS));
    }
    // Directional inequalities — always (Cardano leads, Midnight reflects later).
    if m.u > c.l {
        return Err(format!("M.U ({}) > C.L ({})", m.u, c.l));
    }
    if c.u > m.l {
        return Err(format!("C.U ({}) > M.L ({})", c.u, m.l));
    }
    if c.r > m.r {
        return Err(format!("C.R ({}) > M.R ({})", c.r, m.r));
    }

    if phase == Phase::Quiescent {
        if c.r != m.r {
            return Err(format!("quiescent: C.R ({}) != M.R ({})", c.r, m.r));
        }
        // C.L mirrors M.U plus whatever is still waiting in the subminimal pool.
        if c.l != m.u + w {
            return Err(format!(
                "quiescent: C.L ({}) != M.U + W ({} + {})",
                c.l, m.u, w
            ));
        }
        if c.u + w != m.l {
            return Err(format!(
                "quiescent: C.U + W ({} + {}) != M.L ({})",
                c.u, w, m.l
            ));
        }
    }
    Ok(())
}

#[track_caller]
fn assert_cross_chain_invariants(s: &Snapshot, phase: Phase) {
    if let Err(reason) = check_cross_chain_invariants(s, phase) {
        panic!("cross-chain invariant violated ({phase:?}): {reason}\n  snapshot = {s:?}");
    }
}

/// Sum of cNIGHT (under `policy_id`) held across `utxos`.
fn cnight_in_utxos(utxos: &[OgmiosUtxo], policy_id: &str) -> u128 {
    utxos
        .iter()
        .map(|u| {
            u.value
                .native_tokens
                .iter()
                .filter(|(pid, _)| hex::encode(pid) == policy_id)
                .flat_map(|(_, assets)| assets.iter())
                .map(|a| a.amount as u128)
                .sum::<u128>()
        })
        .sum()
}

/// Read both chains' pools + the subminimal wait-pool into a [`Snapshot`].
#[allow(clippy::too_many_arguments)]
async fn read_pools(
    midnight: &MidnightClient,
    cardano: &CardanoClient,
    kupo: &KupoClient,
    src_url: &str,
    ledger_state_db: &str,
    ics_address: &str,
    reserve_address: &str,
    policy_id: &str,
) -> Snapshot {
    // ----- Midnight (M.R / M.L / M.U) via genesis replay -----
    let source = Source {
        src_url: Some(src_url.to_string()),
        fetch_concurrency: crate::fetch_concurrency(),
        fetch_compute_concurrency: None,
        src_files: None,
        dust_warp: false,
        ignore_block_context: false,
        fetch_only_cached: false,
        fetch_cache: crate::fetch_cache_config(),
        ledger_state_db: ledger_state_db.to_string(),
    };
    let np = read_night_pools(source)
        .await
        .expect("read_night_pools (Midnight pool replay) failed");
    let m_r = np.reserve;
    let m_l = np.locked;
    // Unlocked = everything that isn't reserved or locked (treasury + utxos + rewards + …).
    let m_u = S_STARS
        .checked_sub(m_r + m_l)
        .expect("reserve + locked exceed total supply");

    // ----- Cardano (C.L / C.R via ogmios, C.U via kupo) -----
    let c_l = cnight_in_utxos(&cardano.query_utxos(ics_address).await, policy_id);
    let c_r = cnight_in_utxos(&cardano.query_utxos(reserve_address).await, policy_id);
    let minted_total = kupo
        .cnight_total(policy_id)
        .await
        .expect("kupo cnight_total failed");
    let c_u = minted_total
        .checked_sub(c_l + c_r)
        .expect("C.L + C.R exceed minted cNIGHT total");

    let waitpool = read_waitpool(midnight)
        .await
        .expect("read subminimal wait-pool failed");

    Snapshot {
        midnight: Pools {
            r: m_r,
            l: m_l,
            u: m_u,
        },
        cardano: Pools {
            r: c_r,
            l: c_l,
            u: c_u,
        },
        waitpool,
    }
}

/// Accumulated subminimal cNIGHT (STARS) sitting in `C2MBridge::SubminimalTransfers`.
async fn read_waitpool(
    midnight: &MidnightClient,
) -> Result<u128, Box<dyn std::error::Error + Send + Sync>> {
    let addr = mn_meta::storage().c2m_bridge().subminimal_transfers();
    let sum = match midnight
        .online_client()
        .at_current_block()
        .await?
        .storage()
        .try_fetch(&addr, ())
        .await?
    {
        Some(value) => value.decode()?.sum as u128,
        None => 0,
    };
    Ok(sum)
}

/// What disposition a flood deposit should get on Midnight.
#[derive(Debug, Clone, Copy)]
enum Disposition {
    /// Valid recipient, hash pre-approved → `UserTransfer` → `DistributeNight`.
    Approved,
    /// Valid recipient, hash not approved → `UnapprovedTransfer` → `UnlockToTreasury`.
    Unapproved,
    /// Malformed recipient metadata → `InvalidTransfer` → `UnlockToTreasury`.
    Invalid,
    /// Below `c_to_m_bridge_min_amount` → accumulates, then flushes to Treasury.
    Subminimal,
}

/// One planned flood deposit.
struct FloodStep {
    disposition: Disposition,
    amount: u64,
    recipient: [u8; 32],
}

/// Build the deterministic flood plan: a diverse mix of dispositions with seeded
/// amounts and rotating recipients. Every disposition is represented.
fn build_flood_plan() -> Vec<FloodStep> {
    let mut rng = StdRng::seed_from_u64(FLOOD_RNG_SEED);
    // A few distinct recipients (rotated) so the mix isn't single-target.
    let recipients: [[u8; 32]; 3] = [[0x11; 32], [0x22; 32], [0x33; 32]];
    let dispositions = [
        Disposition::Approved,
        Disposition::Unapproved,
        Disposition::Subminimal,
        Disposition::Invalid,
        Disposition::Approved,
        Disposition::Subminimal,
    ];
    dispositions
        .into_iter()
        .enumerate()
        .map(|(i, disposition)| {
            let amount = match disposition {
                Disposition::Subminimal => SUBMINIMAL_STARS,
                // Above the 1000-STAR minimum; varied per step.
                _ => rng.random_range(40_000_000..100_000_000),
            };
            FloodStep {
                disposition,
                amount,
                recipient: recipients[i % recipients.len()],
            }
        })
        .collect()
}

/// Find the (single) UTxO at `client`'s address that carries cNIGHT — the chained
/// input for the next cooperative lock.
async fn find_cnight_utxo(client: &CardanoClient, policy_id: &str) -> OgmiosUtxo {
    client
        .utxos()
        .await
        .into_iter()
        .find(|u| cnight_in_utxos(std::slice::from_ref(u), policy_id) > 0)
        .expect("flood wallet holds no cNIGHT UTxO")
}

#[e2e_test]
async fn cross_chain_invariants_hold_under_flood() {
    let _serial = lock_c2m_bridge_serial().await;
    let settings = Settings::default();
    let midnight = MidnightClient::new(settings.node_client.clone()).await;
    let funded =
        CardanoClient::new_from_funded(settings.ogmios_client.clone(), settings.constants.clone())
            .await;
    let kupo = KupoClient::new(settings.kupo_client.base_url.clone());

    let src_url = midnight.base_url().to_string();
    let policy = config::cnight_token_policy_id();
    let ics = midnight
        .ics_validator_address()
        .await
        .expect("read ICS validator address");
    let reserve = midnight
        .reserve_validator_address()
        .await
        .expect("read reserve validator address");

    // Per-suite ledger cache so repeated Midnight replays are incremental.
    let tmp = tempfile::tempdir().expect("create tempdir");
    let ledger_db = tmp
        .path()
        .join("invariant_ledger_cache")
        .to_string_lossy()
        .to_string();

    let snapshot = |label: &'static str| {
        let (m, c, k, u, db, i, r, p) = (
            &midnight, &funded, &kupo, &src_url, &ledger_db, &ics, &reserve, &policy,
        );
        async move {
            let s = read_pools(m, c, k, u, db, i, r, p).await;
            tracing::info!(
                "{label}: M(R={} L={} U={}) C(R={} L={} U={}) W={}",
                s.midnight.r,
                s.midnight.l,
                s.midnight.u,
                s.cardano.r,
                s.cardano.l,
                s.cardano.u,
                s.waitpool,
            );
            s
        }
    };

    // ----- assert @ genesis (clean seeded mirror) -----
    let genesis = snapshot("genesis").await;
    assert_cross_chain_invariants(&genesis, Phase::Quiescent);

    // Lower the subminimal flush threshold so subminimal deposits flush promptly.
    if read_subminimal_flush_threshold(&midnight)
        .await
        .expect("read subminimal flush threshold")
        != FLOOD_FLUSH_THRESHOLD_STARS
    {
        set_subminimal_threshold_via_governance(&midnight, FLOOD_FLUSH_THRESHOLD_STARS)
            .await
            .expect("set subminimal flush threshold via governance");
    }

    // ----- set up the cooperative flood wallet -----
    // A dedicated wallet W funded with ADA (fees) and a chunk of the seeded circulating
    // cNIGHT. Moving cNIGHT to W keeps it circulating (C.U unchanged) and avoids any
    // contention on the shared funded address (the faucet never touches the cNIGHT UTxO).
    let flood =
        CardanoClient::new(settings.ogmios_client.clone(), settings.constants.clone()).await;
    let flood_addr = flood.address_as_bech32();
    global_faucet_manager()
        .await
        .request_tokens(&flood_addr, FLOOD_WALLET_ADA)
        .await;

    let funded_cnight = find_cnight_utxo(&funded, &policy).await;
    let move_tx = funded
        .spend_cnight(&funded_cnight, &flood_addr)
        .await
        .expect("move seeded cNIGHT to flood wallet");
    funded
        .find_utxo_by_tx_id(&flood_addr, hex::encode(move_tx.transaction.id))
        .await
        .expect("flood wallet never received cNIGHT");
    tracing::info!("flood wallet {flood_addr} funded with ADA + circulating cNIGHT");

    // The first lock spends both the cNIGHT UTxO and the ADA UTxO; afterwards the single
    // change UTxO (cNIGHT + ADA) chains into each subsequent lock.
    let mut inputs: Vec<OgmiosUtxo> = flood.utxos().await;

    let plan = build_flood_plan();
    let wave_size = 3;
    for (wave_idx, wave) in plan.chunks(wave_size).enumerate() {
        for step in wave {
            let recipient = match step.disposition {
                Disposition::Invalid => BridgeTransferRecipient::Invalid,
                _ => BridgeTransferRecipient::Address(step.recipient),
            };
            let prepared = flood
                .make_cooperative_bridge_transfer(&inputs, &ics, step.amount, recipient)
                .await
                .expect("build cooperative bridge transfer");

            if matches!(step.disposition, Disposition::Approved) {
                approve_mc_tx_hash_via_governance(&midnight, prepared.tx_id)
                    .await
                    .expect("pre-approve bridge tx hash via governance");
            }

            funded
                .submit_tx(prepared.signed_tx_bytes)
                .await
                .expect("submit cooperative bridge transfer");
            tracing::info!(
                "wave {wave_idx}: submitted {:?} amount={} tx={}",
                step.disposition,
                step.amount,
                hex::encode(prepared.tx_id),
            );

            // Wait for the change to confirm, then chain it as the next input.
            let change = funded
                .find_utxos_by_tx_id(&flood_addr, hex::encode(prepared.tx_id))
                .await;
            inputs = vec![
                change
                    .into_iter()
                    .find(|u| cnight_in_utxos(std::slice::from_ref(u), &policy) > 0)
                    .expect("cooperative transfer produced no cNIGHT change"),
            ];
        }

        // Mid-flood: only the directional inequalities are guaranteed (Cardano leads).
        let s = snapshot("post-wave").await;
        assert_cross_chain_invariants(&s, Phase::Continuous);
    }

    // ----- drive to quiescence -----
    // Poll until the bridge has observed every deposit and the pools satisfy the strict
    // quiescent relations (wait-pool accounted), stable across two consecutive reads.
    let end = drive_to_quiescence(&snapshot).await;
    assert_cross_chain_invariants(&end, Phase::Quiescent);
    tracing::info!("cross-chain invariants hold at genesis, through the flood, and at quiescence");
}

/// Poll `snapshot` until the quiescent relations hold for two consecutive reads, or
/// fail loudly after the budget. Returns the final (quiescent) snapshot.
async fn drive_to_quiescence<F, Fut>(snapshot: &F) -> Snapshot
where
    F: Fn(&'static str) -> Fut,
    Fut: std::future::Future<Output = Snapshot>,
{
    const POLL: Duration = Duration::from_secs(8);
    const BUDGET: Duration = Duration::from_secs(240);
    let start = std::time::Instant::now();
    let mut consecutive_ok = 0;
    let mut last = snapshot("quiescing").await;
    loop {
        let s = snapshot("quiescing").await;
        last = s;
        if check_cross_chain_invariants(&s, Phase::Quiescent).is_ok() {
            consecutive_ok += 1;
            if consecutive_ok >= 2 {
                return s;
            }
        } else {
            consecutive_ok = 0;
        }
        if start.elapsed() > BUDGET {
            // Surface the precise unmet relation.
            assert_cross_chain_invariants(&last, Phase::Quiescent);
            return last;
        }
        sleep(POLL).await;
    }
}

/// Negative control (manual): minting fresh cNIGHT with no matching lock inflates the
/// circulating Cardano pool (C.U) beyond the Midnight locked pool (M.L), so the monitor
/// must report a violation. Proves the monitor fails when it should.
///
/// `#[ignore]` — it irreversibly perturbs local-env state (extra cNIGHT in circulation),
/// so it is run by hand on a dedicated/fresh instance, not as part of the suite.
#[e2e_test]
#[ignore = "manual: irreversibly mints extra cNIGHT to prove the monitor catches a breach"]
async fn negative_control_extra_mint_breaks_invariants() {
    let _serial = lock_c2m_bridge_serial().await;
    let settings = Settings::default();
    let midnight = MidnightClient::new(settings.node_client.clone()).await;
    let cardano =
        CardanoClient::new_from_funded(settings.ogmios_client.clone(), settings.constants.clone())
            .await;
    let kupo = KupoClient::new(settings.kupo_client.base_url.clone());

    let src_url = midnight.base_url().to_string();
    let policy = config::cnight_token_policy_id();
    let ics = midnight.ics_validator_address().await.expect("ICS address");
    let reserve = midnight
        .reserve_validator_address()
        .await
        .expect("reserve address");
    let tmp = tempfile::tempdir().expect("tempdir");
    let ledger_db = tmp.path().join("neg_ledger").to_string_lossy().to_string();

    let before = read_pools(
        &midnight, &cardano, &kupo, &src_url, &ledger_db, &ics, &reserve, &policy,
    )
    .await;
    assert_cross_chain_invariants(&before, Phase::Continuous);

    // Mint a large slug of fresh cNIGHT (no lock) — pushes C.U above M.L.
    let collateral = global_faucet_manager()
        .await
        .request_tokens(&cardano.address_as_bech32(), 5_000_000)
        .await;
    cardano
        .mint_tokens(S_STARS as u64, &collateral)
        .await
        .expect("mint extra cNIGHT");
    // Give kupo a moment to index the mint.
    sleep(Duration::from_secs(10)).await;

    let after = read_pools(
        &midnight, &cardano, &kupo, &src_url, &ledger_db, &ics, &reserve, &policy,
    )
    .await;
    tracing::info!(?after, "post-mint snapshot");
    let result = check_cross_chain_invariants(&after, Phase::Continuous);
    assert!(
        result.is_err(),
        "negative control: monitor should have reported a violation after an unmatched \
         cNIGHT mint, but invariants still held: {after:?}"
    );
    tracing::info!("negative control: monitor correctly flagged the breach: {result:?}");
}
