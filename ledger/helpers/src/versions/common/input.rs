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
	DB, Input, LedgerContext, ProofPreimage, QualifiedInfo, Segment, ShieldedTokenType, Sp, StdRng,
	TokenInfo, WalletSeed, WalletState,
};
use itertools::Itertools;
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct InputInfo<O> {
	pub origin: O,
	pub token_type: ShieldedTokenType,
	pub value: u128,
}

impl<O> TokenInfo for InputInfo<O> {
	fn token_type(&self) -> ShieldedTokenType {
		self.token_type
	}
	fn value(&self) -> u128 {
		self.value
	}
}

pub trait BuildInput<D: DB + Clone>: TokenInfo + Send + Sync {
	fn build(
		&mut self,
		rng: &mut StdRng,
		context: Arc<LedgerContext<D>>,
	) -> Input<ProofPreimage, D>;
}

impl InputInfo<WalletSeed> {
	pub fn min_match_coin<D: DB + Clone>(&self, wallet: &WalletState<D>) -> Sp<QualifiedInfo, D> {
		let coins = wallet
			.coins
			.iter()
			.map(|(_nullifier, coin)| coin)
			.filter(|coin| coin.type_ == self.token_type && coin.value >= self.value)
			.sorted_by_key(|coin| coin.value)
			.collect::<Vec<Sp<QualifiedInfo, D>>>();

		coins
			.first()
			.unwrap_or_else(|| {
				panic!(
					"There are no fundings of Token {:?} and amount >= {:?} to spend by Wallet {:?}",
					self.token_type, self.value, wallet
				)
			})
			.clone()
	}
}

impl<D: DB + Clone> BuildInput<D> for InputInfo<WalletSeed> {
	fn build(
		&mut self,
		rng: &mut StdRng,
		context: Arc<LedgerContext<D>>,
	) -> Input<ProofPreimage, D> {
		context.with_wallet_from_seed(self.origin, |wallet| {
			let coin: Sp<QualifiedInfo, D> = self.min_match_coin(&wallet.shielded.state);

			// Update the `InputInfo` value with the actual coin value that is going to be spent
			self.value = coin.value;

			let (updated_walet, input) = wallet
				.shielded
				.state
				.spend(rng, wallet.shielded.secret_keys(), &coin, Segment::Guaranteed.into())
				.expect("Failed to spend coin");

			// Update wallet
			wallet.shielded.state = updated_walet;

			input
		})
	}
}

// TODO: impl BuildOutput for OutputInfo<ContractAddress>
