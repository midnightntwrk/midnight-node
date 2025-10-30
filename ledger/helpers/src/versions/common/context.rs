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

#![cfg(feature = "can-panic")]

use super::{
	ArenaKey, BlockContext, CostDuration, DB, DUST_EXPECTED_FILES, Deserializable, DustResolver,
	Event, FetchMode, HashOutput, LedgerState, Loader, MidnightDataProvider, Offer, OutputMode,
	PUBLIC_PARAMS, ProofKind, PureGeneratorPedersen, Resolver, Serializable, SignatureKind,
	StandardTransaction, Storable, SyntheticCost, SystemTransaction, Tagged, Timestamp,
	Transaction, TransactionContext, TransactionHash, TransactionResult, Utxo, VerifiedTransaction,
	Wallet, WalletAddress, WalletSeed, WellFormedStrictness, default_storage, deserialize,
	mn_ledger_serialize as serialize, mn_ledger_storage as storage,
};
use derive_where::derive_where;
use hex::encode as hex_encode;
use lazy_static::lazy_static;
use rand::{Rng, RngCore, SeedableRng, rngs::SmallRng};
use std::{
	collections::{HashMap, HashSet},
	marker::PhantomData,
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

#[derive(Debug, Storable)]
#[derive_where(Clone)]
#[storable(db = D)]
pub struct StorableSyntheticCost<D: DB> {
	read_time: u64,
	compute_time: u64,
	block_usage: u64,
	bytes_written: u64,
	bytes_churned: u64,
	_marker: PhantomData<D>,
}

impl<D: DB> StorableSyntheticCost<D> {
	fn zero() -> Self {
		Self {
			read_time: 0,
			compute_time: 0,
			block_usage: 0,
			bytes_written: 0,
			bytes_churned: 0,
			_marker: PhantomData,
		}
	}
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

impl<D: DB> From<SyntheticCost> for StorableSyntheticCost<D> {
	fn from(value: SyntheticCost) -> Self {
		Self {
			read_time: value.read_time.into_picoseconds(),
			compute_time: value.compute_time.into_picoseconds(),
			block_usage: value.block_usage,
			bytes_written: value.bytes_written,
			bytes_churned: value.bytes_churned,
			_marker: PhantomData,
		}
	}
}
impl<D: DB> From<StorableSyntheticCost<D>> for SyntheticCost {
	fn from(value: StorableSyntheticCost<D>) -> Self {
		Self {
			read_time: CostDuration::from_picoseconds(value.read_time),
			compute_time: CostDuration::from_picoseconds(value.compute_time),
			block_usage: value.block_usage,
			bytes_written: value.bytes_written,
			bytes_churned: value.bytes_churned,
		}
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
						println!(
							"Partially failing result {failure:#?}\nof applying tx {tx:#?} \nto update Local Ledger State for tx_context {tx_context:#?}\n"
						);
						(new_ledger_state, offers, events, cost)
					},
					TransactionResult::Failure(failure) => {
						println!(
							"Failing result {failure:#?}\nof applying tx {tx:#?} \nto update Local Ledger State for tx_context {tx_context:#?}\n"
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
						println!(
							"Failing result {err:#?}\nof applying system tx {tx:#?}\nto update Local Ledger State for tx_context {tx_context:#?}\n"
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

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)] // Transaction has the same thing internally
pub enum SerdeTransaction<S: SignatureKind<D>, P: ProofKind<D>, D: DB>
where
	Transaction<S, P, PureGeneratorPedersen, D>: Tagged,
{
	Midnight(Transaction<S, P, PureGeneratorPedersen, D>),
	System(SystemTransaction),
}

impl<S: SignatureKind<D>, P: ProofKind<D>, D: DB> SerdeTransaction<S, P, D>
where
	Transaction<S, P, PureGeneratorPedersen, D>: Tagged,
{
	pub fn as_midnight(&self) -> Option<&Transaction<S, P, PureGeneratorPedersen, D>> {
		match &self {
			Self::Midnight(tx) => Some(tx),
			_ => None,
		}
	}

	pub fn network_id(&self) -> Option<&str> {
		match &self {
			Self::Midnight(Transaction::Standard(StandardTransaction { network_id, .. })) => {
				Some(network_id)
			},
			_ => None,
		}
	}

	pub fn serialize_inner(&self) -> Result<Vec<u8>, std::io::Error> {
		match &self {
			Self::Midnight(tx) => super::serialize(tx),
			Self::System(tx) => super::serialize(tx),
		}
	}

	pub fn transaction_hash(&self) -> TransactionHash {
		match self {
			SerdeTransaction::Midnight(transaction) => transaction.transaction_hash(),
			SerdeTransaction::System(system_transaction) => system_transaction.transaction_hash(),
		}
	}
}

impl<S: SignatureKind<D>, P: ProofKind<D>, D: DB> Serializable for SerdeTransaction<S, P, D>
where
	Transaction<S, P, PureGeneratorPedersen, D>: Tagged,
{
	fn serialize(&self, writer: &mut impl std::io::Write) -> std::io::Result<()> {
		match self {
			Self::Midnight(tx) => {
				<u8 as Serializable>::serialize(&0, writer)?;
				Transaction::serialize(tx, writer)?;
			},
			Self::System(tx) => {
				<u8 as Serializable>::serialize(&1, writer)?;
				SystemTransaction::serialize(tx, writer)?;
			},
		}
		Ok(())
	}

	fn serialized_size(&self) -> usize {
		match self {
			Self::Midnight(tx) => 1 + Transaction::serialized_size(tx),
			Self::System(tx) => 1 + SystemTransaction::serialized_size(tx),
		}
	}
}

impl<S: SignatureKind<D>, P: ProofKind<D>, D: DB> Deserializable for SerdeTransaction<S, P, D>
where
	Transaction<S, P, PureGeneratorPedersen, D>: Tagged,
{
	fn deserialize(reader: &mut impl std::io::Read, recursion_depth: u32) -> std::io::Result<Self> {
		let discriminant = <u8 as Deserializable>::deserialize(reader, recursion_depth)?;
		match discriminant {
			0 => Ok(Self::Midnight(Transaction::deserialize(reader, recursion_depth)?)),
			1 => Ok(Self::System(SystemTransaction::deserialize(reader, recursion_depth)?)),
			_ => Err(::std::io::Error::new(
				::std::io::ErrorKind::InvalidData,
				"unrecognised discriminant for SerdeTransaction",
			)),
		}
	}
}

impl<S: SignatureKind<D>, P: ProofKind<D>, D: DB> serde::Serialize for SerdeTransaction<S, P, D>
where
	Transaction<S, P, PureGeneratorPedersen, D>: Tagged,
{
	fn serialize<SE: serde::Serializer>(&self, serializer: SE) -> Result<SE::Ok, SE::Error> {
		let serialized_bytes = match self {
			Self::Midnight(tx) => super::serialize(tx),
			Self::System(tx) => super::serialize(tx),
		}
		.map_err(serde::ser::Error::custom)?;

		serde::Serialize::serialize(&serialized_bytes, serializer)
	}
}

impl<'a, S: SignatureKind<D>, P: ProofKind<D>, D: DB> serde::Deserialize<'a>
	for SerdeTransaction<S, P, D>
where
	Transaction<S, P, PureGeneratorPedersen, D>: Tagged,
{
	fn deserialize<DE: serde::Deserializer<'a>>(deserializer: DE) -> Result<Self, DE::Error> {
		let bytes = <Vec<u8> as serde::Deserialize>::deserialize(deserializer)?;
		if !bytes.starts_with(serialize::GLOBAL_TAG.as_bytes()) {
			return Err(serde::de::Error::custom("missing global tag"));
		}

		macro_rules! try_deserialize_as {
			($ty:ident, $ctor:ident) => {
				if bytes[serialize::GLOBAL_TAG.as_bytes().len()..]
					.starts_with($ty::tag().as_bytes())
				{
					return Ok(Self::$ctor(
						deserialize(bytes.as_slice()).map_err(serde::de::Error::custom)?,
					));
				}
			};
		}

		try_deserialize_as!(Transaction, Midnight);
		try_deserialize_as!(SystemTransaction, System);

		Err(serde::de::Error::custom("unrecognized tag"))
	}
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TransactionWithContext<S: SignatureKind<D>, P: ProofKind<D>, D: DB>
where
	Transaction<S, P, PureGeneratorPedersen, D>: Tagged,
{
	#[serde(bound = "")]
	pub tx: SerdeTransaction<S, P, D>,
	pub block_context: BlockContext,
}

impl<S: SignatureKind<D>, P: ProofKind<D>, D: DB> TransactionWithContext<S, P, D>
where
	Transaction<S, P, PureGeneratorPedersen, D>: Tagged,
{
	pub fn new(
		tx: Transaction<S, P, PureGeneratorPedersen, D>,
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

		Self { tx: SerdeTransaction::Midnight(tx), block_context }
	}

	pub fn block_context(&self) -> BlockContext {
		self.block_context.clone()
	}
}

impl<S: SignatureKind<D>, P: ProofKind<D>, D: DB> Deserializable for TransactionWithContext<S, P, D>
where
	Transaction<S, P, PureGeneratorPedersen, D>: Tagged,
{
	fn deserialize(
		reader: &mut impl std::io::Read,
		recursion_depth: u32,
	) -> Result<Self, std::io::Error> {
		Ok(TransactionWithContext {
			tx: Deserializable::deserialize(reader, recursion_depth)?,
			block_context: Deserializable::deserialize(reader, recursion_depth)?,
		})
	}
}

impl<S: SignatureKind<D>, P: ProofKind<D>, D: DB> Serializable for TransactionWithContext<S, P, D>
where
	Transaction<S, P, PureGeneratorPedersen, D>: Tagged,
{
	fn serialize(&self, writer: &mut impl std::io::Write) -> Result<(), std::io::Error> {
		Serializable::serialize(&self.tx, writer)?;
		Serializable::serialize(&self.block_context, writer)?;
		Ok(())
	}

	fn serialized_size(&self) -> usize {
		Serializable::serialized_size(&self.tx) + Serializable::serialized_size(&self.block_context)
	}
}

impl<S: SignatureKind<D>, P: ProofKind<D>, D: DB> Tagged for TransactionWithContext<S, P, D>
where
	Transaction<S, P, PureGeneratorPedersen, D>: Tagged,
{
	fn tag() -> std::borrow::Cow<'static, str> {
		std::borrow::Cow::Borrowed("transaction-with-context[v1]")
	}

	fn tag_unique_factor() -> String {
		format!(
			"({},{})",
			Transaction::<S, P, PureGeneratorPedersen, D>::tag(),
			BlockContext::tag()
		)
	}
}
