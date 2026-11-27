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

//! Property tests for [`TxValidationCache`]. The properties, and every test of them, are in
//! [`properties`]. The rest is harness: a tree of ledger states, and calls that send its
//! transactions to random places in it.
//!
//! Compiled once per ledger version, as a child of that version's `revalidation_tests`, which
//! holds everything that differs between them.

mod disk;
mod history;
mod model;
mod properties;
mod threads;
mod tree;

use super::{self as shim, helpers, ledger, proofs};
use crate::ephemeral::{EphemeralDb, ensure_storage};
use helpers::{Duration, FixedPoint, INITIAL_PARAMETERS, LedgerParameters};
use ledger::{
	BlockContext, Bridge, ProofReceipt, TX_VALIDATION_CACHE_CAPACITY, Timestamp,
	TransactionSignature, TxValidationCache, TxValidationCacheOutcome, TxValidationKey, api,
	ledger_storage_local::{DefaultDB, arena::TypedArenaKey, db::DB},
	types::LedgerApiError,
};
use midnight_primitives_ledger::{LedgerMetrics, LedgerMetricsExt, LedgerOnchainExt};
use parity_scale_codec::Encode;
use proptest::{prelude::*, sample::Index, strategy::ValueTree};
use sp_externalities::Externalities;
use std::{
	collections::BTreeSet,
	sync::{Arc, Mutex, OnceLock},
};
use tree::{Branch, Edge, Maintenance, Tree, build_tree};

/// Generated trees in the smoke set, besides the hand-written one.
const SMOKE_GENERATED_TREES: usize = 3;

/// Built once per test binary. Generated from a fixed seed, so CI always sees the same trees.
fn smoke_trees() -> &'static [Tree] {
	static TREES: OnceLock<Vec<Tree>> = OnceLock::new();
	TREES.get_or_init(|| {
		let mut runner = proptest::test_runner::TestRunner::deterministic();
		let generated = (0..SMOKE_GENERATED_TREES)
			.map(|_| tree_spec().new_tree(&mut runner).expect("spec generates").current());
		std::iter::once(spec()).chain(generated).map(|spec| build_tree(&spec)).collect()
	})
}

/// An index into `smoke_trees`, the hand-written tree for half the histories: it is laid out to
/// reach the rare cases.
fn smoke_tree() -> impl Strategy<Value = usize> {
	prop_oneof![Just(0), 0..=SMOKE_GENERATED_TREES]
}

/// A shielded transfer comes with the block end the next one needs.
fn edges() -> impl Strategy<Value = Vec<Edge>> {
	let seeds = || (0..3usize, 0..3usize);
	let edge = prop_oneof![
		6 => seeds().prop_map(|(from, to)| vec![Edge::Transfer { from, to }]),
		3 => seeds().prop_map(|(from, to)| vec![Edge::ShieldedTransfer { from, to }, Edge::EndBlock]),
		3 => (0..3usize).prop_map(|payer| vec![Edge::Store { payer }]),
		3 => Just(vec![Edge::EndBlock]),
		1 => prop_oneof![
			Just(vec![Edge::Params(raise_fees)]),
			Just(vec![Edge::Params(shorten_dust_grace)]),
			(0..3usize).prop_map(|payer| vec![
				Edge::Maintain { payer, update: Maintenance::RotateAuthority }
			]),
		],
	];
	prop::collection::vec(edge, 1..10).prop_map(|edges| edges.concat())
}

/// A leaf nothing can follow: no transfer fits under the byte limit, no maintenance update under
/// the metadata limit, and no call after its key is rotated or removed.
fn last_leaf() -> impl Strategy<Value = Branch> {
	let maintain = |update| (0..3usize).prop_map(move |payer| Edge::Maintain { payer, update });
	prop_oneof![
		1 => Just(Edge::Params(shrink_tx_byte_limit)),
		// Ledger 8 has no metadata limit.
		1 => Just(Edge::Params(shim::SHRINK_CONTRACT_METADATA_LIMIT.unwrap_or(shrink_tx_byte_limit))),
		2 => maintain(Maintenance::RotateKey),
		2 => maintain(Maintenance::RemoveKey),
	]
	.prop_map(|edge| Branch { edges: vec![edge, Edge::EndBlock], children: vec![] })
}

