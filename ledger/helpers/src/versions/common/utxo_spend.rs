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
	BuilderContext, DB, IntentHash, SigningKey, UnshieldedTokenType, Utxo, UtxoId, UtxoSpend,
	WalletSeed, signature_verifying_key,
};
use crate::CoinSelectionStrategy;
use async_trait::async_trait;
use itertools::Itertools;
use std::sync::Arc;

#[derive(Debug)]
pub struct PinnedUtxoNotFound {
	pub intent_hash: IntentHash,
	pub output_no: u32,
	pub token_type: UnshieldedTokenType,
	pub seed: WalletSeed,
}

#[derive(Debug, thiserror::Error)]
pub enum UtxoSelectionError {
	#[error("insufficient UTXOs: need {required} of token {token_type:?} from seed {seed:?}")]
	InsufficientBalance { required: u128, token_type: UnshieldedTokenType, seed: WalletSeed },
	#[error("no UTXO of token {token_type:?} with value >= {min_value} for seed {seed:?}")]
	NoMatchingUtxo { min_value: u128, token_type: UnshieldedTokenType, seed: WalletSeed },
	#[error("pinned UTXO not found: {0:?}")]
	PinnedUtxoNotFound(Box<PinnedUtxoNotFound>),
	#[error("arithmetic overflow in UTXO selection")]
	ArithmeticOverflow,
}

pub struct UtxoSpendInfo<O> {
	pub value: u128,
	pub owner: O,
	pub token_type: UnshieldedTokenType,
	pub intent_hash: Option<IntentHash>,
	pub output_number: Option<u32>,
}

#[async_trait]
pub trait BuildUtxoSpend<D: DB + Clone, C: BuilderContext<D>>: Send + Sync {
	async fn build(&self, context: Arc<C>) -> UtxoSpend;
	fn signing_key(&self, context: Arc<C>) -> SigningKey;
}

impl UtxoSpendInfo<WalletSeed> {
	async fn min_match_utxo<D: DB + Clone, C: BuilderContext<D>>(
		&self,
		context: Arc<C>,
	) -> Result<Utxo, UtxoSelectionError> {
		let utxos = context.unshielded_utxos(self.owner.clone()).await;

		utxos
			.into_iter()
			.map(|(utxo, _ctime)| utxo)
			.filter(|utxo| {
				utxo.type_ == self.token_type
					&& utxo.value >= self.value
					&& self.intent_hash.is_none_or(|h| utxo.intent_hash == h)
					&& self.output_number.is_none_or(|o| utxo.output_no == o)
			})
			.sorted_by_key(|utxo| utxo.value)
			.next()
			.ok_or(UtxoSelectionError::NoMatchingUtxo {
				min_value: self.value,
				token_type: self.token_type,
				seed: self.owner.clone(),
			})
	}

	/// Returns a vector of UtxoSpendInfo matching Utxos selected from the wallet to cover required_value
	/// of a token_type from the wallet specified by seed and remaining value of change.
	pub async fn utxos_to_cover_value<D: DB + Clone, C: BuilderContext<D>>(
		context: Arc<C>,
		seed: WalletSeed,
		required_value: u128,
		token_type: UnshieldedTokenType,
		strategy: CoinSelectionStrategy,
	) -> Result<(Vec<UtxoSpendInfo<WalletSeed>>, u128), UtxoSelectionError> {
		let utxos = context.unshielded_utxos(seed.clone()).await;
		let matching_inputs = utxos
			.into_iter()
			.map(|(utxo, _ctime)| utxo)
			.filter(|utxo| utxo.type_ == token_type)
			.map(|utxo| UtxoSpendInfo {
				value: utxo.value,
				owner: seed.clone(),
				token_type: utxo.type_,
				intent_hash: Some(utxo.intent_hash),
				output_number: Some(utxo.output_no),
			})
			.collect();
		Self::select_inputs(matching_inputs, required_value, strategy).ok_or(
			UtxoSelectionError::InsufficientBalance { required: required_value, token_type, seed },
		)
	}

