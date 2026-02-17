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

use async_trait::async_trait;
use std::{convert::Infallible, sync::Arc};

use midnight_node_ledger_helpers::{
	ClaimMintInfo, DefaultDB, FromContext, LedgerContext, ProofProvider, RewardsInfo,
	TransactionWithContext, Wallet, WalletSeed,
};

use crate::{
	serde_def::{BuiltTransactions, DeserializedTransactionsWithContext, SourceTransactions},
	tx_generator::builder::{BuildTxs, ClaimRewardsArgs},
};

pub struct ClaimRewardsBuilder {
	funding_seed: String,
	rng_seed: Option<[u8; 32]>,
	amount: u128,
}

impl ClaimRewardsBuilder {
	pub fn new(args: ClaimRewardsArgs) -> Self {
		Self { funding_seed: args.funding_seed, rng_seed: args.rng_seed, amount: args.amount }
	}
}

#[async_trait]
impl BuildTxs for ClaimRewardsBuilder {
	type Error = Infallible;

	fn relevant_wallet_seeds(&self) -> Vec<WalletSeed> {
		let funding_seed = Wallet::<DefaultDB>::wallet_seed_decode(&self.funding_seed);
		vec![funding_seed]
	}

	async fn build_txs_from(
		&self,
		_received_tx: SourceTransactions,
		context: Option<Arc<LedgerContext<DefaultDB>>>,
		prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Result<BuiltTransactions, Self::Error> {
		let context_arc = context.expect("ClaimRewardsBuilder requires context");

		// - Calculate the funding `WalletSeed` (can be more than one)
		let funding_seed = Wallet::<DefaultDB>::wallet_seed_decode(&self.funding_seed);

		// - Transaction info
		let mut tx_info =
			ClaimMintInfo::new_from_context(context_arc.clone(), prover_arc.clone(), self.rng_seed);

		// - Mint
		let rewards = RewardsInfo { owner: funding_seed, value: self.amount };

		tx_info.set_rewards(rewards);

		#[cfg(not(feature = "erase-proof"))]
		let tx = tx_info.prove().await;

		#[cfg(feature = "erase-proof")]
		let tx = tx_info.erase_proof().await;

		let tx_with_context = TransactionWithContext::new(tx, None);

		let typed =
			DeserializedTransactionsWithContext { initial_tx: tx_with_context, batches: vec![] };
		Ok(BuiltTransactions::from_typed(typed))
	}
}
