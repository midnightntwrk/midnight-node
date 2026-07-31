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
//!
//! ## Abuse resistance
//!
//! Serializing the arena is by far the most expensive thing this node does on behalf of a remote
//! peer, and the request that triggers it is ~44 bytes. Three bounds keep that asymmetry from being
//! a remote CPU amplifier, in increasing order of importance:
//!
//! 1. **Snapshot memo** (`SNAPSHOT_CACHE_ENTRIES`) — the many range requests a client makes while
//!    paging one blob cost one serialization, not one each. Holding several entries also means
//!    concurrent clients converging on *nearby* finalized targets share the work.
//! 2. **Replay penalty** (`MAX_NUMBER_OF_SAME_REQUESTS_PER_PEER`) — an honest client asks for each
//!    `(target, offset)` exactly once. Mirrors substrate's `state_request_handler`.
//! 3. **Per-peer serialization budget** (`MAX_SERIALIZATIONS_PER_PEER`) — the load-bearing one.
//!    A memo of any fixed size is defeated by cycling `target_hash` across more distinct blocks
//!    than it holds, so the budget bounds how much expensive work a single peer can ever induce,
//!    independent of the memo size. It is charged *before* the work, so an over-budget peer is
//!    refused rather than served-and-then-penalised — the reputation change only accelerates
//!    eviction; the refusal is the actual resource bound.

use std::{marker::PhantomData, num::NonZeroUsize, sync::Arc, time::Duration};

use futures::StreamExt;
use lru::LruCache;
use parity_scale_codec::{Decode, Encode};
use sc_client_api::{Backend, StorageProvider};
use sc_network::{
	MAX_RESPONSE_SIZE, NetworkBackend, PeerId, ReputationChange,
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

/// How many times a peer may replay a byte-identical `(target, offset)` request before being
/// penalised. Matches substrate's `state_request_handler`.
const MAX_NUMBER_OF_SAME_REQUESTS_PER_PEER: usize = 2;

/// How many distinct target blocks one peer may make us serialize the arena for.
///
/// An honest warp client needs exactly **one**: its warp target. Every subsequent range request for
/// that target is a memo hit, and a client that fails verification and retries against us re-uses
/// the same target. The headroom covers a peer that legitimately re-warps to a newer target over a
/// long-lived connection (e.g. after its own restart).
const MAX_SERIALIZATIONS_PER_PEER: usize = 4;

/// How many distinct target blocks' compressed snapshots to memoize.
///
/// Deliberately small: this bounds *memory* (each entry is a full compressed arena), while
/// [`MAX_SERIALIZATIONS_PER_PEER`] — not this — bounds the *CPU* an attacker can induce. Three
/// entries let a few clients on slightly different finalized targets share serializations without
/// holding several arena-sized blobs resident indefinitely.
const SNAPSHOT_CACHE_ENTRIES: usize = 3;

mod rep {
	use sc_network::ReputationChange as Rep;

	/// Peer replayed a byte-identical `(target, offset)` request. Same penalty substrate applies
	/// for the same behaviour on the state protocol.
	pub const SAME_REQUEST: Rep = Rep::new(i32::MIN, "Same ledger-sync request multiple times");

	/// Peer induced more full-arena serializations than any honest client needs, by cycling
	/// `target_hash` to defeat the snapshot memo. Heavy but not an instant ban — the refusal in
	/// [`super::LedgerSyncRequestHandler::blob_for`] is the resource bound; this just gets the
	/// peer evicted sooner.
	pub const TARGET_CYCLING: Rep = Rep::new(-(1 << 20), "Ledger-sync target cycling");
}

/// Key of [`LedgerSyncRequestHandler::seen_requests`].
///
/// `Hash`/`Eq` are written by hand rather than derived so the impls don't pick up a spurious
/// `B: Hash + Eq` bound from the generic parameter (as substrate's equivalent does).
struct SeenRequestsKey<B: BlockT> {
	peer: PeerId,
	target: B::Hash,
	offset: u64,
}

impl<B: BlockT> Clone for SeenRequestsKey<B> {
	fn clone(&self) -> Self {
		Self { peer: self.peer, target: self.target, offset: self.offset }
	}
}

impl<B: BlockT> PartialEq for SeenRequestsKey<B> {
	fn eq(&self, other: &Self) -> bool {
		self.peer == other.peer && self.target == other.target && self.offset == other.offset
	}
}

impl<B: BlockT> Eq for SeenRequestsKey<B> {}

impl<B: BlockT> std::fmt::Debug for SeenRequestsKey<B> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("SeenRequestsKey")
			.field("peer", &self.peer)
			.field("target", &self.target)
			.field("offset", &self.offset)
			.finish()
	}
}

