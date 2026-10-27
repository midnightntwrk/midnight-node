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

//! Independent model of the cNIGHT observation rules, written from the specification of
//! the Cardano→Midnight DUST bridge rather than from the pallet:
//!
//! 1. A Cardano reward address is *registered* iff exactly one mapping UTXO for it is
//!    live; its DUST key is that UTXO's key. Zero or several live mappings mean unregistered.
//! 2. A cNIGHT UTXO created for a registered address starts DUST generation for that
//!    address's DUST key, keyed by a nonce derived from the UTXO id.
//! 3. Spending such a UTXO stops the generation it started; spending a UTXO whose
//!    creation never started generation does nothing.
//! 4. The observed Cardano position never regresses.
//! 5. The same observation stream always yields the same state (byte-exact inherents).

use midnight_primitives_cnight_observation::{
	CardanoPosition, CardanoRewardAddressBytes, DustPublicKeyBytes,
};
use midnight_primitives_mainchain_follower::{ObservedUtxo, ObservedUtxoData};
use sp_io::hashing::blake2_256;
use std::collections::BTreeMap;

/// (reward address, mapping UTXO tx hash, mapping UTXO index).
pub type MappingKey = (CardanoRewardAddressBytes, [u8; 32], u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
	Create,
	Destroy,
}

/// What the ledger must be told, per the model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedDustEvent {
	pub value: u128,
	pub owner: DustPublicKeyBytes,
	pub time_secs: u64,
	pub action: Action,
	pub nonce: [u8; 32],
}

#[derive(Clone, Debug, Default)]
pub struct ObservationModel {
	pub mapping: BTreeMap<MappingKey, DustPublicKeyBytes>,
	/// Generation started per UTXO nonce, with the DUST key it generates for.
	pub generating: BTreeMap<[u8; 32], DustPublicKeyBytes>,
	pub next_position: Option<CardanoPosition>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
	Applied(Vec<ExpectedDustEvent>),
	/// Rule 4: rejected without any state change.
	Regression,
}

impl ObservationModel {
	pub fn registered_key(&self, addr: &CardanoRewardAddressBytes) -> Option<DustPublicKeyBytes> {
		let mut keys = self.mapping.iter().filter(|((a, _, _), _)| a == addr).map(|(_, k)| k);
		match (keys.next(), keys.next()) {
			(Some(k), None) => Some(k.clone()),
			_ => None,
		}
	}

	pub fn observe(&mut self, utxos: &[ObservedUtxo], next: &CardanoPosition) -> Verdict {
		if let Some(prev) = &self.next_position
			&& next < prev
		{
			return Verdict::Regression;
		}
		let mut events = Vec::new();
		for utxo in utxos {
			let time_secs = utxo.header.tx_position.block_timestamp.0 as u64 / 1000;
			let (utxo_tx, utxo_ix) = (utxo.header.utxo_tx_hash.0, utxo.header.utxo_index.0);
			match &utxo.data {
				ObservedUtxoData::Registration(r) => {
					self.mapping.insert(
						(r.cardano_reward_address, utxo_tx, utxo_ix),
						r.dust_public_key.clone(),
					);
				},
				ObservedUtxoData::Deregistration(d) => {
					self.mapping.remove(&(d.cardano_reward_address, utxo_tx, utxo_ix));
				},
				ObservedUtxoData::AssetCreate(c) => {
					if let Some(owner) = self.registered_key(&c.owner) {
						let nonce = asset_nonce(&c.utxo_tx_hash.0, c.utxo_tx_index);
						self.generating.insert(nonce, owner.clone());
						events.push(ExpectedDustEvent {
							value: c.value,
							owner,
							time_secs,
							action: Action::Create,
							nonce,
						});
					}
				},
				ObservedUtxoData::AssetSpend(s) => {
					let nonce = asset_nonce(&s.utxo_tx_hash.0, s.utxo_tx_index);
					if let Some(owner) = self.generating.remove(&nonce) {
						events.push(ExpectedDustEvent {
							value: s.value,
							owner,
							time_secs,
							action: Action::Destroy,
							nonce,
						});
					}
				},
			}
		}
		self.next_position = Some(next.clone());
		Verdict::Applied(events)
	}
}

/// Rule 2's nonce: hash of the tag and the created UTXO's id.
pub fn asset_nonce(utxo_tx_hash: &[u8; 32], utxo_tx_index: u16) -> [u8; 32] {
	blake2_256(&[b"asset_create" as &[u8], utxo_tx_hash, &utxo_tx_index.to_be_bytes()].concat())
}
