// This file is part of midnight-indexer.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{
    domain::{LedgerState, dust::DustGenerationDtimeUpdateEntry, storage::Storage},
    infra::api::{
        ApiResult, ContextExt, OptionExt, ResultExt,
        v4::{
            HexEncodable, HexEncoded, directives::beta, dust::DustAddress,
            merkle_tree_collapsed_update::MerkleTreeCollapsedUpdate,
        },
    },
};
use async_graphql::{Context, SimpleObject, Subscription, Union};
use async_stream::try_stream;
use futures::{Stream, TryStreamExt};
use indexer_common::domain::{ProtocolVersion, Subscriber};
use std::{marker::PhantomData, pin::pin};

/// An event of the dust generations subscription.
// The `Dust*` prefix on every variant matches the existing GraphQL type
// names; renaming would change the public union members.
#[allow(clippy::enum_variant_names)]
#[derive(Union)]
pub enum DustGenerationsEvent {
    DustGenerationsItem(DustGenerationsItem),
    DustGenerationsProgress(DustGenerationsProgress),
    DustGenerationDtimeUpdateItem(DustGenerationDtimeUpdateItem),
}

pub struct DustGenerationsSubscription<S, B> {
    _s: PhantomData<S>,
    _b: PhantomData<B>,
}

impl<S, B> Default for DustGenerationsSubscription<S, B> {
    fn default() -> Self {
        Self {
            _s: PhantomData,
            _b: PhantomData,
        }
    }
}

/// A dust generations item with optional collapsed Merkle tree update.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(directive = beta::apply())]
pub struct DustGenerationsItem {
    /// Index of this output in the dust commitment Merkle tree.
    pub commitment_mt_index: u64,
    /// Index of this output in the dust generation Merkle tree.
    pub generation_mt_index: u64,
    /// The hex-encoded owner (dust address).
    pub owner: HexEncoded,
    /// The NIGHT value backing this output, in STAR.
    pub value: String,
    /// The DUST value at creation, in SPECK.
    pub initial_value: String,
    /// Hex-encoded hash of the NIGHT UTXO that backs this dust output.
    pub backing_night: HexEncoded,
    /// The creation timestamp.
    pub ctime: u64,
    /// The originating transaction ID (indexer-internal BIGSERIAL).
    pub transaction_id: u64,
    /// The hex-encoded originating transaction hash (32-byte chain identifier).
    pub transaction_hash: HexEncoded,
    /// Collapsed Merkle tree update filling the gap before this entry.
    pub collapsed_merkle_tree: Option<MerkleTreeCollapsedUpdate>,
}

/// Progress indicator for dust generations subscription (includes final collapsed update).
#[derive(Debug, Clone, SimpleObject)]
#[graphql(directive = beta::apply())]
pub struct DustGenerationsProgress {
    /// The highest index processed so far.
    pub highest_index: u64,
    /// Final collapsed Merkle tree update covering remaining range.
    pub collapsed_merkle_tree: Option<MerkleTreeCollapsedUpdate>,
}

/// A dust generation dtime update emitted when the backing Night UTXO is
/// spent and the entry's decay time is set.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(directive = beta::apply())]
pub struct DustGenerationDtimeUpdateItem {
    /// Generation-tree index of the entry whose dtime changed.
    pub generation_mt_index: u64,
    /// The hex-encoded owner (dust address).
    pub owner: HexEncoded,
    /// Hex-encoded hash of the NIGHT UTXO that backs this dust output.
    pub night_utxo_hash: HexEncoded,
    /// The decay time as observed in this ledger event.
    pub new_dtime: u64,
    /// The originating transaction ID (indexer-internal BIGSERIAL).
    pub transaction_id: u64,
    /// The hex-encoded originating transaction hash (32-byte chain identifier).
    pub transaction_hash: HexEncoded,
    /// Hex-encoded tagged-serialised `TreeInsertionPath<DustGenerationInfo>`
    /// from the originating ledger event. Wallets deserialise this and hand
    /// it to `generating_tree.update_from_evidence(...)`.
    pub tree_insertion_path: HexEncoded,
}

