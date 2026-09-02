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

mod metrics;

use crate::{
    application::metrics::Metrics,
    domain::{
        Block, BlockRef, DParameter, LedgerState, SystemParametersChange, TermsAndConditions,
        Transaction,
        node::{self, Node},
        storage::Storage,
    },
};
use anyhow::{Context, bail};
use async_stream::stream;
use fastrace::{Span, future::FutureExt, prelude::SpanContext, trace};
use futures::{Stream, StreamExt, TryStreamExt, future::ok};
use indexer_common::domain::{
    BlockIndexed, BridgeEventIndexed, LedgerVersion, NetworkId, Publisher,
    SerializedLedgerStateKey, UnshieldedUtxoIndexed,
};
use log::{debug, info, warn};
use parking_lot::RwLock;
use serde::Deserialize;
use std::{
    collections::{HashSet, VecDeque},
    error::Error as StdError,
    num::{NonZeroU64, NonZeroUsize},
    pin::pin,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    select,
    signal::unix::Signal,
    sync::mpsc,
    task::{self},
    time::sleep,
};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub network_id: NetworkId,
    pub blocks_buffer: usize,
    pub caught_up_max_distance: u32,
    pub caught_up_leeway: u32,

    /// Per-block time budget for the storage-core gc-v1 mark-and-sweep pass.
    /// Set to "0s" to disable garbage collection.
    #[serde(with = "humantime_serde")]
    pub gc_bound: Duration,

    /// Run the gc pass every this many blocks (default 1). During catch-up a pass rarely finds
    /// orphans, so amortizing it over several blocks trades a slightly larger arena for less
    /// per-block overhead; the time budget per pass stays gc_bound.
    #[serde(default = "gc_block_interval_default")]
    pub gc_block_interval: NonZeroU64,

    /// How many recent blocks' ledger state keys stay persisted as gc roots before the oldest
    /// is unpersisted. Must comfortably exceed indexer-api's block-hash snapshot reads, e.g.
    /// the dust generations subscription's max_snapshot_age, or those reads hit culled state.
    pub ledger_state_retention: NonZeroUsize,
}

fn gc_block_interval_default() -> NonZeroU64 {
    NonZeroU64::MIN
}

