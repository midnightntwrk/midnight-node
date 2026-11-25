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

//! P8a and P8b cases the random histories reach too rarely.

use super::ledger::{
	api,
	ledger_storage_local::{
		DefaultDB,
		arena::{ArenaKey, TypedArenaKey},
		db::DB,
	},
};
use super::{
	Action, At, Call, Class, Classes, Exts, ProofReceipt, Step, Storage,
	TX_VALIDATION_CACHE_CAPACITY, Timestamp, TxValidationCache, TxValidationCacheOutcome,
	cache_key, check_history, lookup, missing_state, model, one_hot, peek, predict, run, shim,
	smoke_tree, smoke_trees, target,
	tree::{RUNTIME_VERSION, TreeBridge},
};
use proptest::{prelude::*, sample::Index, strategy::ValueTree};

proptest! {
	#![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

	/// P8b: after off-chain mempool validation accepts a tx, a fresh cache holds a receipt that
	/// `validate_guaranteed_execution` and `apply_transaction` on-chain, at the same state, hit.
	fn offchain_receipt_serves_onchain(
		tree in any::<Index>(),
		[kind, tx] in any::<[Index; 2]>(),
		node in prop_oneof![Just(At::Parent), any::<Index>().prop_map(At::Any)],
		dt in -60i64..=60,
		runtime_version in 1u32..=2,
		[check_utxos, skew_tblock] in any::<[bool; 2]>(),
	) {
		let trees = smoke_trees();
		let tree = &trees[tree.index(trees.len())];
		let step =
			|call| Step { kind, tx, node: node.clone(), dt, parent_gap: 0, runtime_version, call };
		let offchain = step(Call::Validate {
			pre_dispatch: false,
			skew_tblock: false,
			check_utxos,
			ephemeral: true,
			get_tx_details: false,
		});
		let onchain = [
			Call::Validate {
				pre_dispatch: true,
				skew_tblock,
				check_utxos,
				ephemeral: false,
				get_tx_details: false,
			},
			Call::Apply { skew_tblock, filter_only_failed_segments: true, net_intra_tx_utxos: true },
		];
		for call in onchain.map(step) {
			let mut exts = Exts::new();
			let cache = TxValidationCache::new(TX_VALIDATION_CACHE_CAPACITY);
			if run::<DefaultDB>(tree, &offchain, &mut exts, &cache).is_err() {
				return Ok(());
			}
			let model = model::ModelCall::new(tree, &call);
			let key = cache_key(tree, target(tree, &call).0, runtime_version);
			let receipt = peek(&cache, &key);
			let outcome = predict(
				&mut exts,
				Storage::OnChain,
				&model.state_key,
				model.tblock,
				&key,
				receipt.as_ref(),
			);
			prop_assert_ne!(&outcome, &TxValidationCacheOutcome::CacheMiss, "{:?}", call);
			let passing = model::attempts(receipt.as_ref(), true, &model).pop().unwrap();

			let (result, moved) = exts.outcome_moved(|exts| run::<DefaultDB>(tree, &call, exts, &cache));
			if result.is_ok() {
				prop_assert_eq!(moved, one_hot(&passing), "{:?}", call);
			}
		}
	}
}

/// P8a, row [`model::OutcomeRow::NotLoadable`]: a receipt whose ledger root is small enough to be
/// stored inline, and one of whose children is gone, is a miss and gets evicted.
///
/// A root is inlined when it serializes to at most `SMALL_OBJECT_LIMIT` bytes, children inlined
/// in it included. Genesis after a block end still fits, and `Bridge` keys a state before
/// persisting it, which would turn the key into a reference.
fn receipt_on_a_gone_inline_root_misses() {
	let tree = &smoke_trees()[0];
	let genesis = &tree.nodes[0];
	let root = TreeBridge::post_block_update(
		&mut Exts::new().onchain.ext(),
		&genesis.state_key,
		genesis.block_context.clone(),
	)
	.unwrap();
	let key: TypedArenaKey<api::Ledger<DefaultDB>, <DefaultDB as DB>::Hasher> =
		api::new().tagged_deserialize(&root).unwrap();
	assert!(matches!(key.key, ArenaKey::Direct(_)), "genesis after a block end is inlined");
	let node = &tree.nodes[tree.txs[0].parent];
	let key = cache_key(tree, 0, RUNTIME_VERSION);
	let tblock = Timestamp::from_secs(node.block_context.tblock);
	let cache = TxValidationCache::new(TX_VALIDATION_CACHE_CAPACITY);
	cache
		.0
		.insert(key.clone(), ProofReceipt { state_key: missing_state(&root), tblock });

	let outcome = lookup(&mut Exts::new(), Storage::OnChain, &node.state_key, tblock, &key, &cache);
	assert_eq!(outcome, TxValidationCacheOutcome::CacheMiss);
	assert_eq!(peek(&cache, &key), None, "the receipt is evicted");
}

/// A skewed call at a block start, an hour after its parent block: its corrected tblock is an hour
/// before the block's, where every tree tx fails `well_formed`, so ledger 8 retries at the block's
/// own. Random steps never pass a retry: every tree state is minutes from every build, so the
/// corrected tblock fails no check the block's passes. Run with and without a mempool receipt at
/// the block's tblock, which the retry then strictly hits.
fn retry_at_the_block_tblock() {
	let strategy = (smoke_tree(), any::<[Index; 2]>(), any::<[bool; 2]>(), 1u32..=2);
	let mut runner = proptest::test_runner::TestRunner::deterministic();
	let trees = smoke_trees();
	let mut classes = Classes::new();
	for _ in 0..64 {
		let (tree, [kind, tx], [apply, receipt], runtime_version) =
			strategy.new_tree(&mut runner).unwrap().current();
		let step = |call, parent_gap| {
			Action::Call(Step {
				kind,
				tx,
				node: At::Parent,
				dt: 0,
				parent_gap,
				runtime_version,
				call,
			})
		};
		let mempool = Call::Validate {
			pre_dispatch: false,
			skew_tblock: false,
			check_utxos: true,
			ephemeral: false,
			get_tx_details: false,
		};
		let skewed = if apply {
			Call::Apply {
				skew_tblock: true,
				filter_only_failed_segments: true,
				net_intra_tx_utxos: true,
			}
		} else {
			Call::Validate {
				pre_dispatch: true,
				skew_tblock: true,
				check_utxos: true,
				ephemeral: false,
				get_tx_details: false,
			}
		};
		let actions: Vec<_> = receipt
			.then(|| step(mempool, 0))
			.into_iter()
			.chain([step(skewed, 3600)])
			.collect();
		check_history(&trees[tree], TX_VALIDATION_CACHE_CAPACITY, &actions, &mut classes).unwrap();
	}
	println!("{classes:#?}");
	if shim::RETRY_AT_BLOCK_TBLOCK {
		assert!(classes.contains_key(&Class::Retried(true)), "no retry passed");
	}
}

suite_tests!(
	offchain_receipt_serves_onchain,
	receipt_on_a_gone_inline_root_misses,
	retry_at_the_block_tblock,
);
