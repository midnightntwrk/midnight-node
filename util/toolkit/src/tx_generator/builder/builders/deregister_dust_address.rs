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

use std::{convert::Infallible, sync::Arc};

use async_trait::async_trait;
use midnight_node_ledger_helpers::{
	BuildIntent, BuildUtxoOutput, BuildUtxoSpend, DefaultDB, DustRegistrationBuilder, FromContext,
	IntentInfo, LedgerContext, NIGHT, ProofProvider, Segment, StandardTrasactionInfo,
	TransactionWithContext, UnshieldedOfferInfo, UtxoOutputInfo, UtxoSpendInfo, Wallet,
};

use crate::{
	ProofType, SignatureType,
	progress::Spin,
	serde_def::{DeserializedTransactionsWithContext, SourceTransactions},
	tx_generator::builder::{BuildTxs, DeregisterDustAddressArgs},
};

pub struct DeregisterDustAddressBuilder {
	seed: String,
	rng_seed: Option<[u8; 32]>,
	funding_seed: String,
}

impl DeregisterDustAddressBuilder {
	pub fn new(args: DeregisterDustAddressArgs) -> Self {
		Self { seed: args.wallet_seed, rng_seed: args.rng_seed, funding_seed: args.funding_seed }
	}
}

#[async_trait]
impl BuildTxs for DeregisterDustAddressBuilder {
	type Error = Infallible;

	async fn build_txs_from(
		&self,
		received_tx: SourceTransactions<SignatureType, ProofType>,
		prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Result<DeserializedTransactionsWithContext<SignatureType, ProofType>, Self::Error> {
		let spin = Spin::new("building deregister dust address transaction...");

		let seed = Wallet::<DefaultDB>::wallet_seed_decode(&self.seed);
		let funding_seed = Wallet::<DefaultDB>::wallet_seed_decode(&self.funding_seed);

		let network_id = received_tx.network();
		let context: LedgerContext<DefaultDB> =
			LedgerContext::new_from_wallet_seeds(network_id.to_string(), &[seed, funding_seed]);

		for block in &received_tx.blocks {
			context.update_from_block(block.transactions.clone(), block.context.clone(), None);
		}

		let context = Arc::new(context);

		let mut tx_info = StandardTrasactionInfo::new_from_context(
			context.clone(),
			prover_arc.clone(),
			self.rng_seed,
		);

		let inputs = context.with_ledger_state(|ledger_state| {
			context.with_wallet_from_seed(seed, |wallet| {
				wallet
					.unshielded_utxos(ledger_state)
					.iter()
					.filter(|utxo| utxo.type_ == NIGHT)
					.map(|utxo| UtxoSpendInfo {
						value: utxo.value,
						owner: seed,
						token_type: NIGHT,
						intent_hash: None,
						output_number: None,
					})
					.collect::<Vec<_>>()
			})
		});

		let outputs: Vec<Box<dyn BuildUtxoOutput<DefaultDB>>> = inputs
			.iter()
			.map(|input| {
				let output: Box<dyn BuildUtxoOutput<DefaultDB>> = Box::new(UtxoOutputInfo {
					value: input.value,
					owner: input.owner,
					token_type: input.token_type,
				});
				output
			})
			.collect();

		let inputs: Vec<Box<dyn BuildUtxoSpend<DefaultDB>>> = inputs
			.into_iter()
			.map(|input| {
				let input: Box<dyn BuildUtxoSpend<DefaultDB>> = Box::new(input);
				input
			})
			.collect::<Vec<_>>();

		let guaranteed_unshielded_offer = UnshieldedOfferInfo { inputs, outputs };
		let intent_info = IntentInfo {
			guaranteed_unshielded_offer: Some(guaranteed_unshielded_offer),
			fallible_unshielded_offer: None,
			actions: vec![],
		};

		let boxed_intent: Box<dyn BuildIntent<DefaultDB>> = Box::new(intent_info);
		tx_info.add_intent(Segment::Fallible.into(), boxed_intent);

		// Deregistration: pass dust_address: None instead of Some(dust_address)
		context.with_wallet_from_seed(seed, |wallet| {
			tx_info.add_dust_registration(DustRegistrationBuilder {
				signing_key: wallet.unshielded.signing_key().clone(),
				dust_address: None,
			});
		});

		tx_info.set_funding_seeds(vec![funding_seed]);
		tx_info.use_mock_proofs_for_fees(true);

		let tx = tx_info.prove().await.expect("Balancing TX failed");

		let tx_with_context = TransactionWithContext::new(tx, None);

		spin.finish("generated tx.");

		Ok(DeserializedTransactionsWithContext { initial_tx: tx_with_context, batches: vec![] })
	}
}
