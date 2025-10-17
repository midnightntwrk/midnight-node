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
	DB, IntentHash, LedgerContext, SigningKey, Sp, UnshieldedTokenType, Utxo, UtxoSpend, Wallet,
	WalletSeed,
};
use itertools::Itertools;
use std::sync::Arc;

pub struct UtxoSpendInfo<O> {
	pub value: u128,
	pub owner: O,
	pub token_type: UnshieldedTokenType,
	pub intent_hash: Option<IntentHash>,
	pub output_number: Option<u32>,
}

pub trait BuildUtxoSpend<D: DB + Clone>: Send + Sync {
	fn build(&self, context: Arc<LedgerContext<D>>) -> UtxoSpend;
	fn signing_key(&self, context: Arc<LedgerContext<D>>) -> SigningKey;
}

impl UtxoSpendInfo<WalletSeed> {
	pub fn min_match_utxo<D: DB + Clone>(
		&self,
		context: Arc<LedgerContext<D>>,
		wallet: &Wallet<D>,
	) -> Sp<Utxo, D> {
		context.with_ledger_state(|ledger_state| {
			let owner = wallet.unshielded.signing_key().verifying_key();

			ledger_state
				.utxo
				.utxos
				.iter()
				.filter(|utxo| {
					utxo.0.type_ == self.token_type
						&& utxo.0.value >= self.value
						&& utxo.0.owner == owner.clone().into()
						&& self.intent_hash.is_none_or(|h| utxo.0.intent_hash == h)
						&& self.output_number.is_none_or(|o| utxo.0.output_no == o)
				})
				.sorted_by_key(|utxo| utxo.0.value)
				.next()
				.unwrap_or_else(|| {
					panic!(
						"There are no fundings of token {:?} and amount >= {:?} to spend by Wallet {:?}",
						self.token_type, self.value, wallet
					);
				})
				.0
				.clone()
		})
	}
}

impl<D: DB + Clone> BuildUtxoSpend<D> for UtxoSpendInfo<WalletSeed> {
	fn build(&self, context: Arc<LedgerContext<D>>) -> UtxoSpend {
		context.with_wallet_from_seed(self.owner, |wallet| {
			let utxo = self.min_match_utxo(context.clone(), wallet);
			UtxoSpend {
				value: utxo.value,
				owner: wallet.unshielded.signing_key().verifying_key(),
				type_: utxo.type_,
				intent_hash: utxo.intent_hash,
				output_no: utxo.output_no,
			}
		})
	}

	fn signing_key(&self, context: Arc<LedgerContext<D>>) -> SigningKey {
		context.with_wallet_from_seed(self.owner, |wallet| wallet.unshielded.signing_key().clone())
	}
}

// TODO: impl<D: DB + Clone> BuildUtxoSpend<D> for UtxoSpendInfo<VerifyingKey>
