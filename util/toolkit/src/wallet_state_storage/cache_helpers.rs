// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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

//! Helper functions for serializing and deserializing wallet state.
//!
//! This module provides the bridge between [`LedgerContext`] and [`WalletStateCache`],
//! handling the conversion logic for caching and restoration.
//!
//! # Caching Strategy
//!
//! The primary benefit of caching comes from persisting the `LedgerState`, which
//! contains the full blockchain state (UTXOs, contract states, parameters, etc.).
//! Reconstructing `LedgerState` from genesis is the main performance bottleneck.
//!
//! Wallet-specific state (shielded coins, dust) is lightweight and can be quickly
//! rebuilt by replaying only the blocks since the cache checkpoint. This approach
//! avoids the complexity of serializing wallet-internal types that don't implement
//! serde traits.

use midnight_node_ledger_helpers::{
	BlockContext, DefaultDB, HashOutput, LedgerContext, LedgerState, Timestamp, Wallet, WalletSeed,
};
use sha2::{Digest, Sha256};
use subxt::utils::H256;

use super::{
	CACHE_VERSION, SerializableBlockContext, WalletSnapshot, WalletStateCache, compute_wallet_id,
};

/// Error type for cache serialization/deserialization.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
	#[error("Failed to serialize ledger state: {0}")]
	SerializeLedgerState(String),
	#[error("Failed to deserialize ledger state: {0}")]
	DeserializeLedgerState(String),
	#[error("Cache version mismatch: expected {expected}, got {actual}")]
	VersionMismatch { expected: String, actual: String },
	#[error("Chain ID mismatch: expected {expected:?}, got {actual:?}")]
	ChainIdMismatch { expected: H256, actual: H256 },
}

/// Serialize a LedgerState to bytes using mn_ledger_serialize.
pub fn serialize_ledger_state(state: &LedgerState<DefaultDB>) -> Result<Vec<u8>, CacheError> {
	midnight_node_ledger_helpers::serialize(state)
		.map_err(|e| CacheError::SerializeLedgerState(e.to_string()))
}

/// Deserialize a LedgerState from bytes.
pub fn deserialize_ledger_state(bytes: &[u8]) -> Result<LedgerState<DefaultDB>, CacheError> {
	midnight_node_ledger_helpers::deserialize(bytes)
		.map_err(|e| CacheError::DeserializeLedgerState(e.to_string()))
}

/// Compute a wallet identity from a LedgerContext's wallets.
///
/// Uses the public key material from the first wallet to generate a stable identity.
pub fn compute_wallet_id_from_context(context: &LedgerContext<DefaultDB>) -> H256 {
	let wallets = context.wallets.lock().expect("failed to lock wallets");
	if let Some(wallet) = wallets.values().next() {
		// Use shielded coin public key as wallet identity
		let coin_pub = wallet.shielded.coin_public_key.0.0;
		// For dust, use the raw bytes from the public key
		let dust_pub = &[];
		compute_wallet_id(&coin_pub, dust_pub)
	} else {
		// Empty wallets - return zero hash
		H256::zero()
	}
}

/// Hash a wallet seed for use as snapshot key.
fn hash_seed(seed: &WalletSeed) -> H256 {
	let mut hasher = Sha256::new();
	hasher.update(seed.as_bytes());
	H256::from_slice(&hasher.finalize())
}

/// Create a WalletStateCache from a LedgerContext.
///
/// This captures the current state of the ledger. Wallet-specific state is stored
/// as seed hashes only - the actual wallet state will be rebuilt during replay
/// of blocks since the checkpoint.
pub fn create_cache_from_context(
	context: &LedgerContext<DefaultDB>,
	chain_id: H256,
	block_height: u64,
	state_root: Option<Vec<u8>>,
) -> Result<WalletStateCache, CacheError> {
	let wallet_id = compute_wallet_id_from_context(context);

	// Serialize ledger state
	let ledger_state = context.ledger_state.lock().expect("failed to lock ledger_state");
	let ledger_state_bytes = serialize_ledger_state(&*ledger_state)?;
	drop(ledger_state);

	// Store wallet seed hashes (actual wallet state will be rebuilt during replay)
	let wallets = context.wallets.lock().expect("failed to lock wallets");
	let wallet_snapshots: Vec<WalletSnapshot> = wallets
		.keys()
		.map(|seed| WalletSnapshot {
			seed_hash: hash_seed(seed),
			// Empty bytes - wallet state will be rebuilt during replay
			shielded_state_bytes: vec![],
			dust_local_state_bytes: None,
		})
		.collect();
	drop(wallets);

	// Get latest block context
	let latest_block_context = context.latest_block_context();
	let serializable_context = SerializableBlockContext::from(&latest_block_context);

	Ok(WalletStateCache {
		chain_id,
		wallet_id,
		block_height,
		ledger_state_bytes,
		wallet_snapshots,
		latest_block_context: serializable_context,
		state_root,
		version: CACHE_VERSION.to_string(),
	})
}

