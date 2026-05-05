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
	DB, Input, LedgerContext, Nullifier, ProofPreimage, QualifiedInfo, Segment, ShieldedTokenType,
	Sp, StdRng, TokenInfo, WalletSeed, WalletState,
};
use itertools::Itertools;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum ShieldedCoinSelectionError {
	#[error(
		"insufficient shielded coins: need {required} of token {token_type:?} from seed {seed:?}"
	)]
	InsufficientBalance { required: u128, token_type: ShieldedTokenType, seed: WalletSeed },
	#[error("arithmetic overflow in shielded coin selection")]
	ArithmeticOverflow,
}

#[derive(Clone, Copy)]
pub struct InputInfo<O> {
	pub origin: O,
	pub token_type: ShieldedTokenType,
	pub value: u128,
	pub nullifier: Option<Nullifier>,
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
			.filter(|(nullifier, coin)| {
				if let Some(ref exact_nullifier) = self.nullifier {
					exact_nullifier == nullifier
				} else {
					coin.type_ == self.token_type && coin.value >= self.value
				}
			})
			.map(|(_nullifier, coin)| coin)
			.sorted_by_key(|coin| coin.value)
			.collect::<Vec<Sp<QualifiedInfo, D>>>();

		coins
			.first()
			.unwrap_or_else(|| {
				panic!(
					"There is no single UTXO with {:?} and amount >= {:?} to spend by {:?}",
					self.token_type, self.value, wallet
				)
			})
			.clone()
	}

	/// Returns a vector of InputInfo matching coins selected from the wallet to cover
	/// required_value of a token_type, plus the remaining change value.
	pub fn coins_to_cover_value<D: DB + Clone>(
		context: Arc<LedgerContext<D>>,
		seed: WalletSeed,
		required_value: u128,
		token_type: ShieldedTokenType,
	) -> Result<(Vec<InputInfo<WalletSeed>>, u128), ShieldedCoinSelectionError> {
		context.with_wallet_from_seed(seed.clone(), |wallet| {
			let matching_inputs: Vec<InputInfo<WalletSeed>> = wallet
				.shielded
				.state
				.coins
				.iter()
				.filter(|(_nullifier, coin)| coin.type_ == token_type)
				.map(|(nullifier, coin)| InputInfo {
					origin: seed.clone(),
					token_type,
					value: coin.value,
					nullifier: Some(nullifier),
				})
				.collect();
			Self::select_inputs(matching_inputs, required_value).ok_or(
				ShieldedCoinSelectionError::InsufficientBalance {
					required: required_value,
					token_type,
					seed: seed.clone(),
				},
			)
		})
	}

	/// From given `inputs` select coins totaling at least `required`.
	/// Returns selected coins and change.
	fn select_inputs(
		mut inputs: Vec<InputInfo<WalletSeed>>,
		required: u128,
	) -> Option<(Vec<InputInfo<WalletSeed>>, u128)> {
		let mut total = 0u128;
		let mut selected = vec![];
		while !inputs.is_empty() {
			let idx = inputs
				.iter()
				.position(|qi| qi.value.checked_add(total).is_some_and(|sum| sum > required))
				.unwrap_or(inputs.len() - 1);
			let input = inputs.swap_remove(idx);
			total = total.checked_add(input.value)?;
			selected.push(input);
			if let Some(change) = total.checked_sub(required) {
				return Some((selected, change));
			}
		}
		None
	}
}

impl<D: DB + Clone> BuildInput<D> for InputInfo<WalletSeed> {
	fn build(
		&mut self,
		rng: &mut StdRng,
		context: Arc<LedgerContext<D>>,
	) -> Input<ProofPreimage, D> {
		context.with_wallet_from_seed(self.origin.clone(), |wallet| {
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

#[cfg(test)]
mod tests {
	use super::super::HashOutput;
	use super::*;

	fn test_seed() -> WalletSeed {
		WalletSeed::Short([0u8; 16])
	}

	fn test_token_type() -> ShieldedTokenType {
		ShieldedTokenType(HashOutput([0u8; 32]))
	}

	fn make_input(value: u128) -> InputInfo<WalletSeed> {
		InputInfo { origin: test_seed(), token_type: test_token_type(), value, nullifier: None }
	}

	#[test]
	fn select_inputs_exact_match() {
		let inputs = vec![make_input(100)];
		let result = InputInfo::select_inputs(inputs, 100);
		let (selected, change) = result.expect("should select inputs");
		assert_eq!(selected.len(), 1);
		assert_eq!(change, 0);
	}

	#[test]
	fn select_inputs_multiple_sum_to_required() {
		let inputs = vec![make_input(60), make_input(40)];
		let result = InputInfo::select_inputs(inputs, 100);
		let (selected, change) = result.expect("should select inputs");
		assert_eq!(selected.len(), 2);
		assert_eq!(change, 0);
	}

	#[test]
	fn select_inputs_change_produced() {
		let inputs = vec![make_input(150)];
		let result = InputInfo::select_inputs(inputs, 100);
		let (selected, change) = result.expect("should select inputs");
		assert_eq!(selected.len(), 1);
		assert_eq!(change, 50);
	}

	#[test]
	fn select_inputs_accumulation_overflow_returns_none() {
		let half_plus_one = u128::MAX / 2 + 1;
		let inputs = vec![make_input(half_plus_one), make_input(half_plus_one)];
		let result = InputInfo::select_inputs(inputs, u128::MAX);
		assert!(result.is_none(), "accumulation overflow should return None");
	}

	#[test]
	fn select_inputs_comparison_overflow_does_not_panic() {
		// Verifies that the checked_add in the .position() comparison closure
		// handles overflow gracefully instead of panicking. After the first input
		// is selected, total becomes large enough that qi.value + total overflows
		// for the remaining input. The is_some_and pattern treats this as
		// "not greater than required," falling through to the default index.
		let large = u128::MAX / 2 + 1;
		let inputs = vec![make_input(large), make_input(large), make_input(large)];
		let result = InputInfo::select_inputs(inputs, u128::MAX);
		// After selecting two inputs, total overflows on checked_add -> returns None
		assert!(result.is_none());
	}

	#[test]
	fn select_inputs_zero_required() {
		let inputs = vec![make_input(50)];
		let result = InputInfo::select_inputs(inputs, 0);
		let (selected, change) = result.expect("zero required should select first input");
		assert_eq!(selected.len(), 1);
		assert_eq!(change, 50);
	}

	#[test]
	fn select_inputs_insufficient_returns_none() {
		let inputs = vec![make_input(30), make_input(20)];
		let result = InputInfo::select_inputs(inputs, 100);
		assert!(result.is_none(), "insufficient inputs should return None");
	}
}
