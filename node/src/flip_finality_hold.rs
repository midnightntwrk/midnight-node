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

//! GRANDPA safety hold for the AURA→BABE flip.
//!
//! While a flip is pending, [`FlipFinalityHold`] keeps this node's GRANDPA votes at or below the
//! parent of the block that dispatched `schedule_flip`. `midnight-node revert` cannot cross a
//! finalized block, so as long as the hold stands operators can revert to the state before the
//! flip was scheduled if the first BABE blocks go wrong. The hold lifts once the branch being
//! voted on contains the first BABE-authored block plus `extra_confirmations` more.
//!
//! # Signals
//!
//! - **Flip pending at X**: `pallet_consensus_engine::EngineState` in the state of X is
//!   `ScheduledFlip` or `Babe`. Nothing cheaper exists: `schedule_flip` emits no event, the
//!   header carries the same AURA + BABE `SecondaryPlain` digests before and after scheduling,
//!   and [`ConsensusEngineApi::active_engine`] reports `Aura` for both `Aura` and
//!   `ScheduledFlip`. A raw storage read is a trie lookup with no WASM instantiation, so it is
//!   also cheaper than a runtime call. It couples the node to the pallet's storage layout; the
//!   key and type come from the same `pallet-consensus-engine` crate the runtime is built from,
//!   and the rule only acts when the runtime at the target exposes [`ConsensusEngineApi`].
//!   A follow-up could add an `engine_state()` method behind a `ConsensusEngineApi` version bump
//!   and drop the storage coupling.
//! - **Schedule block**: `EngineState` only moves `Aura → ScheduledFlip → Babe` along a branch,
//!   so the schedule block is the lowest block on the target's branch whose state is not `Aura`.
//!   It is found by collecting the branch's hashes from the target down to the vote base (header
//!   reads only) and binary-searching them with `O(log n)` storage reads.
//! - **BABE confirmation**: header only, via [`authoring_engine`]. Every pre-flip block of the
//!   pallet's era carries a BABE `SecondaryPlain` digest *after* the AURA one, so "has a BABE
//!   digest" is not enough; the authoring engine must be BABE.
//!
//! # Cost
//!
//! Before a flip is scheduled a vote costs one storage read at the target. While a hold is active
//! the anchor found for each target is cached, so the next vote walks back only to the previous
//! target. Once the finalized block is BABE-authored the rule switches itself off for the rest of
//! the process.

use crate::engine_digests::authoring_engine;
use midnight_node_runtime::opaque::Block;
use midnight_primitives_consensus_engine::{ActiveEngine, ConsensusEngineApi};
use pallet_consensus_engine::State;
use parity_scale_codec::Decode;
use sc_client_api::{Backend, StorageProvider};
use sc_consensus_grandpa::{VotingRule, VotingRuleResult};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_core::storage::StorageKey;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, NumberFor};
use std::collections::{HashMap, VecDeque};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const LOG_TARGET: &str = "flip-finality-hold";

/// Default number of BABE blocks required on top of the first one before the hold lifts.
///
/// One: the first BABE block proves the seeded epoch tree verifies a BABE block and accepts the
/// epoch-0 announcement it carries; the second is the first block verified against the tree
/// *after* that announcement was imported, usually from a different author. It costs one slot of
/// extra hold.
pub const DEFAULT_EXTRA_CONFIRMATIONS: u32 = 1;

/// Anchors kept in the cache; a hold normally needs one entry per tracked fork tip.
const ANCHOR_CACHE_CAPACITY: usize = 256;

type Hash = <Block as BlockT>::Hash;
type Number = NumberFor<Block>;

/// Header facts the decision needs about one block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BlockFacts {
	pub number: Number,
	pub parent: Hash,
	/// Whether [`authoring_engine`] says BABE authored the block.
	pub babe_authored: bool,
}

/// Per-block chain facts, abstracted so [`decide`] can be tested without a client.
pub(crate) trait ChainFacts {
	fn block(&self, hash: Hash) -> Result<BlockFacts, String>;
	/// `EngineState` in the post-state of `hash`; `Aura` when the storage item is absent.
	fn engine_state(&self, hash: Hash) -> Result<State, String>;
	/// Whether the runtime at `hash` exposes [`ConsensusEngineApi`].
	fn has_engine_api(&self, hash: Hash) -> Result<bool, String>;
}

