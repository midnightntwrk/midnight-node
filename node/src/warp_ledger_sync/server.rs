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

//! Ledger-sync server handler.
//!
//! Answers [`LedgerSyncRequest`]s from warp-syncing peers by serializing this (fully synced) node's
//! `Ledger`-rooted arena snapshot at the requested finalized block, Snappy-compressing it, and
//! serving the requested compressed byte range. Patterned on substrate's `state_request_handler.rs`.
//!
//! Verification is the *client's* job: the server is untrusted, so it performs no crypto —
//! it only serves bytes whose recomputed root the client checks against the on-chain `StateKey`.

use std::{marker::PhantomData, sync::Arc, time::Duration};

use futures::StreamExt;
use parity_scale_codec::{Decode, Encode};
use sc_client_api::{Backend, StorageProvider};
use sc_network::{
	MAX_RESPONSE_SIZE, NetworkBackend,
	request_responses::{IncomingRequest, OutgoingResponse},
};
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};

use super::{
	LOG_TARGET,
	protocol::{LedgerSyncRequest, build_response, compress_snapshot, ledger_sync_protocol_name},
	read_state_key,
};

/// Max bytes a peer may put in a single request (the request is tiny: a hash + two integers).
const MAX_REQUEST_SIZE: u64 = 1024;
/// Request timeout, matching substrate's state protocol.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(40);

/// Handler for incoming ledger-sync requests from warp-syncing peers.
///
/// Memoizes the serialized blob for the most-recently-served target block so that the many
/// byte-range requests a single client makes while paging the blob do not each re-serialize the
/// (multi-million node) arena.
pub struct LedgerSyncRequestHandler<B: BlockT, Client, BE> {
	client: Arc<Client>,
	/// Whether the ledger arena uses the unified ParityDb layout (selects the DB instantiation the
	/// serializer dispatches to — see [`midnight_node_ledger::serialize_ledger_snapshot`]).
	unified: bool,
	request_receiver: async_channel::Receiver<IncomingRequest>,
	/// `(target_block, compressed serialized blob)` memo for the last block served.
	cache: Option<(B::Hash, CachedSnapshot)>,
	_phantom: PhantomData<BE>,
}

#[derive(Clone)]
struct CachedSnapshot {
	compressed_blob: Arc<Vec<u8>>,
	raw_len: u64,
}

