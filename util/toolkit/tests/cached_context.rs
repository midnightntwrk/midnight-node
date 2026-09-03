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

//! Integration tests verifying `build_fork_aware_context_cached` produces the
//! same result as `build_fork_aware_context_raw` across all cache scenarios.

use midnight_node_ledger_helpers::{
	DefaultDB, LedgerContext, UnshieldedSignatureScheme, WalletSeed,
	fork::raw_block_data::{LedgerVersion, RawBlockData},
	ledger_8, serialize_untagged,
};
use midnight_node_toolkit::fetcher::wallet_state_cache::{
	serialize_ledger_state_fast, serialize_ledger_state_fast_8, wallet_cache_key,
};
use midnight_node_toolkit::{
	fetcher::fetch_storage::{WalletStateCaching, file_backend::FileBackend},
	serde_def::SourceTransactions,
	tx_generator::{
		builder::{build_fork_aware_context_cached, build_fork_aware_context_raw},
		source::GetTxsFromFile,
	},
};
use subxt::utils::H256;

fn load_genesis_source() -> SourceTransactions {
	let genesis_path =
		format!("{}/test-data/genesis/genesis_block_undeployed.mn", env!("CARGO_MANIFEST_DIR"));
	let batches = GetTxsFromFile::load_single_or_multiple(&genesis_path)
		.expect("failed to load genesis file");
	let mut source = SourceTransactions::from_batches(batches.batches, true, None);

	assign_block_numbers(&mut source);

	// protection from dummy tests
	assert!(
		source.chain_id().is_some(),
		"genesis must produce a valid chain_id for caching tests to be meaningful"
	);
	assert!(source.blocks.len() >= 2);
	source
}

/// Assign sequential block numbers and deterministic hashes so `chain_id()`
/// returns `Some` (it looks for a block with `number == 1`).
fn assign_block_numbers(source: &mut SourceTransactions) {
	for (i, block) in source.blocks.iter_mut().enumerate() {
		block.number = i as u64;
		block.hash = {
			let mut h = [0u8; 32];
			h[..8].copy_from_slice(&(i as u64).to_le_bytes());
			h
		};
	}
}

fn wallet_seed(hex_byte: u8) -> WalletSeed {
	let hex = format!("{:0>64}", format!("{:02x}", hex_byte));
	WalletSeed::try_from_hex_str(&hex).unwrap()
}

fn assert_contexts_equal(
	label: &str,
	cached: &LedgerContext<DefaultDB>,
	raw: &LedgerContext<DefaultDB>,
	seeds: &[WalletSeed],
) {
	// Compare ledger state
	let cached_bytes = {
		let state = cached.ledger_state.lock().unwrap();
		serialize_ledger_state_fast(&state).unwrap()
	};
	let raw_bytes = {
		let state = raw.ledger_state.lock().unwrap();
		serialize_ledger_state_fast(&state).unwrap()
	};
	assert!(!cached_bytes.is_empty(), "{label}: cached ledger state serialized to empty");
	assert_eq!(cached_bytes, raw_bytes, "{label}: ledger state diverged");

	// Compare per-wallet state
	let cached_wallets = cached.wallets.lock().unwrap();
	let raw_wallets = raw.wallets.lock().unwrap();
	assert_eq!(cached_wallets.len(), raw_wallets.len(), "{label}: wallet count mismatch");

	for seed in seeds {
		let cw = cached_wallets
			.get(seed)
			.unwrap_or_else(|| panic!("{label}: cached wallet missing"));
		let rw = raw_wallets.get(seed).unwrap_or_else(|| panic!("{label}: raw wallet missing"));

		let cs = serialize_untagged(&cw.shielded.state).expect("serialize cached shielded");
		let rs = serialize_untagged(&rw.shielded.state).expect("serialize raw shielded");
		assert!(!cs.is_empty(), "{label}: shielded state serialized to empty for seed {seed:?}");
		assert_eq!(cs, rs, "{label}: shielded state diverged for seed {seed:?}");

		let cd = cw
			.dust
			.dust_local_state
			.as_ref()
			.map(|s| serialize_untagged(&**s).expect("serialize cached dust"));
		let rd = rw
			.dust
			.dust_local_state
			.as_ref()
			.map(|s| serialize_untagged(&**s).expect("serialize raw dust"));
		assert_eq!(cd, rd, "{label}: dust state diverged for seed {seed:?}");
	}
}

