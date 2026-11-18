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

//! Property tests for [`TxValidationCache`]: replay a fixed tree's transactions in random orders
//! and places, and check the cache never changes a verdict.

mod proofs;
mod tree;

use super::{
	BlockContext, Bridge, TX_VALIDATION_CACHE_CAPACITY, TransactionSignature, TxValidationCache,
	ledger_storage_local::DefaultDB,
	types::{LedgerApiError, TransactionError},
};
use crate::ephemeral::{EphemeralDb, ensure_storage};
use midnight_ledger_unsafe_helpers::ledger_9::{
	Duration, FixedPoint, INITIAL_PARAMETERS, LedgerParameters,
};
use midnight_primitives_ledger::LedgerOnchainExt;
use parity_scale_codec::Encode;
use proptest::{prelude::*, sample::Index, strategy::ValueTree};
use sp_externalities::Externalities;
use std::sync::OnceLock;
use tree::{Branch, Edge, RUNTIME_VERSION, Tree, TreeBridge, build_tree};

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

fn edge() -> impl Strategy<Value = Edge> {
	prop_oneof![
		6 => (0..3usize, 0..3usize).prop_map(|(from, to)| Edge::Transfer { from, to }),
		3 => Just(Edge::EndBlock),
		1 => prop_oneof![Just(Edge::Params(raise_fees)), Just(Edge::Params(shorten_dust_grace))],
	]
}

/// Specs `build_tree` can always build. The byte limit only ever ends a leaf: nothing after it
/// fits.
fn tree_spec() -> impl Strategy<Value = Branch> {
	let edges = || prop::collection::vec(edge(), 1..10);
	let leaf = edges().prop_map(|edges| Branch { edges, children: vec![] });
	leaf.prop_recursive(3, 24, 3, move |inner| {
		let byte_limit_leaf = Branch {
			edges: vec![Edge::Params(shrink_tx_byte_limit), Edge::EndBlock],
			children: vec![],
		};
		(edges(), prop::collection::vec(prop_oneof![4 => inner, 1 => Just(byte_limit_leaf)], 1..4))
			.prop_map(|(edges, children)| Branch { edges, children })
	})
	.prop_filter("a tree needs a transaction to send", Branch::has_transfer)
}