	/// Look up a specific set of UTXOs (by intent_hash + output_no) belonging to `seed`
	/// and `token_type`, and return them as `UtxoSpendInfo` together with the change
	/// (sum - required). Errors if any requested UTXO is missing from the wallet or
	/// if the pinned sum is below `required_value`.
	pub async fn utxos_by_ids<D: DB + Clone, C: BuilderContext<D>>(
		context: Arc<C>,
		seed: WalletSeed,
		required_value: u128,
		token_type: UnshieldedTokenType,
		utxo_ids: &[UtxoId],
	) -> Result<(Vec<UtxoSpendInfo<WalletSeed>>, u128), UtxoSelectionError> {
		let utxos = context.unshielded_utxos(seed.clone()).await;
		let mut selected: Vec<UtxoSpendInfo<WalletSeed>> = Vec::with_capacity(utxo_ids.len());
		let mut total: u128 = 0;
		for &UtxoId { intent_hash, output_number } in utxo_ids {
			let utxo = utxos
				.iter()
				.map(|(utxo, _ctime)| utxo)
				.find(|utxo| {
					utxo.intent_hash == intent_hash
						&& utxo.output_no == output_number
						&& utxo.type_ == token_type
				})
				.ok_or_else(|| {
					UtxoSelectionError::PinnedUtxoNotFound(Box::new(PinnedUtxoNotFound {
						intent_hash,
						output_no: output_number,
						token_type,
						seed: seed.clone(),
					}))
				})?;
			total = total.saturating_add(utxo.value);
			selected.push(UtxoSpendInfo {
				value: utxo.value,
				owner: seed.clone(),
				token_type: utxo.type_,
				intent_hash: Some(utxo.intent_hash),
				output_number: Some(utxo.output_no),
			});
		}
		let change =
			total
				.checked_sub(required_value)
				.ok_or(UtxoSelectionError::InsufficientBalance {
					required: required_value,
					token_type,
					seed: seed.clone(),
				})?;
		Ok((selected, change))
	}

	/// From given `inputs` select coins totaling at least `required`, ordered by `strategy`.
	/// Returns selected coins and change.
	fn select_inputs<O>(
		mut inputs: Vec<UtxoSpendInfo<O>>,
		required: u128,
		strategy: CoinSelectionStrategy,
	) -> Option<(Vec<UtxoSpendInfo<O>>, u128)> {
		inputs.sort_by_key(|input| input.value);
		if matches!(strategy, CoinSelectionStrategy::LargestFirst) {
			inputs.reverse();
		}

		let mut total = 0u128;
		let mut selected = Vec::with_capacity(inputs.len());
		for input in inputs {
			total = total.checked_add(input.value)?;
			selected.push(input);
			if let Some(change) = total.checked_sub(required) {
				return Some((selected, change));
			}
		}
		None
	}
}

#[async_trait]
impl<D: DB + Clone, C: BuilderContext<D>> BuildUtxoSpend<D, C> for UtxoSpendInfo<WalletSeed> {
	async fn build(&self, context: Arc<C>) -> UtxoSpend {
		let owner = context.with_wallet_from_seed(self.owner.clone(), |wallet| {
			signature_verifying_key(wallet.unshielded.signing_key().verifying_key())
		});
		// If self identifies an UTXO then use it, otherwise find the best matching UTXO.
		match (self.intent_hash, self.output_number) {
			(Some(intent_hash), Some(output_no)) => UtxoSpend {
				value: self.value,
				owner,
				type_: self.token_type,
				intent_hash,
				output_no,
			},
			_ => {
				let utxo = self.min_match_utxo(context.clone()).await.expect("UTXO lookup failed");
				UtxoSpend {
					value: utxo.value,
					owner,
					type_: utxo.type_,
					intent_hash: utxo.intent_hash,
					output_no: utxo.output_no,
				}
			},
		}
	}

	fn signing_key(&self, context: Arc<C>) -> SigningKey {
		context.with_wallet_from_seed(self.owner.clone(), |wallet| {
			wallet.unshielded.signing_key().clone()
		})
	}
}

// TODO: impl<D: DB + Clone> BuildUtxoSpend<D> for UtxoSpendInfo<VerifyingKey>

#[cfg(test)]
mod tests {
	use super::super::HashOutput;
	use super::*;

	fn test_seed() -> WalletSeed {
		WalletSeed::Short([0u8; 16])
	}

