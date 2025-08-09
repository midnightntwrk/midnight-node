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

use crate::{
	builder::{
		BuildTxs, ClaimMintInfo, DefaultDB, DeserializedTransactionsWithContext, FromContext,
		HashOutput, LedgerContext, MintCoinInfo, NIGHT, Nonce, ProofProvider, ProofType,
		SignatureType, TransactionWithContext, Wallet,
	},
	tx_generator::builder::ClaimMintArgs,
};

const NONCE: Nonce = Nonce(HashOutput([0u8; 32]));

pub struct ClaimMintBuilder {
	funding_seed: String,
	rng_seed: Option<[u8; 32]>,
	amount: u128,
}

impl ClaimMintBuilder {
	pub fn new(args: ClaimMintArgs) -> Self {
		Self { funding_seed: args.funding_seed, rng_seed: args.rng_seed, amount: args.amount }
	}
}

#[async_trait]
impl BuildTxs for ClaimMintBuilder {
	type Error = Infallible;
	async fn build_txs_from(
		&self,
		received_tx: DeserializedTransactionsWithContext<SignatureType, ProofType>,
		prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Result<DeserializedTransactionsWithContext<SignatureType, ProofType>, Self::Error> {
		// - Calculate the funding `WalletSeed` (can be more than one)
		let funding_seed = Wallet::<DefaultDB>::wallet_seed_decode(&self.funding_seed);
		let inputs_wallet_seeds = vec![funding_seed];

		// initialize `LedgerContext` with the wallets
		let context = LedgerContext::new_from_wallet_seeds(&inputs_wallet_seeds);

		// update the context applying all existing previous txs queried from source (either genesis or live network)
		let previous_txs = received_tx.flat();
		context.update_from_txs(previous_txs);

		let context_arc = Arc::new(context);

		// - Transaction info
		let mut tx_info =
			ClaimMintInfo::new_from_context(context_arc.clone(), prover_arc.clone(), self.rng_seed);

		// - Mint
		let coin_info = MintCoinInfo {
			origin: funding_seed,
			token_type: NIGHT,
			value: self.amount,
			nonce: NONCE,
		};

		tx_info.set_coin(coin_info);

		#[cfg(not(feature = "erase-proof"))]
		let tx = tx_info.prove().await;

		#[cfg(feature = "erase-proof")]
		let tx = tx_info.erase_proof().await;

		let tx_with_context = TransactionWithContext::new(tx, None);

		Ok(DeserializedTransactionsWithContext { initial_tx: tx_with_context, batches: vec![] })
	}
}