/// Forks at the root and below, with sibling branches spending the same NIGHT and dust.
fn spec() -> Branch {
	use Edge::{EndBlock as E, Params as P};
	let t = |from, to| Edge::Transfer { from, to };
	let leaf = |edges| Branch { edges, children: vec![] };
	Branch {
		edges: vec![t(0, 1), t(1, 2), E, t(2, 0), E],
		children: vec![
			Branch {
				edges: vec![t(0, 2), t(0, 1), E, t(1, 0), E, t(2, 1), E],
				children: vec![
					leaf(vec![t(0, 1), E]),
					leaf(vec![P(raise_fees), t(0, 1), t(1, 2), E]),
				],
			},
			Branch {
				edges: vec![t(0, 2), E, t(2, 1), t(1, 0), E],
				children: vec![
					leaf(vec![P(shrink_tx_byte_limit), E]),
					leaf(vec![P(shorten_dust_grace), t(1, 2), E, t(2, 0), E]),
					leaf(vec![t(2, 0), E, E, t(0, 1), E]),
				],
			},
			leaf(vec![E, t(1, 2), E]),
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
	Validate { pre_dispatch: bool, skew_tblock: bool, check_utxos: bool, ephemeral: bool },
	Apply { skew_tblock: bool, filter_only_failed_segments: bool, net_intra_tx_utxos: bool },
}

#[derive(Debug, Clone)]
struct Step {
	tx: Index,
	/// `None` sends the tx to the node it was applied at.
	node: Option<Index>,
	/// Added to the node's `tblock`.
	dt: i64,
	runtime_version: u32,
	call: Call,
}

fn call() -> impl Strategy<Value = Call> {
	prop_oneof![
		any::<[bool; 4]>().prop_map(|[pre_dispatch, skew_tblock, check_utxos, ephemeral]| {
			Call::Validate { pre_dispatch, skew_tblock, check_utxos, ephemeral }
		}),
		any::<[bool; 3]>().prop_map(
			|[skew_tblock, filter_only_failed_segments, net_intra_tx_utxos]| Call::Apply {
				skew_tblock,
				filter_only_failed_segments,
				net_intra_tx_utxos
			}
		),
	]
}

fn step() -> impl Strategy<Value = Step> {
	const HOUR: i64 = 3600;
	(
		any::<Index>(),
		prop::option::weighted(2.0 / 3.0, any::<Index>()),
		// Intent TTLs run an hour, the dust grace period three.
		prop_oneof![3 => Just(0), 2 => -60i64..=60, 3 => -4 * HOUR..=4 * HOUR],
		1u32..=2,
		call(),
	)
		.prop_map(|(tx, node, dt, runtime_version, call)| Step {
			tx,
			node,
			dt,
			runtime_version,
			call,
		})
}

struct Exts {
	onchain: sp_io::TestExternalities,
	offchain: sp_io::TestExternalities,
}

impl Exts {
	fn new() -> Self {
		let mut onchain = sp_io::TestExternalities::default();
		onchain.register_extension(LedgerOnchainExt::default());
		Self { onchain, offchain: sp_io::TestExternalities::default() }
	}
}

/// The step's outcome, `Ok` values SCALE-encoded so every entry point compares alike.
fn run(
	tree: &Tree,
	step: &Step,
	exts: &mut Exts,
	cache: &TxValidationCache,
) -> Result<Vec<u8>, LedgerApiError> {
	let tx = &tree.txs[step.tx.index(tree.txs.len())];
	let node = &tree.nodes[step.node.map_or(tx.parent, |n| n.index(tree.nodes.len()))];
	let block_context = BlockContext {
		tblock: node.block_context.tblock.saturating_add_signed(step.dt),
		..node.block_context.clone()
	};
	let (key, bytes, rv) = (&node.state_key[..], &tx.bytes[..], step.runtime_version);

	match step.call {
		Call::Validate { pre_dispatch, skew_tblock, check_utxos, ephemeral } => {
			if ephemeral {
				let mut ext = exts.offchain.ext();
				ensure_storage::<DefaultDB>(&mut ext);
				validate::<EphemeralDb<DefaultDB>>(
					&mut ext,
					key,
					bytes,
					block_context,
					rv,
					pre_dispatch,
					skew_tblock,
					check_utxos,
					cache,
				)
			} else {
				validate::<DefaultDB>(
					&mut exts.onchain.ext(),
					key,
					bytes,
					block_context,
					rv,
					pre_dispatch,
					skew_tblock,
					check_utxos,
					cache,
				)
			}
		},
		Call::Apply { skew_tblock, filter_only_failed_segments, net_intra_tx_utxos } => {
			TreeBridge::apply_transaction(
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
fn validate<D: crate::ledger_9::ledger_storage_local::db::DB>(
	ext: &mut dyn Externalities,
	key: &[u8],
	bytes: &[u8],
	block_context: BlockContext,
	rv: u32,
	pre_dispatch: bool,
	skew_tblock: bool,
	check_utxos: bool,
	cache: &TxValidationCache,
) -> Result<Vec<u8>, LedgerApiError>
where
	D::Hasher: storage_sha2::digest::OutputSizeUser<OutputSize = storage_sha2::digest::consts::U32>,
{
	type B<D> = Bridge<TransactionSignature, D, proofs::UnverifiedProofs>;
	if pre_dispatch {
		B::<D>::validate_guaranteed_execution(
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
		B::<D>::validate_transaction(
			ext,
			key,
			bytes,
			block_context,
			rv,
			u64::MAX,
			false,
			check_utxos,
			cache,
		)
		.map(|(hash, _)| hash.to_vec())
	}
}

/// The dummy proofs are the only thing letting tree transactions through, and their bytes
/// deserialize under both proof kinds.
#[test]
fn dummy_proofs_pass_only_unverified() {
	let tree = &smoke_trees()[0];
	let tx = &tree.txs[0];
	let node = &tree.nodes[tx.parent];
	let mut ext = sp_io::TestExternalities::default();
	let mut ext = ext.ext();
	let validate = |ext: &mut dyn Externalities, real: bool| {
		let cache = TxValidationCache::new(1);
		let bc = node.block_context.clone();
		if real {
			Bridge::<TransactionSignature, DefaultDB>::validate_transaction(
				ext,
				&node.state_key,
				&tx.bytes,
				bc,
				RUNTIME_VERSION,
				u64::MAX,
				false,
				true,
				&cache,
			)
		} else {
			TreeBridge::validate_transaction(
				ext,
				&node.state_key,
				&tx.bytes,
				bc,
				RUNTIME_VERSION,
				u64::MAX,
				false,
				true,
				&cache,
			)
		}
	};

	validate(&mut ext, false).expect("UnverifiedProofs accepts the dummy proofs");
	let real = validate(&mut ext, true).unwrap_err();
	assert!(
		matches!(real, LedgerApiError::Transaction(TransactionError::Malformed(_))),
		"ProofMarker must reject the dummy proofs as malformed, got {real:?}"
	);
}

/// P1 over one call history: a warm cache returns what an empty one would, exact error included.
fn check_warm_matches_cold(tree: &Tree, steps: &[Step]) -> Result<(), TestCaseError> {
	let mut exts = Exts::new();
	let warm = TxValidationCache::new(TX_VALIDATION_CACHE_CAPACITY);
	for step in steps {
		let warm_result = run(tree, step, &mut exts, &warm);
		let cold = TxValidationCache::new(TX_VALIDATION_CACHE_CAPACITY);
		let cold_result = run(tree, step, &mut exts, &cold);
		prop_assert_eq!(warm_result, cold_result, "{:?}", step);
	}
	Ok(())
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

	/// P1 over the fixed smoke trees, for CI.
	#[test]
	fn warm_cache_gives_the_cold_verdict(
		tree in any::<Index>(),
		steps in prop::collection::vec(step(), 1..40),
	) {
		let trees = smoke_trees();
		check_warm_matches_cold(&trees[tree.index(trees.len())], &steps)?;
	}
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

	/// P1 with a fresh tree per case, so a failure shrinks the tree as well as the steps.
	///
	/// Every tree's states stay in the process-wide storage, about 0.6 MB per case. Scale by
	/// running repeatedly rather than raising `PROPTEST_CASES` without limit: each run draws a new
	/// seed.
	///
	/// `PROPTEST_CASES=1000 cargo test --release -p midnight-node-ledger --lib -- --ignored \
	///     warm_cache_gives_the_cold_verdict_on_generated_trees`
	#[test]
	#[ignore = "slow: run nightly, scaled with PROPTEST_CASES"]
	fn warm_cache_gives_the_cold_verdict_on_generated_trees(
		spec in tree_spec(),
		steps in prop::collection::vec(step(), 1..60),
	) {
		check_warm_matches_cold(&build_tree(&spec), &steps)?;
	}
}