/// Specs `build_tree` can always build: the contract is deployed first, and only a last leaf
/// changes what nothing may follow. Every fork has one, so calls and updates made before it can
/// be sent past it.
fn tree_spec() -> impl Strategy<Value = Branch> {
	let leaf = edges().prop_map(|edges| Branch { edges, children: vec![] });
	leaf.prop_recursive(3, 24, 3, move |inner| {
		(edges(), prop::collection::vec(inner, 1..3), last_leaf()).prop_map(
			|(edges, mut children, last)| {
				children.push(last);
				Branch { edges, children }
			},
		)
	})
	.prop_filter("a tree needs a transaction to send", Branch::has_tx)
	.prop_map(|mut spec| {
		spec.edges.insert(0, Edge::Deploy { payer: 0 });
		spec
	})
}

/// Forks at the root and below, with sibling branches spending the same NIGHT, dust and shielded
/// coins.
///
/// Every branch calls `store`, so forks disagree on the contract's state, and many leaves end in a
/// rotation or removal of its key, so calls made before can be sent past one. Two forks rotate the
/// maintenance authority; the others' updates are signed by the original one, so they are sent
/// across an authority change.
fn spec() -> Branch {
	use Edge::{EndBlock as E, Params as P};
	use Maintenance::*;
	let t = |from, to| Edge::Transfer { from, to };
	let s = |from, to| Edge::ShieldedTransfer { from, to };
	let c = |payer| Edge::Store { payer };
	let m = |payer, update| Edge::Maintain { payer, update };
	let leaf = |edges| Branch { edges, children: vec![] };
	Branch {
		edges: vec![
			Edge::Deploy { payer: 0 },
			t(0, 1),
			s(0, 1),
			c(1),
			t(1, 2),
			E,
			t(2, 0),
			s(1, 2),
			c(2),
			E,
		],
		children: vec![
			Branch {
				edges: vec![
					s(0, 2),
					c(0),
					t(0, 2),
					t(0, 1),
					E,
					m(1, RotateAuthority),
					t(1, 0),
					s(2, 1),
					E,
					c(1),
					t(2, 1),
					E,
				],
				children: vec![
					leaf(vec![s(1, 0), c(0), t(0, 1), E, m(0, RotateKey), E]),
					leaf(vec![P(raise_fees), t(0, 1), c(2), s(1, 0), t(1, 2), E]),
					leaf(vec![m(2, RotateKey), E]),
				],
			},
			Branch {
				edges: vec![s(0, 2), c(2), t(0, 2), E, t(2, 1), t(1, 0), E],
				children: [
					leaf(vec![P(shrink_tx_byte_limit), E]),
					leaf(vec![P(shorten_dust_grace), t(1, 2), E, s(2, 0), t(2, 0), E]),
					leaf(vec![t(2, 0), c(1), E, E, s(1, 0), t(0, 1), E, m(2, RotateKey), E]),
					leaf(vec![m(0, RotateKey), E]),
					leaf(vec![m(1, RemoveKey), E]),
				]
				.into_iter()
				.chain(shim::SHRINK_CONTRACT_METADATA_LIMIT.map(|f| leaf(vec![P(f), E])))
				.collect(),
			},
			Branch {
				edges: vec![s(0, 1), E, s(2, 0), c(1), t(1, 2), E, m(2, RotateAuthority), E],
				children: vec![
					leaf(vec![m(0, RemoveKey), E]),
					leaf(vec![t(2, 1), c(0), E]),
					leaf(vec![m(1, RotateKey), E]),
				],
			},
			leaf(vec![s(0, 1), m(1, RotateKey), E]),
			leaf(vec![m(2, RemoveKey), E]),
		],
	}
}

