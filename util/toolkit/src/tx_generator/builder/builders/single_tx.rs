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

use std::{collections::HashMap, convert::Infallible, sync::Arc};

use async_trait::async_trait;
use midnight_node_ledger_helpers::{
	BuildInput, BuildIntent, BuildOutput, BuildUtxoOutput, BuildUtxoSpend, DefaultDB,
	FromContext as _, InputInfo, IntentInfo, LedgerContext, OfferInfo, OutputInfo, ProofProvider,
	Segment, ShieldedTokenType, ShieldedWallet, StandardTrasactionInfo, TransactionWithContext,
	UnshieldedOfferInfo, UnshieldedTokenType, UnshieldedWallet, UtxoOutputInfo, UtxoSpendInfo,
	Wallet, WalletAddress, WalletSeed,
};

use crate::{
	ProofType, SignatureType,
	progress::Spin,
	serde_def::{DeserializedTransactionsWithContext, SourceTransactions},
	tx_generator::builder::{BuildTxs, SingleTxArgs},
};

pub struct SingleTxBuilder {
	shielded_amount: Option<u128>,
	shielded_token_type: ShieldedTokenType,
	unshielded_amount: Option<u128>,
	unshielded_token_type: UnshieldedTokenType,
	source_seed: String,
	destination_address: Vec<WalletAddress>,
	rng_seed: Option<[u8; 32]>,
}

impl SingleTxBuilder {
	pub fn new(args: SingleTxArgs) -> Self {
		let SingleTxArgs {
			shielded_amount,
			shielded_token_type,
			unshielded_amount,
			unshielded_token_type,
			source_seed,
			destination_address,
			rng_seed,
		} = args;
		Self {
			shielded_amount,
			shielded_token_type,
			unshielded_amount,
			unshielded_token_type,
			source_seed,
			destination_address,
			rng_seed,
		}
	}

	pub fn build() {}
}

#[async_trait]
impl BuildTxs for SingleTxBuilder {
	type Error = Infallible;
	async fn build_txs_from(
		&self,
		received_tx: SourceTransactions<SignatureType, ProofType>,
		prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Result<DeserializedTransactionsWithContext<SignatureType, ProofType>, Self::Error> {
		let spin = Spin::new("generating single tx...");

		let funding_seed = Wallet::<DefaultDB>::wallet_seed_decode(&self.source_seed);
		let network_id = received_tx.network();
		let context = LedgerContext::new_from_wallet_seeds(network_id, &[funding_seed]);
		for block in received_tx.blocks {
			context.update_from_block(block.transactions, block.context, block.state_root.clone());
		}

		let context = Arc::new(context);

		// - Transaction info
		let mut tx_info = StandardTrasactionInfo::new_from_context(
			context.clone(),
			prover_arc.clone(),
			self.rng_seed,
			None,
		);

		let shielded_wallets: Vec<ShieldedWallet<DefaultDB>> =
			self.destination_address.iter().filter_map(|d| d.try_into().ok()).collect();

		let unshielded_wallets: Vec<UnshieldedWallet> =
			self.destination_address.iter().filter_map(|d| d.try_into().ok()).collect();

		if shielded_wallets.len() + unshielded_wallets.len() < self.destination_address.len() {
			log::error!("Not all --destination_address values were successfully parsed.");
			log::error!("destination_addresses: {:#?}", self.destination_address);
			panic!("destination_address parse error");
		}

		if !shielded_wallets.is_empty() && self.shielded_amount.is_none() {
			log::error!("Passing shielded wallet addresses requires --shielded-amount");
			panic!("missing --shielded-amount");
		}

		if !unshielded_wallets.is_empty() && self.unshielded_amount.is_none() {
			log::error!("Passing unshielded wallet addresses requires --unshielded-amount");
			panic!("missing --unshielded-amount");
		}

		if !shielded_wallets.is_empty() {
			let offer = self.build_shielded_offer(
				context.clone(),
				funding_seed,
				shielded_wallets,
				self.shielded_amount.unwrap(),
			);
			tx_info.set_guaranteed_offer(offer);
		}

		if !unshielded_wallets.is_empty() {
			let intents = self.build_unshielded_intents(
				context.clone(),
				funding_seed,
				unshielded_wallets,
				self.unshielded_amount.unwrap(),
			);
			tx_info.set_intents(intents);
		}

		tx_info.set_wallet_seeds(vec![funding_seed]);
		tx_info.use_mock_proofs_for_fees(true);

		if tx_info.is_empty() {
			log::error!("transaction is empty! No valid destination_addresses were found");
			log::error!("destination_addresses: {:#?}", self.destination_address);
			panic!("transaction empty");
		}

		let tx = tx_info.prove().await.expect("Balancing TX failed");

		let tx_with_context = TransactionWithContext::new(tx, None);

		spin.finish("generated tx.");

		Ok(DeserializedTransactionsWithContext { initial_tx: tx_with_context, batches: Vec::new() })
	}
}

impl SingleTxBuilder {
	fn build_shielded_offer(
		&self,
		context: Arc<LedgerContext<DefaultDB>>,
		funding_seed: WalletSeed,
		output_wallets: Vec<ShieldedWallet<DefaultDB>>,
		amount: u128,
	) -> OfferInfo<DefaultDB> {
		let total_required = amount * output_wallets.len() as u128;

		let input_info = InputInfo {
			origin: funding_seed,
			token_type: self.shielded_token_type,
			value: total_required,
		};

		let inputs_info: Vec<Box<dyn BuildInput<DefaultDB>>> = vec![Box::new(input_info)];

		let mut outputs_info: Vec<Box<dyn BuildOutput<DefaultDB>>>;

		// Outputs info
		outputs_info = output_wallets
			.iter()
			.map(|wallet| {
				let output: Box<dyn BuildOutput<DefaultDB>> = Box::new(OutputInfo {
					destination: wallet.clone(),
					token_type: self.shielded_token_type,
					value: amount,
				});
				output
			})
			.collect();

		let funding_wallet = context.clone().wallet_from_seed(funding_seed);
		let input_amount = input_info.min_match_coin(&funding_wallet.shielded.state).value;
		let remaining_coins = input_amount - total_required;

		// Create an `Output` to its self with the remaining coins to avoid spending the whole `Input`
		let output_info_refund: Box<dyn BuildOutput<DefaultDB>> = Box::new(OutputInfo {
			destination: funding_seed,
			token_type: self.shielded_token_type,
			value: remaining_coins,
		});

		outputs_info.push(output_info_refund);

		OfferInfo { inputs: inputs_info, outputs: outputs_info, transients: vec![] }
	}

