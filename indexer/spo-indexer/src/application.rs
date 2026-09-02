// This file is part of midnight-indexer.
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

use crate::{
    domain::{
        CandidateRegistration, Epoch, PoolMetadata, SPO, SPOEpochPerformance, SPOHistory,
        SPOStatus, Validator, ValidatorMembership,
        storage::{SqlxTransaction, Storage},
    },
    infra::spo_client::{SLOT_DURATION, SPOClient},
    utils::{hex_to_bytes, remove_hex_prefix},
};
use anyhow::Context;
use blake2::{
    Blake2bVar,
    digest::{Update, VariableOutput},
};
use log::{debug, error, info, warn};
use serde::Deserialize;
use std::{
    cmp,
    collections::{HashMap, hash_map::Entry},
    time::{Duration, Instant},
};
use subxt::utils::to_hex;
use tokio::{
    select,
    signal::unix::Signal,
    time::{interval, sleep},
};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub interval: u32,
    /// Stake refresh config (mandatory).
    pub stake_refresh: StakeRefreshConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StakeRefreshConfig {
    /// How often to refresh stake data in seconds.
    pub period_secs: u64,
    /// Number of pools to fetch per cycle.
    pub page_size: u32,
    /// Max requests per second to Blockfrost (rudimentary rate limit).
    pub max_rps: u32,
}

pub async fn run(
    config: Config,
    client: SPOClient,
    storage: impl Storage,
    mut sigterm: Signal,
) -> anyhow::Result<()> {
    let st_cfg = config.stake_refresh.clone();
    let storage_bg = storage.clone();
    let client_bg = client.clone();
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(st_cfg.period_secs.max(60)));
        // Initial delay to avoid hammering on startup.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            if let Err(error) = refresh_stake_snapshots(&client_bg, &storage_bg, &st_cfg).await {
                error!("stake refresh failed: {error:?}");
            }
        }
    });

    let poll_interval = Duration::from_secs(config.interval.into());

    loop {
        select! {
            result = process_next_epoch(poll_interval, &client, &storage) => {
                result?;
            }
            _ = sigterm.recv() => {
                warn!("SIGTERM received");
                return Ok(());
            }
        }
    }
}

async fn process_next_epoch(
    poll_interval: Duration,
    client: &SPOClient,
    storage: &impl Storage,
) -> anyhow::Result<()> {
    let Some(epoch) = get_epoch_to_process(client, storage).await? else {
        debug!("latest epoch reached");
        sleep(poll_interval).await;
        return Ok(());
    };
    info!(epoch_no = epoch.epoch_no; "processing epoch");

    // Fetch everything from the network before opening a transaction. Holding
    // the SQLite writer across HTTP calls starves concurrent writers (e.g. the
    // chain-indexer or the stake-refresh task) beyond `busy_timeout` and trips
    // `database is locked` errors.
    let prefetch_started = Instant::now();
    let committee = client.get_committee(epoch.epoch_no).await?;
    let raw_spos = client.get_spo_registrations(epoch.epoch_no).await?;
    let membership = committee_to_membership(client, &committee);

    let entries = raw_spos
        .candidate_registrations
        .values()
        .flatten()
        .map(|reg| (get_cardano_id(&reg.mainchain_pub_key), reg))
        .collect::<Vec<_>>();

    let mut blocks_produced: HashMap<String, u32> = HashMap::new();
    let mut val_to_registration: HashMap<String, CandidateRegistration> = HashMap::new();
    let mut pool_metadata: HashMap<String, PoolMetadata> = HashMap::new();

    for (cardano_id, raw_spo) in &entries {
        // Normalize all keys by stripping optional 0x prefix for consistency with DB values.
        let spo_sk = remove_hex_prefix(&raw_spo.sidechain_pub_key).to_owned();
        val_to_registration.insert(spo_sk.clone(), (*raw_spo).clone());
        *blocks_produced.entry(spo_sk).or_insert(0) += 1;

        if let Entry::Vacant(slot) = pool_metadata.entry(cardano_id.clone()) {
            let meta = client
                .get_pool_metadata(cardano_id.clone())
                .await
                .unwrap_or_else(|_| PoolMetadata::placeholder(cardano_id.clone()));
            slot.insert(meta);
        }
    }

    debug!(
        epoch_no = epoch.epoch_no,
        registrations = val_to_registration.len(),
        unique_pools = pool_metadata.len(),
        prefetch_ms = elapsed_ms(prefetch_started);
        "spo: network pre-fetch complete"
    );

    // All network work is done; now persist in a single short transaction.
    let tx_started = Instant::now();
    let mut tx = storage.create_tx().await?;
    storage.save_epoch(&epoch, &mut tx).await?;
    storage.save_membership(&membership, &mut tx).await?;

    for (cardano_id, raw_spo) in &entries {
        let meta = pool_metadata
            .get(cardano_id)
            .expect("metadata inserted for every entry above");
        storage.save_pool_meta(meta, &mut tx).await?;
        save_spo_identity(storage, raw_spo, cardano_id.clone(), &mut tx).await?;
        save_spo_history(storage, raw_spo, epoch.epoch_no.into(), &mut tx).await?;
    }

    debug!(committee_size = committee.len(); "committee");
    if !committee.is_empty() {
        let blocks_remainder = client.epoch_duration % committee.len() as u32;
        let expected_blocks = get_expected_blocks(client, &epoch, committee.len() as u32);

        for (index, spo) in committee.iter().enumerate() {
            let spo_sk = remove_hex_prefix(&spo.sidechain_pubkey).to_owned();
            // Only count if the validator has produced a block.
            if let Some(&produced_count) = blocks_produced.get(&spo_sk) {
                let raw_spo = val_to_registration
                    .get(&spo_sk)
                    .expect("validator should have registration");
                let cardano_id = get_cardano_id(&raw_spo.mainchain_pub_key);

                let spo_performance = SPOEpochPerformance {
                    spo_sk,
                    epoch_no: epoch.epoch_no as u64,
                    expected_blocks: expected_blocks
                        + (if (index as u32) < blocks_remainder {
                            1
                        } else {
                            0
                        }),
                    produced_blocks: produced_count as u64,
                    identity_label: cardano_id,
                };

                storage
                    .save_spo_performance(&spo_performance, &mut tx)
                    .await?;
            }
        }
    }

    tx.commit().await?;
    info!(
        epoch_no = epoch.epoch_no,
        tx_ms = elapsed_ms(tx_started);
        "processed epoch"
    );
    Ok(())
}

