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

use crate::{DB, LedgerContext, Sp, UnshieldedTokenType, Utxo, UtxoSpend, Wallet, WalletSeed};
use itertools::Itertools;
use std::sync::Arc;

pub struct UtxoSpendInfo<O> {
	pub value: u128,
	pub owner: O,
	pub token_type: UnshieldedTokenType,
}

pub trait BuildUtxoSpend<D: DB + Clone> {
	fn build(&self, context: Arc<LedgerContext<D>>) -> UtxoSpend;
}

impl UtxoSpendInfo<WalletSeed> {
	pub fn min_match_utxo<D: DB + Clone>(
		&self,
		context: Arc<LedgerContext<D>>,
		wallet: &Wallet<D>,
	) -> Arc<Sp<Utxo, D>> {
		context.with_ledger_state(|ledger_state| {
			let owner = wallet.unshielded.signing_key().verifying_key();

			let utxos = ledger_state
				.utxo
				.utxos
				.iter()
				.filter(|utxo| {
					utxo.type_ == self.token_type
						&& utxo.value >= self.value
						&& utxo.owner == owner.clone().into()
				})
				.sorted_by_key(|utxo| utxo.value)
				.collect::<Vec<Arc<Sp<Utxo, D>>>>();

			utxos
				.first()
				.unwrap_or_else(|| {
					panic!(
						"There are not fundings of Token {:?} to spend by Wallet {:?}",
						self.token_type, wallet
					)
				})
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
}

// TODO: impl<D: DB + Clone> BuildUtxoSpend<D> for UtxoSpendInfo<VerifyingKey>