#[Subscription]
impl<S, B> DustGenerationsSubscription<S, B>
where
    S: Storage,
    B: Subscriber,
{
    /// Subscribe to a dust address's generations as a consistent snapshot at `block_hash`. The
    /// wallet's owned generation entries, interleaved with collapsed Merkle tree updates for the
    /// non-owned gaps, are served at that block's state (deterministic, independent of the tip).
    /// The dtime updates for the wallet's own generations in `(dtime_cutoff_height, block_hash]`
    /// are issued first, as a clean delta for the generations set rather than the tree (pass `0`
    /// to replay all). The subscription completes once emitted; re-subscribe at a newer
    /// `block_hash` to advance.
    #[graphql(directive = beta::apply())]
    async fn dust_generations<'a>(
        &self,
        cx: &'a Context<'a>,
        dust_address: DustAddress,
        block_hash: HexEncoded,
        dtime_cutoff_height: u64,
    ) -> impl Stream<Item = ApiResult<DustGenerationsEvent>> {
        let storage = cx.get_storage::<S>();
        let dust_generations_config = cx.get_subscription_config().dust_generations;
        let batch_size = dust_generations_config.batch_size;
        let max_snapshot_age = dust_generations_config.max_snapshot_age;
        let network_id = cx.get_network_id();
        let quotas = cx.get_subscription_quotas();
        let per_connection_counter = cx.get_per_connection_counter();

        try_stream! {
            let _quota_guard = quotas
                .try_acquire(per_connection_counter, None)
                .map_err_into_client_error(|| "subscription limit exceeded")?;

            let dust_address_bytes = dust_address
                .try_into_domain(network_id)
                .map_err_into_client_error(|| "invalid bech32m dust address")?;

            let block_hash = block_hash
                .hex_decode()
                .map_err_into_client_error(|| "invalid block hash")?;

            // Pin the whole snapshot to one block: load the ledger state at `block_hash` so the
            // generation tree (and every collapsed update built from it) reflects the state as of
            // that block. This is what makes the response deterministic and free of tip-drift.
            let (snapshot_block_id, snapshot_height, protocol_version, ledger_state_key) = storage
                .get_ledger_state_at(block_hash)
                .await
                .map_err_into_server_error(|| "get ledger state at block")?
                .some_or_client_error(|| "unknown block hash")?;

            // The chain-indexer only keeps the ledger states of the most recent
            // ledger_state_retention blocks loadable (older ones are garbage collected), so
            // reject stale snapshots up front with a re-subscribe hint instead of failing
            // mid-stream on culled state.
            let latest_height = storage
                .get_latest_block()
                .await
                .map_err_into_server_error(|| "get latest block")?
                .map(|block| block.height)
                .unwrap_or_default();
            (latest_height.saturating_sub(snapshot_height) <= max_snapshot_age)
                .then_some(())
                .some_or_client_error(|| {
                    "block hash is older than the snapshot freshness window, re-subscribe with \
                     a more recent block hash"
                })?;

            // Verify the root node is still in the ledger DB before loading: loading culled
            // state panics inside storage-core rather than returning an error, so this check is
            // what turns a gc'd snapshot (possible when max_snapshot_age is misconfigured to
            // exceed the chain-indexer's ledger_state_retention) into a clean client error.
            LedgerState::root_loadable(&ledger_state_key, protocol_version.ledger_version())
                .map_err_into_server_error(|| "check ledger state availability")?
                .then_some(())
                .some_or_client_error(|| {
                    "ledger state for this block is no longer available, re-subscribe with \
                     a more recent block hash"
                })?;

            let ledger_state =
                LedgerState::load(&ledger_state_key, protocol_version.ledger_version())
                    .map_err_into_server_error(|| "load ledger state at block")?;

            // Exclusive end of the generation tree at this block.
            let end_index = ledger_state.dust_generations_first_free();
            let last_index = end_index.saturating_sub(1);

            // 1. dtime delta: the wallet's own generations spent in `(cutoff, block_hash]`. Map the
            //    cutoff height to the internal block id; `snapshot_block_id` bounds the delta at
            //    `block_hash` so the response is deterministic (independent of the current tip).
            //    `0` means "from the start" (no prior sync).
            let cutoff_block_id = if dtime_cutoff_height == 0 {
                0
            } else {
                let height = u32::try_from(dtime_cutoff_height)
                    .map_err_into_client_error(|| "dtime_cutoff_height out of range")?;
                storage
                    .get_block_by_height(height)
                    .await
                    .map_err_into_server_error(|| "get block by height for dtime cutoff")?
                    .map(|block| block.id)
                    .unwrap_or(0)
            };

            let updates = storage
                .get_dust_generation_dtime_updates(
                    &dust_address_bytes,
                    cutoff_block_id,
                    snapshot_block_id,
                    0,
                    batch_size,
                )
                .await;
            let mut updates = pin!(updates);
            while let Some(update) = updates
                .try_next()
                .await
                .map_err_into_server_error(|| "get next dtime update")?
            {
                yield DustGenerationsEvent::DustGenerationDtimeUpdateItem(dtime_update_item(update));
            }

            // 2. The wallet's owned generation entries up to the block, interleaved with collapsed
            //    Merkle tree updates (pinned to the block) filling the non-owned gaps.
            let mut cursor = 0u64;
            let entries = storage
                .get_dust_generation_entries(&dust_address_bytes, 0, last_index, batch_size)
                .await;
            let mut entries = pin!(entries);
            while let Some(entry) = entries
                .try_next()
                .await
                .map_err_into_server_error(|| "get next dust generation entry")?
            {
                let collapsed_merkle_tree = make_collapsed_update(
                    &ledger_state,
                    cursor,
                    entry.generation_mt_index,
                    protocol_version,
                )?;

                cursor = entry.generation_mt_index + 1;

                yield DustGenerationsEvent::DustGenerationsItem(DustGenerationsItem {
                    commitment_mt_index: entry.commitment_mt_index,
                    generation_mt_index: entry.generation_mt_index,
                    owner: entry.owner.hex_encode(),
                    value: entry.value.to_string(),
                    initial_value: entry.initial_value.to_string(),
                    backing_night: entry.backing_night.hex_encode(),
                    ctime: entry.ctime,
                    transaction_id: entry.transaction_id,
                    transaction_hash: entry.transaction_hash.hex_encode(),
                    collapsed_merkle_tree,
                });
            }

            // 3. Final collapsed update covering the remaining range, then complete the snapshot.
            //    An empty generation tree (no leaves) has no final segment.
            let final_update = if end_index == 0 {
                None
            } else {
                make_final_collapsed_update(&ledger_state, cursor, last_index, protocol_version)?
            };
            yield DustGenerationsEvent::DustGenerationsProgress(DustGenerationsProgress {
                highest_index: last_index,
                collapsed_merkle_tree: final_update,
            });
        }
    }
}

