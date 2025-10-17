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
	CoinInfo, ContractAddress, DB, LedgerContext, Output, ProofPreimage, Segment,
	ShieldedTokenType, ShieldedWallet, StdRng, TokenInfo, WalletSeed,
};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct OutputInfo<D> {
	pub destination: D,
	pub token_type: ShieldedTokenType,
	pub value: u128,
}

impl<O> TokenInfo for OutputInfo<O> {
	fn token_type(&self) -> ShieldedTokenType {
		self.token_type
	}
	fn value(&self) -> u128 {
		self.value
	}
}

impl<D> OutputInfo<D> {
	fn coin_info(&self, rng: &mut StdRng) -> CoinInfo {
		CoinInfo::new(rng, self.value, self.token_type)
	}
}

pub trait BuildOutput<D: DB + Clone>: TokenInfo + Send + Sync {
	fn build(&self, rng: &mut StdRng, context: Arc<LedgerContext<D>>) -> Output<ProofPreimage, D>;
}

impl<D: DB + Clone> BuildOutput<D> for OutputInfo<ContractAddress> {
	fn build(&self, rng: &mut StdRng, _context: Arc<LedgerContext<D>>) -> Output<ProofPreimage, D> {
		let coin_info = self.coin_info(rng);
		Output::new_contract_owned(rng, &coin_info, Segment::Guaranteed.into(), self.destination)
			.expect("Invalid output attributes")
	}
}

impl<D: DB + Clone> BuildOutput<D> for OutputInfo<WalletSeed> {
	fn build(&self, rng: &mut StdRng, context: Arc<LedgerContext<D>>) -> Output<ProofPreimage, D> {
		context.with_wallet_from_seed(self.destination, |wallet| {
			let coin_info = self.coin_info(rng);

			wallet.shielded.state = wallet
				.shielded
				.state
				.watch_for(&wallet.shielded.secret_keys().coin_public_key(), &coin_info);

			Output::new(
				rng,
				&coin_info,
				Segment::Guaranteed.into(),
				&wallet.shielded.secret_keys().coin_public_key(),
				Some(wallet.shielded.secret_keys().enc_public_key()),
			)
			.expect("Invalid output attributes")
		})
	}
}

impl<D: DB + Clone> BuildOutput<D> for OutputInfo<ShieldedWallet<D>> {
	fn build(&self, rng: &mut StdRng, _context: Arc<LedgerContext<D>>) -> Output<ProofPreimage, D> {
		let coin_info = self.coin_info(rng);

		Output::new(
			rng,
			&coin_info,
			Segment::Guaranteed.into(),
			&self.destination.coin_public_key,
			Some(self.destination.enc_public_key),
		)
		.expect("Invalid output attributes")
	}
}
