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

use super::{
	ArenaKey, BlockContext, DB, DUST_EXPECTED_FILES, DustResolver, Event, FetchMode, LedgerState,
	Loader, MidnightDataProvider, Offer, OutputMode, PUBLIC_PARAMS, ProofKind,
	PureGeneratorPedersen, Resolver, SerdeTransaction, SignatureKind, Storable, SyntheticCost,
	Tagged, Transaction, TransactionContext, TransactionResult, Utxo, VerifiedTransaction, Wallet,
	WalletAddress, WalletSeed, WellFormedStrictness, default_storage,
	mn_ledger_serialize as serialize, mn_ledger_storage as storage, types::StorableSyntheticCost,
};
use derive_where::derive_where;
use hex::encode as hex_encode;
use lazy_static::lazy_static;
use std::{
	collections::{HashMap, HashSet},
	sync::Mutex,
};
use tokio::sync::Mutex as MutexTokio;

lazy_static! {
	pub static ref DEFAULT_RESOLVER: Resolver = Resolver::new(
		PUBLIC_PARAMS.clone(),
		DustResolver(
			MidnightDataProvider::new(
				FetchMode::OnDemand,
				OutputMode::Log,
				DUST_EXPECTED_FILES.to_owned(),
			)
			.expect("resolver could not be created")
		),
		Box::new(|_key_location| Box::pin(std::future::ready(Ok(None)))),
	);
}

pub struct LedgerContext<D: DB + Clone> {
	pub ledger_state: Mutex<LedgerState<D>>,
	pub wallets: Mutex<HashMap<WalletSeed, Wallet<D>>>,
	pub resolver: MutexTokio<&'static Resolver>,
}

#[derive(Debug, Storable)]
#[derive_where(Clone)]
#[storable(db = D)]
struct StorableLedgerState<D: DB> {
	state: LedgerState<D>,
	block_fullness: StorableSyntheticCost<D>,
}

impl<D: DB> StorableLedgerState<D> {
	fn new(state: LedgerState<D>) -> Self {
		Self { state, block_fullness: StorableSyntheticCost::zero() }
	}
}

impl<D: DB> Tagged for StorableLedgerState<D> {
	fn tag() -> std::borrow::Cow<'static, str> {
		<LedgerState<D> as Tagged>::tag()
	}

	fn tag_unique_factor() -> String {
		<LedgerState<D> as Tagged>::tag_unique_factor()
	}
}

impl<D: DB + Clone> LedgerContext<D> {
	pub fn new(network_id: impl Into<String>) -> Self {
		Self {
			ledger_state: Mutex::new(LedgerState::new(network_id)),
			wallets: Mutex::new(HashMap::new()),
			resolver: MutexTokio::new(&DEFAULT_RESOLVER),
		}
	}

	pub fn new_from_wallet_seeds(
		network_id: impl Into<String>,
		wallet_seeds: &[WalletSeed],
	) -> Self {
		let ledger_state = LedgerState::new(network_id);
		let wallets = Mutex::new(HashMap::new());

		// Use default `Resolver` for Zswaps
		let resolver = MutexTokio::new(&*DEFAULT_RESOLVER);

		for seed in wallet_seeds {
			let wallet = Wallet::default(*seed, &ledger_state);
			wallets
				.lock()
				.expect("Error locking `LedgerContext` wallets")
				.insert(*seed, wallet);
		}

		Self { ledger_state: Mutex::new(ledger_state), wallets, resolver }
	}

	pub fn update_from_block<S: SignatureKind<D>, P: ProofKind<D> + std::fmt::Debug>(
		&self,
		txs: Vec<SerdeTransaction<S, P, D>>,
		block_context: BlockContext,
		state_root: Option<Vec<u8>>,
	) where
		Transaction<S, P, PureGeneratorPedersen, D>: Tagged,
	{
		let mut total_cost = SyntheticCost::ZERO;
		for tx in txs {
			let (events, cost) = self.update_from_tx(&tx, &block_context);
			for wallet in
				self.wallets.lock().expect("Error locking `LedgerContext` wallets").values_mut()
			{
				if let Err(error) = wallet.update_dust_from_tx(&events) {
					// TODO: should we have better error handling here?
					println!("Failed to replay events for Dust monitoring: {error}");
				}
			}
			total_cost = total_cost + cost;
		}

		// Only when done processing txs for the same block, it's time to call `post_block_update`
		let mut latest_ledger_state =
			self.ledger_state.lock().expect("Error locking `LedgerContext` ledger_state");
		*latest_ledger_state = latest_ledger_state
			.post_block_update(block_context.tblock, total_cost)
			.expect("Error applying block updates");
		if let Some(expected_root) = state_root {
			match Self::compute_state_root(&*latest_ledger_state) {
				Some(actual_root) if actual_root != expected_root => {
					panic!(
						"Ledger state root mismatch: expected {}, actual {}",
						hex_encode(&expected_root),
						hex_encode(&actual_root),
					);
				},
				Some(_) => {},
				None => println!("Failed to compute local ledger state root for comparison"),
			}
		}
		// Update Local Wallets
		for wallet in
			self.wallets.lock().expect("Error locking `LedgerContext` wallets").values_mut()
		{
			wallet.update_dust_from_block(&block_context);
		}
	}