	fn build_unshielded_intents(
		&self,
		context: Arc<LedgerContext<DefaultDB>>,
		source_seed: WalletSeed,
		output_wallets: Vec<UnshieldedWallet>,
		amount_to_send_per_output: u128,
	) -> HashMap<u16, Box<dyn BuildIntent<DefaultDB>>> {
		let total_required = amount_to_send_per_output * output_wallets.len() as u128;

		let utxo_spend_info = UtxoSpendInfo {
			value: total_required,
			owner: source_seed,
			token_type: self.unshielded_token_type,
			intent_hash: None,
			output_number: None,
		};

		let funding_wallet = context.clone().wallet_from_seed(source_seed);
		let min_match_utxo = utxo_spend_info.min_match_utxo(context, &funding_wallet);

		let input_info: Box<dyn BuildUtxoSpend<DefaultDB>> = Box::new(utxo_spend_info);

		// Outputs info
		let mut outputs_info: Vec<Box<dyn BuildUtxoOutput<DefaultDB>>> = output_wallets
			.iter()
			.map(|wallet| {
				let output: Box<dyn BuildUtxoOutput<DefaultDB>> = Box::new(UtxoOutputInfo {
					value: amount_to_send_per_output,
					owner: wallet.clone(),
					token_type: self.unshielded_token_type,
				});
				output
			})
			.collect();

		let input_amount = min_match_utxo.value;
		let remaining_nights = input_amount - total_required;

		// Create an `UtxoOutput` to its self with the remaining nights to avoid spending the whole `UtxoSpend`
		let output_info_refund: Box<dyn BuildUtxoOutput<DefaultDB>> = Box::new(UtxoOutputInfo {
			value: remaining_nights,
			owner: source_seed,
			token_type: self.unshielded_token_type,
		});

		if remaining_nights > 0 {
			outputs_info.push(output_info_refund);
		}

		let guaranteed_unshielded_offer_info =
			UnshieldedOfferInfo { inputs: vec![input_info], outputs: outputs_info };

		let intent_info = IntentInfo {
			guaranteed_unshielded_offer: Some(guaranteed_unshielded_offer_info),
			fallible_unshielded_offer: None,
			actions: vec![],
		};
		let boxed_intent: Box<dyn BuildIntent<DefaultDB>> = Box::new(intent_info);

		let mut intents = HashMap::new();
		intents.insert(Segment::Fallible.into(), boxed_intent);

		intents
	}
}
