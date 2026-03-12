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

//! RPC endpoint for querying peer reputation and ban status.

use futures::channel::oneshot;
use jsonrpsee::{
	core::{RpcResult, async_trait},
	proc_macros::rpc,
	types::error::{ErrorObject, INTERNAL_ERROR_CODE, INVALID_PARAMS_CODE},
};
use sc_network::{ReputationChange, service::traits::NetworkPeers};
use sc_rpc::system::Request;
use sc_rpc_api::check_if_safe;
use sc_utils::mpsc::TracingUnboundedSender;
use serde::{Deserialize, Serialize};
use sp_runtime::traits::Block as BlockT;
use std::sync::Arc;

/// Reputation threshold below which a peer is considered banned.
/// Mirrors `BANNED_THRESHOLD` from `sc_network::peer_store`.
const BANNED_THRESHOLD: i32 = 71 * (i32::MIN / 100);

/// Peer information enriched with reputation and ban status.
///
/// JSON Schema for this type is provided manually in the OpenRPC document
/// because the generic type parameters (`Hash`, `Number`) do not implement
/// `JsonSchema`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerReputationInfo<Hash, Number> {
	/// Peer ID (base58-encoded).
	pub peer_id: String,
	/// Roles advertised by the peer.
	pub roles: String,
	/// Best block hash known for this peer (0x-prefixed hex, 32 bytes).
	pub best_hash: Hash,
	/// Best block number known for this peer.
	pub best_number: Number,
	/// Current reputation score.
	pub reputation: i32,
	/// Whether the peer is currently banned (reputation below threshold).
	pub is_banned: bool,
}

#[rpc(server, namespace = "network")]
pub trait PeerInfoApi<Hash, Number> {
	/// Returns reputation info for all connected peers.
	#[method(name = "peerReputations")]
	async fn peer_reputations(&self) -> RpcResult<Vec<PeerReputationInfo<Hash, Number>>>;

	/// Returns reputation info for a single peer by its base58-encoded peer ID.
	#[method(name = "peerReputation")]
	async fn peer_reputation(&self, peer_id: String)
	-> RpcResult<PeerReputationInfo<Hash, Number>>;

	/// Unbans a peer by boosting its reputation above the ban threshold.
	#[method(name = "unbanPeer", with_extensions)]
	async fn unban_peer(&self, peer_id: String) -> RpcResult<()>;
}

pub struct PeerInfoRpc<Block: BlockT> {
	network: Arc<dyn NetworkPeers + Send + Sync>,
	system_rpc_tx: TracingUnboundedSender<Request<Block>>,
}

impl<Block: BlockT> PeerInfoRpc<Block> {
	pub fn new(
		network: Arc<dyn NetworkPeers + Send + Sync>,
		system_rpc_tx: TracingUnboundedSender<Request<Block>>,
	) -> Self {
		Self { network, system_rpc_tx }
	}
}

#[async_trait]
impl<Block>
	PeerInfoApiServer<
		Block::Hash,
		<<Block as BlockT>::Header as sp_runtime::traits::Header>::Number,
	> for PeerInfoRpc<Block>
where
	Block: BlockT,
{
	async fn peer_reputations(
		&self,
	) -> RpcResult<
		Vec<
			PeerReputationInfo<
				Block::Hash,
				<<Block as BlockT>::Header as sp_runtime::traits::Header>::Number,
			>,
		>,
	> {
		let (tx, rx) = oneshot::channel();
		self.system_rpc_tx.unbounded_send(Request::Peers(tx)).map_err(|e| {
			ErrorObject::owned(
				INTERNAL_ERROR_CODE,
				format!("Failed to send peers request: {e}"),
				None::<()>,
			)
		})?;

		let peers = rx.await.map_err(|e| {
			ErrorObject::owned(
				INTERNAL_ERROR_CODE,
				format!("Failed to receive peers: {e}"),
				None::<()>,
			)
		})?;

		let results = peers
			.into_iter()
			.map(|peer| {
				let pid: sc_network::service::traits::PeerId =
					peer.peer_id.parse().unwrap_or(sc_network::service::traits::PeerId::random());
				let reputation = self.network.peer_reputation(&pid);
				PeerReputationInfo {
					peer_id: peer.peer_id,
					roles: peer.roles,
					best_hash: peer.best_hash,
					best_number: peer.best_number,
					reputation,
					is_banned: reputation < BANNED_THRESHOLD,
				}
			})
			.collect();

		Ok(results)
	}

	async fn peer_reputation(
		&self,
		peer_id: String,
	) -> RpcResult<
		PeerReputationInfo<
			Block::Hash,
			<<Block as BlockT>::Header as sp_runtime::traits::Header>::Number,
		>,
	> {
		let pid: sc_network::service::traits::PeerId = peer_id.parse().map_err(|_| {
			ErrorObject::owned(
				INVALID_PARAMS_CODE,
				format!("Invalid peer ID: {peer_id}"),
				None::<()>,
			)
		})?;

		let (tx, rx) = oneshot::channel();
		self.system_rpc_tx.unbounded_send(Request::Peers(tx)).map_err(|e| {
			ErrorObject::owned(
				INTERNAL_ERROR_CODE,
				format!("Failed to send peers request: {e}"),
				None::<()>,
			)
		})?;

		let peers = rx.await.map_err(|e| {
			ErrorObject::owned(
				INTERNAL_ERROR_CODE,
				format!("Failed to receive peers: {e}"),
				None::<()>,
			)
		})?;

		let peer = peers.into_iter().find(|p| p.peer_id == peer_id).ok_or_else(|| {
			ErrorObject::owned(
				INVALID_PARAMS_CODE,
				format!("Peer not found: {peer_id}"),
				None::<()>,
			)
		})?;

		let reputation = self.network.peer_reputation(&pid);
		Ok(PeerReputationInfo {
			peer_id: peer.peer_id,
			roles: peer.roles,
			best_hash: peer.best_hash,
			best_number: peer.best_number,
			reputation,
			is_banned: reputation < BANNED_THRESHOLD,
		})
	}

	async fn unban_peer(&self, ext: &jsonrpsee::Extensions, peer_id: String) -> RpcResult<()> {
		check_if_safe(ext)?;

		let pid: sc_network::service::traits::PeerId = peer_id.parse().map_err(|_| {
			ErrorObject::owned(
				INVALID_PARAMS_CODE,
				format!("Invalid peer ID: {peer_id}"),
				None::<()>,
			)
		})?;

		self.network
			.report_peer(pid, ReputationChange::new(i32::MAX, "manual unban via RPC"));

		Ok(())
	}
}
