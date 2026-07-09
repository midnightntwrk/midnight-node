use crate::api::cardano::CardanoClient;
use crate::config::{NodeClientSettings, OgmiosClientSettings};
use blake2::digest::{Update, VariableOutput};
use blake2::Blake2bVar;
use hex::ToHex;
use midnight_node_ledger_helpers::{
    DefaultDB, DustWallet, LedgerParameters, WalletSeed, deserialize, serialize_untagged,
};
use midnight_node_metadata::midnight_metadata_latest::c_night_observation::storage::utxo_owners::Output as UtxoOwners;
use midnight_node_metadata::midnight_metadata_latest::runtime_types::midnight_primitives::bridge::BridgeRecipient;
use midnight_node_metadata::midnight_metadata_latest::runtime_types::sp_partner_chains_bridge::BridgeTransferV1;
use midnight_node_metadata::midnight_metadata_latest::federated_authority_observation::events::{CouncilMembersReset, TechnicalCommitteeMembersReset};
use midnight_node_metadata::midnight_metadata_latest::runtime_types::midnight_primitives_cnight_observation::ObservedUtxo;
use midnight_node_metadata::midnight_metadata_latest::midnight_system::events::SystemTransactionApplied;
use midnight_node_metadata::midnight_metadata_latest::{
	self as mn_meta,
	c_night_observation::{self},
	bridge::{self},
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use subxt::extrinsics::ExtrinsicEvents;
use subxt::rpcs::{RpcClient, rpc_params};
use subxt::tx::TransactionProgress;
use subxt::utils::H256;
use subxt::{OnlineClient, SubstrateConfig};
use tokio::time::{sleep, timeout, Instant};

/// D-Parameter response from RPC
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DParameterResponse {
    /// Number of permissioned candidates
    pub num_permissioned_candidates: u16,
    /// Number of registered candidates
    pub num_registered_candidates: u16,
}

/// Sidechain status response from sidechain_getStatus RPC
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidechainStatusResponse {
    /// Current sidechain epoch number
    pub epoch: u64,
    /// Current slot within the epoch
    pub slot: u64,
    /// Slots per epoch configuration
    pub slots_per_epoch: u32,
    /// Slot duration in milliseconds
    #[serde(default)]
    pub slot_duration: Option<u64>,
}

/// Ariadne parameters response from systemParameters_getAriadneParameters RPC
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AriadneParametersResponse {
    /// The D-parameter (from pallet-system-parameters)
    pub d_parameter: DParameterResponse,
    /// List of permissioned candidates from Cardano Aiken contracts
    pub permissioned_candidates: Option<Vec<serde_json::Value>>,
    /// Map of candidate registrations
    pub candidate_registrations: serde_json::Value,
}

/// C-to-M bridge data from all `Bridge::handle_transfers` calls of one block.
#[derive(Debug)]
pub struct C2MBridgePalletCalls {
    /// `C2MBridge` pallet specific events emitted in the block.
    pub c2m_bridge_events: Vec<mn_meta::c2m_bridge::Event>,
    /// Transfers passed as argument to the `Bridge::handle_transfers` calls.
    pub transfers: Vec<BridgeTransferV1<BridgeRecipient>>,
    /// `MidnightSystem::SystemTransactionApplied` events emitted in the block. Potentially other than C2M bridge as well.
    pub system_transactions_applied: Vec<SystemTransactionApplied>,
}

pub struct MidnightClient {
    /// Wrapped in `RwLock` so the poll loop in `await_cnight_observations`
    /// can replace the inner client when subxt's background task dies
    /// (the "restart required" / "background task closed" error is
    /// terminal for the OnlineClient instance — every subsequent call
    /// on it fails forever, so we must rebuild it). Reads are cheap:
    /// `OnlineClient<T>` is internally Arc-wrapped, so the clone the
    /// getter returns is just an Arc bump.
    online_client: std::sync::RwLock<OnlineClient<SubstrateConfig>>,
    rpc_client: RpcClient,
    base_url: String,
}

impl MidnightClient {
    pub async fn new(node_settings: NodeClientSettings) -> Self {
        let online_client =
            OnlineClient::<SubstrateConfig>::from_insecure_url(&node_settings.base_url)
                .await
                .expect("Failed to initialize online client");
        let rpc_client = RpcClient::from_insecure_url(&node_settings.base_url)
            .await
            .expect("Failed to initialize RPC client");
        Self {
            online_client: std::sync::RwLock::new(online_client),
            rpc_client,
            base_url: node_settings.base_url,
        }
    }

    /// Get a clone of the current `OnlineClient`. Cheap — internally
    /// `Arc`-wrapped, so the clone is just an `Arc` bump.
    pub fn online_client(&self) -> OnlineClient<SubstrateConfig> {
        self.online_client.read().unwrap().clone()
    }

    /// Replace the inner `OnlineClient` with a freshly-built one. Used
    /// to recover from "background task closed" errors after a transport
    /// hiccup — those leave the previous client permanently broken.
    async fn reconnect_online_client(&self) -> Result<(), Box<dyn std::error::Error>> {
        let new_client = OnlineClient::<SubstrateConfig>::from_insecure_url(&self.base_url).await?;
        *self.online_client.write().unwrap() = new_client;
        Ok(())
    }

    pub fn new_seed() -> WalletSeed {
        let seed_bytes: [u8; 32] = rand::random();
        tracing::info!("Midnight seed: {}", hex::encode(seed_bytes));
        WalletSeed::from(seed_bytes)
    }

    pub fn new_dust_hex(wallet_seed: WalletSeed) -> String {
        let dust_wallet = DustWallet::<DefaultDB>::default(wallet_seed, None);
        let dust_public = dust_wallet.public_key;
        let mut dust_bytes = serialize_untagged(&dust_public).unwrap();
        if dust_bytes.len() == 32 {
            dust_bytes.push(0);
        }
        let dust_public_hex = dust_bytes.encode_hex::<String>();
        tracing::info!("Dust public key hex: {}", dust_public_hex);
        dust_public_hex
    }

    /// Wait until at least `cardano_blocks` Cardano blocks AND
    /// `midnight_blocks` Midnight blocks have elapsed past the moment of
    /// the call. Used by tests that submit two batches upfront and need
    /// the second batch to land in a Cardano block that's measurably
    /// past the first — sufficient that at least one Midnight
    /// `process_tokens` extrinsic fires between them, processing the
    /// first batch but not the second.
    ///
    /// Why block-based instead of time-based: Cardano block time differs
    /// per env (~20s on Preview, sub-second on local-env), and Midnight
    /// is 6s everywhere. A time-based sleep tuned for one env breaks the
    /// other. Block counts encode the requirement directly: "ensure N
    /// process_tokens-firing opportunities pass before the second
    /// submission".
    pub async fn wait_for_block_spacing(
        ogmios_settings: &OgmiosClientSettings,
        midnight_client: &MidnightClient,
        cardano_blocks: u64,
        midnight_blocks: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let cardano_start = CardanoClient::current_block_height(ogmios_settings)
            .await
            .ok_or("wait_for_block_spacing: failed to read initial Cardano tip")?;
        let midnight_start = midnight_client.get_finalized_block_number().await?;
        tracing::info!(
            "wait_for_block_spacing: waiting for {cardano_blocks} cardano + {midnight_blocks} midnight \
             blocks past (cardano #{cardano_start}, midnight #{midnight_start})"
        );
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let cardano_now = match CardanoClient::current_block_height(ogmios_settings).await {
                Some(h) => h,
                None => continue,
            };
            let midnight_now = match midnight_client.get_finalized_block_number().await {
                Ok(b) => b,
                Err(_) => continue,
            };
            let cardano_delta = cardano_now.saturating_sub(cardano_start);
            let midnight_delta = midnight_now.saturating_sub(midnight_start);
            if cardano_delta >= cardano_blocks && midnight_delta >= midnight_blocks {
                tracing::info!(
                    "wait_for_block_spacing: satisfied (cardano #{cardano_now}, +{cardano_delta}; \
                     midnight #{midnight_now}, +{midnight_delta})"
                );
                return Ok(());
            }
        }
    }

    /// Wait until Midnight has observed every Cardano tx in `tx_ids`, then
    /// return the matching `ExtrinsicEvents` (one per `process_tokens`
    /// extrinsic that carried one of the tx_ids).
    ///
    /// This is the only barrier needed for a cNIGHT observation test.
    /// The helper polls Midnight's `nextCardanoPosition` watermark at
    /// the current head every 5s until it crosses a Cardano-block
    /// target (default: tip at call time; explicit via
    /// [`Self::await_cnight_observations_at`]). Once it does,
    /// binary-search to localise the first Midnight block whose
    /// `process_tokens` crossed the target, then walk backward decoding
    /// `process_tokens` extrinsics until every tx_id is matched.
    ///
    /// Why polling instead of a `stream_blocks` subscription: long-lived
    /// subxt subscriptions are fragile on multi-hour Preview waits — any
    /// TCP retransmit timeout or middleware idle-killer kills the
    /// subscription permanently. Polling is one short RPC every 5s;
    /// transient errors (including "background task closed; restart
    /// required") trigger an automatic `OnlineClient` rebuild on the
    /// next iteration.
    pub async fn await_cnight_observations(
        &self,
        tx_ids: &[[u8; 32]],
        ogmios_settings: &OgmiosClientSettings,
        timeout_duration: Duration,
    ) -> Result<Vec<ExtrinsicEvents<SubstrateConfig>>, Box<dyn std::error::Error>> {
        // Snapshot a Cardano tip that is *guaranteed* past any tx the
        // caller just submitted (see
        // [`CardanoClient::snapshot_tip_after_advance`]). Tests that
        // submit a *second* batch upfront with
        // [`Self::wait_for_block_spacing`] snapshot the tip themselves
        // and call [`Self::await_cnight_observations_at`] with the
        // explicit target instead.
        let target = CardanoClient::snapshot_tip_after_advance(ogmios_settings)
            .await
            .ok_or("await_cnight_observations: failed to read Cardano tip for target snapshot")?;
        self.await_cnight_observations_at(tx_ids, target, ogmios_settings, timeout_duration)
            .await
    }

    /// Like [`Self::await_cnight_observations`] but with an explicit
    /// target Cardano block (instead of snapshotting the tip on call).
    /// Use this when the test needs to wait for a *subset* of its
    /// submissions — typically the spacing tests
    /// (`spend_cnight_producing_dust`,
    /// `stop_dust_producing_after_deregistration_and_rotation`) that
    /// submit a second batch upfront but want the first await to
    /// resolve before the second batch lands.
    ///
    /// The target must be a Cardano block such that every tx_id you
    /// want observed has landed at or before it. A snapshot of
    /// `CardanoClient::current_block_height` taken *after* the
    /// of-interest submissions and *before* any unwanted submissions
    /// satisfies that.
    pub async fn await_cnight_observations_at(
        &self,
        tx_ids: &[[u8; 32]],
        target: u64,
        ogmios_settings: &OgmiosClientSettings,
        timeout_duration: Duration,
    ) -> Result<Vec<ExtrinsicEvents<SubstrateConfig>>, Box<dyn std::error::Error>> {
        let total = tx_ids.len();

        // `k` is the Cardano security parameter — used for the
        // follower-lag sanity check and the heartbeat ETA display.
        let k = CardanoClient::cardano_security_parameter(ogmios_settings)
            .await
            .map(|v| v as u64)
            .unwrap_or(0);

        tracing::info!(
            "await_cnight_observations: {} tx_id(s), target_cardano_block={} (k={}): [{}]",
            total,
            target,
            k,
            tx_ids
                .iter()
                .map(|t| format!("0x{}", hex::encode(t)))
                .collect::<Vec<_>>()
                .join(", "),
        );

        // Empirical Midnight follower lag on Preview/qanet: ~30 Cardano
        // blocks beyond k. Under normal conditions the watermark trails
        // the target by ~k+30; if the initial diff is materially larger
        // we log a one-shot WARN so a stuck follower is obvious early
        // rather than after a multi-hour timeout.
        const BLOCK_STABILITY_MARGIN: u64 = 30;
        let mut sanity_check_done = false;

        // Short interval is important for tests that submit a second batch
        // a few minutes after a first batch (e.g. spend_cnight_producing_dust):
        // the helper needs to detect the first-batch `mint_target` watermark
        // crossing well *before* the second-batch `spend_target` crosses, so
        // the test has a window to read `balance_before` at the
        // mint-observed / spend-not-yet state. See `PRE_AWAIT_SUBMISSION_SPACING`
        // in `tests/lib.rs` for the spacing/poll-interval interaction.
        const POLL_INTERVAL: Duration = Duration::from_secs(5);

        // The watermark normally advances every few minutes; a longer
        // freeze means the follower (or its db-sync feed) is stuck, so fail
        // with a diagnosis now instead of burning the outer timeout.
        const WATERMARK_STALL_LIMIT: Duration = Duration::from_secs(45 * 60);
        let mut last_advance: Option<(u64, Instant)> = None;

        let inner = async {
            loop {
                // Each iteration is two short-lived RPC calls: head lookup +
                // storage fetch at that head. Transient errors (Connection
                // Closed, ETIMEDOUT, …) are caught, logged, and retried on
                // the next iteration. The outer `tokio::time::timeout`
                // bounds the total wait.
                let probe = async {
                    let head_num = self.get_finalized_block_number().await?;
                    let watermark = self.read_next_cardano_position_at(head_num).await?;
                    Ok::<_, Box<dyn std::error::Error>>((head_num, watermark))
                }
                .await;

                match probe {
                    Ok((head_num, watermark)) if watermark >= target => {
                        tracing::info!(
                            "await_cnight_observations: watermark @ midnight #{head_num} \
                             = {} >= target {}; scanning past blocks",
                            watermark,
                            target,
                        );
                        return self.scan_past_for_tx_ids(tx_ids, head_num, target).await;
                    }
                    Ok((head_num, watermark)) => {
                        // One-shot sanity check: under normal conditions the
                        // initial watermark trails the target by ~k + 30
                        // (k = Cardano stability, +30 = Midnight follower
                        // processing lag observed on Preview). Anything
                        // materially larger means the follower is lagging
                        // beyond its usual budget and the wait will be
                        // longer than expected.
                        if !sanity_check_done {
                            sanity_check_done = true;
                            if k > 0 {
                                let expected_diff = k + BLOCK_STABILITY_MARGIN;
                                let actual_diff = target.saturating_sub(watermark);
                                if actual_diff > expected_diff.saturating_mul(3) / 2 {
                                    tracing::warn!(
                                        "await_cnight_observations: Midnight follower appears to \
                                         be lagging. watermark={watermark} target={target} \
                                         diff={actual_diff}, expected ~{expected_diff} \
                                         (k={k} + ~{BLOCK_STABILITY_MARGIN} processing lag). \
                                         Test wait will exceed the usual stability window."
                                    );
                                }
                            }
                        }

                        // Stall detector (see WATERMARK_STALL_LIMIT above).
                        match last_advance {
                            Some((last_wm, _)) if watermark > last_wm => {
                                last_advance = Some((watermark, Instant::now()));
                            }
                            Some((last_wm, since)) if since.elapsed() > WATERMARK_STALL_LIMIT => {
                                return Err(format!(
                                    "await_cnight_observations: watermark stalled at {last_wm} \
                                     for {:?} (target {target}); the Midnight follower or its \
                                     db-sync feed appears stuck",
                                    since.elapsed(),
                                )
                                .into());
                            }
                            Some(_) => {}
                            None => {
                                last_advance = Some((watermark, Instant::now()));
                            }
                        }

                        // Heartbeat shows per-test progress: how far the
                        // watermark is from this test's target. The Cardano
                        // tip is included for absolute-time context but the
                        // primary signal is `target - watermark`.
                        let cardano_tip = CardanoClient::current_block_height(ogmios_settings)
                            .await
                            .map(|tip| format!(" (cardano tip={tip})"))
                            .unwrap_or_default();
                        let blocks_to_target = target.saturating_sub(watermark);
                        tracing::info!(
                            "await_cnight_observations: polling; midnight #{head_num} \
                             watermark={watermark} target={target} ({blocks_to_target} blocks \
                             behind target){cardano_tip}, 0/{total} observed",
                        );
                    }
                    Err(e) => {
                        // subxt's `OnlineClient` runs an internal background
                        // task for its websocket. When that task dies (TCP
                        // retransmit timeout, middleware idle-killer, brief
                        // outage…) every subsequent call on the same client
                        // fails forever with "background task closed;
                        // restart required". Detect that string and rebuild
                        // the client — without this we'd just log the same
                        // WARN every 5s for the remainder of the test.
                        let err_msg = e.to_string();
                        let needs_reconnect = err_msg.contains("restart required")
                            || err_msg.contains("background task closed");
                        if needs_reconnect {
                            tracing::warn!(
                                "await_cnight_observations: subxt background task closed; \
                                 rebuilding OnlineClient",
                            );
                            if let Err(reconnect_err) = self.reconnect_online_client().await {
                                tracing::warn!(
                                    "await_cnight_observations: reconnect failed (will retry in {:?}): {reconnect_err}",
                                    POLL_INTERVAL,
                                );
                            }
                        } else {
                            tracing::warn!(
                                "await_cnight_observations: poll failed (will retry in {:?}): {e}",
                                POLL_INTERVAL,
                            );
                        }
                    }
                }
                tokio::time::sleep(POLL_INTERVAL).await;
            }
        };

        match timeout(timeout_duration, inner).await {
            Ok(Ok(events)) => Ok(events),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(format!(
                "await_cnight_observations timed out after {timeout_duration:?}"
            )
            .into()),
        }
    }

    /// Read `cNightObservation.nextCardanoPosition.block_number` at a
    /// specific Midnight block (by number). Used by the past-scan path
    /// to binary-search the watermark across history.
    async fn read_next_cardano_position_at(
        &self,
        midnight_block: u64,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        // Wrap the RPC sequence in a timeout + reconnect retry loop. Without
        // this, a transient transport stall (subxt's "background task closed"
        // or a hung TCP read) inside `at_block(...)` or `storage().try_fetch(...)`
        // would freeze the past-scan walk for the remainder of the outer
        // helper's timeout.
        const PER_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(30);
        const MAX_ATTEMPTS: u32 = 5;
        let addr = mn_meta::storage()
            .c_night_observation()
            .next_cardano_position();
        for attempt in 1..=MAX_ATTEMPTS {
            let result = timeout(PER_ATTEMPT_TIMEOUT, async {
                let at = self.online_client().at_block(midnight_block).await?;
                let value = at
                    .storage()
                    .try_fetch(addr.clone(), ())
                    .await?
                    .map(|v| v.decode())
                    .transpose()?;
                Ok::<_, Box<dyn std::error::Error>>(
                    value.map(|p| p.block_number as u64).unwrap_or(0),
                )
            })
            .await;
            match result {
                Ok(Ok(v)) => return Ok(v),
                Ok(Err(e)) => {
                    let msg = e.to_string();
                    if (msg.contains("restart required") || msg.contains("background task closed"))
                        && attempt < MAX_ATTEMPTS
                    {
                        tracing::warn!(
                            "read_next_cardano_position_at(#{midnight_block}): transport error \
                             (attempt {attempt}/{MAX_ATTEMPTS}); reconnecting: {e}"
                        );
                        let _ = self.reconnect_online_client().await;
                        continue;
                    }
                    return Err(e);
                }
                Err(_) => {
                    if attempt < MAX_ATTEMPTS {
                        tracing::warn!(
                            "read_next_cardano_position_at(#{midnight_block}): timed out after \
                             {PER_ATTEMPT_TIMEOUT:?} (attempt {attempt}/{MAX_ATTEMPTS}); reconnecting"
                        );
                        let _ = self.reconnect_online_client().await;
                        continue;
                    }
                    return Err(format!(
                        "read_next_cardano_position_at(#{midnight_block}) timed out after \
                         {MAX_ATTEMPTS} attempts"
                    )
                    .into());
                }
            }
        }
        unreachable!()
    }

    /// Binary-search for the smallest Midnight block `M` where the
    /// `nextCardanoPosition` watermark is at least `target`. Such an
    /// `M` is the Midnight block whose `process_tokens` extrinsic
    /// crossed `target` (the previous block's watermark was strictly
    /// less than `target`, by monotonicity).
    async fn binary_search_watermark(
        &self,
        target: u64,
        head_num: u64,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        let mut lo: u64 = 1;
        let mut hi: u64 = head_num;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let wm = self.read_next_cardano_position_at(mid).await?;
            if wm >= target {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        Ok(lo)
    }

    /// Past-scan: locate `M_safe` via binary search on the watermark, then
    /// walk backward decoding each Midnight block's extrinsics looking for
    /// `process_tokens` calls that carried our tx_ids. The walk is bounded
    /// by how far back the earliest tx_id's observation lies (at most
    /// ~k Cardano blocks worth of Midnight blocks past `M_safe`).
    async fn scan_past_for_tx_ids(
        &self,
        tx_ids: &[[u8; 32]],
        head_num: u64,
        target: u64,
    ) -> Result<Vec<ExtrinsicEvents<SubstrateConfig>>, Box<dyn std::error::Error>> {
        use std::collections::HashSet;

        let total = tx_ids.len();
        let m_safe = self.binary_search_watermark(target, head_num).await?;
        tracing::info!(
            "await_cnight_observations: binary search settled on midnight #{m_safe}; \
             walking backward for {} tx_id(s)",
            total,
        );

        let mut remaining: HashSet<[u8; 32]> = tx_ids.iter().copied().collect();
        let mut collected: Vec<ExtrinsicEvents<SubstrateConfig>> = Vec::new();
        let mut m = m_safe;
        const WALK_PER_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(30);
        const WALK_MAX_ATTEMPTS: u32 = 5;
        while !remaining.is_empty() && m > 0 {
            // Same transport-resilience wrapping as `read_next_cardano_position_at`:
            // wrap the per-block RPC sequence in a timeout + reconnect retry loop
            // so a stalled `at_block` / `extrinsics().fetch()` / `ext.events()` call
            // doesn't freeze the walk indefinitely.
            type BlockScan = (Vec<ExtrinsicEvents<SubstrateConfig>>, Vec<[u8; 32]>);
            let mut walk_result: Option<BlockScan> = None;
            for attempt in 1..=WALK_MAX_ATTEMPTS {
                let attempt_outcome = timeout(WALK_PER_ATTEMPT_TIMEOUT, async {
                    let at = self.online_client().at_block(m).await?;
                    let exts = at.extrinsics().fetch().await?;
                    let mut block_collected: Vec<ExtrinsicEvents<SubstrateConfig>> = Vec::new();
                    let mut block_matched: Vec<[u8; 32]> = Vec::new();
                    for ext in exts.iter().filter_map(Result::ok) {
                        let Ok(decoded) = ext.decode_call_data_as::<mn_meta::Call>() else {
                            continue;
                        };
                        let Some(utxos) = Self::extract_process_tokens_utxos(&decoded) else {
                            continue;
                        };
                        if utxos.is_empty() {
                            continue;
                        }
                        let matched: Vec<[u8; 32]> = utxos
                            .iter()
                            .map(|u| u.header.tx_hash.0)
                            .filter(|h| remaining.contains(h) && !block_matched.contains(h))
                            .collect();
                        if matched.is_empty() {
                            continue;
                        }
                        // Stage matches locally; commit to `remaining`
                        // only after the whole block scan succeeds, so a
                        // mid-block failure + retry doesn't drop matched
                        // ids whose events haven't been collected yet.
                        let events = ext.events().await?;
                        block_matched.extend(&matched);
                        block_collected.push(events);
                    }
                    Ok::<_, Box<dyn std::error::Error>>((block_collected, block_matched))
                })
                .await;
                match attempt_outcome {
                    Ok(Ok(v)) => {
                        walk_result = Some(v);
                        break;
                    }
                    Ok(Err(e)) => {
                        let msg = e.to_string();
                        if (msg.contains("restart required")
                            || msg.contains("background task closed"))
                            && attempt < WALK_MAX_ATTEMPTS
                        {
                            tracing::warn!(
                                "scan_past: walk at midnight #{m}: transport error \
                                 (attempt {attempt}/{WALK_MAX_ATTEMPTS}); reconnecting: {e}"
                            );
                            let _ = self.reconnect_online_client().await;
                            continue;
                        }
                        return Err(e);
                    }
                    Err(_) => {
                        if attempt < WALK_MAX_ATTEMPTS {
                            tracing::warn!(
                                "scan_past: walk at midnight #{m}: timed out after \
                                 {WALK_PER_ATTEMPT_TIMEOUT:?} (attempt {attempt}/{WALK_MAX_ATTEMPTS}); \
                                 reconnecting"
                            );
                            let _ = self.reconnect_online_client().await;
                            continue;
                        }
                        return Err(format!(
                            "scan_past: walk at midnight #{m} timed out after \
                             {WALK_MAX_ATTEMPTS} attempts"
                        )
                        .into());
                    }
                }
            }
            if let Some((mut block_events, block_matched)) = walk_result {
                for h in &block_matched {
                    remaining.remove(h);
                    tracing::info!(
                        "await_cnight_observations: scan-back matched tx_id 0x{} \
                         in midnight #{m} ({}/{total} observed, waiting on {})",
                        hex::encode(h),
                        total - remaining.len(),
                        remaining.len(),
                    );
                }
                collected.append(&mut block_events);
            }
            if m == 0 {
                break;
            }
            m -= 1;
        }

        if !remaining.is_empty() {
            let missing: Vec<String> = remaining
                .iter()
                .map(|h| format!("0x{}", hex::encode(h)))
                .collect();
            return Err(format!(
                "await_cnight_observations: scan-back from #{m_safe} exhausted \
                 without finding: [{}]",
                missing.join(", "),
            )
            .into());
        }
        Ok(collected)
    }

    /// Latest GRANDPA-finalized block number.
    pub async fn get_finalized_block_number(&self) -> Result<u64, Box<dyn std::error::Error>> {
        Ok(self
            .online_client()
            .at_current_block()
            .await?
            .block_number())
    }

    /// Subscribe to the bridge handler extrinsic, skipping any block at or before `min_block_number`.
    pub async fn subscribe_to_c2m_bridge_transfers(
        &self,
        timeout_duration: Duration,
        min_block_number: u64,
    ) -> Result<C2MBridgePalletCalls, Box<dyn std::error::Error>> {
        tracing::info!("Subscribing for C-to-M transfer extrinsic");
        let mut blocks_sub = self.online_client().stream_blocks().await?;

        let inner = async {
            while let Some(block_result) = blocks_sub.next().await {
                let block = block_result?;
                let block_number = block.number();
                if block_number <= min_block_number {
                    tracing::debug!("Skipping block {block_number} not after {min_block_number}");
                    continue;
                }
                tracing::info!("Streamed block {}", block_number);
                let block_ref = block.at().await?;
                let extrinsics = block_ref.extrinsics().fetch().await?;

                let transfers: Vec<BridgeTransferV1<BridgeRecipient>> = extrinsics
                    .iter()
                    .filter_map(|res| {
                        res.ok()
                            .and_then(|ext| ext.decode_call_data_as::<mn_meta::Call>().ok())
                            .and_then(MidnightClient::extract_bridge_calls)
                    })
                    .flatten()
                    .collect();
                if transfers.is_empty() {
                    continue;
                }

                let block_events = block_ref.events().fetch().await?;

                let mut c2m_bridge_events = Vec::new();
                for ev in block_events.iter().filter_map(Result::ok) {
                    if let Ok(mn_meta::Event::C2MBridge(inner)) = ev.decode_as::<mn_meta::Event>() {
                        c2m_bridge_events.push(inner);
                    }
                }

                let mut system_transactions_applied = Vec::new();
                for ev in block_events.iter().filter_map(Result::ok) {
                    if let Some(Ok(sta)) = ev.decode_fields_as::<SystemTransactionApplied>() {
                        system_transactions_applied.push(sta);
                    }
                }

                let result = C2MBridgePalletCalls {
                    c2m_bridge_events,
                    transfers,
                    system_transactions_applied,
                };
                return Ok(result);
            }
            Err("Did not find bridge extrinsics".into())
        };

        timeout(timeout_duration, inner)
            .await
            .unwrap_or_else(|_| Err("Timeout waiting for bridge exrinsics".into()))
    }

    pub fn calculate_nonce(prefix: &[u8], tx_hash: [u8; 32], tx_index: u16) -> String {
        let mut hasher = Blake2bVar::new(32).expect("valid output size");

        hasher.update(prefix);
        hasher.update(&tx_hash);
        hasher.update(&tx_index.to_be_bytes());

        let mut out = [0u8; 32];
        hasher
            .finalize_variable(&mut out)
            .expect("finalize succeeds");
        hex::encode(out)
    }

    fn extract_process_tokens_utxos(call: &mn_meta::Call) -> Option<&Vec<ObservedUtxo>> {
        match call {
            mn_meta::Call::CNightObservation(c_night_observation::Call::process_tokens {
                utxos,
                ..
            }) => Some(utxos),
            _ => None,
        }
    }

    fn extract_bridge_calls(call: mn_meta::Call) -> Option<Vec<BridgeTransferV1<BridgeRecipient>>> {
        match call {
            mn_meta::Call::Bridge(bridge::Call::handle_transfers { transfers, .. }) => {
                Some(transfers.0)
            }
            _ => None,
        }
    }

    pub async fn query_night_utxo_owners(
        &self,
        utxo: String,
    ) -> Result<Option<UtxoOwners>, Box<dyn std::error::Error>> {
        let nonce = hex::decode(&utxo).unwrap();
        let storage_address = mn_meta::storage().c_night_observation().utxo_owners();

        let owners = self
            .online_client()
            .at_current_block()
            .await?
            .storage()
            .try_fetch(storage_address, (H256(nonce.try_into().unwrap()),))
            .await?
            .map(|v| v.decode())
            .transpose()?;

        Ok(owners)
    }

    pub async fn ics_validator_address(&self) -> Result<String, Box<dyn std::error::Error>> {
        let addr = mn_meta::storage()
            .bridge()
            .main_chain_scripts_configuration();

        let scripts = self
            .online_client()
            .at_current_block()
            .await?
            .storage()
            .try_fetch(addr, ())
            .await?
            .map(|v| v.decode())
            .transpose()?
            .ok_or("Bridge::MainChainScriptsConfiguration is not set in storage")?;

        let bytes = scripts.illiquid_circulation_supply_validator_address.0.0;
        Ok(String::from_utf8(bytes)?)
    }

    /// Read the active `LedgerParameters` via the `get_ledger_parameters` runtime API.
    pub async fn get_ledger_parameters(
        &self,
    ) -> Result<LedgerParameters, Box<dyn std::error::Error>> {
        let call = mn_meta::runtime_apis::RuntimeApi
            .midnight_runtime_api()
            .get_ledger_parameters();
        let bytes = self
            .online_client()
            .at_current_block()
            .await?
            .runtime_apis()
            .call(call)
            .await?
            .map_err(|e| format!("get_ledger_parameters runtime API returned error: {e:?}"))?;
        Ok(deserialize(&mut &bytes[..])?)
    }

    pub async fn subscribe_to_federated_authority_events(
        &self,
    ) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("Checking for federated authority observation events");

        // Track which events we've found
        let mut found_council_reset = false;
        let mut found_tech_committee_reset = false;

        // Helper to check events in a block
        let check_block_events = |events: subxt::events::Events<SubstrateConfig>,
                                  block_number: u64,
                                  found_council: &mut bool,
                                  found_tech: &mut bool| {
            // Check for CouncilMembersReset event
            if let Some(event) = events.find::<CouncilMembersReset>().flatten().next() {
                tracing::info!(
                    "✓ Found CouncilMembersReset event in block #{} with {} members",
                    block_number,
                    event.members.len()
                );
                *found_council = true;
            }

            // Check for TechnicalCommitteeMembersReset event
            if let Some(event) = events
                .find::<TechnicalCommitteeMembersReset>()
                .flatten()
                .next()
            {
                tracing::info!(
                    "✓ Found TechnicalCommitteeMembersReset event in block #{} with {} members",
                    block_number,
                    event.members.len()
                );
                *found_tech = true;
            }
        };

        // First, check historical finalized blocks for the events
        // The events may have been emitted before we started listening
        let finalized_at = self.online_client().at_current_block().await?;
        let current_finalized = finalized_at.block_number();

        tracing::info!(
            "Checking historical blocks 1 to {} for federated authority events...",
            current_finalized
        );

        // Check historical blocks from genesis (block 1) up to current finalized
        // We start from block 1 because events are typically emitted early when
        // the mainchain follower first observes the governance contracts
        for block_num in 1..=current_finalized {
            let block_hash: H256 = self
                .rpc_client
                .request("chain_getBlockHash", rpc_params![block_num])
                .await?;

            let at_block = self.online_client().at_block(block_hash).await?;
            let events = at_block.events().fetch().await?;

            check_block_events(
                events,
                block_num,
                &mut found_council_reset,
                &mut found_tech_committee_reset,
            );

            if found_council_reset && found_tech_committee_reset {
                tracing::info!("✓ Both federated authority events found in historical blocks");
                return Ok(());
            }
        }

        tracing::info!(
            "Events not found in historical blocks. Council: {}, TechCommittee: {}",
            found_council_reset,
            found_tech_committee_reset
        );

        // If not found in history, subscribe to new finalized blocks
        tracing::info!("Subscribing to new finalized blocks for remaining events...");
        let mut blocks_sub = self.online_client().stream_blocks().await?;

        let result = timeout(Duration::from_secs(120), async {
            while let Some(block) = blocks_sub.next().await {
                let block = block?;
                let block_number = block.header().number;
                tracing::info!("Checking block #{block_number} for federated authority events");

                let block_ref = block.at().await?;
                let events = block_ref.events().fetch().await?;

                check_block_events(
                    events,
                    block_number,
                    &mut found_council_reset,
                    &mut found_tech_committee_reset,
                );

                if found_council_reset && found_tech_committee_reset {
                    return Ok(());
                }
            }
            Err("Did not find all federated authority events".into())
        })
        .await;

        result.unwrap_or_else(|_| Err("Timeout waiting for federated authority events".into()))
    }

    /// Get the current D-Parameter via RPC.
    ///
    /// Returns the number of permissioned and registered candidates.
    pub async fn get_d_parameter(&self) -> Result<DParameterResponse, Box<dyn std::error::Error>> {
        let response: DParameterResponse = self
            .rpc_client
            .request("systemParameters_getDParameter", rpc_params![])
            .await?;

        Ok(response)
    }

    /// Get the D-Parameter at a specific block hash.
    pub async fn get_d_parameter_at(
        &self,
        block_hash: H256,
    ) -> Result<DParameterResponse, Box<dyn std::error::Error>> {
        let response: DParameterResponse = self
            .rpc_client
            .request("systemParameters_getDParameter", rpc_params![block_hash])
            .await?;

        Ok(response)
    }

    /// Get the current best block hash from the node.
    pub async fn get_best_block_hash(&self) -> Result<H256, Box<dyn std::error::Error>> {
        let hash: H256 = self
            .rpc_client
            .request("chain_getBlockHash", rpc_params![])
            .await?;
        Ok(hash)
    }

    /// Get block hash at a specific block height/number.
    pub async fn get_block_hash_at_height(
        &self,
        block_number: u32,
    ) -> Result<H256, Box<dyn std::error::Error>> {
        let block_hash: Option<H256> = self
            .rpc_client
            .request("chain_getBlockHash", rpc_params![block_number])
            .await?;

        block_hash.ok_or_else(|| format!("No block found at height {}", block_number).into())
    }

    /// Wait for a new finalized block and return its hash.
    pub async fn wait_for_next_finalized_block(&self) -> Result<H256, Box<dyn std::error::Error>> {
        let mut blocks_sub = self.online_client().stream_blocks().await?;

        let result = timeout(Duration::from_secs(30), async {
            if let Some(block_result) = blocks_sub.next().await {
                let block = block_result?;
                tracing::info!("New finalized block #{}", block.header().number);
                return Ok(block.hash());
            }
            Err("No block received".into())
        })
        .await;

        result.unwrap_or_else(|_| Err("Timeout waiting for finalized block".into()))
    }

    /// Get Ariadne parameters including permissioned candidates and D-Parameter.
    ///
    /// The D-Parameter is sourced from pallet-system-parameters (on-chain),
    /// while permissioned candidates come from Cardano Aiken contracts.
    pub async fn get_ariadne_parameters(
        &self,
        epoch_number: u64,
        d_parameter_at: Option<H256>,
    ) -> Result<AriadneParametersResponse, Box<dyn std::error::Error>> {
        let response: AriadneParametersResponse = match d_parameter_at {
            Some(hash) => {
                self.rpc_client
                    .request(
                        "systemParameters_getAriadneParameters",
                        rpc_params![epoch_number, hash],
                    )
                    .await?
            }
            None => {
                self.rpc_client
                    .request(
                        "systemParameters_getAriadneParameters",
                        rpc_params![epoch_number],
                    )
                    .await?
            }
        };

        Ok(response)
    }

    // ========== Sidechain Status and Authority Methods ==========
    // Used for authority selection verification

    /// Get the current sidechain status including epoch number.
    pub async fn get_sidechain_status(
        &self,
    ) -> Result<SidechainStatusResponse, Box<dyn std::error::Error>> {
        let response: SidechainStatusResponse = self
            .rpc_client
            .request("sidechain_getStatus", rpc_params![])
            .await?;

        Ok(response)
    }

    /// Get the current sidechain epoch number.
    pub async fn get_current_epoch(&self) -> Result<u64, Box<dyn std::error::Error>> {
        let status = self.get_sidechain_status().await?;
        Ok(status.epoch)
    }

    /// Wait until the sidechain reaches a specific epoch.
    ///
    /// Polls the sidechain status every 2 seconds until the target epoch is reached,
    /// with a maximum timeout.
    pub async fn wait_for_epoch(
        &self,
        target_epoch: u64,
        timeout_secs: u64,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        let start = Instant::now();
        let poll_interval = Duration::from_secs(2);

        loop {
            let status = self.get_sidechain_status().await?;
            tracing::info!(
                "Current epoch: {}, slot: {}, target: {}",
                status.epoch,
                status.slot,
                target_epoch
            );

            if status.epoch >= target_epoch {
                tracing::info!("✓ Reached target epoch {}", status.epoch);
                return Ok(status.epoch);
            }

            if start.elapsed() > Duration::from_secs(timeout_secs) {
                return Err(format!(
                    "Timeout waiting for epoch {} (current: {})",
                    target_epoch, status.epoch
                )
                .into());
            }

            sleep(poll_interval).await;
        }
    }

    /// Node WebSocket URL — useful for callers (e.g. governance flows running
    /// in dev-dependencies) that need to open their own client to the same node.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    // ========== Midnight Transaction Submission Methods ==========
    // Used for DDoS mitigation E2E tests (TC-0003-06)

    /// Submit a raw Midnight transaction and watch for result.
    /// Returns the transaction progress if submission succeeds.
    pub async fn submit_midnight_tx(
        &self,
        tx_bytes: Vec<u8>,
    ) -> Result<
        TransactionProgress<
            SubstrateConfig,
            subxt::client::OnlineClientAtBlockImpl<SubstrateConfig>,
        >,
        Box<dyn std::error::Error>,
    > {
        let mn_tx = mn_meta::tx().midnight().send_mn_transaction(tx_bytes);
        let unsigned_extrinsic = self.online_client().tx().await?.create_unsigned(&mn_tx)?;
        Ok(unsigned_extrinsic.submit_and_watch().await?)
    }

    /// Submit a Midnight transaction expecting it to be rejected at pre_dispatch.
    /// Returns Ok(error_message) if rejected as expected.
    /// Returns Err if the transaction was unexpectedly accepted.
    pub async fn submit_expecting_rejection(
        &self,
        tx_bytes: Vec<u8>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        tracing::info!("Submitting transaction expecting rejection...");
        // A rejection can surface two ways, both valid:
        //  (1) submit_and_watch errors at submission time — e.g. "already imported" /
        //      "temporarily banned", common when replaying a tx whose original is still in
        //      (or was just pruned from) the pool;
        //  (2) the tx is watched and then fails pre_dispatch/execution.
        // Only case (2) reaches wait_for_finalized_success, so catch case (1) here rather
        // than propagating it as an unexpected error.
        let progress = match self.submit_midnight_tx(tx_bytes).await {
            Ok(progress) => progress,
            Err(e) => {
                tracing::info!("Transaction rejected at submission as expected: {e}");
                return Ok(e.to_string());
            }
        };
        match progress.wait_for_finalized_success().await {
            Err(e) => {
                tracing::info!("Transaction rejected as expected: {}", e);
                Ok(e.to_string())
            }
            Ok(_) => Err(
                "Transaction was unexpectedly accepted - should have been rejected at pre_dispatch"
                    .into(),
            ),
        }
    }

    /// Submit a Midnight transaction expecting it to succeed.
    /// Waits for the transaction to be included in a block.
    pub async fn submit_expecting_success(
        &self,
        tx_bytes: Vec<u8>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("Submitting transaction expecting success...");
        let mut progress = self.submit_midnight_tx(tx_bytes).await?;

        // Wait for inclusion in block
        while let Some(status) = progress.next().await {
            match status? {
                subxt::tx::TransactionStatus::InBestBlock(block_info) => {
                    tracing::info!(
                        "Transaction included in best block: {:?}",
                        block_info.block_hash()
                    );
                    return Ok(());
                }
                subxt::tx::TransactionStatus::InFinalizedBlock(block_info) => {
                    tracing::info!(
                        "Transaction finalized in block: {:?}",
                        block_info.block_hash()
                    );
                    return Ok(());
                }
                subxt::tx::TransactionStatus::Error { message } => {
                    return Err(format!("Transaction error: {}", message).into());
                }
                subxt::tx::TransactionStatus::Invalid { message } => {
                    return Err(format!("Transaction invalid: {}", message).into());
                }
                subxt::tx::TransactionStatus::Dropped { message } => {
                    return Err(format!("Transaction dropped: {}", message).into());
                }
                _ => {
                    // Continue waiting for other statuses
                }
            }
        }
        Err("Transaction progress ended without confirmation".into())
    }

    /// Get the state of a contract by its address at the best block.
    pub async fn get_contract_state(
        &self,
        contract_address: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        self.get_contract_state_at(contract_address, None).await
    }

    /// Get the state of a contract by its address, optionally at a specific block hash.
    pub async fn get_contract_state_at(
        &self,
        contract_address: &str,
        at: Option<H256>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let response: String = match at {
            Some(hash) => {
                self.rpc_client
                    .request(
                        "midnight_contractState",
                        rpc_params![contract_address, hash],
                    )
                    .await?
            }
            None => {
                self.rpc_client
                    .request("midnight_contractState", rpc_params![contract_address])
                    .await?
            }
        };
        Ok(response)
    }
}
