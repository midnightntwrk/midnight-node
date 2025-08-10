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

use lazy_static::lazy_static;
use rand::{Rng, RngCore, SeedableRng, rngs::SmallRng};
use serde::{Deserialize, Serialize};
use std::{
	cell::RefCell,
	collections::HashMap,
	sync::Mutex,
	time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex as MutexTokio;

use crate::{
	BlockContext, DB, Deserializable, HashOutput, LedgerState, NetworkId, PUBLIC_PARAMS,
	PedersenRandomness, ProofKind, Resolver, Serializable, SignatureKind, Timestamp, Transaction,
	TransactionContext, TransactionResult, Utxo, Version, Versioned, Wallet, WalletAddress,
	WalletSeed, deserialize, serialize,
};

lazy_static! {
	pub static ref DEFAULT_RESOLVER: Resolver = Resolver::new(
		PUBLIC_PARAMS.clone(),
		Box::new(|_key_location| Box::pin(std::future::ready(Ok(None)))),
	);
}

pub struct LedgerContext<D: DB + Clone> {
	pub ledger_state: Mutex<LedgerState<D>>,
	pub wallets: Mutex<HashMap<WalletSeed, Wallet<D>>>,
	pub resolver: MutexTokio<&'static Resolver>,
}

impl<D: DB + Clone> Default for LedgerContext<D> {
	fn default() -> Self {
		Self {
			ledger_state: Mutex::new(LedgerState::new()),
			wallets: Mutex::new(HashMap::new()),
			resolver: MutexTokio::new(&DEFAULT_RESOLVER),
		}
	}
}

impl<D: DB + Clone> LedgerContext<D> {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn new_from_wallet_seeds(wallet_seeds: &[WalletSeed]) -> Self {
		let ledger_state = Mutex::new(LedgerState::new());
		let wallets = Mutex::new(HashMap::new());

		// Use default `Resolver` for Zswaps
		let resolver = MutexTokio::new(&*DEFAULT_RESOLVER);

		for seed in wallet_seeds {
			let wallet = Wallet::default(*seed);
			wallets
				.lock()
				.expect("Error locking `LedgerContext` wallets")
				.insert(*seed, wallet);
		}

		Self { ledger_state, wallets, resolver }
	}

	pub fn update_from_txs<S: SignatureKind<D>, P: ProofKind<D> + std::fmt::Debug>(
		&self,
		txs: Vec<TransactionWithContext<S, P, D>>,
	) {
		// Group txs that have been processed in the same block
		let groups_by_block_context = Self::group_txs_by_block_context(txs.clone());

		// Update the `LedgerState` per block
		for (block_context, txs) in groups_by_block_context {
			txs.iter().for_each(|tx| {
                // Update Local Wallets
                for (_seed, wallet) in
                    self.wallets.lock().expect("Error locking `LedgerContext` wallets").iter_mut()
                {
                    wallet.update_state_from_tx(tx.clone());
                }

                let tx: Transaction<S, P, PedersenRandomness, D> = tx.clone().into();

                // Update Ledger State
                let tx_context = self.tx_context(block_context.clone());
                let (new_ledger_state, result) = tx_context.ref_state.apply(&tx, &tx_context);

                if let TransactionResult::Failure(failure) = result {
                    println!("Failing result {failure:#?}\nof applying tx {tx:#?} \nto update Local Ledger State for tx_context {tx_context:#?}\n")
                }

                *self.ledger_state.lock().expect("Error locking `LedgerContext` ledger_state") = new_ledger_state;
            });

			// Only when done processing txs for the same block, it's time to call `post_block_update`
			let now = SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.expect("Time went backwards")
				.as_secs();

			let timestamp = Timestamp::from_secs(now);

			let mut latest_ledger_state =
				self.ledger_state.lock().expect("Error locking `LedgerContext` ledger_state");
			*latest_ledger_state = latest_ledger_state.post_block_update(timestamp);
		}
	}

	#[allow(clippy::type_complexity)]
	pub fn group_txs_by_block_context<S: SignatureKind<D>, P: ProofKind<D> + std::fmt::Debug>(
		txs: Vec<TransactionWithContext<S, P, D>>,
	) -> Vec<(BlockContext, Vec<TransactionWithContext<S, P, D>>)> {
		let mut grouped_txs = Vec::new();

		if txs.is_empty() {
			return grouped_txs;
		}

		let mut current_group = Vec::new();
		let mut current_parent_hash = txs[0].block_context.parent_block_hash;
		let mut current_context = txs[0].block_context.clone();

		for tx in txs {
			if tx.block_context.parent_block_hash == current_parent_hash {
				current_group.push(tx);
			} else {
				grouped_txs.push((current_context, current_group));
				current_parent_hash = tx.block_context.parent_block_hash;
				current_context = tx.block_context.clone();
				current_group = vec![tx];
			}
		}

		// Push the last group
		if !current_group.is_empty() {
			grouped_txs.push((current_context, current_group));
		}

		grouped_txs
	}

	pub fn utxos(&self, address: WalletAddress) -> Vec<Utxo> {
		self.ledger_state
			.lock()
			.expect("Error locking `LedgerContext` ledger_state")
			.utxo
			.utxos
			.0
			.iter()
			.filter(|utxo| &utxo.0.owner.0.0.to_vec() == address.data())
			.map(|utxo| (*utxo.0).clone())
			.collect::<Vec<_>>()
	}

	pub async fn update_resolver(&self, resolver: &'static Resolver) {
		let mut resolver_guard = self.resolver.lock().await;

		*resolver_guard = resolver
	}

	pub async fn resolver(&self) -> &Resolver {
		self.resolver.lock().await.clone()
	}

	pub fn wallet_from_seed(&self, seed: WalletSeed) -> Wallet<D> {
		let mut wallet_guard = self.wallets.lock().expect("Error locking `LedgerContext` wallets");
		let wallet = Self::wallet_for_seed(&mut wallet_guard, seed);

		Wallet {
			root_seed: wallet.root_seed,
			shielded: wallet.shielded.clone(),
			unshielded: wallet.unshielded.clone(),
		}
	}

	/// Helper to get or create a wallet for a seed within an existing lock.
	fn wallet_for_seed(
		wallets: &mut HashMap<WalletSeed, Wallet<D>>,
		seed: WalletSeed,
	) -> &mut Wallet<D> {
		wallets.get_mut(&seed).unwrap_or_else(|| {
			panic!("Wallet with seed {seed:?} does not exists in the `LedgerContext")
		})
	}

	/// Operate on a single wallet identified by seed.
	pub fn with_wallet_from_seed<F, R>(&self, seed: WalletSeed, f: F) -> R
	where
		F: FnOnce(&mut Wallet<D>) -> R,
	{
		let mut wallet_guard = self.wallets.lock().expect("Error locking `LedgerContext` wallets");
		let wallet = Self::wallet_for_seed(&mut wallet_guard, seed);
		f(wallet)
	}

	/// Operate on two wallets identified by origin and destination seeds.
	pub fn with_wallets_from_seeds<F, R>(
		&self,
		origin_seed: WalletSeed,
		destination_seed: WalletSeed,
		f: F,
	) -> R
	where
		F: FnOnce(&mut Wallet<D>, &mut Wallet<D>) -> R,
	{
		let mut wallet_guard = self.wallets.lock().expect("Error locking `LedgerContext` wallets");
		let origin_wallet = Self::wallet_for_seed(&mut wallet_guard, origin_seed);

		let mut wallet_guard = self.wallets.lock().expect("Error locking `LedgerContext` wallets");
		let destination_wallet = Self::wallet_for_seed(&mut wallet_guard, destination_seed);

		f(origin_wallet, destination_wallet)
	}

	pub fn with_ledger_state<F, R>(&self, f: F) -> R
	where
		F: FnOnce(&mut LedgerState<D>) -> R,
	{
		let mut ledger_state =
			self.ledger_state.lock().expect("Error locking `LedgerContext` ledger_state");
		f(&mut ledger_state)
	}

	pub fn tx_context(&self, block_context: BlockContext) -> TransactionContext<D> {
		self.with_ledger_state(|ledger_state| TransactionContext {
			ref_state: ledger_state.clone(),
			block_context,
			whitelist: None,
		})
	}
}

thread_local! {
	pub static NETWORK_ID: RefCell<NetworkId> = const { RefCell::new(NetworkId::Undeployed) };
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionWithContext<S: SignatureKind<D>, P: ProofKind<D>, D: DB + Clone> {
	#[serde(with = "tx_serde")]
	pub tx: Transaction<S, P, PedersenRandomness, D>,
	pub block_context: BlockContext,
}

impl<S: SignatureKind<D>, P: ProofKind<D>, D: DB + Clone> TransactionWithContext<S, P, D> {
	pub fn new(
		tx: Transaction<S, P, PedersenRandomness, D>,
		parent_block_hash_seed: Option<u64>,
	) -> Self {
		let now = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("Time went backwards")
			.as_secs();
		let delay: u64 = 0;
		let ttl = now + delay;
		let timestamp = Timestamp::from_secs(ttl);

		// In case `parent_block_hash_seed` wasn't specified, a randmon one is chosen
		let parent_block_hash_seed =
			parent_block_hash_seed.unwrap_or_else(|| rand::thread_rng().r#gen());

		// Calculate a deterministic `parent_block_hash` based on the seed
		let mut rng = SmallRng::seed_from_u64(parent_block_hash_seed);
		let mut array = [0u8; 32];
		rng.fill_bytes(&mut array);
		let parent_block_hash = HashOutput(array);

		let block_context = BlockContext { tblock: timestamp, tblock_err: 30, parent_block_hash };

		Self { tx, block_context }
	}

	pub fn block_context(&self) -> BlockContext {
		self.block_context.clone()
	}
}

impl<S: SignatureKind<D>, P: ProofKind<D>, D: DB + Clone> Deserializable
	for TransactionWithContext<S, P, D>
{
	fn versioned_deserialize<R: std::io::Read>(
		reader: &mut R,
		_version: Option<&Version>,
		recursion_depth: u32,
	) -> Result<Self, std::io::Error> {
		Ok(TransactionWithContext {
			tx: Deserializable::deserialize(reader, recursion_depth)?,
			block_context: Deserializable::deserialize(reader, recursion_depth)?,
		})
	}
}

impl<S: SignatureKind<D>, P: ProofKind<D>, D: DB + Clone> Serializable
	for TransactionWithContext<S, P, D>
{
	fn unversioned_serialize<W: std::io::Write>(
		value: &Self,
		writer: &mut W,
	) -> Result<(), std::io::Error> {
		Serializable::serialize(&value.tx, writer)?;
		Serializable::serialize(&value.block_context, writer)?;
		Ok(())
	}

	fn unversioned_serialized_size(value: &Self) -> usize {
		Serializable::serialized_size(&value.tx)
			+ Serializable::serialized_size(&value.block_context)
	}
}

impl<S: SignatureKind<D>, P: ProofKind<D>, D: DB + Clone> Versioned
	for TransactionWithContext<S, P, D>
{
	const VERSION: Option<Version> =
		<Transaction<S, P, PedersenRandomness, D> as Versioned>::VERSION;
	const NETWORK_SPECIFIC: bool = true;
}

impl<S: SignatureKind<D>, P: ProofKind<D>, D: DB + Clone> From<TransactionWithContext<S, P, D>>
	for Transaction<S, P, PedersenRandomness, D>
{
	fn from(value: TransactionWithContext<S, P, D>) -> Self {
		value.tx
	}
}

pub(crate) mod tx_serde {
	use super::*;
	use serde::{Deserialize, Deserializer, Serialize, Serializer};

	pub(crate) fn serialize<SE, S, P, D>(
		tx: &Transaction<S, P, PedersenRandomness, D>,
		s: SE,
	) -> Result<SE::Ok, SE::Error>
	where
		SE: Serializer,
		S: SignatureKind<D>,
		P: ProofKind<D>,
		D: DB + Clone,
	{
		let network_id = NETWORK_ID.with(|network_id| *network_id.borrow());
		let serialized_bytes =
			super::serialize(tx, network_id).map_err(serde::ser::Error::custom)?;
		let hex_string = hex::encode(&serialized_bytes);

		hex_string.serialize(s)
	}

	pub(crate) fn deserialize<'de, DE, S, P, D>(
		deserializer: DE,
	) -> Result<Transaction<S, P, PedersenRandomness, D>, DE::Error>
	where
		DE: Deserializer<'de>,
		S: SignatureKind<D>,
		P: ProofKind<D>,
		D: DB + Clone,
	{
		let network_id = NETWORK_ID.with(|network_id| *network_id.borrow());
		let hex_string = <String as Deserialize>::deserialize(deserializer)?;
		let bytes = hex::decode(&hex_string).map_err(serde::de::Error::custom)?;

		super::deserialize(bytes.as_slice(), network_id).map_err(serde::de::Error::custom)
	}
}