/// Far enough that transfers paying `FEE_ESTIMATE` underpay afterwards. Set from the initial
/// price, not the current one: generated specs repeat it, and compounding would soon make fees
/// unpayable.
fn raise_fees(params: &mut LedgerParameters) {
	params.fee_prices.overall_price =
		INITIAL_PARAMETERS.fee_prices.overall_price * FixedPoint::from(100.0);
}

/// Inside the TTL window, so a dust spend can expire while its intent is still live.
fn shorten_dust_grace(params: &mut LedgerParameters) {
	params.dust.dust_grace_period = Duration::from_secs(600);
}

/// Below any transfer's size.
fn shrink_tx_byte_limit(params: &mut LedgerParameters) {
	params.limits.transaction_byte_limit = 1_000;
}

#[derive(Debug, Clone)]
enum Call {
	Validate {
		pre_dispatch: bool,
		skew_tblock: bool,
		check_utxos: bool,
		ephemeral: bool,
		/// Mempool path only.
		get_tx_details: bool,
	},
	Apply {
		skew_tblock: bool,
		filter_only_failed_segments: bool,
		net_intra_tx_utxos: bool,
	},
}

/// Where a step sends its tx.
#[derive(Debug, Clone)]
enum At {
	/// The node it was applied at.
	Parent,
	Any(Index),
	/// The tip of a branch below where the tx was applied, so it has been applied there, and every
	/// key rotation, removal or parameter change on the way applies.
	Below(Index),
	/// The tip of a branch the tx was not applied on, which may have spent the same coins.
	Across(Index),
}

#[derive(Debug, Clone)]
struct Step {
	/// The kind of tx, so every kind is sent as often, however few of it the tree holds.
	kind: Index,
	tx: Index,
	node: At,
	/// Added to the node's `tblock`.
	dt: i64,
	/// Taken off the parent block's time, which a skewed call's corrected tblock follows.
	parent_gap: u64,
	runtime_version: u32,
	call: Call,
}

#[derive(Debug, Clone)]
enum Action {
	Call(Step),
	/// Drops the receipt, as capacity eviction would.
	Evict {
		tx: Index,
		runtime_version: u32,
	},
	/// Points the receipt at a state no storage holds.
	Orphan {
		tx: Index,
		runtime_version: u32,
	},
	/// Repeats an earlier call that wrote a receipt, after losing the receipt if `lose` says so.
	/// Random steps rarely meet a receipt for their own key, so this is how a history reaches the
	/// hit rows. Each kind of tx is as likely to be repeated, so the rarer kinds meet hits too.
	Again {
		kind: Index,
		earlier: Index,
		/// `None` keeps the earlier call's node.
		node: Option<At>,
		call: Call,
		/// Under the other runtime version, which usually has no receipt.
		other_version: bool,
		lose: Option<Lost>,
	},
}

fn call() -> impl Strategy<Value = Call> {
	prop_oneof![
		any::<[bool; 5]>().prop_map(
			|[pre_dispatch, skew_tblock, check_utxos, ephemeral, get_tx_details]| {
				Call::Validate { pre_dispatch, skew_tblock, check_utxos, ephemeral, get_tx_details }
			}
		),
		any::<[bool; 3]>().prop_map(
			|[skew_tblock, filter_only_failed_segments, net_intra_tx_utxos]| Call::Apply {
				skew_tblock,
				filter_only_failed_segments,
				net_intra_tx_utxos
			}
		),
	]
}

fn at() -> impl Strategy<Value = At> {
	prop_oneof![
		2 => Just(At::Parent),
		3 => any::<Index>().prop_map(At::Any),
		1 => any::<Index>().prop_map(At::Below),
	]
}