/// Outcome of [`decide`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
	/// Vote is not restricted.
	Free(FreeReason),
	/// Vote at most for this block.
	Hold { hash: Hash, number: Number },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FreeReason {
	/// The target is not above the base, nothing to restrict.
	NothingToFinalize,
	/// `EngineState` at the target is `Aura`.
	NoFlip,
	/// The runtime at the target lacks [`ConsensusEngineApi`].
	NoEngineApi,
	/// The branch holds the first BABE block plus the required confirmations.
	BabeConfirmed,
	/// The target's own state could not be read.
	Unreadable(String),
}

/// Where the hold sits for a given target, independent of the vote base.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Anchor {
	/// Parent of the schedule block.
	ScheduleParent { hash: Hash, number: Number },
	/// The schedule block was at or below the vote base when the anchor was computed, so the
	/// best the rule can do is hold at the base.
	AtOrBelowBase,
}

/// Bounded FIFO map from a voted target to its [`Anchor`]. A block's state never changes, so
/// entries never go stale; eviction only bounds memory.
pub(crate) struct AnchorCache {
	map: HashMap<Hash, Anchor>,
	order: VecDeque<Hash>,
	capacity: usize,
}

impl AnchorCache {
	pub(crate) fn new(capacity: usize) -> Self {
		Self { map: HashMap::new(), order: VecDeque::new(), capacity: capacity.max(1) }
	}

	fn get(&self, hash: &Hash) -> Option<Anchor> {
		self.map.get(hash).copied()
	}

	fn insert(&mut self, hash: Hash, anchor: Anchor) {
		if self.map.contains_key(&hash) {
			return;
		}
		while self.order.len() >= self.capacity {
			if let Some(old) = self.order.pop_front() {
				self.map.remove(&old);
			}
		}
		self.order.push_back(hash);
		self.map.insert(hash, anchor);
	}
}

/// Decide how far a vote for `target` may go, given the vote `base` (an ancestor of `target`).
///
/// Read failures at the target leave the vote unrestricted, like a runtime without the API.
/// Read failures further down, once a flip is known to be pending, hold at the base.
pub(crate) fn decide<F: ChainFacts>(
	facts: &F,
	base: (Hash, Number),
	target: (Hash, Number),
	extra_confirmations: u32,
	cache: &mut AnchorCache,
) -> Verdict {
	let (base_hash, base_number) = base;
	let (target_hash, target_number) = target;
	if target_number <= base_number {
		return Verdict::Free(FreeReason::NothingToFinalize);
	}

	match facts.engine_state(target_hash) {
		Ok(State::Aura) => return Verdict::Free(FreeReason::NoFlip),
		Ok(State::ScheduledFlip | State::Babe) => {},
		Err(e) => return Verdict::Free(FreeReason::Unreadable(e)),
	}
	match facts.has_engine_api(target_hash) {
		Ok(true) => {},
		Ok(false) => return Verdict::Free(FreeReason::NoEngineApi),
		Err(e) => return Verdict::Free(FreeReason::Unreadable(e)),
	}

	let hold_at_base = Verdict::Hold { hash: base_hash, number: base_number };

	match babe_confirmed(facts, target_hash, extra_confirmations.saturating_add(1)) {
		Ok(true) => return Verdict::Free(FreeReason::BabeConfirmed),
		Ok(false) => {},
		Err(e) => {
			log::warn!(target: LOG_TARGET, "cannot count BABE blocks below {target_hash:?}: {e}; holding at base");
			return hold_at_base;
		},
	}

	let anchor = match find_anchor(facts, base_number, target_hash, cache) {
		Ok(anchor) => anchor,
		Err(e) => {
			log::warn!(target: LOG_TARGET, "cannot locate the flip schedule block below {target_hash:?}: {e}; holding at base");
			return hold_at_base;
		},
	};
	cache.insert(target_hash, anchor);

	match anchor {
		Anchor::ScheduleParent { hash, number } if number >= base_number => {
			Verdict::Hold { hash, number }
		},
		Anchor::ScheduleParent { .. } | Anchor::AtOrBelowBase => hold_at_base,
	}
}