async fn assert_cache_empty(backend: &dyn WalletStateCaching, chain_id: H256) {
	assert_eq!(backend.get_latest_ledger_height(chain_id).await, None);
	assert!(backend.get_all_cached_wallet_heights(chain_id).await.is_empty());
}

async fn verify_cache_state(
	backend: &dyn WalletStateCaching,
	chain_id: H256,
	blocks: usize,
	wallets: Vec<WalletSeed>,
) {
	assert_eq!(backend.get_latest_ledger_height(chain_id).await, Some(blocks as u64 - 1));
	let wallet_states: Vec<_> = backend
		.get_wallet_states(
			chain_id,
			&wallets
				.iter()
				.map(|s| wallet_cache_key(s, UnshieldedSignatureScheme::Schnorr))
				.collect::<Vec<H256>>(),
		)
		.await
		.into_iter()
		.flatten()
		.collect();
	assert_eq!(wallet_states.len(), wallets.len());
}

// ---------------------------------------------------------------------------
// Test scenarios (backend-agnostic)
// ---------------------------------------------------------------------------

async fn test_cache_and_restore(backend: &dyn WalletStateCaching, source: &SourceTransactions) {
	let seeds = vec![wallet_seed(0x01), wallet_seed(0x02)];

	let raw = build_fork_aware_context_raw(&source, &seeds).into_ledger9().unwrap();

	let cached = build_fork_aware_context_cached(&seeds, &source, Some(backend), 0)
		.await
		.into_ledger9()
		.unwrap();
	verify_cache_state(backend, source.chain_id().unwrap(), source.blocks.len(), seeds.clone())
		.await;

	assert_contexts_equal("2 seeds", &cached, &raw, &seeds);

	let cached = build_fork_aware_context_cached(&seeds, &source, Some(backend), 0).await;
	verify_cache_state(backend, source.chain_id().unwrap(), source.blocks.len(), seeds.clone())
		.await;

	let cached = cached.into_ledger9().expect("cached: expected ledger 8");

	assert_contexts_equal("2 seeds restored", &cached, &raw, &seeds);
}

async fn test_split_cached(backend: &dyn WalletStateCaching, source: &SourceTransactions) {
	let seed1 = vec![wallet_seed(0x01)];
	let _ = build_fork_aware_context_cached(&seed1, &source, Some(backend), 0).await;
	verify_cache_state(backend, source.chain_id().unwrap(), source.blocks.len(), seed1).await;

	let seeds = vec![wallet_seed(0x01), wallet_seed(0x02)];
	let cached_ctx = build_fork_aware_context_cached(&seeds, &source, Some(backend), 0).await;
	verify_cache_state(backend, source.chain_id().unwrap(), source.blocks.len(), seeds.clone())
		.await;

	let raw_ctx = build_fork_aware_context_raw(&source, &seeds);

	let cached = cached_ctx.into_ledger9().expect("cached: expected ledger 8");
	let raw = raw_ctx.into_ledger9().expect("raw: expected ledger 8");

	assert_contexts_equal("split_cached", &cached, &raw, &seeds);
}

#[tokio::test]
async fn file_cached_context() {
	let source = load_genesis_source();
	let tmp = tempfile::TempDir::new().expect("failed to create temp dir");
	let backend = FileBackend::new(tmp.path());
	assert_cache_empty(&backend, source.chain_id().unwrap()).await;

	test_cache_and_restore(&backend, &source).await;

	let tmp2 = tempfile::TempDir::new().expect("failed to create temp dir");
	let backend2 = FileBackend::new(tmp2.path());
	test_split_cached(&backend2, &source).await;
}