fn step() -> impl Strategy<Value = Step> {
	const HOUR: i64 = 3600;
	(
		any::<[Index; 2]>(),
		at(),
		// Intent TTLs run an hour, the dust grace period three.
		prop_oneof![3 => Just(0), 2 => -60i64..=60, 3 => -4 * HOUR..=4 * HOUR],
		1u32..=2,
		call(),
	)
		.prop_map(|([kind, tx], node, dt, runtime_version, call)| Step {
			kind,
			tx,
			node,
			dt,
			parent_gap: 0,
			runtime_version,
			call,
		})
}

fn action() -> impl Strategy<Value = Action> {
	let key = || (any::<Index>(), 1u32..=2);
	prop_oneof![
		8 => step().prop_map(Action::Call),
		1 => key().prop_map(|(tx, runtime_version)| Action::Evict { tx, runtime_version }),
		1 => key().prop_map(|(tx, runtime_version)| Action::Orphan { tx, runtime_version }),
	]
}

/// Production capacity, or one small enough that moka evicts and refuses entries on its own.
fn capacity() -> impl Strategy<Value = u64> {
	prop_oneof![Just(TX_VALIDATION_CACHE_CAPACITY), Just(2)]
}

struct Exts {
	onchain: sp_io::TestExternalities,
	offchain: sp_io::TestExternalities,
	metrics: Arc<Mutex<Option<LedgerMetrics>>>,
}

impl Exts {
	fn new() -> Self {
		let metrics = LedgerMetrics::register(&prometheus_endpoint::Registry::new()).unwrap();
		let metrics = Arc::new(Mutex::new(Some(metrics)));
		let mut onchain = sp_io::TestExternalities::default();
		onchain.register_extension(LedgerOnchainExt::default());
		onchain.register_extension(LedgerMetricsExt::new(metrics.clone()));
		let mut offchain = sp_io::TestExternalities::default();
		offchain.register_extension(LedgerMetricsExt::new(metrics.clone()));
		Self { onchain, offchain, metrics }
	}

	/// Cache misses, strict hits and revalidation hits recorded so far.
	fn outcome_counts(&self) -> [u64; 3] {
		let metrics = self.metrics.lock().unwrap();
		let m = metrics.as_ref().unwrap();
		[
			m.tx_validation_cache_misses.with_label_values(&[]).get(),
			m.tx_validation_cache_hits.with_label_values(&["strict"]).get(),
			m.tx_validation_cache_hits.with_label_values(&["revalidation"]).get(),
		]
	}

	/// Which outcome counter `f` moved.
	fn outcome_moved<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> (T, [u64; 3]) {
		let before = self.outcome_counts();
		let result = f(self);
		let after = self.outcome_counts();
		(result, [0, 1, 2].map(|i| after[i] - before[i]))
	}
}

/// The counter an outcome moves, as `outcome_counts` orders them.
fn one_hot(outcome: &TxValidationCacheOutcome) -> [u64; 3] {
	match outcome {
		TxValidationCacheOutcome::CacheMiss => [1, 0, 0],
		TxValidationCacheOutcome::StrictCacheHit => [0, 1, 0],
		TxValidationCacheOutcome::RevalidationHit => [0, 0, 1],
	}
}

/// `lookup_receipt` for a call at `state_key`, through the call's storage.
fn lookup(
	exts: &mut Exts,
	storage: Storage,
	state_key: &[u8],
	tblock: Timestamp,
	key: &TxValidationKey,
	cache: &TxValidationCache,
) -> TxValidationCacheOutcome {
	fn at<D: DB>(
		state_key: &[u8],
		tblock: Timestamp,
		key: &TxValidationKey,
		cache: &TxValidationCache,
	) -> TxValidationCacheOutcome
	where
		D::Hasher:
			storage_sha2::digest::OutputSizeUser<OutputSize = storage_sha2::digest::consts::U32>,
	{
		type B<D> = Bridge<TransactionSignature, D, proofs::UnverifiedProofs>;
		let ledger = B::<D>::load_ledger(&api::new(), state_key).expect("tree states stay stored");
		B::<D>::lookup_receipt(&ledger, state_key, tblock, key, cache)
			.map_or(TxValidationCacheOutcome::CacheMiss, |(_, outcome)| outcome)
	}
	match storage {
		Storage::OnChain => at::<DefaultDB>(state_key, tblock, key, cache),
		Storage::OffChain => {
			ensure_storage::<DefaultDB>(&mut exts.offchain.ext());
			at::<EphemeralDb<DefaultDB>>(state_key, tblock, key, cache)
		},
	}
}

