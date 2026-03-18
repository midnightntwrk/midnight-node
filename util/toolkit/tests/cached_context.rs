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

use midnight_node_ledger_helpers::{DefaultDB, LedgerContext, WalletSeed, serialize_untagged};
use midnight_node_toolkit::fetcher::wallet_state_cache::{hash_seed, serialize_ledger_state_fast};
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
	let mut source = SourceTransactions::from_batches(batches.batches, true);

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
		.get_wallet_states(chain_id, &wallets.iter().map(hash_seed).collect::<Vec<H256>>())
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

	let raw = build_fork_aware_context_raw(&source, &seeds).into_ledger8().unwrap();

	let cached = build_fork_aware_context_cached(&seeds, &source, Some(backend))
		.await
		.into_ledger8()
		.unwrap();
	verify_cache_state(backend, source.chain_id().unwrap(), source.blocks.len(), seeds.clone())
		.await;

	assert_contexts_equal("2 seeds", &cached, &raw, &seeds);

	let cached = build_fork_aware_context_cached(&seeds, &source, Some(backend)).await;
	verify_cache_state(backend, source.chain_id().unwrap(), source.blocks.len(), seeds.clone())
		.await;

	let cached = cached.into_ledger8().expect("cached: expected ledger 8");

	assert_contexts_equal("2 seeds restored", &cached, &raw, &seeds);
}

async fn test_split_cached(backend: &dyn WalletStateCaching, source: &SourceTransactions) {
	let seed1 = vec![wallet_seed(0x01)];
	let _ = build_fork_aware_context_cached(&seed1, &source, Some(backend)).await;
	verify_cache_state(backend, source.chain_id().unwrap(), source.blocks.len(), seed1).await;

	let seeds = vec![wallet_seed(0x01), wallet_seed(0x02)];
	let cached_ctx = build_fork_aware_context_cached(&seeds, &source, Some(backend)).await;
	verify_cache_state(backend, source.chain_id().unwrap(), source.blocks.len(), seeds.clone())
		.await;

	let raw_ctx = build_fork_aware_context_raw(&source, &seeds);

	let cached = cached_ctx.into_ledger8().expect("cached: expected ledger 8");
	let raw = raw_ctx.into_ledger8().expect("raw: expected ledger 8");

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