// ---------------------------------------------------------------------------
// Cache generation dispatch: ledger-8 chains and the 8->9 crossing
// ---------------------------------------------------------------------------

/// A synthetic chain of empty blocks: `l8` ledger-8 blocks from genesis followed
/// by `l9` ledger-9 blocks. Empty blocks need no ledger-8 transaction fixture yet
/// still drive the whole cache/restore/replay path per generation.
fn synthetic_source(l8: u64, l9: u64) -> SourceTransactions {
	let block = |number: u64, version: LedgerVersion| RawBlockData {
		hash: [0; 32],
		parent_hash: [0; 32],
		number,
		ledger_version: version,
		transactions: vec![],
		tblock_secs: 1_700_000_000 + number * 6,
		tblock_err: 30,
		parent_block_hash: [0; 32],
		last_block_time_secs: 1_700_000_000 + number.saturating_sub(1) * 6,
		state_root: None,
		state: None,
	};
	let blocks = (0..l8)
		.map(|n| block(n, LedgerVersion::Ledger8))
		.chain((l8..l8 + l9).map(|n| block(n, LedgerVersion::Ledger9)))
		.collect();
	let mut source = SourceTransactions::new(blocks, "undeployed");
	assign_block_numbers(&mut source);
	assert!(source.chain_id().is_some());
	source
}

fn assert_contexts_equal_8(
	label: &str,
	cached: &ledger_8::context::LedgerContext<ledger_8::DefaultDB>,
	raw: &ledger_8::context::LedgerContext<ledger_8::DefaultDB>,
	seeds: &[WalletSeed],
) {
	let cached_bytes = serialize_ledger_state_fast_8(&cached.ledger_state.lock().unwrap()).unwrap();
	let raw_bytes = serialize_ledger_state_fast_8(&raw.ledger_state.lock().unwrap()).unwrap();
	assert!(!cached_bytes.is_empty(), "{label}: cached ledger state serialized to empty");
	assert_eq!(cached_bytes, raw_bytes, "{label}: ledger state diverged");

	let cached_wallets = cached.wallets.lock().unwrap();
	let raw_wallets = raw.wallets.lock().unwrap();
	assert_eq!(cached_wallets.len(), raw_wallets.len(), "{label}: wallet count mismatch");
	for seed in seeds {
		let seed_8 = ledger_8::WalletSeed::try_from(seed.as_bytes()).unwrap();
		let cw = cached_wallets
			.get(&seed_8)
			.unwrap_or_else(|| panic!("{label}: cached wallet missing"));
		let rw = raw_wallets
			.get(&seed_8)
			.unwrap_or_else(|| panic!("{label}: raw wallet missing"));
		let cs = ledger_8::serialize_untagged(&cw.shielded.state).unwrap();
		let rs = ledger_8::serialize_untagged(&rw.shielded.state).unwrap();
		assert_eq!(cs, rs, "{label}: shielded state diverged for seed {seed:?}");
	}
}