pub async fn run(
    config: Config,
    node: impl Node,
    mut storage: impl Storage,
    publisher: impl Publisher,
    mut sigterm: Signal,
) -> anyhow::Result<()> {
    let Config {
        network_id,
        blocks_buffer,
        caught_up_max_distance,
        caught_up_leeway,
        gc_bound,
        gc_block_interval,
        ledger_state_retention,
    } = config;

    // Get info from highest block.
    let highest_block_ref = storage
        .get_highest_block()
        .await
        .context("get highest block")?
        .map(|(block_ref, _, _)| block_ref);

    let highest_block_height = highest_block_ref.map(|info| info.height);
    info!(highest_block_height:?; "starting indexing");

    // Seed the parent-block timestamp from the highest stored block so the first block processed
    // after a restart bumps its first regular transaction's well-formed `tblock` off the true
    // parent block time. Without this, `parent_block_timestamp` would be seeded from the resumed
    // block's own timestamp, over-bumping `tblock` and risking a spurious `IntentTtlExpired`
    // rejection of a transaction the node accepted. `0` (empty DB) keeps the genesis behavior.
    let initial_parent_block_timestamp = storage
        .get_highest_block_timestamp()
        .await
        .context("get highest block timestamp")?
        .unwrap_or(0);

    // Initialize metrics.
    let transaction_count = storage
        .get_transaction_count()
        .await
        .context("get transaction count")?;
    let contract_action_count = storage
        .get_contract_action_count()
        .await
        .context("get contract action count")?;
    let metrics = Metrics::new(
        highest_block_height,
        transaction_count,
        contract_action_count,
    );

    // Load/initialize ledger state. Seed the retention window with the newest blocks' keys
    // (oldest first, each with the ledger version it was persisted under) so a restart keeps
    // balancing the previous run's persists instead of stranding them as permanent gc roots;
    // per block, persist() is then balanced by an unpersist() once the key leaves the window,
    // letting gc-v1 reclaim orphan nodes while recent snapshots stay loadable for indexer-api's
    // block-hash reads (e.g. the dust generations subscription). Keys whose roots are no
    // longer persisted are skipped: after a retention increase, older keys have already been
    // unpersisted, and unpersisting them again would corrupt the root counts.
    let newest_ledger_state_keys = storage
        .get_newest_ledger_state_keys(ledger_state_retention)
        .await
        .context("get newest ledger state keys")?
        .into_iter()
        .map(|(protocol_version, key)| {
            let ledger_version = protocol_version.ledger_version();
            LedgerState::root_hash_bytes(&key, ledger_version)
                .map(|root_hash| (key, ledger_version, root_hash))
        })
        .collect::<Result<Vec<_>, _>>()
        .context("get ledger state root hashes")?;
    let newest_count = newest_ledger_state_keys.len();

    // Must precede the seeding below, and is idempotent, hence unconditional; see
    // `LedgerState::repair_root_counts`.
    let repair = LedgerState::repair_root_counts(
        newest_ledger_state_keys
            .iter()
            .map(|(key, ledger_version, _)| (key, *ledger_version)),
    )
    .context("repair ledger state root counts")?;
    if repair.raised_roots > 0 || repair.culled_roots > 0 {
        warn!(repair:?; "repaired under-counted ledger state gc roots");
    } else {
        info!(repair:?; "ledger state gc root counts are consistent");
    }

    let persisted_root_hashes = LedgerState::persisted_root_hashes();
    let mut persisted_ledger_state_keys = newest_ledger_state_keys
        .into_iter()
        .filter(|(_, _, root_hash)| persisted_root_hashes.contains(root_hash))
        .map(|(key, ledger_version, _)| (key, ledger_version))
        .collect::<VecDeque<_>>();
    info!(
        seeded = persisted_ledger_state_keys.len(),
        skipped_unrooted = newest_count - persisted_ledger_state_keys.len();
        "seeded ledger state retention window"
    );

    let mut ledger_state = match persisted_ledger_state_keys.back() {
        Some((ledger_state_key, ledger_version)) => {
            LedgerState::load(ledger_state_key, *ledger_version).context("load ledger state")?
        }

        None if highest_block_ref.is_some() => bail!(
            "no persisted ledger state root found within the retention window; the ledger DB \
             cannot be resumed"
        ),

        None => LedgerState::new(network_id.clone(), LedgerVersion::OLDEST)
            .context("create ledger state")?,
    };

    let highest_block_on_node = Arc::new(RwLock::new(None));

    // Spawn task to set info for highest block on node.
    let mut highest_block_on_node_task = task::spawn({
        let node = node.clone();
        let highest_block_on_node = highest_block_on_node.clone();

        async move {
            let highest_blocks = node
                .highest_blocks()
                .await
                .context("get stream of highest blocks")?;

            highest_blocks
                .try_for_each(|block_info| {
                    info!(
                        hash:% = block_info.hash,
                        height = block_info.height;
                        "highest finalized block on node"
                    );

                    *highest_block_on_node.write() = Some(block_info);

                    ok(())
                })
                .await
                .context("get next block of highest_blocks")?;

            warn!("highest_block_on_node_task completed");

            Ok::<_, anyhow::Error>(())
        }
    });

    // Spawn task to index blocks.
    let mut index_blocks_task = task::spawn({
        let node = node.clone();

        async move {
            // Stream combinators only make progress while the consumer polls them, so a
            // `buffered` adapter cannot fetch ahead while a block is being processed. Run the
            // block stream on its own task feeding a bounded channel so fetching the next
            // blocks overlaps indexing, with at most `blocks_buffer` blocks in flight.
            let (block_tx, mut block_rx) = mpsc::channel(blocks_buffer.max(1));
            task::spawn({
                let node = node.clone();
                async move {
                    let blocks = node_blocks(highest_block_ref, node);
                    let mut blocks = pin!(blocks);
                    while let Some(block) = blocks.next().await
                        && block_tx.send(block).await.is_ok()
                    {}
                }
            });
            let blocks = stream! {
                while let Some(block) = block_rx.recv().await {
                    yield block;
                }
            };
            let mut blocks = pin!(blocks);
            let mut caught_up = false;
            let mut blocks_since_gc = 0;
            let mut parent_block_timestamp = initial_parent_block_timestamp;

            loop {
                let (next_ledger_state, new_ledger_state_key) = get_and_index_block(
                    caught_up_max_distance,
                    caught_up_leeway,
                    &mut blocks,
                    ledger_state,
                    &network_id,
                    &highest_block_on_node,
                    &mut caught_up,
                    &mut parent_block_timestamp,
                    &mut storage,
                    &publisher,
                    &metrics,
                    &node,
                )
                .in_span(Span::root("get-and-index-block", SpanContext::random()))
                .await?;

                ledger_state = next_ledger_state;

                // Keep the newest ledger_state_retention keys persisted and unpersist the
                // oldest beyond the window, each with the version it was persisted under
                // (they differ at a protocol upgrade boundary). This makes aged-out states'
                // arena nodes eligible for gc while recent snapshots stay loadable for
                // indexer-api's block-hash reads.
                persisted_ledger_state_keys
                    .push_back((new_ledger_state_key, ledger_state.ledger_version()));
                while persisted_ledger_state_keys.len() > ledger_state_retention.get() {
                    if let Some((key, version)) = persisted_ledger_state_keys.pop_front() {
                        LedgerState::unpersist(&key, version)
                            .context("unpersist ledger state beyond retention window")?;
                    }
                }

                // Run a time-bounded mark-and-sweep pass every gc_block_interval blocks; skip
                // when disabled.
                blocks_since_gc += 1;
                if !gc_bound.is_zero() && blocks_since_gc >= gc_block_interval.get() {
                    blocks_since_gc = 0;
                    let started = Instant::now();
                    let nodes_culled = LedgerState::gc(gc_bound);
                    let elapsed = started.elapsed();
                    metrics.record_gc(elapsed, nodes_culled);
                    if nodes_culled > 0 {
                        debug!(
                            nodes_culled,
                            elapsed:?;
                            "gc pass culled orphan arena nodes"
                        );
                    }
                }
            }
        }
    });

    select! {
        result = &mut highest_block_on_node_task => {
            let result = result
                .context("highest_block_on_node_task panicked")
                .and_then(|r| r.context("highest_block_on_node_task failed"));
            index_blocks_task.abort();
            result
        },

        result = &mut index_blocks_task => {
            let result = result
                .context("index_blocks_task panicked")
                .and_then(|r: anyhow::Result<()>| r.context("index_blocks_task failed"));
            highest_block_on_node_task.abort();
            result
        },

        _ = sigterm.recv() => {
            warn!("SIGTERM received");
            highest_block_on_node_task.abort();
            index_blocks_task.abort();
            Ok(())
        }
    }
}