async fn refresh_stake_snapshots(
    client: &SPOClient,
    storage: &impl Storage,
    cfg: &StakeRefreshConfig,
) -> anyhow::Result<()> {
    let limit = cfg.page_size as i64;
    let mut total_updated = 0u32;
    let main_epoch = client
        .get_sidechain_status()
        .await
        .ok()
        .map(|s| s.mainchain.epoch as i64);

    // Cursor-based paging: resume after last_pool_id, then wrap to start.
    let start_after = storage.get_stake_refresh_cursor().await?;
    let after = start_after.clone();

    // First page: after last_pool_id.
    let mut pool_ids = if let Some(ref last) = after {
        storage.get_pool_ids_after(last, limit).await?
    } else {
        storage.get_pool_ids(limit, 0).await?
    };

    // If empty, wrap-around from beginning.
    if pool_ids.is_empty() {
        pool_ids = storage.get_pool_ids(limit, 0).await?;
    }

    if pool_ids.is_empty() {
        return Ok(());
    }

    // Rate limiting.
    let sleep_per_req_ms = if cfg.max_rps == 0 {
        0
    } else {
        (1000 / cfg.max_rps.max(1)) as u64
    };

    // Use one transaction per pool so the shared SQLite writer is not held
    // across Blockfrost HTTP calls and rate-limit sleeps, which would otherwise
    // starve the connection pool for concurrent readers/writers.
    for pid in pool_ids.iter() {
        match client.get_pool_data(pid).await {
            Ok(pd) => {
                let tx_started = Instant::now();
                let mut tx = storage
                    .create_tx()
                    .await
                    .with_context(|| format!("begin tx for pool {pid}"))?;
                storage
                    .save_stake_snapshot(
                        pid,
                        pd.live_stake,
                        pd.active_stake,
                        pd.live_delegators,
                        pd.live_saturation,
                        pd.declared_pledge,
                        pd.live_pledge,
                        &mut tx,
                    )
                    .await
                    .with_context(|| format!("save_stake_snapshot for pool {pid}"))?;
                storage
                    .insert_stake_history(
                        pid,
                        main_epoch,
                        pd.live_stake,
                        pd.active_stake,
                        pd.live_delegators,
                        pd.live_saturation,
                        pd.declared_pledge,
                        pd.live_pledge,
                        &mut tx,
                    )
                    .await
                    .with_context(|| format!("insert_stake_history for pool {pid}"))?;
                tx.commit()
                    .await
                    .with_context(|| format!("commit tx for pool {pid}"))?;
                let tx_ms = elapsed_ms(tx_started);
                if tx_ms > 1_000 {
                    warn!(pid, tx_ms; "spo: stake snapshot tx held > 1s");
                }
                total_updated += 1;
            }
            Err(error) => {
                error!("stake refresh for {pid} failed: {error:?}");
            }
        }

        // Advance the cursor per pool, not once after the loop: pools committed so far stay
        // committed when a later pool fails, and re-reading them on the next cycle would
        // append duplicate spo_stake_history rows (the table has no unique key).
        storage
            .set_stake_refresh_cursor(Some(pid.as_str()))
            .await
            .with_context(|| format!("advance stake refresh cursor to {pid}"))?;

        if sleep_per_req_ms > 0 {
            sleep(Duration::from_millis(sleep_per_req_ms)).await;
        }
    }

    if total_updated > 0 {
        info!(total_updated, cursor:? = pool_ids.last(); "stake refresh completed");
    }
    Ok(())
}