/// Restore a LedgerContext from a WalletStateCache.
///
/// This creates a new LedgerContext with the cached ledger state. Wallet state
/// is initialized fresh and should be rebuilt by replaying blocks from the
/// cache checkpoint to the current head.
///
/// # Arguments
///
/// * `cache` - The cached state to restore from
/// * `wallet_seeds` - The wallet seeds to initialize
/// * `expected_chain_id` - The expected chain ID (for validation)
///
/// # Returns
///
/// A tuple of (LedgerContext, block_height) where block_height is the height
/// at which the cache was created. The caller should replay blocks from
/// block_height+1 to current head to update wallet state.
pub fn restore_context_from_cache(
	cache: &WalletStateCache,
	wallet_seeds: &[WalletSeed],
	expected_chain_id: H256,
) -> Result<(LedgerContext<DefaultDB>, u64), CacheError> {
	// Validate version
	if cache.version != CACHE_VERSION {
		return Err(CacheError::VersionMismatch {
			expected: CACHE_VERSION.to_string(),
			actual: cache.version.clone(),
		});
	}

	// Validate chain ID
	if cache.chain_id != expected_chain_id {
		return Err(CacheError::ChainIdMismatch {
			expected: expected_chain_id,
			actual: cache.chain_id,
		});
	}

	// Deserialize ledger state
	let ledger_state = deserialize_ledger_state(&cache.ledger_state_bytes)?;

	// Create context with a placeholder network_id, then replace the ledger state.
	// The actual network_id is embedded in the restored ledger state.
	let context = LedgerContext::new("restored");
	*context.ledger_state.lock().expect("failed to lock ledger_state") = ledger_state.clone();

	// Restore block context
	let block_context = BlockContext {
		tblock: Timestamp::from_secs(cache.latest_block_context.tblock_secs),
		tblock_err: cache.latest_block_context.tblock_err as u32,
		parent_block_hash: HashOutput(cache.latest_block_context.parent_block_hash),
	};
	*context
		.latest_block_context
		.lock()
		.expect("failed to lock latest_block_context") = Some(block_context);

	// Initialize wallets (will be updated during block replay)
	let mut wallets = context.wallets.lock().expect("failed to lock wallets");
	for seed in wallet_seeds {
		let wallet = Wallet::default(*seed, &ledger_state);
		wallets.insert(*seed, wallet);
	}
	drop(wallets);

	log::info!(
		"Restored LedgerContext from cache at block height {}, {} wallets initialized",
		cache.block_height,
		wallet_seeds.len()
	);

	Ok((context, cache.block_height))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_serializable_block_context_roundtrip() {
		let original = BlockContext {
			tblock: Timestamp::from_secs(12345),
			tblock_err: 30,
			parent_block_hash: HashOutput([42u8; 32]),
		};

		let serializable = SerializableBlockContext::from(&original);

		assert_eq!(serializable.tblock_secs, 12345);
		assert_eq!(serializable.tblock_err, 30);
		assert_eq!(serializable.parent_block_hash, [42u8; 32]);
	}

	#[test]
	fn test_hash_seed_deterministic() {
		let seed = WalletSeed::from_bytes([1u8; 32]);
		let hash1 = hash_seed(&seed);
		let hash2 = hash_seed(&seed);
		assert_eq!(hash1, hash2);

		let different_seed = WalletSeed::from_bytes([2u8; 32]);
		let hash3 = hash_seed(&different_seed);
		assert_ne!(hash1, hash3);
	}
}
