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

//! Ledger-sync client driver + verification.
//!
//! After warp + state-sync complete (target block N captured by the monitor), this drives the
//! client side of the protocol: read the on-chain `StateKey` at N (from the warp-recovered trie),
//! fetch the Snappy-compressed `Ledger`-rooted arena blob in byte ranges from peers, decompress it
//! to the canonical blob, then hand that blob to
//! [`midnight_node_ledger::import_verified_ledger_snapshot`], which verifies its root against the
//! `StateKey` and persists it on success.
//!
//! Verification + persistence live in the ledger crate (next to the arena); this module is pure
//! network orchestration. No peer is trusted: a bad blob fails the root check and is discarded.
//!
//! Untrusted peers are bounded in time as well as in content — see `fetch_blob_from` for the
//! per-chunk timeout and whole-transfer throughput floor that stop one peer from holding recovery
//! open indefinitely.

use std::{
	marker::PhantomData,
	sync::Arc,
	time::{Duration, Instant},
};

use parity_scale_codec::{Decode, Encode};
use sc_client_api::{Backend, StorageProvider};
use sc_network::{
	IfDisconnected, NetworkRequest, PeerId, ProtocolName, request_responses::RequestFailure,
};
use sp_runtime::traits::Block as BlockT;
use tokio::time::timeout;

use super::{
	LOG_TARGET,
	protocol::{
		ChunkAssembler, DecompressError, LedgerSyncRequest, LedgerSyncResponse,
		MAX_LEDGER_SYNC_CHUNK, decompress_snapshot, required_chunk_len, validate_snapshot_lengths,
	},
	read_state_key,
};

/// How often to log a heartbeat while the (synchronous, CPU-bound) arena verify/import runs on the
/// blocking pool — so a slow, large-arena recovery is observable instead of looking hung.
const HEARTBEAT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15);

/// Drives ledger-arena recovery against peers over the ledger-sync request/response protocol.
///
/// `Network` is `?Sized` so the node's `Arc<dyn NetworkService>` handle (which has `NetworkRequest`
/// as a supertrait) can be passed directly.
pub struct LedgerSyncClient<B: BlockT, Client, BE, Network: ?Sized> {
	client: Arc<Client>,
	network: Arc<Network>,
	protocol_name: ProtocolName,
	/// Whether the local arena uses the unified ParityDb layout (forwarded to the importer).
	unified: bool,
	_phantom: PhantomData<(B, BE)>,
}