	fn compute_state_root(state: &LedgerState<D>) -> Option<Vec<u8>> {
		let storage = default_storage::<D>();
		let ledger = StorableLedgerState::new(state.clone());
		let sp = storage.arena.alloc(ledger);
		super::serialize(&sp.hash()).ok()
	}

	pub fn update_from_tx<S: SignatureKind<D>, P: ProofKind<D> + std::fmt::Debug>(
		&self,
		tx: &SerdeTransaction<S, P, D>,
		block_context: &BlockContext,
	) -> (Vec<Event<D>>, SyntheticCost)
	where
		Transaction<S, P, PureGeneratorPedersen, D>: Tagged,
	{
		let tx_context = self.tx_context(block_context.clone());

		let strictness: WellFormedStrictness =
			if block_context.parent_block_hash == Default::default() {
				let mut lax: WellFormedStrictness = Default::default();
				lax.enforce_balancing = false;
				lax
			} else {
				Default::default()
			};

		// Update Ledger State
		let (new_ledger_state, offers, events, cost) = match &tx {
			SerdeTransaction::Midnight(tx) => {
				let valid_tx: VerifiedTransaction<_> = tx
					.well_formed(&tx_context.ref_state, strictness, tx_context.block_context.tblock)
					.expect("applying invalid transaction");
				let cost = tx
					.cost(&tx_context.ref_state.parameters, false)
					.expect("error calculating fees");

				let (new_ledger_state, result) = tx_context.ref_state.apply(&valid_tx, &tx_context);
				let offers = Self::successful_shielded_offers(tx, &result);
				match result {
					TransactionResult::Success(events) => (new_ledger_state, offers, events, cost),
					TransactionResult::PartialSuccess(failure, events) => {
						let hash = hex::encode(tx.transaction_hash().0.0);
						println!(
							"Partially failing result {failure:?} of applying tx 0x{hash} to update Local Ledger State"
						);
						(new_ledger_state, offers, events, cost)
					},
					TransactionResult::Failure(failure) => {
						let hash = hex::encode(tx.transaction_hash().0.0);
						println!(
							"Failing result {failure:?} of applying tx 0x{hash} \nto update Local Ledger State"
						);
						(new_ledger_state, offers, vec![], SyntheticCost::ZERO)
					},
				}
			},
			SerdeTransaction::System(tx) => {
				let cost = tx.cost(&tx_context.ref_state.parameters);
				match tx_context.ref_state.apply_system_tx(tx, block_context.tblock) {
					Ok((new_state, events)) => (new_state, vec![], events, cost),
					Err(err) => {
						let hash = hex::encode(tx.transaction_hash().0.0);
						println!(
							"Failing result {err:?} of applying system tx {hash} to update Local Ledger State"
						);
						(tx_context.ref_state.clone(), vec![], vec![], cost)
					},
				}
			},
		};

		// Update Local Wallets
		for wallet in
			self.wallets.lock().expect("Error locking `LedgerContext` wallets").values_mut()
		{
			wallet.update_state_from_offers(&offers);
		}

		*self.ledger_state.lock().expect("Error locking `LedgerContext` ledger_state") =
			new_ledger_state;
		(events, cost)
	}

	fn successful_shielded_offers<S: SignatureKind<D>, P: ProofKind<D>>(
		tx: &Transaction<S, P, PureGeneratorPedersen, D>,
		result: &TransactionResult<D>,
	) -> Vec<Offer<P::LatestProof, D>> {
		let failed_segments = match result {
			TransactionResult::Success(_) => HashSet::new(),
			TransactionResult::Failure(_) => return vec![],
			TransactionResult::PartialSuccess(results, _) => {
				let mut failures = HashSet::new();
				for (segment, result) in results {
					if result.is_err() {
						failures.insert(segment);
					}
				}
				failures
			},
		};
		let Transaction::Standard(stx) = tx else {
			return vec![];
		};
		let mut offers = vec![];
		if let Some(guaranteed) = &stx.guaranteed_coins {
			offers.push((**guaranteed).clone());
		}
		for entry in stx.fallible_coins.iter() {
			let segment = *entry.0;
			let fallible = &entry.1;
			if !failed_segments.contains(&segment) {
				offers.push((**fallible).clone());
			}
		}
		offers
	}

	pub fn utxos(&self, address: WalletAddress) -> Vec<Utxo> {
		self.ledger_state
			.lock()
			.expect("Error locking `LedgerContext` ledger_state")
			.utxo
			.utxos
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
			dust: wallet.dust.clone(),
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