	fn test_token_type() -> UnshieldedTokenType {
		UnshieldedTokenType(HashOutput([0u8; 32]))
	}

	fn make_utxo(value: u128) -> UtxoSpendInfo<WalletSeed> {
		UtxoSpendInfo {
			value,
			owner: test_seed(),
			token_type: test_token_type(),
			intent_hash: None,
			output_number: None,
		}
	}

	#[test]
	fn select_inputs_exact_match() {
		let inputs = vec![make_utxo(100)];
		let result = UtxoSpendInfo::select_inputs(inputs, 100, CoinSelectionStrategy::LargestFirst);
		let (selected, change) = result.expect("should select inputs");
		assert_eq!(selected.len(), 1);
		assert_eq!(change, 0);
	}

	#[test]
	fn select_inputs_change_produced() {
		let inputs = vec![make_utxo(150)];
		let result = UtxoSpendInfo::select_inputs(inputs, 100, CoinSelectionStrategy::LargestFirst);
		let (selected, change) = result.expect("should select inputs");
		assert_eq!(selected.len(), 1);
		assert_eq!(change, 50);
	}

	#[test]
	fn select_inputs_accumulation_overflow_returns_none() {
		let half_plus_one = u128::MAX / 2 + 1;
		let inputs = vec![make_utxo(half_plus_one), make_utxo(half_plus_one)];
		let result =
			UtxoSpendInfo::select_inputs(inputs, u128::MAX, CoinSelectionStrategy::LargestFirst);
		assert!(result.is_none(), "accumulation overflow should return None");
	}

	#[test]
	fn select_inputs_overflow_with_remaining_inputs_returns_none() {
		// After two inputs the accumulator overflows; the remaining input must not
		// cause a panic, and the call must return None.
		let large = u128::MAX / 2 + 1;
		let inputs = vec![make_utxo(large), make_utxo(large), make_utxo(large)];
		let result =
			UtxoSpendInfo::select_inputs(inputs, u128::MAX, CoinSelectionStrategy::LargestFirst);
		assert!(result.is_none());
	}

	#[test]
	fn select_inputs_multiple_sum_to_required() {
		let inputs = vec![make_utxo(60), make_utxo(40)];
		let result = UtxoSpendInfo::select_inputs(inputs, 100, CoinSelectionStrategy::LargestFirst);
		let (selected, change) = result.expect("should select inputs");
		assert_eq!(selected.len(), 2);
		assert_eq!(change, 0);
	}

	#[test]
	fn select_inputs_zero_required() {
		let inputs = vec![make_utxo(50)];
		let result = UtxoSpendInfo::select_inputs(inputs, 0, CoinSelectionStrategy::LargestFirst);
		let (selected, change) = result.expect("zero required should select first input");
		assert_eq!(selected.len(), 1);
		assert_eq!(change, 50);
	}

	#[test]
	fn select_inputs_insufficient_returns_none() {
		let inputs = vec![make_utxo(30), make_utxo(20)];
		let result = UtxoSpendInfo::select_inputs(inputs, 100, CoinSelectionStrategy::LargestFirst);
		assert!(result.is_none(), "insufficient inputs should return None");
	}

	#[test]
	fn select_inputs_largest_first_minimizes_count() {
		let inputs = vec![make_utxo(10), make_utxo(20), make_utxo(100)];
		let result = UtxoSpendInfo::select_inputs(inputs, 25, CoinSelectionStrategy::LargestFirst);
		let (selected, change) = result.expect("should select inputs");
		assert_eq!(selected.len(), 1);
		assert_eq!(selected[0].value, 100);
		assert_eq!(change, 75);
	}

	#[test]
	fn select_inputs_smallest_first_consolidates_dust() {
		let inputs = vec![make_utxo(10), make_utxo(20), make_utxo(100)];
		let result = UtxoSpendInfo::select_inputs(inputs, 25, CoinSelectionStrategy::SmallestFirst);
		let (selected, change) = result.expect("should select inputs");
		assert_eq!(selected.len(), 2);
		assert_eq!(selected[0].value, 10);
		assert_eq!(selected[1].value, 20);
		assert_eq!(change, 5);
	}
}