impl<B, Client, BE, Network> LedgerSyncClient<B, Client, BE, Network>
where
	B: BlockT,
	BE: Backend<B> + 'static,
	Client: StorageProvider<B, BE> + Send + Sync + 'static,
	Network: NetworkRequest + Send + Sync + ?Sized + 'static,
{
	pub fn new(
		client: Arc<Client>,
		network: Arc<Network>,
		protocol_name: ProtocolName,
		unified: bool,
	) -> Self {
		Self { client, network, protocol_name, unified, _phantom: PhantomData }
	}

	/// Recover, verify, and import the ledger arena at `target` (the captured warp target N) by
	/// trying `peers` in order. Returns `Ok` as soon as one peer yields a complete blob that
	/// verifies against the on-chain `StateKey` and imports; otherwise [`ClientError::AllPeersFailed`].
	///
	/// The caller must hold the authoring/import gate while this runs (single-writer arena).
	pub async fn recover(&self, target: B::Hash, peers: &[PeerId]) -> Result<(), ClientError> {
		let state_key = read_state_key::<B, Client, BE>(&self.client, target)?
			.ok_or(ClientError::NoStateKey)?;

		if peers.is_empty() {
			return Err(ClientError::NoPeers);
		}

		for &peer in peers {
			let blob = match self.fetch_blob_from(peer, target).await {
				Ok(blob) => blob,
				Err(e) => {
					log::debug!(target: LOG_TARGET, "ledger fetch from {peer} failed: {e}; trying next peer");
					continue;
				},
			};

			// Verification + import reconstructs the entire ledger arena via the native multi-pass
			// `Arena::deserialize_sp` — a synchronous, CPU-bound computation that scales with arena
			// size and can run for seconds (or, on a large load-tested arena, minutes). Running it
			// inline on this async worker would monopolize a runtime thread, starve networking/RPC,
			// and give no sign of progress (the node looks hung). Offload it to the blocking pool and
			// emit a heartbeat so a slow recovery is observable. Verification still happens inside the
			// importer (recomputed root must equal `state_key`); a failure means the peer served bad
			// data — discard and try the next one. (A reputation report on the peer belongs here.)
			let unified = self.unified;
			let blob_len = blob.len();
			let state_key = state_key.clone();
			log::info!(
				target: LOG_TARGET,
				"Verifying + importing ledger arena snapshot from {peer} ({blob_len} bytes); \
				 large arenas can take a while…"
			);
			let started = std::time::Instant::now();
			let mut import = tokio::task::spawn_blocking(move || {
				midnight_node_ledger::import_verified_ledger_snapshot(unified, &blob, &state_key)
			});
			let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
			heartbeat.tick().await; // consume the immediate first tick
			let outcome = loop {
				tokio::select! {
					res = &mut import => break res,
					_ = heartbeat.tick() => log::info!(
						target: LOG_TARGET,
						"…still verifying ledger arena snapshot from {peer} ({:.0?} elapsed)",
						started.elapsed(),
					),
				}
			};
			match outcome {
				Ok(Ok(())) => {
					log::info!(
						target: LOG_TARGET,
						"Recovered + verified ledger arena at {target:?} from {peer} \
						 ({blob_len} bytes) in {:.1?}",
						started.elapsed(),
					);
					return Ok(());
				},
				Ok(Err(e)) => {
					log::warn!(
						target: LOG_TARGET,
						"ledger import from {peer} failed after {:.1?}: {e}; trying next peer",
						started.elapsed(),
					);
				},
				Err(join_err) => {
					// The blocking task panicked or was cancelled — treat as a failed attempt.
					log::warn!(
						target: LOG_TARGET,
						"ledger import task from {peer} did not complete: {join_err}; trying next peer",
					);
				},
			}
		}

		Err(ClientError::AllPeersFailed)
	}

	/// Fetch the full compressed blob from a single peer by paging contiguous byte ranges in order,
	/// then decompress it to the canonical `Ledger`-rooted blob.
	///
	/// Two independent bounds keep one peer from monopolising recovery:
	/// - **Size**: every chunk must be full-size ([`required_chunk_len`]) — an honest server always
	///   fills the requested range — so a peer drip-feeding tiny (or empty) chunks fails
	///   immediately instead of tying the client up in an unbounded *number* of requests.
	/// - **Time**: [`CHUNK_TIMEOUT`] per range plus a whole-transfer deadline from
	///   [`transfer_budget`]. Full-size chunks bound the request *count*, not the wall clock: a peer
	///   answering each 1 MiB range just inside the protocol's 40 s timeout would otherwise hold
	///   recovery open for hours (a slowloris). The deadline scales with the advertised size, so a
	///   genuinely large arena over a slow link still completes, but a peer must sustain
	///   [`MIN_THROUGHPUT_BYTES_PER_SEC`] to keep its turn.
	///
	/// (Parallel / multi-peer range fetch is a possible future optimization; the
	/// `ChunkAssembler` already supports resume by `next_offset`.)
	async fn fetch_blob_from(&self, peer: PeerId, target: B::Hash) -> Result<Vec<u8>, ClientError> {
		let started = Instant::now();

		// First range establishes the compressed transfer length and expected raw size. It is the
		// one request that may legitimately be slow — on a cold server cache it pays for the whole
		// arena serialization — so it gets the protocol's own request timeout rather than the
		// tighter per-chunk one applied to the rest of the transfer.
		let first = self.request_range(peer, target, 0).await?;
		let compressed_total_len = first.compressed_total_len;
		let raw_total_len = first.raw_total_len;
		validate_snapshot_lengths(compressed_total_len, raw_total_len)?;
		let mut assembler = ChunkAssembler::new(compressed_total_len);
		ensure_full_chunk(&first)?;
		assembler.accept(first.offset, &first.bytes)?;

		// Budget starts from the size the peer just advertised, measured from the start of the
		// transfer so the peer cannot buy extra time by having been slow to answer the first range.
		let budget = transfer_budget(compressed_total_len);
		let deadline = started + budget;

		while !assembler.is_complete() {
			let elapsed = started.elapsed();
			if elapsed >= budget {
				return Err(ClientError::TransferTooSlow {
					got: assembler.next_offset(),
					total: compressed_total_len,
					elapsed,
				});
			}
			// Cap each range at the shorter of the per-chunk timeout and the remaining budget, so
			// the last request cannot overrun the deadline it was checked against.
			let chunk_timeout = CHUNK_TIMEOUT.min(deadline - Instant::now());
			let offset = assembler.next_offset();
			let next = timeout(chunk_timeout, self.request_range(peer, target, offset))
				.await
				.map_err(|_| ClientError::ChunkTimeout { offset, waited: chunk_timeout })??;
			if next.compressed_total_len != compressed_total_len
				|| next.raw_total_len != raw_total_len
			{
				return Err(ClientError::InconsistentResponse);
			}
			ensure_full_chunk(&next)?;
			assembler.accept(next.offset, &next.bytes)?;
		}

		let compressed = assembler.into_blob()?;
		Ok(decompress_snapshot(&compressed, raw_total_len)?)
	}

	async fn request_range(
		&self,
		peer: PeerId,
		target: B::Hash,
		offset: u64,
	) -> Result<LedgerSyncResponse, ClientError> {
		let request =
			LedgerSyncRequest { target_hash: target, offset, max_len: MAX_LEDGER_SYNC_CHUNK };
		let (bytes, _protocol) = self
			.network
			.request(
				peer,
				self.protocol_name.clone(),
				request.encode(),
				None,
				IfDisconnected::ImmediateError,
			)
			.await?;
		Ok(LedgerSyncResponse::decode(&mut &bytes[..])?)
	}
}