/// An infinite stream of node blocks, neither with duplicates, nor with gaps or otherwise
/// unexpected blocks.
fn node_blocks<N>(
    mut highest_block: Option<BlockRef>,
    mut node: N,
) -> impl Stream<Item = Result<node::Block, N::Error>>
where
    N: Node,
{
    stream! {
        loop {
            let blocks = node.finalized_blocks(highest_block);
            let mut blocks = pin!(blocks);

            while let Some(block) = blocks.next().await {
                if let Ok(block) = &block {
                    let parent_hash = block.parent_hash;
                    let (highest_hash, highest_height) = highest_block
                        .map(|BlockRef { hash, height }| (hash, height))
                        .unzip();

                    // In case of unexpected blocks, e.g. because of a gap or the node lagging
                    // behind, break and rerun the `finalized_blocks` stream.
                    if parent_hash != highest_hash.unwrap_or_default() {
                        warn!(
                            parent_hash:%,
                            height = block.height,
                            highest_hash:?,
                            highest_height:?;
                            "unexpected block"
                        );
                        break;
                    }

                    highest_block = Some(block.into());
                }

                yield block;
            }

            // Sleep to avoid busy-spin.
            sleep(Duration::from_millis(100)).await;
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[trace]
async fn get_and_index_block<E, N>(
    caught_up_max_distance: u32,
    caught_up_leeway: u32,
    blocks: &mut (impl Stream<Item = Result<node::Block, E>> + Unpin),
    ledger_state: LedgerState,
    network_id: &NetworkId,
    highest_block_on_node: &Arc<RwLock<Option<BlockRef>>>,
    caught_up: &mut bool,
    parent_block_timestamp: &mut u64,
    storage: &mut impl Storage,
    publisher: &impl Publisher,
    metrics: &Metrics,
    node: &N,
) -> anyhow::Result<(LedgerState, SerializedLedgerStateKey)>
where
    E: StdError + Send + Sync + 'static,
    N: Node,
{
    let block_fetch_started = Instant::now();
    let block = get_next_block(blocks).await?;
    metrics.record_block_fetch(block_fetch_started.elapsed());

    let result = index_block(
        caught_up_max_distance,
        caught_up_leeway,
        block,
        ledger_state,
        network_id,
        highest_block_on_node,
        caught_up,
        parent_block_timestamp,
        storage,
        publisher,
        metrics,
        node,
    )
    .await?;

    Ok(result)
}

#[trace]
async fn get_next_block<E>(
    blocks: &mut (impl Stream<Item = Result<node::Block, E>> + Unpin),
) -> anyhow::Result<node::Block>
where
    E: StdError + Send + Sync + 'static,
{
    blocks
        .try_next()
        .await
        .context("get next block from node")
        .and_then(|o| o.context("no more block from node"))
}

#[allow(clippy::too_many_arguments)]
#[trace]
async fn index_block<N>(
    caught_up_max_distance: u32,
    caught_up_leeway: u32,
    block: node::Block,
    mut ledger_state: LedgerState,
    network_id: &NetworkId,
    highest_block_on_node: &Arc<RwLock<Option<BlockRef>>>,
    caught_up: &mut bool,
    parent_block_timestamp: &mut u64,
    storage: &mut impl Storage,
    publisher: &impl Publisher,
    metrics: &Metrics,
    node: &N,
) -> anyhow::Result<(LedgerState, SerializedLedgerStateKey)>
where
    N: Node,
{
    let block_processing_started = Instant::now();

    // Capture the node's zswap merkle tree root (domain type) before `try_into` serializes it, to
    // compare against the zswap merkle tree root in the ledger state below.
    let zswap_merkle_tree_root = block.zswap_merkle_tree_root;

    // System parameters ride on the node block; capture them before the conversion consumes it.
    let d_parameter = block.d_parameter.clone();
    let terms_and_conditions = block.terms_and_conditions.clone();

    let block_conversion_started = Instant::now();
    let (mut block, transactions) = block.try_into().context("convert node block into domain")?;
    metrics.record_block_conversion(block_conversion_started.elapsed());

    let ledger_update_started = Instant::now();
    let ledger_version = block.protocol_version.ledger_version();
    ledger_state = if block.height == 0 {
        // The genesis block establishes the chain's ledger version. The inherited
        // bootstrap state was created at OLDEST and is replaced by the genesis state below, so
        // seed a fresh state at the block's version rather than translating across versions
        // (which is not supported, e.g. V8 to V9).
        LedgerState::new(network_id.clone(), ledger_version).context("create ledger state")?
    } else if ledger_state.ledger_version() == ledger_version {
        // Same ledger version (the common case): `translate` is a no-op, so keep it on the
        // async path and avoid the cost of parking a runtime worker every block.
        ledger_state
            .translate(ledger_version)
            .context("translate ledger state")?
    } else {
        // Cross-version hard-fork boundary (fires once, at `apply + 1`): the v8 -> v9
        // translation walks the entire ledger arena and is synchronous and CPU-bound, so run it
        // via `block_in_place` so it does not stall other tasks on this runtime worker.
        tokio::task::block_in_place(|| ledger_state.translate(ledger_version))
            .context("translate ledger state")?
    };

    if *parent_block_timestamp == 0 {
        *parent_block_timestamp = block.timestamp;
    };

    let apply_transactions = |ledger_state: &mut LedgerState| {
        ledger_state
            .apply_transactions(
                transactions,
                block.parent_hash,
                block.timestamp,
                *parent_block_timestamp,
                // Only reproduce the node's mempool-cached first-tx `tblock` bump for non-genesis
                // blocks; genesis (height 0) transactions never transited the mempool.
                block.height > 0,
            )
            .context("apply transactions to ledger state")
    };

    // Apply transactions to ledger state with special handling for genesis block.
    let (transactions, ledger_parameters) = if block.height == 0 {
        // At genesis compare ledger state roots of genesis and block from node to detect whether
        // genesis already includes transactions (post-block-0) or not (pre-block-0).

        if let Some(ledger_state_root) = block.ledger_state_root.as_ref() {
            let genesis_ledger_state = node
                .fetch_genesis_ledger_state()
                .await
                .context("fetch genesis ledger state")?;
            let genesis_ledger_state = LedgerState::from_genesis(
                genesis_ledger_state,
                block.protocol_version.ledger_version(),
            )
            .context("create ledger state from genesis")?;
            let genesis_ledger_state_root = genesis_ledger_state
                .root()
                .context("compute genesis ledger state root")?;

            if *ledger_state_root == genesis_ledger_state_root {
                info!("post-block-0: applying transactions to fresh state, then use genesis state");

                let transactions_ledger_parameters = apply_transactions(&mut ledger_state)?;
                ledger_state = genesis_ledger_state;

                transactions_ledger_parameters
            } else {
                info!("pre-block-0: applying transactions to genesis state");

                ledger_state = genesis_ledger_state;
                apply_transactions(&mut ledger_state)?
            }
        } else {
            // TODO: Remove once support for Node < 0.22 is dropped!
            // Pre Node 0.22: no ledger_state_root RPC! Ignore genesis state.
            apply_transactions(&mut ledger_state)?
        }
    } else {
        // All other blocks, i.e. height > 0.
        apply_transactions(&mut ledger_state)?
    };
    debug!(transactions:?; "transactions applied to ledger state");

    *parent_block_timestamp = block.timestamp;
    block.ledger_parameters = ledger_parameters.serialize()?;
    block.zswap_end_index = ledger_state.zswap_first_free();
    block.dust_commitment_end_index = ledger_state.dust_commitments_first_free();
    block.dust_generation_end_index = ledger_state.dust_generations_first_free();
    block.dust_commitment_merkle_tree_root = ledger_state
        .dust_commitment_merkle_tree_root()
        .context("get dust commitment merkle tree root")?;
    block.dust_generation_merkle_tree_root = ledger_state
        .dust_generation_merkle_tree_root()
        .context("get dust generation merkle tree root")?;

    // Validate ledger state.
    // TODO: Only use ledger state root comparison once support for Node < 0.22 is dropped!
    let ledger_state_root = ledger_state.root().context("get ledger state root")?;
    if let Some(node_ledger_state_root) = block.ledger_state_root.as_ref()
        && *node_ledger_state_root != ledger_state_root
    {
        bail!(
            "ledger state root mismatch for block {} at height {}: node={}, indexer={}",
            block.hash,
            block.height,
            node_ledger_state_root,
            ledger_state_root,
        );
    }
    let local_zswap_merkle_tree_root = ledger_state.zswap_merkle_tree_root();
    if local_zswap_merkle_tree_root != zswap_merkle_tree_root {
        bail!(
            "zswap state root mismatch for block {} at height {}: node={:?}, indexer={:?}",
            block.hash,
            block.height,
            zswap_merkle_tree_root,
            local_zswap_merkle_tree_root,
        );
    }
    metrics.record_ledger_update(ledger_update_started.elapsed());

    // Determine whether caught up, also allowing to fall back a little in that state.
    // Use saturating subtraction to handle the case where streams are temporarily out of order.
    // The two subscriptions (highest_blocks and finalized_blocks) are independent with no
    // ordering guarantee, so node_block_height < block.height may happen under certain rare
    // conditions. This will produce 0 when node_block_height < block.height, treating it as
    // caught up.
    // Using u32::MAX when node_block_height initially is None obviously results in "not caught up"
    // and hence prevents from prematurely signaling readiness.
    let node_block_height = highest_block_on_node
        .read()
        .map(|BlockRef { height, .. }| height)
        .unwrap_or(u64::MAX);
    let distance = node_block_height.saturating_sub(block.height);
    let max_distance = if *caught_up {
        caught_up_max_distance + caught_up_leeway
    } else {
        caught_up_max_distance
    };
    let old_caught_up = *caught_up;
    *caught_up = distance <= max_distance as u64;
    if old_caught_up != *caught_up {
        info!(caught_up:%; "caught-up status changed")
    }

    // Persist ledger state.
    let ledger_persist_started = Instant::now();
    let (new_ledger_state, ledger_state_key) =
        ledger_state.0.persist().context("persist ledger state")?;
    ledger_state = new_ledger_state.into();
    metrics.record_ledger_persist(ledger_persist_started.elapsed());

    // Determine system parameters change if any.
    let system_parameters_started = Instant::now();
    let system_parameters_change =
        determine_system_parameters_change(&block, d_parameter, terms_and_conditions, storage)
            .await
            .context("determine system parameters change")?;
    metrics.record_system_parameters(system_parameters_started.elapsed());

    // Save the block with its related data and system parameters atomically.
    let block_storage_started = Instant::now();
    let max_transaction_id = storage
        .save_block(
            &block,
            &transactions,
            &block.dust_registration_events,
            &ledger_state_key,
            system_parameters_change.as_ref(),
        )
        .await
        .context("save block")?;
    metrics.record_block_storage(block_storage_started.elapsed());

    // Publish BlockIndexed.
    let event_publish_started = Instant::now();
    publisher
        .publish(&BlockIndexed {
            height: block.height,
            max_transaction_id,
            caught_up: *caught_up,
        })
        .await
        .context("publish BlockIndexed event")?;

    // Publish UnshieldedUtxoIndexed events for affected addresses.
    let addresses = transactions
        .iter()
        .flat_map(|transaction| match transaction {
            Transaction::Regular(transaction) => transaction
                .created_unshielded_utxos
                .iter()
                .chain(transaction.spent_unshielded_utxos.iter()),

            Transaction::System(transaction) => {
                transaction.created_unshielded_utxos.iter().chain(&[])
            }
        })
        .map(|utxo| utxo.owner)
        .collect::<HashSet<_>>();
    for address in addresses {
        publisher
            .publish(&UnshieldedUtxoIndexed { address })
            .await
            .context("publish UnshieldedUtxoIndexed event")?;
    }

    // Publish BridgeEventIndexed for each c2m-bridge event.
    for event in &block.bridge_events {
        publisher
            .publish(&BridgeEventIndexed {
                block_height: block.height,
                event: event.clone(),
            })
            .await
            .context("publish BridgeEventIndexed event")?;
    }
    metrics.record_event_publish(event_publish_started.elapsed());

    // Update metrics.
    metrics.update(&block, &transactions, node_block_height, *caught_up);

    info!(
        hash:% = block.hash,
        height = block.height,
        parent_hash:% = block.parent_hash,
        protocol_version:? = block.protocol_version,
        distance,
        caught_up = *caught_up;
        "block indexed"
    );

    metrics.record_block_processing(block_processing_started.elapsed());

    Ok((ledger_state, ledger_state_key))
}

/// Determine whether the system parameters carried on the block differ from the stored ones.
#[trace]
async fn determine_system_parameters_change(
    block: &Block,
    d_parameter: Option<DParameter>,
    terms_and_conditions: Option<TermsAndConditions>,
    storage: &mut impl Storage,
) -> anyhow::Result<Option<SystemParametersChange>> {
    // Get the latest stored parameters.
    let stored_d_param = storage
        .get_latest_d_parameter()
        .await
        .context("get latest D-parameter")?;
    let stored_tc = storage
        .get_latest_terms_and_conditions()
        .await
        .context("get latest terms and conditions")?;

    // Determine what has changed.
    let d_param_changed = d_parameter.as_ref().is_some_and(|current_d| {
        stored_d_param.as_ref().is_none_or(|stored_d| {
            current_d.num_permissioned_candidates != stored_d.num_permissioned_candidates
                || current_d.num_registered_candidates != stored_d.num_registered_candidates
        })
    });

    let tc_changed = match (&terms_and_conditions, &stored_tc) {
        (Some(current_tc), Some(stored_tc)) => {
            current_tc.hash != stored_tc.hash || current_tc.url != stored_tc.url
        }
        (Some(_), None) => true,  // New T&C where none existed.
        (None, Some(_)) => false, // T&C removed - don't record this as a change.
        (None, None) => false,
    };

    if d_param_changed || tc_changed {
        let change = SystemParametersChange {
            block_height: block.height,
            block_hash: block.hash,
            timestamp: block.timestamp,
            d_parameter: if d_param_changed { d_parameter } else { None },
            terms_and_conditions: if tc_changed {
                terms_and_conditions
            } else {
                None
            },
        };

        debug!(
            block_height = block.height,
            d_param_changed,
            tc_changed;
            "system parameters changed"
        );

        Ok(Some(change))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        application::node_blocks,
        domain::{
            BlockRef,
            node::{self, Node},
        },
    };
    use fake::{Fake, Faker};
    use futures::{Stream, StreamExt, TryStreamExt, stream};
    use indexer_common::{
        domain::{BlockHash, ByteArray, ByteVec, ProtocolVersion, ledger::ZswapMerkleTreeRoot},
        error::BoxError,
    };
    use std::{convert::Infallible, sync::LazyLock};

    #[tokio::test]
    async fn test_blocks() -> Result<(), BoxError> {
        let blocks = node_blocks(None, MockNode);
        let heights = blocks
            .take(4)
            .map_ok(|block| block.height)
            .try_collect::<Vec<_>>()
            .await?;
        assert_eq!(heights, vec![0, 1, 2, 3]);

        Ok(())
    }

    #[derive(Clone)]
    struct MockNode;

    impl Node for MockNode {
        type Error = Infallible;

        async fn highest_blocks(
            &self,
        ) -> Result<impl Stream<Item = Result<BlockRef, Self::Error>>, Self::Error> {
            Ok(stream::empty())
        }

        fn finalized_blocks(
            &mut self,
            _highest_block: Option<BlockRef>,
        ) -> impl Stream<Item = Result<node::Block, Self::Error>> {
            stream::iter([&*BLOCK_0, &*BLOCK_1, &*BLOCK_2, &*BLOCK_3])
                .map(|block| Ok(block.to_owned()))
        }

        async fn fetch_genesis_ledger_state(&self) -> Result<ByteVec, Self::Error> {
            Ok(Default::default())
        }
    }

    static BLOCK_0: LazyLock<node::Block> = LazyLock::new(|| node::Block {
        hash: BLOCK_0_HASH,
        height: 0,
        protocol_version: *PROTOCOL_VERSION,
        parent_hash: ZERO_HASH,
        author: Default::default(),
        timestamp: Default::default(),
        zswap_merkle_tree_root: ZswapMerkleTreeRoot::V8(Faker.fake()),
        ledger_state_root: None,
        transactions: Default::default(),
        dust_registration_events: Default::default(),
        bridge_events: Default::default(),
        d_parameter: None,
        terms_and_conditions: None,
    });

    static BLOCK_1: LazyLock<node::Block> = LazyLock::new(|| node::Block {
        hash: BLOCK_1_HASH,
        height: 1,
        protocol_version: *PROTOCOL_VERSION,
        parent_hash: BLOCK_0_HASH,
        author: Default::default(),
        timestamp: Default::default(),
        zswap_merkle_tree_root: ZswapMerkleTreeRoot::V8(Faker.fake()),
        ledger_state_root: None,
        transactions: Default::default(),
        dust_registration_events: Default::default(),
        bridge_events: Default::default(),
        d_parameter: None,
        terms_and_conditions: None,
    });

    static BLOCK_2: LazyLock<node::Block> = LazyLock::new(|| node::Block {
        hash: BLOCK_2_HASH,
        height: 2,
        protocol_version: *PROTOCOL_VERSION,
        parent_hash: BLOCK_1_HASH,
        author: Default::default(),
        timestamp: Default::default(),
        zswap_merkle_tree_root: ZswapMerkleTreeRoot::V8(Faker.fake()),
        ledger_state_root: None,
        transactions: Default::default(),
        dust_registration_events: Default::default(),
        bridge_events: Default::default(),
        d_parameter: None,
        terms_and_conditions: None,
    });

    static BLOCK_3: LazyLock<node::Block> = LazyLock::new(|| node::Block {
        hash: BLOCK_3_HASH,
        height: 3,
        protocol_version: *PROTOCOL_VERSION,
        parent_hash: BLOCK_2_HASH,
        author: Default::default(),
        timestamp: Default::default(),
        zswap_merkle_tree_root: ZswapMerkleTreeRoot::V8(Faker.fake()),
        ledger_state_root: None,
        transactions: Default::default(),
        dust_registration_events: Default::default(),
        bridge_events: Default::default(),
        d_parameter: None,
        terms_and_conditions: None,
    });

    const ZERO_HASH: BlockHash = ByteArray([0; 32]);

    const BLOCK_0_HASH: BlockHash = ByteArray([1; 32]);
    const BLOCK_1_HASH: BlockHash = ByteArray([2; 32]);
    const BLOCK_2_HASH: BlockHash = ByteArray([3; 32]);
    const BLOCK_3_HASH: BlockHash = ByteArray([3; 32]);

    #[allow(clippy::zero_prefixed_literal)]
    static PROTOCOL_VERSION: LazyLock<ProtocolVersion> =
        LazyLock::new(|| 0_022_000_u32.try_into().unwrap());
}
