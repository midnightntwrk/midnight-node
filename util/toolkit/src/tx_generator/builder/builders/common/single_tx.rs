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

use std::{
	collections::{HashMap, HashSet},
	convert::Infallible,
	sync::Arc,
};

use super::ledger_helpers_local::{
	BuildInput, BuildIntent, BuildOutput, BuildUtxoOutput, BuildUtxoSpend, BuilderContext,
	CoinSelectionStrategy, DefaultDB, FromContext as _, InputInfo, IntentInfo, OfferInfo,
	OutputInfo, ProofProvider, Segment, ShieldedCoinSelectionError, ShieldedTokenType,
	ShieldedWallet, StandardTrasactionInfo, TransactionWithContext, UnshieldedOfferInfo,
	UnshieldedTokenType, UnshieldedWallet, UtxoId, UtxoOutputInfo, UtxoSelectionError,
	UtxoSpendInfo, WalletAddress, WalletSeed,
};
use async_trait::async_trait;

use crate::{
	progress::Spin,
	serde_def::SourceTransactions,
	tx_generator::builder::{BuildTxs, SingleTxArgs},
};
use midnight_node_ledger_helpers::fork::raw_block_data::SerializedTxBatches;

pub(crate) const MAX_GUARANTEED_OUTPUTS: usize = 2;
const MAX_GUARANTEED_INPUTS_OUTPUTS: usize = 3;

pub struct SingleTxBuilder<C: BuilderContext<DefaultDB>> {
	context: Arc<C>,
	prover: Arc<dyn ProofProvider<DefaultDB>>,
	shielded_amount: Option<u128>,
	shielded_token_type: ShieldedTokenType,
	unshielded_amount: Option<u128>,
	unshielded_token_type: UnshieldedTokenType,
	source_seed: WalletSeed,
	funding_seed: Option<WalletSeed>,
	destination_address: Vec<WalletAddress>,
	input_utxos: Vec<UtxoId>,
	rng_seed: Option<[u8; 32]>,
	coin_selection: CoinSelectionStrategy,
}

impl<C: BuilderContext<DefaultDB>> SingleTxBuilder<C> {
	pub fn new(
		args: SingleTxArgs,
		context: Arc<C>,
		prover: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Self {
		use super::type_convert::*;
		Self {
			context,
			prover,
			shielded_amount: args.shielded_amount,
			shielded_token_type: convert_shielded_token_type(args.shielded_token_type),
			unshielded_amount: args.unshielded_amount,
			unshielded_token_type: convert_unshielded_token_type(args.unshielded_token_type),
			source_seed: convert_wallet_seed(args.source_seed),
			funding_seed: args.funding_seed.map(convert_wallet_seed),
			destination_address: args
				.destination_address
				.iter()
				.map(convert_wallet_address)
				.collect(),
			input_utxos: {
				let mut seen: HashSet<([u8; 32], u32)> = HashSet::new();
				args.input_utxos
					.iter()
					.filter(|id| seen.insert((id.intent_hash.0.0, id.output_number)))
					.map(convert_utxo_id)
					.collect()
			},
			rng_seed: args.rng_seed,
			coin_selection: args.coin_selection,
		}
	}

	pub fn build() {}
}

#[async_trait]
impl<C: BuilderContext<DefaultDB>> BuildTxs for SingleTxBuilder<C> {
	type Error = Infallible;