/// Wall clock a peer gets to answer a single range request once the transfer is under way.
///
/// Deliberately well under the protocol's 40 s request timeout: by this point the server is serving
/// from its memoized blob, so a range is a memcpy and a send. A peer that has simply gone quiet is
/// dropped in seconds rather than tying up a slot for the full protocol timeout.
const CHUNK_TIMEOUT: Duration = Duration::from_secs(10);

/// Throughput a peer must sustain across the whole transfer to keep its turn.
///
/// Any node healthy enough to be worth recovering from serves state sync far faster than this; the
/// floor exists to convert "technically still responding" into a failure the driver can act on.
const MIN_THROUGHPUT_BYTES_PER_SEC: u64 = 256 * 1024;

/// Fixed allowance added to the throughput-derived budget, covering per-request round trips and a
/// server whose first response had to serialize the arena.
const TRANSFER_GRACE: Duration = Duration::from_secs(60);

/// Wall clock allowed for a whole single-peer transfer of `compressed_total_len` bytes.
///
/// Scales with the advertised size rather than being a flat timeout, so a large arena over a slow
/// link is still recoverable while a peer that stops making real progress is dropped.
///
/// The argument is the *compressed* length, so the worst case a peer can advertise is
/// `max_compress_len(MAX_LEDGER_SYNC_RAW_BYTES)` (~1.17 GiB), not the 1 GiB raw ceiling — about 81
/// minutes. For a realistic arena, minutes.
fn transfer_budget(compressed_total_len: u64) -> Duration {
	TRANSFER_GRACE
		.saturating_add(Duration::from_secs(compressed_total_len / MIN_THROUGHPUT_BYTES_PER_SEC))
}

/// Require a response chunk to be full-size for its offset (see [`required_chunk_len`]). Oversized
/// chunks are fine (more progress than required); the assembler's overflow check still bounds them.
fn ensure_full_chunk(response: &LedgerSyncResponse) -> Result<(), ClientError> {
	let required = required_chunk_len(response.compressed_total_len, response.offset);
	if (response.bytes.len() as u64) < required {
		return Err(ClientError::UndersizedChunk {
			offset: response.offset,
			got: response.bytes.len() as u64,
			required,
		});
	}
	Ok(())
}