/// `lookup` against a cache holding only a copy of `receipt`: the oracle must neither evict from
/// the cache under test, which would pre-empt the call's own eviction, nor read through moka.
fn predict(
	exts: &mut Exts,
	storage: Storage,
	state_key: &[u8],
	tblock: Timestamp,
	key: &TxValidationKey,
	receipt: Option<&model::Receipt>,
) -> TxValidationCacheOutcome {
	let probe = TxValidationCache::new(TX_VALIDATION_CACHE_CAPACITY);
	if let Some((state_key, tblock)) = receipt.cloned() {
		probe.0.insert(key.clone(), ProofReceipt { state_key, tblock });
	}
	lookup(exts, storage, state_key, tblock, key, &probe)
}

/// The step's tx and node indices, and the block context it runs in.
fn target(tree: &Tree, step: &Step) -> (usize, usize, BlockContext) {
	let kinds: BTreeSet<_> = tree.txs.iter().map(|tx| tx.kind).collect();
	let kind = kinds.iter().nth(step.kind.index(kinds.len())).unwrap();
	let of_kind: Vec<_> = (0..tree.txs.len()).filter(|&i| tree.txs[i].kind == *kind).collect();
	let tx = of_kind[step.tx.index(of_kind.len())];
	let parent = tree.txs[tx].parent;
	let node = match &step.node {
		At::Parent => parent,
		At::Any(n) => n.index(tree.nodes.len()),
		At::Below(n) | At::Across(n) => {
			// A wrong-key tx was never applied, so below means below where it was built.
			let applied = tree.txs[tx].child.unwrap_or(parent);
			let below = |m| m == applied || is_ancestor(tree, applied, m);
			let tip = |m: usize| tree.nodes.iter().all(|node| node.parent != Some(m));
			let wanted = matches!(step.node, At::Below(_));
			let tips: Vec<_> =
				(0..tree.nodes.len()).filter(|&m| tip(m) && below(m) == wanted).collect();
			match tips.len() {
				// Only `Across`, in a generated tree with no fork off the tx's path.
				0 => parent,
				len => tips[n.index(len)],
			}
		},
	};
	let bc = &tree.nodes[node].block_context;
	let block_context = BlockContext {
		tblock: bc.tblock.saturating_add_signed(step.dt),
		last_block_time: bc.last_block_time - step.parent_gap,
		..bc.clone()
	};
	(tx, node, block_context)
}

/// The step's outcome, `Ok` values SCALE-encoded so every entry point compares alike. `D` is the
/// on-chain storage; off-chain calls read through it.
fn run<D: DB>(
	tree: &Tree,
	step: &Step,
	exts: &mut Exts,
	cache: &TxValidationCache,
) -> Result<Vec<u8>, LedgerApiError>
where
	D::Hasher: storage_sha2::digest::OutputSizeUser<OutputSize = storage_sha2::digest::consts::U32>,
{
	let (tx, node, block_context) = target(tree, step);
	let (key, bytes, rv) =
		(&tree.nodes[node].state_key[..], &tree.txs[tx].bytes[..], step.runtime_version);

	match step.call {
		Call::Validate { pre_dispatch, skew_tblock, check_utxos, ephemeral, get_tx_details } => {
			if ephemeral {
				let mut ext = exts.offchain.ext();
				ensure_storage::<D>(&mut ext);
				validate::<EphemeralDb<D>>(
					&mut ext,
					key,
					bytes,
					block_context,
					rv,
					pre_dispatch,
					skew_tblock,
					check_utxos,
					get_tx_details,
					cache,
				)
			} else {
				validate::<D>(
					&mut exts.onchain.ext(),
					key,
					bytes,
					block_context,
					rv,
					pre_dispatch,
					skew_tblock,
					check_utxos,
					get_tx_details,
					cache,
				)
			}
		},
		Call::Apply { skew_tblock, filter_only_failed_segments, net_intra_tx_utxos } => {
			shim::apply_transaction::<D, proofs::UnverifiedProofs>(
				&mut exts.onchain.ext(),
				key,
				bytes,
				block_context,
				filter_only_failed_segments,
				net_intra_tx_utxos,
				rv,
				skew_tblock,
				cache,
			)
			.map(|applied| applied.encode())
		},
	}
}