	async fn build_txs_from(
		&self,
		_received_tx: SourceTransactions,
	) -> Result<SerializedTxBatches, Self::Error> {
		let spin = Spin::new("generating single tx...");

		let context = self.context.clone();
		let funding_seed = self.funding_seed.clone().unwrap_or(self.source_seed.clone());

		// - Transaction info
		let mut tx_info = StandardTrasactionInfo::new_from_context(
			context.clone(),
			self.prover.clone(),
			self.rng_seed,
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
			let offer = build_shielded_offer(
				context.clone(),
				self.source_seed.clone(),
				shielded_wallets,
				self.shielded_amount.unwrap(),
				self.shielded_token_type,
				self.coin_selection,
			)
			.expect("insufficient shielded coins for transfer");
			if offer.outputs.len() > MAX_GUARANTEED_OUTPUTS {
				tx_info.set_fallible_offers(HashMap::from([(1, offer)]));
			} else {
				tx_info.set_guaranteed_offer(offer);
			}
		}

		if !unshielded_wallets.is_empty() {
			let intents = build_unshielded_intents(
				context.clone(),
				self.source_seed.clone(),
				unshielded_wallets,
				self.unshielded_amount.unwrap(),
				self.unshielded_token_type,
				&self.input_utxos,
				self.coin_selection,
			)
			.await
			.unwrap_or_else(|error| {
				panic!("failed to select unshielded UTXOs for transfer: {error}")
			});
			tx_info.set_intents(intents);
		}

		tx_info.set_funding_seeds(vec![funding_seed]);
		tx_info.use_mock_proofs_for_fees(true);

		if tx_info.is_empty() {
			log::error!("transaction is empty! No valid destination_addresses were found");
			log::error!("destination_addresses: {:#?}", self.destination_address);
			panic!("transaction empty");
		}

		let tx = tx_info.prove().await.expect("Balancing TX failed");

		let tx_with_context = TransactionWithContext::new(tx, None);

		spin.finish("generated tx.");

		Ok(super::tx_serialization::build_single(tx_with_context))
	}
}

pub(crate) fn build_shielded_offer<C: BuilderContext<DefaultDB>>(
	context: Arc<C>,
	funding_seed: WalletSeed,
	output_wallets: Vec<ShieldedWallet<DefaultDB>>,
	amount: u128,
	token_type: ShieldedTokenType,
	coin_selection: CoinSelectionStrategy,
) -> Result<OfferInfo<DefaultDB, C>, ShieldedCoinSelectionError> {
	let total_required = amount
		.checked_mul(output_wallets.len() as u128)
		.ok_or(ShieldedCoinSelectionError::ArithmeticOverflow)?;

	let (input_infos, change) = InputInfo::coins_to_cover_value(
		context,
		funding_seed.clone(),
		total_required,
		token_type,
		coin_selection,
	)?;

	let inputs_info: Vec<Box<dyn BuildInput<DefaultDB, C>>> = input_infos
		.into_iter()
		.map(|input| {
			let input: Box<dyn BuildInput<DefaultDB, C>> = Box::new(input);
			input
		})
		.collect();

	let mut outputs_info: Vec<Box<dyn BuildOutput<DefaultDB, C>>> = output_wallets
		.iter()
		.map(|wallet| {
			let output: Box<dyn BuildOutput<DefaultDB, C>> =
				Box::new(OutputInfo { destination: wallet.clone(), token_type, value: amount });
			output
		})
		.collect();

	if change > 0 {
		let output_info_refund: Box<dyn BuildOutput<DefaultDB, C>> =
			Box::new(OutputInfo { destination: funding_seed, token_type, value: change });
		outputs_info.push(output_info_refund);
	}

	Ok(OfferInfo { inputs: inputs_info, outputs: outputs_info, transients: vec![] })
}

pub(crate) async fn build_unshielded_intents<C: BuilderContext<DefaultDB>>(
	context: Arc<C>,
	source_seed: WalletSeed,
	output_wallets: Vec<UnshieldedWallet>,
	amount_to_send_per_output: u128,
	token_type: UnshieldedTokenType,
	input_utxos: &[UtxoId],
	coin_selection: CoinSelectionStrategy,
) -> Result<HashMap<u16, Box<dyn BuildIntent<DefaultDB, C>>>, UtxoSelectionError> {
	let total_required = amount_to_send_per_output
		.checked_mul(output_wallets.len() as u128)
		.ok_or(UtxoSelectionError::ArithmeticOverflow)?;

	let (inputs_info, remaining_nights) = if input_utxos.is_empty() {
		UtxoSpendInfo::utxos_to_cover_value(
			context,
			source_seed.clone(),
			total_required,
			token_type,
			coin_selection,
		)
		.await?
	} else {
		UtxoSpendInfo::utxos_by_ids(
			context,
			source_seed.clone(),
			total_required,
			token_type,
			input_utxos,
		)
		.await?
	};

	let inputs_info: Vec<Box<dyn BuildUtxoSpend<DefaultDB, C>>> = inputs_info
		.into_iter()
		.map(|input| {
			let input: Box<dyn BuildUtxoSpend<DefaultDB, C>> = Box::new(input);
			input
		})
		.collect();

	// Outputs info
	let mut outputs_info: Vec<Box<dyn BuildUtxoOutput<DefaultDB, C>>> = output_wallets
		.iter()
		.map(|wallet| {
			let output: Box<dyn BuildUtxoOutput<DefaultDB, C>> = Box::new(UtxoOutputInfo {
				value: amount_to_send_per_output,
				owner: wallet.clone(),
				token_type,
			});
			output
		})
		.collect();

	// Create an `UtxoOutput` to its self with the remaining nights to avoid spending the whole `UtxoSpend`
	let output_info_refund: Box<dyn BuildUtxoOutput<DefaultDB, C>> =
		Box::new(UtxoOutputInfo { value: remaining_nights, owner: source_seed, token_type });

	if remaining_nights > 0 {
		outputs_info.push(output_info_refund);
	}

	let inputs_outputs_len = inputs_info.len() + outputs_info.len();
	let unshielded_offer = UnshieldedOfferInfo { inputs: inputs_info, outputs: outputs_info };

	let intent_info = if inputs_outputs_len > MAX_GUARANTEED_INPUTS_OUTPUTS {
		IntentInfo {
			guaranteed_unshielded_offer: None,
			fallible_unshielded_offer: Some(unshielded_offer),
			actions: vec![],
		}
	} else {
		IntentInfo {
			guaranteed_unshielded_offer: Some(unshielded_offer),
			fallible_unshielded_offer: None,
			actions: vec![],
		}
	};
	let boxed_intent: Box<dyn BuildIntent<DefaultDB, C>> = Box::new(intent_info);

	let mut intents = HashMap::new();
	intents.insert(Segment::Fallible.into(), boxed_intent);

	Ok(intents)
}

#[cfg(test)]
mod tests {
	use super::super::ledger_helpers_local::{
		HashOutput, LedgerContext, ShieldedWallet, UnshieldedWallet,
	};
	use super::*;

