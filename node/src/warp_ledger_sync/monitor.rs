// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! M2.1 — Warp-completion monitor & target capture.
//!
//! Spawned for the lifetime of the node. On a **full sync** the warp path is never observed and the
//! task exits without ever arming the gate. On a **warp sync** it arms the gate, waits for warp +
//! state-sync to finish, captures the target block N, drives the client driver to recover + verify +
//! import the ledger arena, then releases the gate so AURA may author.

use std::{sync::Arc, time::Duration};

use sc_client_api::{Backend, StorageProvider};
use sc_network::{NetworkPeers, NetworkRequest, NetworkStatusProvider, PeerId, ProtocolName};
use sc_network_sync::SyncingService;
use sp_blockchain::HeaderBackend;
use sp_consensus::SyncOracle;
use sp_runtime::traits::Block as BlockT;

use super::{LOG_TARGET, client::LedgerSyncClient, oracle::RecoveryGate};

/// How often to poll sync status. Short so we reliably observe `warp_sync == Some(..)` while warp
/// is in progress (warp takes many seconds), and arm the gate well before warp completes.
const POLL_INTERVAL: Duration = Duration::from_secs(2);
/// Backoff between failed full recovery attempts.
const RETRY_DELAY: Duration = Duration::from_secs(10);

/// Run the recovery monitor to completion (warp path) or early exit (full sync). Intended to be
/// spawned as a non-essential task.
pub async fn run_recovery_monitor<B, Client, BE, Network>(
	client: Arc<Client>,
	sync_service: Arc<SyncingService<B>>,
	network: Arc<Network>,
	gate: Arc<RecoveryGate>,
	protocol_name: ProtocolName,
	unified: bool,
) where
	B: BlockT,
	BE: Backend<B> + 'static,
	Client: HeaderBackend<B> + StorageProvider<B, BE> + Send + Sync + 'static,
	Network: NetworkRequest + NetworkStatusProvider + NetworkPeers + Send + Sync + ?Sized + 'static,
{
	// 1. Detect the warp path and wait for warp + state-sync to finish. We check status *before*
	//    sleeping so we observe `warp_sync == Some(..)` early (it stays `Some` throughout the
	//    multi-second warp), and arm the gate the moment warp is seen — so AURA is gated through the
	//    whole post-warp window (the inner oracle already gates during warp).
	//
	//    Completion is keyed on `state_sync done + finalized_state present` rather than catching the
	//    exact `WarpSyncPhase::Complete` tick (which can be transient): once warp's state-sync has
	//    populated a finalized state, the trie anchor (`StateKey`) we verify against exists.
	let mut saw_warp = false;
	let (target_hash, target_number) = loop {
		let status = sync_service.status().await.ok();

		if let Some(status) = &status {
			if status.warp_sync.is_some() && !saw_warp {
				saw_warp = true;
				gate.arm();
				log::info!(
					target: LOG_TARGET,
					"Warp sync detected; ledger arena recovery armed (authoring gated until verified)"
				);
			}
		}

		if saw_warp {
			let state_sync_done = status.as_ref().map(|s| s.state_sync.is_none()).unwrap_or(false);
			if state_sync_done {
				if let Some(target) = client.info().finalized_state {
					break target;
				}
			}
		} else {
			// Full-sync path: once the node is no longer major-syncing, ledger recovery is never
			// needed — the arena was built block-by-block. Exit without arming.
			if !sync_service.is_major_syncing() {
				log::debug!(target: LOG_TARGET, "Full sync in progress; ledger arena recovery not required");
				return;
			}
		}

		tokio::time::sleep(POLL_INTERVAL).await;
	};
	log::info!(
		target: LOG_TARGET,
		"Recovering ledger arena at warp target #{target_number} ({target_hash:?})"
	);

	// 3. Recover + verify + import, retrying across the current peer set until one succeeds.
	let driver = LedgerSyncClient::new(client, network.clone(), protocol_name, unified);
	loop {
		let peers = recovery_candidate_peers(&*network, &sync_service).await;
		match driver.recover(target_hash, &peers).await {
			Ok(()) => break,
			Err(e) => {
				log::warn!(target: LOG_TARGET, "Ledger arena recovery attempt failed: {e}; retrying");
				tokio::time::sleep(RETRY_DELAY).await;
			},
		}
	}

	// 4. Release the gate: opens both the authoring oracle and the block-import gate.
	gate.mark_ledger_verified();
	log::info!(target: LOG_TARGET, "Ledger arena recovered + verified; authoring + import gate released");
}

/// Gather candidate peers to recover the ledger arena from, sourced from the **network** layer
/// (currently-connected libp2p peers + reserved nodes) rather than [`SyncingService::peers_info`].
///
/// The sync-peer list is emptied by `chain_sync.restart()`, which benign post-warp `UnknownParent`
/// block announcements trigger repeatedly once the servers are producing every 6s. A monitor that
/// reads sync peers therefore sees "no peers" while the libp2p connections (and any reserved nodes)
/// are still fully up — the cause of the 1000-scale recovery stall. Network-level peers survive that
/// churn. `peers_info()` is kept only as a last-resort fallback if the network layer reports none.
async fn recovery_candidate_peers<B, Network>(
	network: &Network,
	sync_service: &SyncingService<B>,
) -> Vec<PeerId>
where
	B: BlockT,
	Network: NetworkStatusProvider + NetworkPeers + ?Sized,
{
	let mut peers = std::collections::HashSet::new();

	// Currently-connected libp2p peers (the `connected_peers` map is keyed by the base58 PeerId).
	if let Ok(state) = network.network_state().await {
		peers.extend(state.connected_peers.keys().filter_map(|id| id.parse::<PeerId>().ok()));
	}
	// Reserved nodes pinned by the operator (e.g. `--reserved-nodes`) also survive sync restarts.
	if let Ok(reserved) = network.reserved_peers().await {
		peers.extend(reserved);
	}
	// Fallback: only if the network layer reported nothing usable.
	if peers.is_empty() {
		if let Ok(info) = sync_service.peers_info().await {
			peers.extend(info.into_iter().map(|(peer, _)| peer));
		}
	}

	peers.into_iter().collect()
}