/// Failure modes of [`LedgerSyncClient::recover`]. All are non-fatal: the monitor leaves the
/// authoring gate closed and retries.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
	#[error("no StateKey present at the target block")]
	NoStateKey,
	#[error("no peers available to recover the ledger from")]
	NoPeers,
	#[error("blockchain error: {0}")]
	Client(#[from] sp_blockchain::Error),
	#[error("network request failed: {0:?}")]
	Request(#[from] RequestFailure),
	#[error("failed to decode response: {0}")]
	Decode(#[from] parity_scale_codec::Error),
	#[error("chunk assembly failed: {0}")]
	Assemble(#[from] super::protocol::AssembleError),
	#[error("peer changed ledger-sync response metadata between chunks")]
	InconsistentResponse,
	#[error(
		"peer served an undersized chunk at offset {offset}: got {got} bytes, required {required}"
	)]
	UndersizedChunk { offset: u64, got: u64, required: u64 },
	#[error("failed to decompress ledger snapshot: {0}")]
	Decompress(#[from] DecompressError),
	#[error("peer did not answer the range at offset {offset} within {waited:?}")]
	ChunkTimeout { offset: u64, waited: Duration },
	#[error(
		"peer served {got} of {total} bytes in {elapsed:?}, below the required \
		 {MIN_THROUGHPUT_BYTES_PER_SEC} B/s"
	)]
	TransferTooSlow { got: u64, total: u64, elapsed: Duration },
	#[error("all peers failed to provide a verifiable snapshot")]
	AllPeersFailed,
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::warp_ledger_sync::protocol::MAX_LEDGER_SYNC_RAW_BYTES;

	#[test]
	fn transfer_budget_scales_with_size_and_is_always_positive() {
		// An empty/tiny transfer still gets the fixed grace, so a fast small blob is never
		// failed for being quick.
		assert_eq!(transfer_budget(0), TRANSFER_GRACE);
		assert_eq!(transfer_budget(1), TRANSFER_GRACE);

		// The budget is grace + size/throughput.
		let ten_mib = 10 * 1024 * 1024;
		assert_eq!(
			transfer_budget(ten_mib),
			TRANSFER_GRACE + Duration::from_secs(ten_mib / MIN_THROUGHPUT_BYTES_PER_SEC)
		);

		// Monotonic: a bigger advertised blob never buys less time.
		assert!(transfer_budget(ten_mib) > transfer_budget(0));
	}

	#[test]
	fn worst_case_transfer_budget_is_bounded() {
		// `transfer_budget` takes the *compressed* length, so the largest value a peer can get past
		// `validate_snapshot_lengths` is snappy's worst-case expansion of the raw ceiling (~1.17
		// GiB), not the 1 GiB raw ceiling itself. Budget against that, or the bound is asserted
		// against an underestimate of what an attacker can actually claim.
		let worst_compressed =
			snap::raw::max_compress_len(MAX_LEDGER_SYNC_RAW_BYTES as usize) as u64;
		assert!(worst_compressed > MAX_LEDGER_SYNC_RAW_BYTES, "compressed ceiling is the larger");

		// Even there, one peer's turn is capped well inside two hours rather than the ~13 hours
		// that `MAX_LEDGER_SYNC_CHUNK`-sized ranges at the 40s protocol timeout would allow.
		let worst = transfer_budget(worst_compressed);
		assert!(
			worst < Duration::from_secs(2 * 60 * 60),
			"worst-case single-peer budget {worst:?} should stay well under 2h"
		);
	}

	#[test]
	fn chunk_timeout_is_tighter_than_the_protocol_timeout() {
		// The point of the per-chunk bound is to drop a quiet peer faster than the 40s
		// request-response timeout would. If this ever inverts, the bound is dead code.
		assert!(CHUNK_TIMEOUT < Duration::from_secs(40));
	}
}