/// Compute a collapsed Merkle tree update to fill the gap between `cursor` and `entry_index`,
/// built from the block-pinned ledger state.
fn make_collapsed_update(
    ledger_state: &LedgerState,
    cursor: u64,
    entry_index: u64,
    protocol_version: ProtocolVersion,
) -> ApiResult<Option<MerkleTreeCollapsedUpdate>> {
    if cursor >= entry_index || entry_index == 0 {
        return Ok(None);
    }

    let update = ledger_state
        .dust_generations_collapsed_update(cursor, entry_index - 1, protocol_version)
        .map_err_into_server_error(|| "create dust generations collapsed update")?;

    Ok(Some(update.into()))
}

/// Compute the final collapsed Merkle tree update covering the remaining range, built from the
/// block-pinned ledger state.
fn make_final_collapsed_update(
    ledger_state: &LedgerState,
    cursor: u64,
    end_index: u64,
    protocol_version: ProtocolVersion,
) -> ApiResult<Option<MerkleTreeCollapsedUpdate>> {
    if cursor > end_index {
        return Ok(None);
    }

    let update = ledger_state
        .dust_generations_collapsed_update(cursor, end_index, protocol_version)
        .map_err_into_server_error(|| "create final dust generations collapsed update")?;

    Ok(Some(update.into()))
}

/// Convert a domain dtime update entry into the GraphQL item.
fn dtime_update_item(update: DustGenerationDtimeUpdateEntry) -> DustGenerationDtimeUpdateItem {
    DustGenerationDtimeUpdateItem {
        generation_mt_index: update.generation_mt_index,
        owner: update.owner.hex_encode(),
        night_utxo_hash: update.night_utxo_hash.hex_encode(),
        new_dtime: update.new_dtime,
        transaction_id: update.transaction_id,
        transaction_hash: update.transaction_hash.hex_encode(),
        tree_insertion_path: update.tree_insertion_path.hex_encode(),
    }
}
