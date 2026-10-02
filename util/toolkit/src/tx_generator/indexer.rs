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
	IndexerClient, UnshieldedSignatureScheme, WalletSeed, WalletSyncState, ledger_8, ledger_9,
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

/// Pick the context matching the indexer chain's ledger generation and sync `seeds` to its tip,
/// resuming each seed from the wallet cache where possible.
///
/// The indexer carries both ledger generations and serves each chain in its own encodings, so the
/// generation is read off the chain rather than assumed.
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

	let client = IndexerClient::new(indexer_url)?;
	let block = client.latest_block().await?;
	let spec_version = block.protocol_version;
	let ledger_version = LedgerVersion::from_spec_version(spec_version).ok_or_else(|| {
		format!("indexer reports protocol version {spec_version}, which is not a supported ledger")
	})?;
	log::info!("Indexer chain is at protocol version {spec_version} ({ledger_version:?})");

	// Block 1's hash is the chain identity, matching `SourceTransactions::chain_id`. An indexer
	// that has not indexed it yet cannot be told apart from another chain's, so caching is off.
	let chain_id = client.block_hash_at(1).await?.map(H256::from);
	// `--ledger-state-db` and `--fetch-cache` are `global = true` with non-empty defaults, so this
	// is on by default; `--fetch-cache inmemory` is the existing off switch.
	let cache =
		chain_id.zip(create_file_wallet_cache(&source.ledger_state_db, &source.fetch_cache));
	let keys: Vec<H256> = seeds
		.iter()
		.map(|seed| {
			let scheme = schemes.get(seed).copied().unwrap_or_default();
			indexer_wallet_cache_key(seed, scheme, ledger_version)
		})
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

	let network = source.network.as_str();
	let concurrency = source.indexer_concurrency;
	let (context, mut synced) = match ledger_version {
		LedgerVersion::Ledger9 => {
			let ctx = ledger_9::IndexerContext::new(indexer_url, network, concurrency)?
				.with_fast_sync(!source.no_fast_sync);
			let synced = ctx.init_wallets(seeds, &resume).await?;
			(IndexerLedgerContext::Ledger9(Arc::new(ctx)), synced)
		},
		LedgerVersion::Ledger8 => {
			// `WalletSeed` is a per-generation type; the replay path converts the same way.
			let seeds_v8: Vec<_> = seeds.iter().cloned().map(convert_wallet_seed).collect();
			let resume_v8 = seeds
				.iter()
				.zip(&seeds_v8)
				.filter_map(|(seed, seed_v8)| Some((seed_v8.clone(), resume.get(seed)?.clone())))
				.collect();
			let ctx = ledger_8::IndexerContext::new(indexer_url, network, concurrency)?
				.with_fast_sync(!source.no_fast_sync);
			let synced_v8 = ctx.init_wallets(&seeds_v8, &resume_v8).await?;
			let synced = seeds
				.iter()
				.zip(&seeds_v8)
				.filter_map(|(seed, seed_v8)| Some((seed.clone(), synced_v8.get(seed_v8)?.clone())))
				.collect();
			(IndexerLedgerContext::Ledger8(Arc::new(ctx)), synced)
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::fetcher::fetch_storage::file_backend::FileBackend;
	use ledger_9::{
		BuilderContext, DefaultDB, DustWallet, HashMapStorage, HashOutput, INITIAL_PARAMETERS,
		Intent, IntentHash, SeedableRng, SerdeTransaction, ShieldedWallet, Signature, StdRng,
		Timestamp, Transaction, UnshieldedOffer, UnshieldedTokenType, UnshieldedWallet, Utxo,
		UtxoOutput, UtxoSpend, Wallet, make_block_context, serialize_untagged,
	};
	use midnight_ledger_unsafe_helpers::UnshieldedUtxoRaw;
	use std::num::NonZeroUsize;

	fn wallet(seed: &ledger_9::WalletSeed) -> Wallet<DefaultDB> {
		Wallet {
			root_seed: Some(seed.clone()),
			shielded: ShieldedWallet::default(seed.clone()),
			unshielded: UnshieldedWallet::default(seed.clone()),
			dust: DustWallet::default(seed.clone(), Some(&INITIAL_PARAMETERS)),
		}
	}

	#[tokio::test]
	async fn pending_tx_changes_the_context_but_not_the_cache() {
		let (alice, bob) =
			(ledger_9::WalletSeed::Short([1; 16]), ledger_9::WalletSeed::Short([2; 16]));
		let (alice_wallet, bob_wallet) = (wallet(&alice), wallet(&bob));
		let token = UnshieldedTokenType(HashOutput([0; 32]));
		let coin = Utxo {
			value: 100,
			owner: alice_wallet.unshielded.user_address,
			type_: token,
			intent_hash: IntentHash(HashOutput([7; 32])),
			output_no: 3,
		};
		let spend = UtxoSpend {
			value: coin.value,
			owner: alice_wallet.unshielded.verifying_key(),
			type_: token,
			intent_hash: coin.intent_hash,
			output_no: coin.output_no,
		};
		let bob_address = bob_wallet.unshielded.user_address;

		// Unroutable: any indexer query fails the test.
		let ctx = ledger_9::IndexerContext::<DefaultDB>::new(
			"http://127.0.0.1:1/api/v4",
			"undeployed",
			NonZeroUsize::MIN,
		)
		.unwrap();
		ctx.insert_wallet(
			alice.clone(),
			alice_wallet,
			vec![(coin.clone(), Timestamp::from_secs(1))],
		);
		ctx.insert_wallet(bob.clone(), bob_wallet, vec![]);

		let dir = tempfile::tempdir().unwrap();
		let chain_id = H256::repeat_byte(0xAB);
		let key = indexer_wallet_cache_key(
			&alice,
			UnshieldedSignatureScheme::Schnorr,
			LedgerVersion::Ledger9,
		);
		let entry = CachedWalletState::from_sync_state(
			key,
			5,
			WalletSyncState {
				shielded_state: None,
				unshielded_utxos: vec![UnshieldedUtxoRaw {
					utxo: serialize_untagged(&coin).unwrap(),
					ctime_secs: 1,
				}],
				unshielded_tx_id: 9,
				dust_state: None,
				dust_event_id: 0,
			},
		);
		let synced = SyncedIndexer {
			context: IndexerLedgerContext::Ledger9(Arc::new(ctx)),
			cache: Some((chain_id, Box::new(FileBackend::new(dir.path())))),
			entries: vec![entry.clone()],
		};
		let IndexerLedgerContext::Ledger9(ctx) = &synced.context else { unreachable!() };

		let mut rng = StdRng::seed_from_u64(0);
		let offer = UnshieldedOffer {
			inputs: vec![spend].into(),
			outputs: vec![UtxoOutput { value: coin.value, owner: bob_address, type_: token }]
				.into(),
			signatures: vec![].into(),
		};
		let intent = Intent::<Signature, _, _, DefaultDB>::new(
			&mut rng,
			Some(offer),
			None,
			vec![],
			vec![],
			vec![],
			None,
			Timestamp::from_secs(1_000),
		);
		let tx = Transaction::new(
			"undeployed",
			HashMapStorage::new().insert(1, intent),
			None,
			HashMapStorage::new(),
		)
		.seal(rng);
		let Transaction::Standard(stx) = &tx else { unreachable!() };
		// Guaranteed outputs hash under segment 0.
		let intent_hash =
			stx.intents.get(&1).unwrap().erase_proofs().erase_signatures().intent_hash(0);
		let block_context = make_block_context(
			Timestamp::from_secs(20),
			HashOutput([0; 32]),
			Timestamp::from_secs(14),
		);

		ctx.apply_pending_tx(&SerdeTransaction::Midnight(tx), &block_context)
			.await
			.unwrap();

		assert!(ctx.unshielded_utxos(alice).await.is_empty(), "the spent UTXO must be gone");
		let received =
			Utxo { value: coin.value, owner: bob_address, type_: token, intent_hash, output_no: 0 };
		assert_eq!(ctx.unshielded_utxos(bob).await, vec![(received, block_context.tblock)]);

		synced.save_cache().await;
		let saved = FileBackend::new(dir.path()).get_wallet_states(chain_id, &[key]).await;
		assert_eq!(saved, vec![Some(entry)], "the cache must hold only indexer-confirmed state");
	}
}