#[allow(clippy::too_many_arguments)]
fn validate<D: DB>(
	ext: &mut dyn Externalities,
	key: &[u8],
	bytes: &[u8],
	block_context: BlockContext,
	rv: u32,
	pre_dispatch: bool,
	skew_tblock: bool,
	check_utxos: bool,
	get_tx_details: bool,
	cache: &TxValidationCache,
) -> Result<Vec<u8>, LedgerApiError>
where
	D::Hasher: storage_sha2::digest::OutputSizeUser<OutputSize = storage_sha2::digest::consts::U32>,
{
	type P = proofs::UnverifiedProofs;
	if pre_dispatch {
		shim::validate_guaranteed_execution::<D, P>(
			ext,
			key,
			bytes,
			block_context,
			rv,
			skew_tblock,
			check_utxos,
			cache,
		)
		.map(|()| vec![])
	} else {
		// The mempool never skews.
		shim::validate_transaction::<D, P>(
			ext,
			key,
			bytes,
			block_context,
			rv,
			u64::MAX,
			get_tx_details,
			check_utxos,
			cache,
		)
		.map(|result| result.encode())
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Storage {
	OnChain,
	OffChain,
}

/// How a receipt was lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Lost {
	Evicted,
	Orphaned,
}

fn cache_key(tree: &Tree, tx: usize, runtime_version: u32) -> TxValidationKey {
	TxValidationKey { runtime_version, tx_hash: tree.txs[tx].hash }
}

/// Unlike `get`, not a read to moka, so the harness doesn't steer which receipts it keeps.
///
/// Compares fields, not the key's `Eq`, so a key that stops separating versions can't hide it.
fn peek(cache: &TxValidationCache, key: &TxValidationKey) -> Option<model::Receipt> {
	cache
		.0
		.iter()
		.find(|(k, _)| k.runtime_version == key.runtime_version && k.tx_hash == key.tx_hash)
		.map(|(_, r)| (r.state_key, r.tblock))
}

fn is_ancestor(tree: &Tree, ancestor: usize, node: usize) -> bool {
	std::iter::successors(tree.nodes[node].parent, |&n| tree.nodes[n].parent).any(|n| n == ancestor)
}

fn storage(call: &Call) -> Storage {
	match call {
		Call::Validate { ephemeral: true, .. } => Storage::OffChain,
		_ => Storage::OnChain,
	}
}

/// The same key with one byte flipped in the first hash it references. A root small enough to
/// be stored inline is not looked up, so only a child of it can be missing.
fn missing_state(state_key: &[u8]) -> Vec<u8> {
	let key: TypedArenaKey<api::Ledger<DefaultDB>, <DefaultDB as DB>::Hasher> =
		api::new().tagged_deserialize(state_key).unwrap();
	let hash = &key.key.refs()[0].0[..];
	let mut bytes = state_key.to_vec();
	let at = bytes
		.windows(hash.len())
		.position(|w| w == hash)
		.expect("the key holds its hashes");
	bytes[at] ^= 1;
	bytes
}
