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
	DB, LedgerState, Utxo, WalletSeed,
	ledger_storage::Storable,
	mn_ledger::{error::EventReplayError, events::Event},
	onchain_runtime::context::BlockContext,
	zswap::Offer,
};

mod dust;
mod hd;
mod shielded;
mod unshielded;

pub use dust::*;
pub use hd::*;
pub use shielded::*;
pub use unshielded::*;

#[derive(Clone, Debug)]
pub struct Wallet<D: DB + Clone> {
	pub root_seed: Option<WalletSeed>,
	pub shielded: ShieldedWallet<D>,
	pub unshielded: UnshieldedWallet,
	pub dust: DustWallet<D>,
}

impl<D: DB + Clone> Wallet<D> {
	pub fn default(root_seed: WalletSeed, ledger_state: &LedgerState<D>) -> Self {
		let shielded = ShieldedWallet::default(root_seed);
		let unshielded = UnshieldedWallet::default(root_seed);
		let dust = DustWallet::default(root_seed, Some(&ledger_state.parameters));

		Self { root_seed: Some(root_seed), shielded, unshielded, dust }
	}

	pub fn update_state_from_offers<P: Storable<D>>(&mut self, offers: &[Offer<P, D>]) {
		let secret_keys = self.shielded.secret_keys().clone();
		for offer in offers {
			self.shielded.state = self.shielded.state.apply(&secret_keys, offer);
		}

		// // TODO UNSHIELDED
		// if let Transaction::ClaimRewards(ref authorized_mint) = tx {
		// 	self.state = self.state.apply_mint(&self.secret_keys, &authorized_mint.mint);
		// }
	}

	pub fn update_dust_from_tx(&mut self, events: &[Event<D>]) -> Result<(), EventReplayError> {
		self.dust.replay_events(events)
	}

	pub fn update_dust_from_block(&mut self, context: &BlockContext) {
		self.dust.process_ttls(context.tblock);
	}

	pub fn unshielded_utxos(&self, ledger_state: &LedgerState<D>) -> Vec<Utxo> {
		let address = self.unshielded.user_address;
		ledger_state
			.utxo
			.utxos
			.iter()
			.filter(|utxo| utxo.0.owner == address)
			.map(|utxo| (*utxo.0).clone())
			.collect()
	}

	#[cfg(feature = "can-panic")]
	pub fn increment_seed(s: &str) -> String {
		let num = u128::from_str_radix(s, 2).expect("Invalid wallet seed");
		let result = num + 1;
		let width = s.len();
		format!("{result:0width$b}")
	}

	#[cfg(feature = "can-panic")]
	pub fn wallet_seed_decode(input: &str) -> WalletSeed {
		input.parse().expect("failed to decode seed")
	}
}