/// Whether the `needed` blocks ending at `target` are all BABE-authored. After the flip every
/// block is, so this is "first BABE block + `needed - 1` more are on the branch". Reads at most
/// `needed` headers.
fn babe_confirmed<F: ChainFacts>(facts: &F, target: Hash, needed: u32) -> Result<bool, String> {
	let mut hash = target;
	for _ in 0..needed {
		let block = facts.block(hash)?;
		if !block.babe_authored {
			return Ok(false);
		}
		hash = block.parent;
	}
	Ok(true)
}

/// Find the [`Anchor`] for `target`, whose state is known not to be `Aura`.
///
/// Walks headers from `target` down to the first block at or below `base_number` (the floor),
/// stopping early at a cached target. Without a cache hit the schedule block lies in
/// `(floor, target]` unless the floor's own state is already past `Aura`; binary search over
/// that range relies on the state being monotone along the branch.
fn find_anchor<F: ChainFacts>(
	facts: &F,
	base_number: Number,
	target: Hash,
	cache: &AnchorCache,
) -> Result<Anchor, String> {
	// `branch[i]` is the block `i` below the target; all are strictly above the floor.
	let mut branch = Vec::new();
	let mut hash = target;
	let floor = loop {
		if let Some(anchor) = cache.get(&hash) {
			return Ok(anchor);
		}
		let block = facts.block(hash)?;
		if block.number <= base_number {
			break (hash, block.number);
		}
		branch.push(hash);
		hash = block.parent;
	};

	if facts.engine_state(floor.0)? != State::Aura {
		return Ok(Anchor::AtOrBelowBase);
	}

	// Invariant: `branch[lo]` is past `Aura`; index `hi` is `Aura` (`hi == len` is the floor).
	let (mut lo, mut hi) = (0usize, branch.len());
	while hi - lo > 1 {
		let mid = lo + (hi - lo) / 2;
		if facts.engine_state(branch[mid])? == State::Aura {
			hi = mid;
		} else {
			lo = mid;
		}
	}
	let parent = branch.get(hi).copied().unwrap_or(floor.0);
	let parent_number = floor.1 + (branch.len() - hi) as Number;
	Ok(Anchor::ScheduleParent { hash: parent, number: parent_number })
}

/// [`ChainFacts`] backed by a full client.
struct ClientFacts<'a, C, BE> {
	client: &'a C,
	engine_state_key: &'a StorageKey,
	_backend: PhantomData<fn() -> BE>,
}

impl<C, BE> ChainFacts for ClientFacts<'_, C, BE>
where
	BE: Backend<Block>,
	C: HeaderBackend<Block> + ProvideRuntimeApi<Block> + StorageProvider<Block, BE>,
	C::Api: ConsensusEngineApi<Block>,
{
	fn block(&self, hash: Hash) -> Result<BlockFacts, String> {
		let header = self
			.client
			.header(hash)
			.map_err(|e| e.to_string())?
			.ok_or_else(|| format!("header {hash:?} not found"))?;
		Ok(BlockFacts {
			number: *header.number(),
			parent: *header.parent_hash(),
			babe_authored: authoring_engine::<Block>(&header) == Some(ActiveEngine::Babe),
		})
	}

	fn engine_state(&self, hash: Hash) -> Result<State, String> {
		match self.client.storage(hash, self.engine_state_key).map_err(|e| e.to_string())? {
			None => Ok(State::Aura),
			Some(data) => State::decode(&mut &data.0[..]).map_err(|e| e.to_string()),
		}
	}

	fn has_engine_api(&self, hash: Hash) -> Result<bool, String> {
		self.client
			.runtime_api()
			.api_version::<dyn ConsensusEngineApi<Block>>(hash)
			.map(|v| v.is_some())
			.map_err(|e| e.to_string())
	}
}

struct Shared {
	/// Set once the finalized block is BABE-authored; the rule is inert from then on.
	released: AtomicBool,
	state: Mutex<HoldState>,
}

struct HoldState {
	anchors: AnchorCache,
	/// Last hold point reported, to log transitions only.
	last_hold: Option<Hash>,
}

/// GRANDPA [`VotingRule`] that holds finality below a pending AURA→BABE flip. See the module docs.
pub struct FlipFinalityHold<BE> {
	extra_confirmations: u32,
	engine_state_key: Arc<StorageKey>,
	shared: Arc<Shared>,
	_backend: PhantomData<fn() -> BE>,
}

