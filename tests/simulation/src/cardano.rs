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

//! Simulated Cardano: a chain of blocks carrying cNIGHT events, and a follower that turns
//! it into observation inherents with injectable faults (delayed visibility, reordering,
//! rollbacks that a naive follower would leak as regressing positions).

use midnight_ledger_unsafe_helpers::{
	DustPublicKey, DustSecretKey, SeedableRng, StdRng, serialize_untagged,
};
use midnight_primitives_cnight_observation::{
	CardanoPosition, CardanoRewardAddressBytes, DustPublicKeyBytes, TimestampUnixMillis,
};
use midnight_primitives_mainchain_follower::{
	CreateData, DeregistrationData, ObservedUtxo, ObservedUtxoData, ObservedUtxoHeader,
	RegistrationData, SpendData, UtxoIndexInTx,
};
use proptest::prelude::*;
use sidechain_domain::{McBlockHash, McTxHash};
use sp_io::hashing::blake2_256;

pub const CARDANO_SLOT_MS: i64 = 1_000;
pub const CARDANO_GENESIS_MS: i64 = 1_700_000_000_000;

/// A cNIGHT-holding wallet: Cardano reward address + Midnight DUST key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wallet {
	pub address: CardanoRewardAddressBytes,
	pub dust_key: DustPublicKeyBytes,
}

impl Wallet {
	pub fn deterministic(index: u8) -> Self {
		let mut addr = [0u8; 29];
		addr[0] = 0xe0;
		addr[1] = index;
		let mut rng = StdRng::from_seed([index; 32]);
		let pk = DustPublicKey::from(DustSecretKey::sample(&mut rng));
		let pk_bytes: Vec<u8> = serialize_untagged(&pk).expect("dust pk");
		Self {
			address: CardanoRewardAddressBytes(addr),
			dust_key: DustPublicKeyBytes(pk_bytes.try_into().expect("33-byte dust pk")),
		}
	}
}

/// One cNIGHT-relevant thing happening in a Cardano transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardanoEvent {
	/// Wallet registers its DUST key (a new mapping UTXO).
	Register { wallet: u8 },
	/// Wallet spends the mapping UTXO created by the `Register` in transaction `reg_tx`.
	Deregister { wallet: u8, reg_tx: TxRef },
	/// A cNIGHT UTXO worth `value` is created for `owner`.
	Create { asset: u8, owner: u8, value: u128 },
	/// The cNIGHT UTXO created in `create_tx` is spent.
	Spend { asset: u8, owner: u8, value: u128, create_tx: TxRef },
}

/// Position of a transaction in the simulated chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TxRef {
	pub block: u32,
	pub tx: u32,
}

#[derive(Clone, Debug)]
pub struct CardanoBlock {
	pub number: u32,
	/// Fork the block was minted on; block and tx hashes derive from it, so a rolled-back
	/// history keeps the common prefix's hashes and changes only the replaced blocks'.
	pub fork_id: u8,
	/// Transactions; each carries exactly one event so tx indices are dense.
	pub txs: Vec<CardanoEvent>,
}

/// A linear Cardano history, forkable: a rollback of the last `depth` blocks replaced by
/// blocks with different hashes and contents.
#[derive(Clone, Debug)]
pub struct CardanoSim {
	pub blocks: Vec<CardanoBlock>,
}

impl CardanoSim {
	fn fork_of(&self, number: u32) -> u8 {
		self.blocks.iter().find(|b| b.number == number).map_or(0, |b| b.fork_id)
	}

	pub fn block_hash(&self, number: u32) -> McBlockHash {
		McBlockHash(blake2_256(
			&[&[0xb1, self.fork_of(number)][..], &number.to_be_bytes()].concat(),
		))
	}

	pub fn tx_hash(&self, r: TxRef) -> McTxHash {
		McTxHash(blake2_256(
			&[&[0x7c, self.fork_of(r.block)][..], &r.block.to_be_bytes(), &r.tx.to_be_bytes()]
				.concat(),
		))
	}

	pub fn timestamp(number: u32) -> TimestampUnixMillis {
		TimestampUnixMillis(CARDANO_GENESIS_MS + number as i64 * CARDANO_SLOT_MS)
	}

