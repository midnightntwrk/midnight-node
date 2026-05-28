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
	BuilderContext, DB, IntentHash, SigningKey, UnshieldedTokenType, Utxo, UtxoSpend, WalletSeed,
};
use async_trait::async_trait;
use itertools::Itertools;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum UtxoSelectionError {
	#[error("insufficient UTXOs: need {required} of token {token_type:?} from seed {seed:?}")]
	InsufficientBalance { required: u128, token_type: UnshieldedTokenType, seed: WalletSeed },
	#[error("no UTXO of token {token_type:?} with value >= {min_value} for seed {seed:?}")]
	NoMatchingUtxo { min_value: u128, token_type: UnshieldedTokenType, seed: WalletSeed },
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
		let utxos = context.unshielded_utxos(self.owner).await;

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
				seed: self.owner,
			})
	}

	/// Returns a vector of UtxoSpendInfo matching Utxos selected from the wallet to cover required_value
	/// of a token_type from the wallet specified by seed and remaining value of change.
	pub async fn utxos_to_cover_value<D: DB + Clone, C: BuilderContext<D>>(
		context: Arc<C>,
		seed: WalletSeed,
		required_value: u128,
		token_type: UnshieldedTokenType,
	) -> Result<(Vec<UtxoSpendInfo<WalletSeed>>, u128), UtxoSelectionError> {
		let utxos = context.unshielded_utxos(seed).await;
		let matching_inputs = utxos
			.into_iter()
			.map(|(utxo, _ctime)| utxo)
			.filter(|utxo| utxo.type_ == token_type)
			.map(|utxo| UtxoSpendInfo {
				value: utxo.value,
				owner: seed,
				token_type: utxo.type_,
				intent_hash: Some(utxo.intent_hash),
				output_number: Some(utxo.output_no),
			})
			.collect();
		Self::select_inputs(matching_inputs, required_value).ok_or(
			UtxoSelectionError::InsufficientBalance { required: required_value, token_type, seed },
		)
	}

	/// From given `inputs` it select coins of at least `required`.
	/// Returns selected coins and change.
	fn select_inputs<O>(
		mut inputs: Vec<UtxoSpendInfo<O>>,
		required: u128,
	) -> Option<(Vec<UtxoSpendInfo<O>>, u128)> {
		let mut total = 0u128;
		let mut selected = vec![];
		while !inputs.is_empty() {
			let idx = inputs
				.iter()
				.position(|qi| qi.value + total > required)
				.unwrap_or(inputs.len() - 1);
			let utxo = inputs.swap_remove(idx);
			total += utxo.value;
			selected.push(utxo);
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
		let owner = context
			.with_wallet_from_seed(self.owner, |wallet| wallet.unshielded.signing_key().verifying_key());
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
		context.with_wallet_from_seed(self.owner, |wallet| wallet.unshielded.signing_key().clone())
	}
}

// TODO: impl<D: DB + Clone> BuildUtxoSpend<D> for UtxoSpendInfo<VerifyingKey>