async fn save_spo_history(
    storage: &impl Storage,
    raw_spo: &CandidateRegistration,
    epoch: u64,
    tx: &mut SqlxTransaction,
) -> anyhow::Result<()> {
    // Normalize to hex without 0x.
    let spo_sk = remove_hex_prefix(&raw_spo.sidechain_pub_key).to_owned();

    let spo = SPOHistory {
        spo_sk: spo_sk.clone(),
        epoch_no: epoch,
        status: if raw_spo.is_valid {
            SPOStatus::Valid
        } else {
            SPOStatus::Invalid
        },
    };

    storage.save_spo_history(&spo, tx).await?;
    Ok(())
}

async fn save_spo_identity(
    storage: &impl Storage,
    raw_spo: &CandidateRegistration,
    cardano_id: String,
    tx: &mut SqlxTransaction,
) -> anyhow::Result<()> {
    // Normalize all hex-like identifiers to avoid mixed representations.
    let spo_sk = remove_hex_prefix(&raw_spo.sidechain_pub_key).to_owned();
    let aura_pk = remove_hex_prefix(&raw_spo.keys.aura).to_owned();
    let main_pk = remove_hex_prefix(&raw_spo.mainchain_pub_key).to_owned();

    let spo = SPO {
        spo_sk: spo_sk.clone(),
        sidechain_pubkey: spo_sk,
        pool_id: cardano_id,
        aura_pubkey: aura_pk,
        mainchain_pubkey: main_pk,
    };

    storage.save_spo(&spo, tx).await?;
    Ok(())
}

fn get_expected_blocks(client: &SPOClient, epoch: &Epoch, committee_size: u32) -> u32 {
    let mx_slots = cmp::min(
        client.slots_per_epoch,
        (epoch.ends_at - epoch.starts_at) as u32 / SLOT_DURATION,
    );
    mx_slots / committee_size
}

fn committee_to_membership(
    client: &SPOClient,
    committee: &[Validator],
) -> Vec<ValidatorMembership> {
    if committee.is_empty() {
        return vec![];
    }

    let slots_per_epoch = client.slots_per_epoch;
    let num_validators = committee.len() as u32;
    let leftover = slots_per_epoch % num_validators;

    committee
        .iter()
        .enumerate()
        .map(|(index, c)| ValidatorMembership {
            epoch_no: c.epoch_no,
            position: c.position,
            // Normalize to hex without 0x for consistency with identity/performance.
            spo_sk: remove_hex_prefix(&c.sidechain_pubkey).to_owned(),
            sidechain_pubkey: remove_hex_prefix(&c.sidechain_pubkey).to_owned(),
            expected_slots: slots_per_epoch / num_validators
                + if leftover > index.try_into().expect("index should fit in u32") {
                    1
                } else {
                    0
                },
        })
        .collect()
}

/// If option is None, it means that we are already at the latest epoch.
async fn get_epoch_to_process(
    client: &SPOClient,
    storage: &impl Storage,
) -> anyhow::Result<Option<Epoch>> {
    let latest_processed = storage.get_latest_epoch().await?;
    let current_epoch = client.get_current_epoch().await?;
    let latest_epoch_num = match latest_processed {
        Some(epoch) => epoch.epoch_no,
        None => client.get_first_epoch_num().await?,
    };

    let time_offset: i64 =
        (current_epoch.epoch_no as i64 - latest_epoch_num as i64) * client.epoch_duration as i64;

    if time_offset == 0 {
        Ok(None)
    } else {
        Ok(Some(Epoch {
            epoch_no: latest_epoch_num + 1,
            starts_at: current_epoch.starts_at - time_offset,
            ends_at: current_epoch.ends_at - time_offset,
        }))
    }
}

fn elapsed_ms(t: Instant) -> u64 {
    t.elapsed().as_millis() as u64
}

fn get_cardano_id(mainchain_pk: &str) -> String {
    let mainchain_pk = hex_to_bytes(mainchain_pk);
    let mut hasher = Blake2bVar::new(28).expect("blake2b output size 28 is valid");
    hasher.update(&mainchain_pk);

    let mut buffer = [0u8; 28];
    hasher
        .finalize_variable(&mut buffer)
        .expect("blake2b finalize should succeed with valid buffer");

    let hex_hash = to_hex(buffer);

    remove_hex_prefix(&hex_hash).to_owned()
}