/// Ledger-8 chain: cold replay saves a ledger-8 snapshot, the warm rerun restores it.
#[tokio::test]
async fn ledger8_chain_cache_and_restore() {
	let source = synthetic_source(6, 0);
	let seeds = vec![wallet_seed(0x01), wallet_seed(0x02)];
	let tmp = tempfile::TempDir::new().unwrap();
	let backend = FileBackend::new(tmp.path());
	let chain_id = source.chain_id().unwrap();

	let raw = build_fork_aware_context_raw(&source, &seeds)
		.into_ledger8()
		.expect("raw: ledger 8");

	let cold = build_fork_aware_context_cached(&seeds, &source, Some(&backend), 0)
		.await
		.into_ledger8()
		.expect("cold: ledger 8");
	verify_cache_state(&backend, chain_id, source.blocks.len(), seeds.clone()).await;
	assert_contexts_equal_8("ledger-8 cold", &cold, &raw, &seeds);

	let warm = build_fork_aware_context_cached(&seeds, &source, Some(&backend), 0)
		.await
		.into_ledger8()
		.expect("warm: ledger 8");
	verify_cache_state(&backend, chain_id, source.blocks.len(), seeds.clone()).await;
	assert_contexts_equal_8("ledger-8 warm restore", &warm, &raw, &seeds);

	// Chunked checkpoints work on ledger 8 too.
	let tmp2 = tempfile::TempDir::new().unwrap();
	let backend2 = FileBackend::new(tmp2.path());
	let chunked = build_fork_aware_context_cached(&seeds, &source, Some(&backend2), 2)
		.await
		.into_ledger8()
		.expect("chunked: ledger 8");
	assert_contexts_equal_8("ledger-8 chunked", &chunked, &raw, &seeds);
}

/// A ledger-8 cache is discarded once the chain has crossed to ledger 9: the
/// run replays from genesis (through the fork) instead of splicing stale
/// wallet state into a ledger-9 block, and re-caches at ledger 9.
#[tokio::test]
async fn ledger8_cache_is_discarded_once_chain_crosses_to_ledger9() {
	let tmp = tempfile::TempDir::new().unwrap();
	let backend = FileBackend::new(tmp.path());

	// Warm the cache on the ledger-8 chain for seed 1 only.
	let l8_source = synthetic_source(6, 0);
	let _ = build_fork_aware_context_cached(&[wallet_seed(0x01)], &l8_source, Some(&backend), 0)
		.await
		.into_ledger8()
		.expect("ledger 8");

	// Same chain, now with ledger-9 blocks appended; one cached and one uncached
	// seed (the CR-1 shape) and the all-cached shape must both replay cleanly.
	let crossed = synthetic_source(6, 4);
	let chain_id = crossed.chain_id().unwrap();
	for seeds in [vec![wallet_seed(0x01), wallet_seed(0x02)], vec![wallet_seed(0x01)]] {
		let raw = build_fork_aware_context_raw(&crossed, &seeds)
			.into_ledger9()
			.expect("raw: ledger 9");
		let cached = build_fork_aware_context_cached(&seeds, &crossed, Some(&backend), 0)
			.await
			.into_ledger9()
			.expect("cached: ledger 9");
		assert_contexts_equal("crossed chain", &cached, &raw, &seeds);
		verify_cache_state(&backend, chain_id, crossed.blocks.len(), seeds).await;
	}
}

/// Mixed ledger-8 cache heights (the CR-3 shape) replay from genesis rather than
/// aborting in the ledger-8 restore.
#[tokio::test]
async fn ledger8_cache_with_mixed_heights_replays_from_genesis() {
	let tmp = tempfile::TempDir::new().unwrap();
	let backend = FileBackend::new(tmp.path());

	let short = synthetic_source(4, 0);
	let _ = build_fork_aware_context_cached(&[wallet_seed(0x01)], &short, Some(&backend), 0).await;
	let long = synthetic_source(6, 0);
	let _ = build_fork_aware_context_cached(&[wallet_seed(0x02)], &long, Some(&backend), 0).await;

	let seeds = vec![wallet_seed(0x01), wallet_seed(0x02)];
	let raw = build_fork_aware_context_raw(&long, &seeds)
		.into_ledger8()
		.expect("raw: ledger 8");
	let cached = build_fork_aware_context_cached(&seeds, &long, Some(&backend), 0)
		.await
		.into_ledger8()
		.expect("cached: ledger 8");
	assert_contexts_equal_8("mixed heights", &cached, &raw, &seeds);
	verify_cache_state(&backend, long.chain_id().unwrap(), long.blocks.len(), seeds).await;
}