	pub fn position(&self, block: u32, tx: u32) -> CardanoPosition {
		CardanoPosition {
			block_hash: self.block_hash(block),
			block_number: block,
			block_timestamp: Self::timestamp(block),
			tx_index_in_block: tx,
		}
	}

	/// Position just past the last transaction of `block`: what a follower reports as the
	/// next position after consuming that block.
	pub fn end_position(&self, block: u32) -> CardanoPosition {
		let n_txs = self.blocks.iter().find(|b| b.number == block).map_or(0, |b| b.txs.len());
		self.position(block, n_txs as u32)
	}

	/// The observation UTXOs of `block`, in transaction order.
	pub fn observed(&self, block: &CardanoBlock, wallets: &[Wallet]) -> Vec<ObservedUtxo> {
		block
			.txs
			.iter()
			.enumerate()
			.map(|(i, ev)| {
				let wallet = |index: &u8| &wallets[*index as usize];
				let here = TxRef { block: block.number, tx: i as u32 };
				let tx_hash = self.tx_hash(here);
				let header = |utxo_tx: McTxHash| ObservedUtxoHeader {
					tx_position: self.position(block.number, i as u32),
					tx_hash,
					utxo_tx_hash: utxo_tx,
					utxo_index: UtxoIndexInTx(0),
				};
				match ev {
					CardanoEvent::Register { wallet: wallet_ix } => ObservedUtxo {
						header: header(tx_hash),
						data: ObservedUtxoData::Registration(RegistrationData {
							cardano_reward_address: wallet(wallet_ix).address,
							dust_public_key: wallet(wallet_ix).dust_key.clone(),
						}),
					},
					CardanoEvent::Deregister { wallet: wallet_ix, reg_tx } => ObservedUtxo {
						header: header(self.tx_hash(*reg_tx)),
						data: ObservedUtxoData::Deregistration(DeregistrationData {
							cardano_reward_address: wallet(wallet_ix).address,
							dust_public_key: wallet(wallet_ix).dust_key.clone(),
						}),
					},
					CardanoEvent::Create { owner, value, .. } => ObservedUtxo {
						header: header(tx_hash),
						data: ObservedUtxoData::AssetCreate(CreateData {
							value: *value,
							owner: wallet(owner).address,
							utxo_tx_hash: tx_hash,
							utxo_tx_index: 0,
						}),
					},
					CardanoEvent::Spend { owner, value, create_tx, .. } => ObservedUtxo {
						header: header(self.tx_hash(*create_tx)),
						data: ObservedUtxoData::AssetSpend(SpendData {
							value: *value,
							owner: wallet(owner).address,
							utxo_tx_hash: self.tx_hash(*create_tx),
							utxo_tx_index: 0,
							spending_tx_hash: tx_hash,
						}),
					},
				}
			})
			.collect()
	}

	/// A rolled-back copy: drops the last `depth` blocks and appends `replacement` new ones
	/// with different hashes (the fork id changes), keeping block numbers dense.
	pub fn fork(&self, depth: usize, replacement: Vec<Vec<CardanoEvent>>) -> CardanoSim {
		let keep = self.blocks.len().saturating_sub(depth);
		let fork_id = self.blocks.iter().map(|b| b.fork_id).max().map_or(1, |f| f + 1);
		let mut blocks: Vec<CardanoBlock> = self.blocks[..keep].to_vec();
		let first_new = blocks.last().map_or(1, |b| b.number + 1);
		for (i, txs) in replacement.into_iter().enumerate() {
			blocks.push(CardanoBlock { number: first_new + i as u32, fork_id, txs });
		}
		CardanoSim { blocks }
	}
}

/// One observation inherent as a follower would hand it to the block producer.
#[derive(Clone, Debug)]
pub struct Observation {
	pub utxos: Vec<ObservedUtxo>,
	pub next: CardanoPosition,
}

