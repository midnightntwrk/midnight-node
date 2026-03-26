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

use super::{
	ArenaKey, BlockContext, DB, DUST_EXPECTED_FILES, DustResolver, Event, FetchMode, LedgerState,
	Loader, MidnightDataProvider, Offer, OutputMode, PUBLIC_PARAMS, ProofKind,
	PureGeneratorPedersen, Resolver, SerdeTransaction, SignatureKind, Sp, Storable, SyntheticCost,
	Tagged, Timestamp, Transaction, TransactionContext, TransactionResult, Utxo,
	VerifiedTransaction, Wallet, WalletAddress, WalletSeed, WellFormedStrictness,
	clamp_and_normalize, compute_overall_fullness, default_storage, deserialize,
	mn_ledger_serialize as serialize, mn_ledger_storage as storage, types::StorableSyntheticCost,
};
use derive_where::derive_where;
use hex::encode as hex_encode;
use lazy_static::lazy_static;
use std::{
	collections::{HashMap, HashSet},
	sync::Mutex,
	time::{SystemTime, UNIX_EPOCH},
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
	pub ledger_state: Mutex<Sp<LedgerState<D>, D>>,
	pub latest_block_context: Mutex<Option<BlockContext>>,
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
			ledger_state: Mutex::new(Sp::new(LedgerState::new(network_id))),
			wallets: Mutex::new(HashMap::new()),
			resolver: MutexTokio::new(&DEFAULT_RESOLVER),
			latest_block_context: Mutex::new(None),
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

		Self {
			ledger_state: Mutex::new(Sp::new(ledger_state)),
			wallets,
			resolver,
			latest_block_context: Mutex::new(None),
		}
	}

	/// Apply all transactions in a block to the ledger, returning events without
	/// processing wallets. Also applies `post_block_update` (fee adjustments).
	fn apply_txs_collect_events<S: SignatureKind<D>, P: ProofKind<D> + std::fmt::Debug>(
		&self,
		txs: &[SerdeTransaction<S, P, D>],
		block_context: &BlockContext,
	) -> Vec<Event<D>>
	where
		Transaction<S, P, PureGeneratorPedersen, D>: Tagged,
	{
		let mut total_cost = SyntheticCost::ZERO;
		let mut all_events: Vec<Event<D>> = Vec::new();
		for tx in txs {
			let (events, cost) = self.update_from_tx(tx, block_context);
			all_events.extend(events);
			total_cost = total_cost + cost;
		}

		let mut latest_ledger_state =
			self.ledger_state.lock().expect("Error locking `LedgerContext` ledger_state");
		let block_limits = latest_ledger_state.parameters.limits.block_limits;
		let normalized_fullness =
			clamp_and_normalize(&total_cost, &block_limits, "update_from_block");
		let overall_fullness = compute_overall_fullness(&normalized_fullness);
		*latest_ledger_state = Sp::new(
			latest_ledger_state
				.post_block_update(block_context.tblock, normalized_fullness, overall_fullness)
				.expect("Error applying block updates"),
		);

		all_events
	}

	/// Replay accumulated dust events to all wallets in parallel (no TTL processing).
	pub fn update_dust_from_events(&self, events: &[Event<D>])
	where
		D: Sync,
	{
		use rayon::prelude::*;
		log::debug!(
			"[perf] flushing {} events for {} wallets",
			events.len(),
			self.wallets.lock().expect("lock").len(),
		);
		self.wallets
			.lock()
			.expect("Error locking `LedgerContext` wallets")
			.par_iter_mut()
			.for_each(|(_, wallet)| {
				wallet
					.update_dust_from_tx(events)
					.unwrap_or_else(|e| panic!("failed to replay dust events: {e}"));
			});
	}

	pub fn update_dust_from_block(&self, block_context: &BlockContext)
	where
		D: Sync,
	{
		use rayon::prelude::*;
		self.wallets
			.lock()
			.expect("Error locking `LedgerContext` wallets")
			.par_iter_mut()
			.for_each(|(_, wallet)| {
				wallet.update_dust_from_block(block_context);
			});
	}

	pub fn update_ledger_state_from_bytes(&self, state: &[u8]) {
		let mut latest_ledger_state =
			self.ledger_state.lock().expect("Error locking `LedgerContext` ledger_state");
		let new_state: LedgerState<D> =
			deserialize(state).expect("failed to deserialize state bytes");
		*latest_ledger_state = Sp::new(new_state);
	}

	/// Updates ledger state with transactions from a block and produces events. Caller must
	/// eventually call `update_dust_from_events` with accumulated events and `update_dust_from_block`
	/// with last processed block if he needs `self.wallets` to be up to date.
	///
	/// Safety: only use during cold-start replay where no concurrent `spend()`/`mark_spent()`
	/// calls are active — `pending_until` and `spent_utxos` clearing depend on per-block
	/// `process_ttls` which is deferred. This is naturally satisfied by the toolkit, which
	/// always replays all blocks to reconstruct state before building any transactions.
	pub fn update_from_block<S: SignatureKind<D>, P: ProofKind<D> + std::fmt::Debug>(
		&self,
		txs: &[SerdeTransaction<S, P, D>],
		block_context: &BlockContext,
		state_root: Option<&Vec<u8>>,
		state: Option<&Vec<u8>>,
	) -> Vec<Event<D>>
	where
		Transaction<S, P, PureGeneratorPedersen, D>: Tagged,
	{
		let events = self.apply_txs_collect_events(txs, block_context);

		// Genesis block: overwrite ledger state with the canonical genesis state,
		// since constructor params aren't directly observable from genesis txs.
		if let Some(state) = state {
			self.update_ledger_state_from_bytes(state);
		}

		self.verify_state_root(block_context, state_root);

		*self.latest_block_context.lock().expect("error locking latest_block_context") =
			Some(block_context.clone());

		events
	}

	pub fn latest_block_context(&self) -> BlockContext {
		self.latest_block_context
			.lock()
			.expect("failed to lock latest_block_context")
			.as_ref()
			.cloned()
			.unwrap_or_else(|| {
				let now = Timestamp::from_secs(
					SystemTime::now()
						.duration_since(UNIX_EPOCH)
						.expect("time has run backwards")
						.as_secs(),
				);
				super::make_block_context(now, Default::default(), Default::default())
			})
	}

	fn verify_state_root(&self, block_context: &BlockContext, state_root: Option<&Vec<u8>>) {
		let latest_ledger_state =
			self.ledger_state.lock().expect("Error locking `LedgerContext` ledger_state");
		if let Some(expected_root) = state_root {
			match Self::compute_state_root(&*latest_ledger_state) {
				Some(actual_root) if actual_root != *expected_root => {
					panic!(
						"Ledger state root mismatch: expected {}, actual {}. Parent block hash: {}",
						hex_encode(expected_root),
						hex_encode(&actual_root),
						hex_encode(block_context.parent_block_hash.0),
					);
				},
				Some(_) => {},
				None => println!("Failed to compute local ledger state root for comparison"),
			}
		}
	}

	fn compute_state_root(state: &LedgerState<D>) -> Option<Vec<u8>> {
		let storage = default_storage::<D>();
		let ledger = StorableLedgerState::new(state.clone());
		let sp = storage.arena.alloc(ledger);
		super::serialize(&sp.as_typed_key()).ok()
	}

	pub fn update_from_tx<S: SignatureKind<D>, P: ProofKind<D> + std::fmt::Debug>(
		&self,
		tx: &SerdeTransaction<S, P, D>,
		block_context: &BlockContext,
	) -> (Vec<Event<D>>, SyntheticCost)
	where
		Transaction<S, P, PureGeneratorPedersen, D>: Tagged,
	{
		let mut ledger_state_guard =
			self.ledger_state.lock().expect("Error locking `LedgerContext` ledger_state");
		let tx_context = TransactionContext {
			ref_state: (**ledger_state_guard).clone(),
			block_context: block_context.clone(),
			whitelist: None,
		};

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

		{
			use rayon::prelude::*;
			self.wallets
				.lock()
				.expect("Error locking `LedgerContext` wallets")
				.par_iter_mut()
				.for_each(|(_, wallet)| {
					wallet.update_state_from_offers(&offers);
				});
		}

		*ledger_state_guard = Sp::new(new_ledger_state);
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
		F: FnOnce(&mut Sp<LedgerState<D>, D>) -> R,
	{
		let mut ledger_state =
			self.ledger_state.lock().expect("Error locking `LedgerContext` ledger_state");
		f(&mut ledger_state)
	}

	pub fn tx_context(&self, block_context: BlockContext) -> TransactionContext<D> {
		self.with_ledger_state(|ledger_state| TransactionContext {
			ref_state: (**ledger_state).clone(),
			block_context,
			whitelist: None,
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::{
		Arc,
		atomic::{AtomicU64, Ordering},
	};

	type TestDB = storage::DefaultDB;

	/// Validates that `with_ledger_state` serializes concurrent read-modify-write
	/// operations on the `ledger_state` mutex — the same mutex that `update_from_tx`
	/// now holds for its full RMW cycle.
	///
	/// The test uses a non-atomic load/yield/store pattern on an external counter
	/// inside the lock scope. Without mutex serialization, `yield_now()` would widen
	/// the race window and cause lost updates (threads reading stale values before
	/// other threads' writes are visible).
	///
	/// Covers: PR767-TC-02 (no lost updates), PR767-TC-03 (no deadlock — test completes).
	#[test]
	fn concurrent_rmw_via_with_ledger_state_prevents_lost_updates() {
		let ctx: Arc<LedgerContext<TestDB>> = Arc::new(LedgerContext::new("test-net"));
		let counter = Arc::new(AtomicU64::new(0));
		let n_threads = 8u64;
		let iterations = 100u64;

		let handles: Vec<_> = (0..n_threads)
			.map(|_| {
				let ctx: Arc<LedgerContext<TestDB>> = Arc::clone(&ctx);
				let counter = Arc::clone(&counter);
				std::thread::spawn(move || {
					for _ in 0..iterations {
						ctx.with_ledger_state(|_state| {
							let current = counter.load(Ordering::Relaxed);
							std::thread::yield_now();
							counter.store(current + 1, Ordering::Relaxed);
						});
					}
				})
			})
			.collect();

		for h in handles {
			h.join().expect("thread panicked");
		}

		assert_eq!(
			counter.load(Ordering::SeqCst),
			n_threads * iterations,
			"Lost updates detected: ledger_state mutex did not serialize concurrent RMW"
		);
	}
}