	fn test_seed() -> WalletSeed {
		WalletSeed::Short([0u8; 16])
	}

	fn test_seed_2() -> WalletSeed {
		WalletSeed::Short([1u8; 16])
	}

	fn test_context() -> Arc<LedgerContext<DefaultDB>> {
		Arc::new(LedgerContext::new("test"))
	}

	#[test]
	fn build_shielded_offer_mul_overflow_returns_arithmetic_error() {
		let context = test_context();
		let wallet1 = ShieldedWallet::default(test_seed());
		let wallet2 = ShieldedWallet::default(test_seed_2());
		let token_type = ShieldedTokenType(HashOutput([0u8; 32]));

		let result = build_shielded_offer(
			context,
			test_seed(),
			vec![wallet1, wallet2],
			u128::MAX,
			token_type,
			CoinSelectionStrategy::default(),
		);

		assert!(matches!(result, Err(ShieldedCoinSelectionError::ArithmeticOverflow)));
	}

	#[tokio::test]
	async fn build_unshielded_intents_mul_overflow_returns_arithmetic_error() {
		let context = test_context();
		let wallet1 = UnshieldedWallet::default(test_seed());
		let wallet2 = UnshieldedWallet::default(test_seed_2());
		let token_type = UnshieldedTokenType(HashOutput([0u8; 32]));

		let result = build_unshielded_intents(
			context,
			test_seed(),
			vec![wallet1, wallet2],
			u128::MAX,
			token_type,
			&[],
			CoinSelectionStrategy::default(),
		)
		.await;

		assert!(matches!(result, Err(UtxoSelectionError::ArithmeticOverflow)));
	}
}