/// Turns a Cardano history into per-Midnight-block observations.
///
/// `release` gives, per Midnight block, how many further Cardano blocks become visible
/// (0 models a delayed/silent Cardano feed: the follower re-reports the same position).
pub fn follow(sim: &CardanoSim, wallets: &[Wallet], release: &[u8]) -> Vec<Observation> {
	let mut cursor = 0usize;
	let mut out = Vec::new();
	let mut next = sim.position(0, 0);
	for &n in release {
		let mut utxos = Vec::new();
		for _ in 0..n {
			let Some(block) = sim.blocks.get(cursor) else { break };
			utxos.extend(sim.observed(block, wallets));
			next = sim.end_position(block.number);
			cursor += 1;
		}
		out.push(Observation { utxos, next: next.clone() });
	}
	out
}

pub const MAX_WALLETS: u8 = 3;

/// Strategy for a well-formed Cardano history: deregistrations reference earlier
/// registrations and spends reference earlier, still-unspent creates, so every event has
/// a Cardano-side cause. Whether the *Midnight* side treats each as effective is exactly
/// what the model decides.
pub fn arb_history(max_blocks: usize, max_txs: usize) -> impl Strategy<Value = CardanoSim> {
	let raw =
		prop::collection::vec(prop::collection::vec(arb_raw_event(), 0..=max_txs), 1..=max_blocks);
	raw.prop_map(bind_history)
}

/// Raw, unbound event choices; `bind_history` resolves references.
#[derive(Clone, Debug)]
pub enum RawEvent {
	Register(u8),
	/// Deregister the k-th live registration of the wallet (modulo count).
	Deregister(u8, u8),
	Create(u8, u128),
	/// Spend the k-th live cNIGHT UTXO of the asset space (modulo count).
	Spend(u8),
}

fn arb_raw_event() -> impl Strategy<Value = RawEvent> {
	prop_oneof![
		3 => (0..MAX_WALLETS).prop_map(RawEvent::Register),
		2 => (0..MAX_WALLETS, any::<u8>()).prop_map(|(w, k)| RawEvent::Deregister(w, k)),
		4 => (0..MAX_WALLETS, 1u128..1_000_000).prop_map(|(w, v)| RawEvent::Create(w, v)),
		3 => any::<u8>().prop_map(RawEvent::Spend),
	]
}

/// Resolves raw choices into a consistent history starting at block 1.
pub fn bind_history(raw: Vec<Vec<RawEvent>>) -> CardanoSim {
	let mut live_regs: Vec<Vec<TxRef>> = vec![Vec::new(); MAX_WALLETS as usize];
	let mut live_assets: Vec<(u8, u8, u128, TxRef)> = Vec::new();
	let mut next_asset = 0u8;
	let mut blocks = Vec::new();
	for (b, raw_txs) in raw.into_iter().enumerate() {
		let number = b as u32 + 1;
		let mut txs = Vec::new();
		for raw_ev in raw_txs {
			let here = TxRef { block: number, tx: txs.len() as u32 };
			let ev = match raw_ev {
				RawEvent::Register(w) => {
					live_regs[w as usize].push(here);
					CardanoEvent::Register { wallet: w }
				},
				RawEvent::Deregister(w, k) => {
					let regs = &mut live_regs[w as usize];
					if regs.is_empty() {
						continue;
					}
					let reg_tx = regs.remove(k as usize % regs.len());
					CardanoEvent::Deregister { wallet: w, reg_tx }
				},
				RawEvent::Create(owner, value) => {
					let asset = next_asset;
					next_asset = next_asset.wrapping_add(1);
					live_assets.push((asset, owner, value, here));
					CardanoEvent::Create { asset, owner, value }
				},
				RawEvent::Spend(k) => {
					if live_assets.is_empty() {
						continue;
					}
					let (asset, owner, value, create_tx) =
						live_assets.remove(k as usize % live_assets.len());
					CardanoEvent::Spend { asset, owner, value, create_tx }
				},
			};
			txs.push(ev);
		}
		blocks.push(CardanoBlock { number, fork_id: 0, txs });
	}
	CardanoSim { blocks }
}

/// Release schedule: per Midnight block, 0..=3 newly visible Cardano blocks.
pub fn arb_release(midnight_blocks: usize) -> impl Strategy<Value = Vec<u8>> {
	prop::collection::vec(0u8..=3, midnight_blocks)
}
