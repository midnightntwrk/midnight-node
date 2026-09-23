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

//! Indexer-backed wallet sync (`--indexer-url`), shared by every command that reads wallets from
//! the indexer instead of replaying blocks. See issue #1186.

use std::{collections::HashMap, sync::Arc};

use midnight_ledger_unsafe_helpers::{
	IndexerClient, UnshieldedSignatureScheme, WalletSeed, WalletSyncState,
	indexer_client::BlockInfo, ledger_8, ledger_9,
};
use midnight_node_ledger_helpers::fork::raw_block_data::LedgerVersion;
use subxt::utils::H256;

use super::builder::{WalletSchemes, builders::ledger_8::type_convert::convert_wallet_seed};
use super::source::{Source, create_file_wallet_cache};
use crate::fetcher::{
	fetch_storage::WalletStateCaching,
	wallet_state_cache::{CachedWalletState, indexer_wallet_cache_key},
};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// An indexer context for the ledger generation of the chain the indexer serves.
pub enum IndexerLedgerContext {
	Ledger8(Arc<ledger_8::IndexerContext<ledger_8::DefaultDB>>),
	Ledger9(Arc<ledger_9::IndexerContext<ledger_9::DefaultDB>>),
}

impl IndexerLedgerContext {
	pub fn version(&self) -> LedgerVersion {
		match self {
			Self::Ledger8(_) => LedgerVersion::Ledger8,
			Self::Ledger9(_) => LedgerVersion::Ledger9,
		}
	}
}

/// A context whose wallets are synced to the indexer's tip, plus the cache entries to persist.
pub struct SyncedIndexer {
	pub context: IndexerLedgerContext,
	cache: Option<(H256, Box<dyn WalletStateCaching>)>,
	entries: Vec<CachedWalletState>,
}

impl SyncedIndexer {
	/// Persist each seed's sync state so the next run resumes from it.
	pub async fn save_cache(&self) {
		if let Some((chain_id, cache)) = &self.cache {
			cache.set_wallet_states(*chain_id, &self.entries).await;
		}
	}
}

/// Build a context for the ledger generation of the chain the indexer serves, with no wallets
/// synced. Also returns the tip block the generation was read from.
///
/// The indexer carries both ledger generations and serves each chain in its own encodings, so the
/// generation is read off the chain rather than assumed.
pub async fn connect_indexer(
	source: &Source,
	indexer_url: &str,
) -> Result<(IndexerLedgerContext, BlockInfo), BoxError> {
	let block = IndexerClient::new(indexer_url)?.latest_block().await?;
	let spec_version = block.protocol_version;
	let ledger_version = LedgerVersion::from_spec_version(spec_version).ok_or_else(|| {
		format!("indexer reports protocol version {spec_version}, which is not a supported ledger")
	})?;
	log::info!("Indexer chain is at protocol version {spec_version} ({ledger_version:?})");

	let network = source.network.as_str();
	let concurrency = source.indexer_concurrency;
	let context = match ledger_version {
		LedgerVersion::Ledger9 => IndexerLedgerContext::Ledger9(Arc::new(
			ledger_9::IndexerContext::new(indexer_url, network, concurrency)?,
		)),
		LedgerVersion::Ledger8 => IndexerLedgerContext::Ledger8(Arc::new(
			ledger_8::IndexerContext::new(indexer_url, network, concurrency)?,
		)),
	};
	Ok((context, block))
}

/// Pick the context matching the indexer chain's ledger generation and sync `seeds` to its tip,
/// resuming each seed from the wallet cache where possible.
pub async fn sync_indexer(
	source: &Source,
	indexer_url: &str,
	seeds: &[WalletSeed],
	schemes: &WalletSchemes,
) -> Result<SyncedIndexer, BoxError> {
	// Only the Schnorr unshielded identity is derivable from the seed here, so an ECDSA seed would
	// silently use the wrong address. The replay path handles ECDSA via `ensure_ecdsa_supported`.
	if schemes.values().any(|s| *s != UnshieldedSignatureScheme::Schnorr) {
		return Err("--indexer-url only supports the Schnorr NIGHT identity; \
		            an `ecdsa:` seed requires the block-replay path (omit --indexer-url)"
			.into());
	}

	let (context, block) = connect_indexer(source, indexer_url).await?;
	let ledger_version = context.version();
	let client = IndexerClient::new(indexer_url)?;

	// Block 1's hash is the chain identity, matching `SourceTransactions::chain_id`. An indexer
	// that has not indexed it yet cannot be told apart from another chain's, so caching is off.
	let chain_id = client.block_hash_at(1).await?.map(H256::from);
	// `--ledger-state-db` and `--fetch-cache` are `global = true` with non-empty defaults, so this
	// is on by default; `--fetch-cache inmemory` is the existing off switch.
	let cache =
		chain_id.zip(create_file_wallet_cache(&source.ledger_state_db, &source.fetch_cache));
	let keys: Vec<H256> = seeds
		.iter()
		.map(|seed| indexer_wallet_cache_key(seed, ledger_version))
		.collect();

	let mut resume: HashMap<WalletSeed, WalletSyncState> = HashMap::new();
	if let Some((chain_id, cache)) = &cache {
		for (seed, entry) in seeds.iter().zip(cache.get_wallet_states(*chain_id, &keys).await) {
			if let Some(entry) = entry {
				log::info!(
					"Resuming indexer sync from cached state at block {}",
					entry.block_height
				);
				resume.insert(seed.clone(), entry.to_sync_state());
			}
		}
	}

	let mut synced = match &context {
		IndexerLedgerContext::Ledger9(ctx) => ctx.init_wallets(seeds, &resume).await?,
		IndexerLedgerContext::Ledger8(ctx) => {
			// `WalletSeed` is a per-generation type; the replay path converts the same way.
			let seeds_v8: Vec<_> = seeds.iter().cloned().map(convert_wallet_seed).collect();
			let resume_v8 = seeds
				.iter()
				.zip(&seeds_v8)
				.filter_map(|(seed, seed_v8)| Some((seed_v8.clone(), resume.get(seed)?.clone())))
				.collect();
			let synced_v8 = ctx.init_wallets(&seeds_v8, &resume_v8).await?;
			seeds
				.iter()
				.zip(&seeds_v8)
				.filter_map(|(seed, seed_v8)| Some((seed.clone(), synced_v8.get(seed_v8)?.clone())))
				.collect()
		},
	};

	let entries = seeds
		.iter()
		.zip(keys)
		.filter_map(|(seed, key)| {
			Some(CachedWalletState::from_sync_state(key, block.height, synced.remove(seed)?))
		})
		.collect();

	Ok(SyncedIndexer { context, cache, entries })
}