impl<B: BlockT> std::hash::Hash for SeenRequestsKey<B> {
	fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
		self.peer.hash(state);
		self.target.hash(state);
		self.offset.hash(state);
	}
}

/// Value of [`LedgerSyncRequestHandler::seen_requests`].
enum SeenRequestsValue {
	/// Seen once, not yet answered.
	First,
	/// Answered `n` times.
	Fulfilled(usize),
}

/// Handler for incoming ledger-sync requests from warp-syncing peers.
///
/// Memoizes serialized blobs so that the many byte-range requests a single client makes while
/// paging a blob do not each re-serialize the (multi-million node) arena, and tracks per-peer
/// request patterns so that neither replaying a range nor cycling target blocks can turn a 44-byte
/// request into unbounded CPU. See the module docs.
pub struct LedgerSyncRequestHandler<B: BlockT, Client, BE> {
	client: Arc<Client>,
	/// Whether the ledger arena uses the unified ParityDb layout (selects the DB instantiation the
	/// serializer dispatches to — see [`midnight_node_ledger::serialize_ledger_snapshot`]).
	unified: bool,
	request_receiver: async_channel::Receiver<IncomingRequest>,
	/// Compressed serialized blobs, keyed by target block.
	snapshot_cache: LruCache<B::Hash, CachedSnapshot>,
	/// Replay detector: how many times each `(peer, target, offset)` has been answered.
	seen_requests: LruCache<SeenRequestsKey<B>, SeenRequestsValue>,
	/// How many arena serializations each peer has already cost us.
	serializations_per_peer: LruCache<PeerId, usize>,
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
	/// via `--serve-warp-ledger-sync`, and any node can opt out with `--no-serve-warp-ledger-sync`:
	/// serializing the multi-million-node arena is this protocol's most CPU-expensive operation, and
	/// it must never compete with a validator's authoring/finality duties (an easy remote DoS
	/// vector). A non-serving node advertises no inbound queue, so the network layer marks the
	/// protocol `Outbound`-only and routes no requests to it — but the protocol is still registered,
	/// so the node can act as a warp-sync *client* and recover its own arena. Returns `None` for the
	/// handler when not serving.
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
			// Two in-flight `(target, offset)` keys per peer, matching substrate's sizing of the
			// same structure; one budget entry per peer.
			let seen_capacity = nonzero(capacity.saturating_mul(2));
			let handler = Self {
				client,
				unified,
				request_receiver,
				snapshot_cache: LruCache::new(nonzero(SNAPSHOT_CACHE_ENTRIES)),
				seen_requests: LruCache::new(seen_capacity),
				serializations_per_peer: LruCache::new(nonzero(capacity)),
				_phantom: PhantomData,
			};
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
			let (result, reputation_changes) = match self.handle_request(&peer, &payload) {
				Ok(bytes) => (Ok(bytes), Vec::new()),
				Err(e) => {
					log::debug!(target: LOG_TARGET, "ledger-sync request from {peer} failed: {e}");
					(Err(()), e.reputation_change().into_iter().collect())
				},
			};
			// A failed send just means the peer disconnected; nothing to do.
			let _ = pending_response.send(OutgoingResponse {
				result,
				reputation_changes,
				sent_feedback: None,
			});
		}
	}

	fn handle_request(&mut self, peer: &PeerId, payload: &[u8]) -> Result<Vec<u8>, HandleError> {
		let req = LedgerSyncRequest::<B::Hash>::decode(&mut &payload[..])?;

		// Replay detection, mirroring substrate's `state_request_handler`: an honest client pages
		// each `(target, offset)` exactly once, so a replay is a broken client or deliberate load.
		let key = SeenRequestsKey { peer: *peer, target: req.target_hash, offset: req.offset };
		match self.seen_requests.get_mut(&key) {
			Some(SeenRequestsValue::First) => {},
			Some(SeenRequestsValue::Fulfilled(requests)) => {
				*requests = requests.saturating_add(1);
				if *requests > MAX_NUMBER_OF_SAME_REQUESTS_PER_PEER {
					return Err(HandleError::SameRequest);
				}
			},
			None => {
				self.seen_requests.put(key.clone(), SeenRequestsValue::First);
			},
		}

		let snapshot = self.blob_for(peer, req.target_hash)?;
		let bytes =
			build_response(&snapshot.compressed_blob, snapshot.raw_len, req.offset, req.max_len)
				.encode();

		// Only now does this count as answered, so a request refused above is not held against the
		// peer as a fulfilment.
		if let Some(value) = self.seen_requests.get_mut(&key)
			&& matches!(value, SeenRequestsValue::First)
		{
			*value = SeenRequestsValue::Fulfilled(1);
		}

		Ok(bytes)
	}

	/// Return the compressed serialized `Ledger`-rooted blob for `target`, building and memoizing it
	/// on a cache miss. Rejects unknown or not-yet-finalized blocks, and refuses peers that have
	/// exhausted their serialization budget.
	fn blob_for(&mut self, peer: &PeerId, target: B::Hash) -> Result<CachedSnapshot, HandleError> {
		if let Some(snapshot) = self.snapshot_cache.get(&target) {
			return Ok(snapshot.clone());
		}

		// Cheap rejections first. An unknown hash or a block beyond our finalized number costs a
		// header lookup, not a serialization, so it consumes no budget and earns no reputation
		// penalty — an honest peer can race finality or a reorg and ask for a block we can't serve.
		let header = self.client.header(target)?.ok_or(HandleError::UnknownBlock)?;
		if *header.number() > self.client.info().finalized_number {
			return Err(HandleError::NotFinalized);
		}

		// Everything past here is the expensive path, so charge it to the peer *before* doing the
		// work: an over-budget peer is refused, not served-then-penalised. A peer cycling targets
		// to defeat the snapshot memo hits this rather than the memo size.
		let spent = self.serializations_per_peer.get(peer).copied().unwrap_or(0);
		if spent >= MAX_SERIALIZATIONS_PER_PEER {
			return Err(HandleError::TargetCycling);
		}
		self.serializations_per_peer.put(*peer, spent + 1);

		// Read the raw `pallet_midnight::StateKey` at the target block.
		let state_key = read_state_key::<B, Client, BE>(&self.client, target)?
			.ok_or(HandleError::NoStateKey)?;

		let blob = midnight_node_ledger::serialize_ledger_snapshot(self.unified, &state_key)
			.map_err(HandleError::Serialize)?;
		let raw_len = blob.len() as u64;
		let compressed_blob = compress_snapshot(&blob).map_err(HandleError::Compress)?;
		log::debug!(
			target: LOG_TARGET,
			"Serialized ledger snapshot for {target:?}: {} bytes raw, {} bytes compressed \
			 (serialization {} of {} for {peer})",
			raw_len,
			compressed_blob.len(),
			spent + 1,
			MAX_SERIALIZATIONS_PER_PEER,
		);

		let snapshot = CachedSnapshot { compressed_blob: Arc::new(compressed_blob), raw_len };
		self.snapshot_cache.put(target, snapshot.clone());
		Ok(snapshot)
	}
}

