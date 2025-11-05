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

use super::super::{
	ArenaKey, BlockContext, ContractAddress, CostDuration, DB, Deserializable, HashOutput, Loader,
	ProofKind, PureGeneratorPedersen, Serializable, SignatureKind, StandardTransaction, Storable,
	SyntheticCost, SystemTransaction, Tagged, Timestamp, Transaction, TransactionHash, Transcript,
	deserialize, mn_ledger_serialize as serialize, mn_ledger_storage as storage,
};
use bip39::Mnemonic;
use derive_where::derive_where;
use rand::{Rng, RngCore, SeedableRng, rngs::SmallRng};
#[cfg(feature = "can-panic")]
use std::str::FromStr;
use std::{
	collections::HashMap,
	marker::PhantomData,
	time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum WalletSeed {
	Short([u8; 16]),
	Medium([u8; 32]),
	Long([u8; 64]),
}

impl Default for WalletSeed {
	fn default() -> Self {
		Self::Medium([0; 32])
	}
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum WalletSeedError {
	#[error("{0}")]
	InvalidHex(#[from] hex::FromHexError),
	#[error("expected 16, 32, or 64 bytes; got {0}")]
	InvalidLength(usize),
	#[error("{0}")]
	InvalidMnemonic(#[from] bip39::Error),
}

impl WalletSeed {
	#[cfg(feature = "can-panic")]
	pub fn try_from_hex_str(value: &str) -> Result<Self, WalletSeedError> {
		let bytes = hex::decode(value)?;
		match bytes.len() {
			16 => Ok(Self::Short(bytes.try_into().unwrap())),
			32 => Ok(Self::Medium(bytes.try_into().unwrap())),
			64 => Ok(Self::Long(bytes.try_into().unwrap())),
			len => Err(WalletSeedError::InvalidLength(len)),
		}
	}

	pub fn try_from_mnemonic(value: &str) -> Result<Self, WalletSeedError> {
		let mnemonic = Mnemonic::parse(value)?;
		Ok(Self::Long(mnemonic.to_seed("")))
	}

	pub fn as_bytes(&self) -> &[u8] {
		match self {
			Self::Short(bytes) => bytes,
			Self::Medium(bytes) => bytes,
			Self::Long(bytes) => bytes,
		}
	}
}

impl From<[u8; 32]> for WalletSeed {
	fn from(value: [u8; 32]) -> Self {
		Self::Medium(value)
	}
}

#[cfg(feature = "can-panic")]
impl FromStr for WalletSeed {
	type Err = WalletSeedError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let s = s.trim();
		if let Ok(seed) = Self::try_from_hex_str(s) { Ok(seed) } else { Self::try_from_mnemonic(s) }
	}
}

pub type MaintenanceCounter = u32;

#[derive(Default, Clone)]
pub struct MaintenanceUpdateBuilder {
	pub num_contract_replace_auth: u32,
	pub num_contract_key_remove: u32,
	pub num_contract_key_insert: u32,
	pub addresses_map: HashMap<ContractAddress, MaintenanceCounter>,
	pub addresses_vec: Vec<ContractAddress>,
}

impl MaintenanceUpdateBuilder {
	pub fn new(
		num_contract_replace_auth: u32,
		num_contract_key_remove: u32,
		num_contract_key_insert: u32,
	) -> Self {
		MaintenanceUpdateBuilder {
			num_contract_replace_auth,
			num_contract_key_remove,
			num_contract_key_insert,
			..Default::default()
		}
	}

	pub fn add_address(&mut self, addr: &ContractAddress, counter: MaintenanceCounter) {
		self.addresses_map.insert(*addr, counter);
		self.addresses_vec.push(*addr);
	}

	pub fn add_addresses(&mut self, addrs: &[ContractAddress], counters: Vec<MaintenanceCounter>) {
		(0..addrs.len()).for_each(|i| self.add_address(&addrs[i], counters[i]));
	}

	pub fn increase_counter(&mut self, addr: ContractAddress) {
		if let Some(counter) = self.addresses_map.get_mut(&addr) {
			*counter = counter.saturating_add(1);
		}
	}
}

#[derive(Debug, Clone)]
pub enum WalletUpdate {
	Yes,
	No,
}

#[derive(Debug, Clone)]
pub enum ContractType {
	MerkleTree,
	// MicroDao,
}

#[derive(Debug, Clone)]
pub struct ZswapContractAddresses {
	pub outputs: Option<Vec<ContractAddress>>,
	pub transients: Option<Vec<ContractAddress>>,
}

pub enum WalletKind {
	Legacy,
	NoLegacy,
}

pub type Transcripts<D> = (Option<Transcript<D>>, Option<Transcript<D>>);

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Segment {
	Guaranteed = 0,
	Fallible = 1,
}

impl From<Segment> for u16 {
	fn from(val: Segment) -> Self {
		match val {
			Segment::Guaranteed => 0,
			Segment::Fallible => 1,
		}
	}
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
	pub fn zero() -> Self {
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

#[cfg(test)]
mod tests {
	use crate::WalletSeed;

	#[test]
	fn should_decode_wallet_seeds_in_different_formats() {
		let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon diesel";
		let mnemonic_seed: WalletSeed = mnemonic.parse().unwrap();
		let hex = "a51c86de32d0791f7cffc3bdff1abd9bb54987f0ed5effc30c936dddbb9afd9d530c8db445e4f2d3ea42a321b260e022aadf05987c9a67ec7b6b6ca1d0593ec9";
		let hex_seed: WalletSeed = hex.parse().unwrap();
		assert_eq!(mnemonic_seed, hex_seed);
	}
}