impl<BE> Clone for FlipFinalityHold<BE> {
	fn clone(&self) -> Self {
		Self {
			extra_confirmations: self.extra_confirmations,
			engine_state_key: self.engine_state_key.clone(),
			shared: self.shared.clone(),
			_backend: PhantomData,
		}
	}
}

impl<BE> FlipFinalityHold<BE> {
	/// `extra_confirmations` BABE blocks beyond the first are required before the hold lifts;
	/// see [`DEFAULT_EXTRA_CONFIRMATIONS`].
	pub fn new(extra_confirmations: u32) -> Self {
		let key =
			pallet_consensus_engine::EngineState::<midnight_node_runtime::Runtime>::hashed_key();
		Self {
			extra_confirmations,
			engine_state_key: Arc::new(StorageKey(key.to_vec())),
			shared: Arc::new(Shared {
				released: AtomicBool::new(false),
				state: Mutex::new(HoldState {
					anchors: AnchorCache::new(ANCHOR_CACHE_CAPACITY),
					last_hold: None,
				}),
			}),
			_backend: PhantomData,
		}
	}

	fn evaluate<C>(
		&self,
		client: &C,
		base: &<Block as BlockT>::Header,
		target: &<Block as BlockT>::Header,
	) -> Option<(Hash, Number)>
	where
		BE: Backend<Block>,
		C: HeaderBackend<Block> + ProvideRuntimeApi<Block> + StorageProvider<Block, BE>,
		C::Api: ConsensusEngineApi<Block>,
	{
		if self.shared.released.load(Ordering::Relaxed) {
			return None;
		}
		let finalized = client.info().finalized_hash;
		if let Ok(Some(header)) = client.header(finalized)
			&& authoring_engine::<Block>(&header) == Some(ActiveEngine::Babe)
		{
			log::info!(target: LOG_TARGET, "finalized block {finalized:?} is BABE-authored; flip finality hold disabled");
			self.shared.released.store(true, Ordering::Relaxed);
			return None;
		}

		let facts = ClientFacts::<C, BE> {
			client,
			engine_state_key: &self.engine_state_key,
			_backend: PhantomData,
		};
		let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
		let verdict = decide(
			&facts,
			(base.hash(), *base.number()),
			(target.hash(), *target.number()),
			self.extra_confirmations,
			&mut state.anchors,
		);

		match verdict {
			Verdict::Hold { hash, number } => {
				if state.last_hold != Some(hash) {
					log::info!(
						target: LOG_TARGET,
						"consensus flip pending: holding GRANDPA votes at #{number} ({hash:?}) below target #{}",
						target.number(),
					);
					state.last_hold = Some(hash);
				}
				Some((hash, number))
			},
			Verdict::Free(reason) => {
				if state.last_hold.take().is_some() {
					log::info!(target: LOG_TARGET, "flip finality hold lifted at target #{}: {reason:?}", target.number());
				}
				if let FreeReason::Unreadable(e) = reason {
					log::warn!(target: LOG_TARGET, "cannot read consensus-engine state at {:?}: {e}; not restricting", target.hash());
				}
				None
			},
		}
	}
}

