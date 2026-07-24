// This file is part of midnight-node.
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

//! Block-production / committee regression guard for the aura-fork removal
//! (#1700) and pallet-session migration (#1800).
//!
//! Over the most-recent finalized blocks it asserts the *deterministic*
//! invariants (the statistical/experimental analysis lives in the manual
//! `local-environment` `block-stats` tool):
//!
//! - **Authorship legitimacy** — every finalized block's AURA author
//!   (`aura.authorities.at(parent)[slot % len]`) is a member of the committee
//!   the committee-management RPC reports for the parent's sidechain epoch.
//!   Since the block is finalized (the node's verifier already checked its
//!   seal), this confirms only legitimate current-committee members produce.
//! - **Committee source agreement** — per epoch, on-chain `aura.authorities`
//!   equals `sidechain_getEpochCommittee` (independent sources agree).
//! - **session/aura alignment** — `session.validators` and `aura.authorities`
//!   have equal length (the #1800 session→aura handoff).
//! - **session index monotonic** and **finalized heights contiguous**.
//!
//! It reads the parent block's state on purpose: the first block of a session
//! is produced by the *previous* committee (rotation runs during that block),
//! so the parent-epoch committee is the right thing to check against — this is
//! what makes the assertions boundary-safe.
//!
//! `#[ignore]` so the parallel suite skips it; `+local-env-ci` runs it as a
//! dedicated final step (after the suite + health check) to maximise the block
//! count it can analyse.

use midnight_node_e2e::api::midnight::MidnightClient;
use midnight_node_e2e::config::Settings;
use midnight_node_e2e::e2e_test;
use std::collections::{HashMap, HashSet};
use subxt::utils::H256;

/// How many of the newest finalized blocks to analyse. Sized to cover a full
/// CI run's production (~120 blocks over the ~25 min `+local-env-ci` job, since
/// this runs as the final step) with headroom, while still bounding the read on
/// a long-lived local chain (thousands of blocks) — cost is ~linear, ~75ms/block.
const WINDOW: u64 = 200;
/// Minimum blocks required to run; below this the chain is too young — soft-skip.
const FLOOR: usize = 12;
/// Fallback sidechain slots-per-epoch if it can't be derived from status.
const DEFAULT_SLOTS_PER_EPOCH: u64 = 5;