/// `NonZeroUsize` from a capacity that is only zero if a caller passed nonsense; clamp rather than
/// panic, since these are all "how much to remember" knobs where 1 is a valid answer.
fn nonzero(n: usize) -> NonZeroUsize {
	NonZeroUsize::new(n.max(1)).expect("max(1) is non-zero; qed")
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
	#[error(
		"peer replayed an identical ledger-sync request more than {MAX_NUMBER_OF_SAME_REQUESTS_PER_PEER} times"
	)]
	SameRequest,
	#[error(
		"peer exhausted its budget of {MAX_SERIALIZATIONS_PER_PEER} arena serializations by cycling target blocks"
	)]
	TargetCycling,
}

impl HandleError {
	/// Reputation change to attach when refusing for this reason.
	///
	/// Only the two abuse patterns are penalised. A malformed, unknown-block, or unfinalized-block
	/// request is cheap to reject and an honest peer racing finality or a reorg can produce one, so
	/// penalising those would ban good peers for our own timing.
	fn reputation_change(&self) -> Option<ReputationChange> {
		match self {
			HandleError::SameRequest => Some(rep::SAME_REQUEST),
			HandleError::TargetCycling => Some(rep::TARGET_CYCLING),
			HandleError::Decode(_)
			| HandleError::Client(_)
			| HandleError::UnknownBlock
			| HandleError::NotFinalized
			| HandleError::NoStateKey
			| HandleError::Serialize(_)
			| HandleError::Compress(_) => None,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sc_network::peer_store::BANNED_THRESHOLD;

	#[test]
	fn only_abuse_patterns_are_penalised() {
		// The two abuse patterns carry a penalty...
		assert!(HandleError::SameRequest.reputation_change().is_some());
		assert!(HandleError::TargetCycling.reputation_change().is_some());

		// ...and the reasons an honest peer can hit — racing finality, a reorg, a truncated
		// request — carry none. Penalising these would ban good peers for our own timing.
		assert!(HandleError::UnknownBlock.reputation_change().is_none());
		assert!(HandleError::NotFinalized.reputation_change().is_none());
		assert!(HandleError::NoStateKey.reputation_change().is_none());
		assert!(
			HandleError::Decode(parity_scale_codec::Error::from("truncated"))
				.reputation_change()
				.is_none()
		);
	}

	/// These are all compile-time constants; the assertions exist to catch a future edit that
	/// silently inverts the intended severity ordering, not to test runtime behaviour.
	#[allow(clippy::assertions_on_constants)]
	#[test]
	fn penalty_severities_are_ordered_as_intended() {
		// Target cycling is refused *before* the work happens, so its reputation change only needs
		// to accelerate eviction. Keeping it above the ban threshold means a peer that trips it
		// through some benign pattern we haven't foreseen can recover, while a persistent offender
		// still accumulates its way to a ban.
		let cycling = rep::TARGET_CYCLING.value;
		assert!(cycling < 0, "must be a penalty");
		assert!(
			cycling > BANNED_THRESHOLD,
			"{cycling} should not ban on a single occurrence (threshold {BANNED_THRESHOLD})"
		);

		// Replaying a byte-identical request has no benign explanation, so it is strictly harsher
		// than target cycling and bans on sight, matching substrate's state protocol.
		assert!(rep::SAME_REQUEST.value < cycling);
	}

	/// Constant guard-rails, as above.
	#[allow(clippy::assertions_on_constants)]
	#[test]
	fn snapshot_cache_holds_more_than_one_target() {
		// A single-entry memo is defeated by alternating two targets: each request evicts the
		// other's blob and forces a fresh arena serialization. The LRU must hold at least two so
		// that alternation is a hit, and the per-peer budget — not the memo — bounds a peer that
		// cycles more targets than the memo can hold.
		assert!(SNAPSHOT_CACHE_ENTRIES >= 2);
		assert!(MAX_SERIALIZATIONS_PER_PEER >= 1, "an honest client needs one serialization");
	}

	#[test]
	fn seen_requests_key_distinguishes_peer_target_and_offset() {
		// Guards the assumption `handle_request` relies on: the replay counter is keyed by
		// (peer, target, offset), so paging distinct offsets never trips the replay penalty.
		let peer = PeerId::random();
		let target = sp_core::H256::repeat_byte(1);
		let mut lru: LruCache<
			SeenRequestsKey<midnight_node_runtime::opaque::Block>,
			SeenRequestsValue,
		> = LruCache::new(nonzero(2));

		let key_at = |offset| SeenRequestsKey::<midnight_node_runtime::opaque::Block> {
			peer,
			target,
			offset,
		};
		lru.put(key_at(0), SeenRequestsValue::First);
		lru.put(key_at(1), SeenRequestsValue::First);
		assert!(lru.get(&key_at(0)).is_some());
		assert!(lru.get(&key_at(1)).is_some());

		// Distinct offsets are distinct keys, so a client paging forward is never a "same request".
		assert_ne!(key_at(0), key_at(1));

		// A different peer asking for the same range is also a distinct key.
		let other = SeenRequestsKey::<midnight_node_runtime::opaque::Block> {
			peer: PeerId::random(),
			target,
			offset: 0,
		};
		assert_ne!(key_at(0), other);
	}
}