impl<C, BE> VotingRule<Block, C> for FlipFinalityHold<BE>
where
	BE: Backend<Block> + 'static,
	C: HeaderBackend<Block>
		+ ProvideRuntimeApi<Block>
		+ StorageProvider<Block, BE>
		+ Send
		+ Sync
		+ 'static,
	C::Api: ConsensusEngineApi<Block>,
{
	fn restrict_vote(
		&self,
		backend: Arc<C>,
		base: &<Block as BlockT>::Header,
		_best_target: &<Block as BlockT>::Header,
		current_target: &<Block as BlockT>::Header,
	) -> VotingRuleResult<Block> {
		let restricted = self.evaluate(&*backend, base, current_target);
		Box::pin(std::future::ready(restricted))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::H256;
	use std::cell::Cell;

	#[derive(Default)]
	struct MockChain {
		blocks: HashMap<Hash, (BlockFacts, State)>,
		no_api: Vec<Hash>,
		unreadable: Vec<Hash>,
		state_reads: Cell<usize>,
		header_reads: Cell<usize>,
	}

	/// Hash of block `number` on fork `fork` (0 is the main chain).
	fn h(fork: u8, number: Number) -> Hash {
		let mut bytes = [0u8; 32];
		bytes[0] = fork;
		bytes[28..].copy_from_slice(&number.to_be_bytes());
		H256(bytes)
	}

	impl MockChain {
		/// Push blocks `from..from+len` of `fork` on top of `parent`.
		fn extend(&mut self, fork: u8, parent: Hash, from: Number, specs: &[(State, bool)]) {
			let mut parent = parent;
			for (i, (state, babe)) in specs.iter().enumerate() {
				let number = from + i as Number;
				let hash = h(fork, number);
				self.blocks
					.insert(hash, (BlockFacts { number, parent, babe_authored: *babe }, *state));
				parent = hash;
			}
		}

		/// Main chain `0..len`: `Aura` before `schedule`, `ScheduledFlip` from `schedule`,
		/// `Babe` from `flip`, BABE-authored from `flip + 1`.
		fn linear(len: Number, schedule: Option<Number>, flip: Option<Number>) -> Self {
			let mut chain = Self::default();
			let specs: Vec<_> = (0..len)
				.map(|n| {
					let state = match (schedule, flip) {
						(_, Some(f)) if n >= f => State::Babe,
						(Some(s), _) if n >= s => State::ScheduledFlip,
						_ => State::Aura,
					};
					(state, flip.is_some_and(|f| n > f))
				})
				.collect();
			chain.extend(0, H256::zero(), 0, &specs);
			chain
		}

		fn decide(
			&self,
			base: Number,
			target: Number,
			extra: u32,
			cache: &mut AnchorCache,
		) -> Verdict {
			decide(self, (h(0, base), base), (h(0, target), target), extra, cache)
		}
	}

	impl ChainFacts for MockChain {
		fn block(&self, hash: Hash) -> Result<BlockFacts, String> {
			self.header_reads.set(self.header_reads.get() + 1);
			self.blocks
				.get(&hash)
				.map(|(b, _)| *b)
				.ok_or_else(|| format!("unknown {hash:?}"))
		}

		fn engine_state(&self, hash: Hash) -> Result<State, String> {
			self.state_reads.set(self.state_reads.get() + 1);
			if self.unreadable.contains(&hash) {
				return Err("unreadable".into());
			}
			self.blocks
				.get(&hash)
				.map(|(_, s)| *s)
				.ok_or_else(|| format!("unknown {hash:?}"))
		}

		fn has_engine_api(&self, hash: Hash) -> Result<bool, String> {
			Ok(!self.no_api.contains(&hash))
		}
	}

	fn cache() -> AnchorCache {
		AnchorCache::new(16)
	}

	fn hold(number: Number) -> Verdict {
		Verdict::Hold { hash: h(0, number), number }
	}

	#[test]
	fn no_flip_is_not_restricted() {
		let chain = MockChain::linear(20, None, None);
		assert_eq!(chain.decide(2, 19, 1, &mut cache()), Verdict::Free(FreeReason::NoFlip));
		assert_eq!(chain.state_reads.get(), 1, "the common path is one storage read");
	}

	#[test]
	fn target_at_base_is_not_restricted() {
		let chain = MockChain::linear(20, Some(5), None);
		assert_eq!(
			chain.decide(10, 10, 1, &mut cache()),
			Verdict::Free(FreeReason::NothingToFinalize)
		);
	}

	#[test]
	fn scheduled_flip_holds_at_parent_of_schedule_block() {
		let chain = MockChain::linear(20, Some(6), None);
		assert_eq!(chain.decide(2, 19, 1, &mut cache()), hold(5));
	}

	#[test]
	fn schedule_block_right_above_base_holds_at_base() {
		let chain = MockChain::linear(20, Some(3), None);
		assert_eq!(chain.decide(2, 19, 1, &mut cache()), hold(2));
	}

	#[test]
	fn schedule_at_or_below_base_holds_at_base() {
		let chain = MockChain::linear(20, Some(6), None);
		assert_eq!(chain.decide(8, 19, 1, &mut cache()), hold(8));
		assert_eq!(chain.decide(6, 19, 1, &mut cache()), hold(6));
	}

	#[test]
	fn executed_flip_without_babe_block_holds() {
		// Flip block 12 is in state `Babe` but still AURA-authored.
		let chain = MockChain::linear(13, Some(6), Some(12));
		assert_eq!(chain.decide(2, 12, 0, &mut cache()), hold(5));
	}

	#[test]
	fn first_babe_block_releases_without_extra_confirmations() {
		let chain = MockChain::linear(14, Some(6), Some(12));
		assert_eq!(chain.decide(2, 13, 0, &mut cache()), Verdict::Free(FreeReason::BabeConfirmed));
	}

	#[test]
	fn extra_confirmations_extend_the_hold() {
		let chain = MockChain::linear(17, Some(6), Some(12));
		// BABE-authored: 13, 14, 15, 16.
		assert_eq!(chain.decide(2, 13, 1, &mut cache()), hold(5));
		assert_eq!(chain.decide(2, 14, 1, &mut cache()), Verdict::Free(FreeReason::BabeConfirmed));
		assert_eq!(chain.decide(2, 15, 3, &mut cache()), hold(5));
		assert_eq!(chain.decide(2, 16, 3, &mut cache()), Verdict::Free(FreeReason::BabeConfirmed));
	}

	#[test]
	fn runtime_without_api_is_not_restricted() {
		let mut chain = MockChain::linear(20, Some(6), None);
		chain.no_api.push(h(0, 19));
		assert_eq!(chain.decide(2, 19, 1, &mut cache()), Verdict::Free(FreeReason::NoEngineApi));
	}

	#[test]
	fn blocks_from_before_the_pallet_read_as_aura() {
		// Blocks 0..10 run a runtime without the pallet or `ConsensusEngineApi`; `EngineState` is
		// absent there, which `ClientFacts` reads as `Aura`. The upgrade lands at 10.
		let mut chain = MockChain::linear(20, Some(15), None);
		chain.no_api.extend((0..10).map(|n| h(0, n)));
		assert_eq!(chain.decide(2, 9, 1, &mut cache()), Verdict::Free(FreeReason::NoFlip));
		assert_eq!(chain.decide(2, 19, 1, &mut cache()), hold(14));
	}

	#[test]
	fn unreadable_target_is_not_restricted() {
		let mut chain = MockChain::linear(20, Some(6), None);
		chain.unreadable.push(h(0, 19));
		assert!(matches!(
			chain.decide(2, 19, 1, &mut cache()),
			Verdict::Free(FreeReason::Unreadable(_))
		));
	}

	#[test]
	fn unreadable_ancestor_holds_at_base() {
		let mut chain = MockChain::linear(40, Some(30), None);
		for n in 3..30 {
			chain.unreadable.push(h(0, n));
		}
		assert_eq!(chain.decide(2, 39, 1, &mut cache()), hold(2));
	}

	#[test]
	fn search_is_logarithmic_and_cached() {
		let chain = MockChain::linear(1025, Some(500), None);
		let mut cache = cache();
		assert_eq!(chain.decide(0, 1000, 1, &mut cache), hold(499));
		assert!(chain.state_reads.get() <= 14, "{} state reads", chain.state_reads.get());

		chain.state_reads.set(0);
		chain.header_reads.set(0);
		assert_eq!(chain.decide(0, 1003, 1, &mut cache), hold(499));
		assert_eq!(chain.state_reads.get(), 1, "only the target's state is read");
		// One for the BABE count, three to reach the cached target 1000.
		assert_eq!(chain.header_reads.get(), 4);
	}

	#[test]
	fn forks_get_their_own_schedule_block() {
		// Main chain schedules at 6; a fork off block 4 schedules at 8.
		let mut chain = MockChain::linear(20, Some(6), None);
		let fork: Vec<_> = (5..20)
			.map(|n| (if n >= 8 { State::ScheduledFlip } else { State::Aura }, false))
			.collect();
		chain.extend(1, h(0, 4), 5, &fork);
		let mut cache = cache();
		assert_eq!(chain.decide(2, 19, 1, &mut cache), hold(5));
		assert_eq!(
			decide(&chain, (h(0, 2), 2), (h(1, 19), 19), 1, &mut cache),
			Verdict::Hold { hash: h(1, 7), number: 7 }
		);
	}

	#[test]
	fn anchor_cache_evicts_oldest() {
		let mut cache = AnchorCache::new(2);
		cache.insert(h(0, 1), Anchor::AtOrBelowBase);
		cache.insert(h(0, 2), Anchor::AtOrBelowBase);
		cache.insert(h(0, 3), Anchor::AtOrBelowBase);
		assert!(cache.get(&h(0, 1)).is_none());
		assert!(cache.get(&h(0, 3)).is_some());
	}
}