#[e2e_test]
#[ignore = "post-suite: run as the final +local-env-ci step for max block count"]
async fn block_production_and_committee_invariants() {
    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    // --- Sidechain slots-per-epoch: epoch = floor(slot / spe) ⇒ slot/epoch ≈ spe.
    let (sc_epoch_now, sc_slot_now) = client
        .get_sidechain_epoch_and_slot()
        .await
        .expect("sidechain_getStatus");
    let spe = if sc_epoch_now > 0 {
        ((sc_slot_now as f64) / (sc_epoch_now as f64)).round() as u64
    } else {
        DEFAULT_SLOTS_PER_EPOCH
    }
    .max(1);
    tracing::info!("slots-per-epoch = {spe}");

    // --- Identity bridge: getEpochCommittee returns sidechain keys, aura.authorities
    // returns aura keys — map between them via the Ariadne permissioned candidates.
    let mc_epoch = client.get_mainchain_epoch().await.expect("mainchain epoch");
    let ariadne = client
        .get_ariadne_parameters(mc_epoch, None)
        .await
        .expect("ariadne parameters");
    let mut aura_to_sidechain: HashMap<String, String> = HashMap::new();
    for cand in ariadne.permissioned_candidates.unwrap_or_default() {
        let sc = cand["sidechainPublicKey"].as_str().map(str::to_lowercase);
        let aura = cand["keys"]["aura"].as_str().map(str::to_lowercase);
        if let (Some(sc), Some(aura)) = (sc, aura) {
            aura_to_sidechain.insert(aura, sc);
        }
    }
    assert!(
        !aura_to_sidechain.is_empty(),
        "could not build aura↔sidechain key map from Ariadne permissioned candidates \
         (mc epoch {mc_epoch}) — cannot relate aura authorities to the committee RPC"
    );

    // --- Collect the newest finalized window.
    let head = client
        .get_finalized_block_number()
        .await
        .expect("finalized head");
    let start = head.saturating_sub(WINDOW).max(1);
    let count = (head - start + 1) as usize;
    if count < FLOOR {
        tracing::warn!(
            "block-production audit: only {count} finalized block(s) (#{start}..#{head}); \
             need {FLOOR} — chain too young, soft-skipping"
        );
        return;
    }
    tracing::info!("auditing finalized blocks #{start}..#{head} ({count} blocks)");

    // Per-epoch caches (aura.authorities & the committee are constant within an epoch).
    let mut committee_by_epoch: HashMap<u64, Vec<String>> = HashMap::new(); // sidechain keys
    let mut aura_by_epoch: HashMap<u64, Vec<String>> = HashMap::new(); // aura keys
    let mut val_len_by_epoch: HashMap<u64, usize> = HashMap::new();
    let mut source_checked: HashSet<u64> = HashSet::new();

    let mut prev: Option<(H256, u64)> = None; // (hash, slot) of the previous height
    let mut last_session: Option<u32> = None;
    let mut blocks_checked = 0usize;
    let mut boundary_blocks = 0usize;

    for height in start..=head {
        let hash = client
            .get_block_hash_at_height(height as u32)
            .await
            .expect("block hash at height");
        let info = client
            .get_block_digest_info(hash)
            .await
            .expect("block digest info");

        let Some(slot) = info.aura_slot else {
            // No AURA pre-digest (e.g. genesis) — break the contiguity chain.
            prev = None;
            continue;
        };

        // session index monotonic (read at this block).
        let session_index = client
            .get_session_index_at(hash)
            .await
            .expect("session index");
        if let Some(prev_idx) = last_session {
            assert!(
                session_index >= prev_idx,
                "session index went backwards at #{height}: {prev_idx} -> {session_index}"
            );
        }
        last_session = Some(session_index);

        if let Some((parent_hash, parent_slot)) = prev {
            // Finalized heights must be contiguous.
            assert_eq!(
                parent_hash,
                info.parent_hash,
                "non-contiguous finalized chain: #{height}'s parent != #{}",
                height - 1
            );

            // The committee that PRODUCED this block is the parent-epoch committee.
            let parent_epoch = parent_slot / spe;
            let own_epoch = slot / spe;
            if parent_epoch != own_epoch {
                boundary_blocks += 1;
            }

            // getEpochCommittee(parent_epoch) — sidechain keys.
            if !committee_by_epoch.contains_key(&parent_epoch) {
                let resp = client
                    .get_epoch_committee(parent_epoch)
                    .await
                    .expect("getEpochCommittee");
                let members = resp
                    .committee
                    .iter()
                    .map(|m| m.sidechain_pub_key.to_lowercase())
                    .collect();
                committee_by_epoch.insert(parent_epoch, members);
            }
            let committee = &committee_by_epoch[&parent_epoch];

            // aura.authorities at parent — aura keys.
            if !aura_by_epoch.contains_key(&parent_epoch) {
                let auth = client
                    .get_aura_authorities_hex_at(parent_hash)
                    .await
                    .expect("aura authorities");
                aura_by_epoch.insert(
                    parent_epoch,
                    auth.into_iter().map(|s| s.to_lowercase()).collect(),
                );
            }
            let aura = &aura_by_epoch[&parent_epoch];
            assert!(
                !aura.is_empty(),
                "empty aura.authorities at parent of #{height}"
            );

            // session.validators length == aura.authorities length (#1800 handoff).
            if !val_len_by_epoch.contains_key(&parent_epoch) {
                let len = client
                    .get_session_validators_len_at(parent_hash)
                    .await
                    .expect("session validators len");
                val_len_by_epoch.insert(parent_epoch, len);
            }
            assert_eq!(
                val_len_by_epoch[&parent_epoch],
                aura.len(),
                "session.validators len != aura.authorities len at parent of #{height} \
                 (epoch {parent_epoch})"
            );

            // Authorship legitimacy: author = aura.authorities[slot % len]; its sidechain
            // key must be in the committee the RPC reports for the parent epoch.
            let author_aura = &aura[(slot % aura.len() as u64) as usize];
            match aura_to_sidechain.get(author_aura) {
                Some(author_sc) => assert!(
                    committee.iter().any(|m| m == author_sc),
                    "block #{height} author {author_aura} (sidechain {author_sc}) is NOT in \
                     getEpochCommittee(epoch {parent_epoch}) = {committee:?}"
                ),
                None => tracing::warn!(
                    "block #{height}: author aura key {author_aura} not in Ariadne map \
                     (likely a just-removed candidate); skipping its membership check"
                ),
            }

            // Committee source agreement (once per epoch): aura.authorities == getEpochCommittee.
            if source_checked.insert(parent_epoch) {
                let mapped: Option<Vec<String>> = aura
                    .iter()
                    .map(|a| aura_to_sidechain.get(a).cloned())
                    .collect();
                match mapped {
                    Some(mut lhs) => {
                        let mut rhs = committee.clone();
                        lhs.sort();
                        rhs.sort();
                        assert_eq!(
                            lhs, rhs,
                            "aura.authorities != getEpochCommittee for epoch {parent_epoch}"
                        );
                    }
                    None => tracing::warn!(
                        "epoch {parent_epoch}: an aura authority is absent from the Ariadne map; \
                         skipping committee-source agreement for this epoch"
                    ),
                }
            }

            blocks_checked += 1;
        }

        prev = Some((hash, slot));
    }

    assert!(
        blocks_checked > 0,
        "no blocks were checked — window collection produced nothing usable"
    );
    tracing::info!(
        "✅ block-production audit passed: {blocks_checked} block(s), \
         {} epoch(s), {boundary_blocks} session-boundary block(s)",
        source_checked.len()
    );
}