impl<B, Client, BE> LedgerSyncRequestHandler<B, Client, BE>
where
	B: BlockT,
	BE: Backend<B> + 'static,
	Client: HeaderBackend<B> + StorageProvider<B, BE> + Send + Sync + 'static,
{
	/// Build the protocol config to register on `net_config` before `build_network`, plus — when
	/// `serve` is true — the handler to spawn via [`run`](Self::run).
	///
	/// `serve` gates the **server** side only. Validators pass `serve = false` unless they opt in
	/// via `--serve-warp-ledger-sync`: serializing the multi-million-node arena is this
	/// protocol's most CPU-expensive operation, and it must never compete with a validator's
	/// authoring/finality duties (an easy remote DoS vector). A non-serving node advertises no
	/// inbound queue, so the network routes no requests to it — but the protocol is still
	/// registered, so the node can act as a warp-sync *client* and recover its own arena. Returns
	/// `None` for the handler when not serving.
	pub fn new<N: NetworkBackend<B, <B as BlockT>::Hash>>(
		genesis_hash: B::Hash,
		fork_id: Option<&str>,
		client: Arc<Client>,
		unified: bool,
		num_peer_hint: usize,
		serve: bool,
	) -> (Option<Self>, N::RequestResponseProtocolConfig) {
		// Only advertise an inbound queue (and build a handler) when this node serves. A `None`
		// inbound queue means the network layer routes no requests to us; we can still *send*
		// requests on the protocol as a warp-sync client.
		let (inbound_queue, handler) = if serve {
			// Reserve one in-flight request slot per peer.
			let capacity = std::cmp::max(num_peer_hint, 1);
			let (tx, request_receiver) = async_channel::bounded(capacity);
			let handler =
				Self { client, unified, request_receiver, cache: None, _phantom: PhantomData };
			(Some(tx), Some(handler))
		} else {
			(None, None)
		};

		let config = N::request_response_config(
			ledger_sync_protocol_name(genesis_hash, fork_id).into(),
			Vec::new(),
			MAX_REQUEST_SIZE,
			MAX_RESPONSE_SIZE,
			REQUEST_TIMEOUT,
			inbound_queue,
		);

		(handler, config)
	}

	/// Run the request-handling loop until the inbound queue closes.
	pub async fn run(mut self) {
		while let Some(IncomingRequest { peer, payload, pending_response }) =
			self.request_receiver.next().await
		{
			let result = match self.handle_request(&payload) {
				Ok(bytes) => Ok(bytes),
				Err(e) => {
					log::debug!(target: LOG_TARGET, "ledger-sync request from {peer} failed: {e}");
					Err(())
				},
			};
			// A failed send just means the peer disconnected; nothing to do.
			let _ = pending_response.send(OutgoingResponse {
				result,
				reputation_changes: Vec::new(),
				sent_feedback: None,
			});
		}
	}

	fn handle_request(&mut self, payload: &[u8]) -> Result<Vec<u8>, HandleError> {
		let req = LedgerSyncRequest::<B::Hash>::decode(&mut &payload[..])?;
		let snapshot = self.blob_for(req.target_hash)?;
		Ok(build_response(&snapshot.compressed_blob, snapshot.raw_len, req.offset, req.max_len)
			.encode())
	}

	/// Return the compressed serialized `Ledger`-rooted blob for `target`, building and memoizing it
	/// on a cache miss. Rejects unknown or not-yet-finalized blocks.
	fn blob_for(&mut self, target: B::Hash) -> Result<CachedSnapshot, HandleError> {
		if let Some((cached, snapshot)) = &self.cache
			&& *cached == target
		{
			return Ok(snapshot.clone());
		}

		// Only serve finalized blocks whose state we hold: an unknown hash or a block beyond our
		// finalized number is rejected (the warp target is always finalized).
		let header = self.client.header(target)?.ok_or(HandleError::UnknownBlock)?;
		if *header.number() > self.client.info().finalized_number {
			return Err(HandleError::NotFinalized);
		}

		// Read the raw `pallet_midnight::StateKey` at the target block.
		let state_key = read_state_key::<B, Client, BE>(&self.client, target)?
			.ok_or(HandleError::NoStateKey)?;

		let blob = midnight_node_ledger::serialize_ledger_snapshot(self.unified, &state_key)
			.map_err(HandleError::Serialize)?;
		let raw_len = blob.len() as u64;
		let compressed_blob = compress_snapshot(&blob).map_err(HandleError::Compress)?;
		log::debug!(
			target: LOG_TARGET,
			"Serialized ledger snapshot for {target:?}: {} bytes raw, {} bytes compressed",
			raw_len,
			compressed_blob.len()
		);

		let snapshot = CachedSnapshot { compressed_blob: Arc::new(compressed_blob), raw_len };
		self.cache = Some((target, snapshot.clone()));
		Ok(snapshot)
	}
}

#[derive(Debug, thiserror::Error)]
enum HandleError {
	#[error("failed to decode request / state key: {0}")]
	Decode(#[from] parity_scale_codec::Error),
	#[error("blockchain error: {0}")]
	Client(#[from] sp_blockchain::Error),
	#[error("requested block is unknown")]
	UnknownBlock,
	#[error("requested block is not finalized")]
	NotFinalized,
	#[error("pallet StateKey not present at requested block")]
	NoStateKey,
	#[error("failed to serialize ledger snapshot: {0}")]
	Serialize(String),
	#[error("failed to compress ledger snapshot: {0}")]
	Compress(snap::Error),
}
