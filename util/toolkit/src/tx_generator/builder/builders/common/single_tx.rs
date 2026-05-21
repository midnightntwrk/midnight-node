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
	BuildInput, BuildIntent, BuildOutput, BuildUtxoOutput, BuildUtxoSpend, CoinSelectionStrategy,
	DefaultDB, FromContext as _, InputInfo, IntentInfo, LedgerContext, OfferInfo, OutputInfo,
	ProofProvider, Segment, ShieldedCoinSelectionError, ShieldedTokenType, ShieldedWallet,
	StandardTrasactionInfo, TransactionWithContext, UnshieldedOfferInfo, UnshieldedTokenType,
	UnshieldedWallet, UtxoId, UtxoOutputInfo, UtxoSelectionError, UtxoSpendInfo, WalletAddress,
	WalletSeed,
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

/// Per-destination shielded output spec, after CLI argument resolution
/// (broadcast / index alignment) has produced one entry per destination.
pub(crate) struct ShieldedOutputSpec<D: super::ledger_helpers_local::DB + Clone> {
	pub wallet: ShieldedWallet<D>,
	pub amount: u128,
	pub token_type: ShieldedTokenType,
}

/// Per-destination unshielded output spec, after CLI argument resolution.
pub(crate) struct UnshieldedOutputSpec {
	pub wallet: UnshieldedWallet,
	pub amount: u128,
	pub token_type: UnshieldedTokenType,
}

pub struct SingleTxBuilder {
	context: Arc<LedgerContext<DefaultDB>>,
	prover: Arc<dyn ProofProvider<DefaultDB>>,
	shielded_outputs: Vec<ShieldedOutputSpec<DefaultDB>>,
	unshielded_outputs: Vec<UnshieldedOutputSpec>,
	source_seed: WalletSeed,
	funding_seed: Option<WalletSeed>,
	input_utxos: Vec<UtxoId>,
	rng_seed: Option<[u8; 32]>,
	coin_selection: CoinSelectionStrategy,
}

impl SingleTxBuilder {
	pub fn new(
		args: SingleTxArgs,
		context: Arc<LedgerContext<DefaultDB>>,
		prover: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Self {
		use super::type_convert::*;

		let local_destinations: Vec<WalletAddress> =
			args.destination_address.iter().map(convert_wallet_address).collect();

		// Convert amount/token vecs to version-local types.
		let shielded_amounts: Vec<u128> = args.shielded_amount.clone();
		let unshielded_amounts: Vec<u128> = args.unshielded_amount.clone();
		let shielded_token_types: Vec<ShieldedTokenType> = args
			.shielded_token_type
			.iter()
			.copied()
			.map(convert_shielded_token_type)
			.collect();
		let unshielded_token_types: Vec<UnshieldedTokenType> = args
			.unshielded_token_type
			.iter()
			.copied()
			.map(convert_unshielded_token_type)
			.collect();

		// Classify each destination address as shielded or unshielded, preserving
		// command-line order on each side. Per-output amounts and token types are
		// later assigned by the order in which destinations of each kind appeared.
		let mut shielded_wallets: Vec<ShieldedWallet<DefaultDB>> = Vec::new();
		let mut unshielded_wallets: Vec<UnshieldedWallet> = Vec::new();
		for (idx, addr) in local_destinations.iter().enumerate() {
			if let Ok(sw) = <ShieldedWallet<DefaultDB>>::try_from(addr) {
				shielded_wallets.push(sw);
			} else if let Ok(uw) = UnshieldedWallet::try_from(addr) {
				unshielded_wallets.push(uw);
			} else {
				log::error!(
					"destination address at position {} does not parse as shielded or unshielded: {:?}",
					idx,
					addr
				);
				panic!("destination_address parse error");
			}
		}

		// Resolve amounts/tokens against the destination count on each side, with
		// broadcast (single value) or per-destination alignment.
		let shielded_outputs = resolve_outputs(
			"shielded",
			&shielded_wallets,
			&shielded_amounts,
			&shielded_token_types,
			|wallet, amount, token_type| ShieldedOutputSpec { wallet, amount, token_type },
		);
		let unshielded_outputs = resolve_outputs(
			"unshielded",
			&unshielded_wallets,
			&unshielded_amounts,
			&unshielded_token_types,
			|wallet, amount, token_type| UnshieldedOutputSpec { wallet, amount, token_type },
		);

		Self {
			context,
			prover,
			shielded_outputs,
			unshielded_outputs,
			source_seed: convert_wallet_seed(args.source_seed),
			funding_seed: args.funding_seed.map(convert_wallet_seed),
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

/// Pair per-destination wallets with their amount and token type, supporting
/// either broadcast (single value applied to all) or full per-destination
/// alignment by index. Panics with a clear message on mismatched lengths.
fn resolve_outputs<W: Clone, Tt: Copy + Default, S>(
	side: &str,
	wallets: &[W],
	amounts: &[u128],
	token_types_local: &[Tt],
	mk: impl Fn(W, u128, Tt) -> S,
) -> Vec<S> {
	let n = wallets.len();
	if n == 0 {
		if !amounts.is_empty() {
			log::warn!(
				"--{side}-amount was provided ({} value(s)) but no {side} destinations were given; ignoring",
				amounts.len()
			);
		}
		if !token_types_local.is_empty() {
			log::warn!(
				"--{side}-token-type was provided ({} value(s)) but no {side} destinations were given; ignoring",
				token_types_local.len()
			);
		}
		return Vec::new();
	}

	if amounts.is_empty() {
		log::error!("--{side}-amount is required when at least one {side} destination is given");
		panic!("missing --{side}-amount");
	}

	if amounts.len() != 1 && amounts.len() != n {
		log::error!(
			"--{side}-amount must be provided once (broadcast) or exactly {n} time(s) to match destinations; got {}",
			amounts.len()
		);
		panic!("--{side}-amount length mismatch");
	}

	if !token_types_local.is_empty() && token_types_local.len() != 1 && token_types_local.len() != n
	{
		log::error!(
			"--{side}-token-type must be omitted, provided once (broadcast), or exactly {n} time(s) to match destinations; got {}",
			token_types_local.len()
		);
		panic!("--{side}-token-type length mismatch");
	}

	(0..n)
		.map(|i| {
			let amount = if amounts.len() == 1 { amounts[0] } else { amounts[i] };
			let token_type = match token_types_local.len() {
				0 => Tt::default(),
				1 => token_types_local[0],
				_ => token_types_local[i],
			};
			mk(wallets[i].clone(), amount, token_type)
		})
		.collect()
}

#[async_trait]
impl BuildTxs for SingleTxBuilder {
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

		if !self.shielded_outputs.is_empty() {
			let offer = build_shielded_offer(
				context.clone(),
				self.source_seed.clone(),
				self.shielded_outputs.iter().map(clone_shielded_spec).collect(),
				self.coin_selection,
			)
			.expect("insufficient shielded coins for transfer");
			if offer.outputs.len() > MAX_GUARANTEED_OUTPUTS {
				tx_info.set_fallible_offers(HashMap::from([(1, offer)]));
			} else {
				tx_info.set_guaranteed_offer(offer);
			}
		}

		if !self.unshielded_outputs.is_empty() {
			let intents = build_unshielded_intents(
				context.clone(),
				self.source_seed.clone(),
				self.unshielded_outputs.iter().map(clone_unshielded_spec).collect(),
				&self.input_utxos,
				self.coin_selection,
			)
			.unwrap_or_else(|error| {
				panic!("failed to select unshielded UTXOs for transfer: {error}")
			});
			tx_info.set_intents(intents);
		}

		tx_info.set_funding_seeds(vec![funding_seed]);
		tx_info.use_mock_proofs_for_fees(true);

		if tx_info.is_empty() {
			log::error!(
				"transaction is empty! No valid destination_addresses were resolved into outputs"
			);
			panic!("transaction empty");
		}

		let tx = tx_info.prove().await.expect("Balancing TX failed");

		let tx_with_context = TransactionWithContext::new(tx, None);

		spin.finish("generated tx.");

		Ok(super::tx_serialization::build_single(tx_with_context))
	}
}

fn clone_shielded_spec(spec: &ShieldedOutputSpec<DefaultDB>) -> ShieldedOutputSpec<DefaultDB> {
	ShieldedOutputSpec {
		wallet: spec.wallet.clone(),
		amount: spec.amount,
		token_type: spec.token_type,
	}
}

fn clone_unshielded_spec(spec: &UnshieldedOutputSpec) -> UnshieldedOutputSpec {
	UnshieldedOutputSpec {
		wallet: spec.wallet.clone(),
		amount: spec.amount,
		token_type: spec.token_type,
	}
}

/// Build a shielded offer that may contain outputs of multiple distinct token
/// types. Inputs are selected separately per token type; one change output per
/// token type is appended when needed.
pub(crate) fn build_shielded_offer(
	context: Arc<LedgerContext<DefaultDB>>,
	funding_seed: WalletSeed,
	outputs: Vec<ShieldedOutputSpec<DefaultDB>>,
	coin_selection: CoinSelectionStrategy,
) -> Result<OfferInfo<DefaultDB>, ShieldedCoinSelectionError> {
	// Sum amounts per token type, in the order each token type first appears so
	// behaviour is deterministic for callers.
	let mut totals: Vec<(ShieldedTokenType, u128)> = Vec::new();
	for spec in &outputs {
		match totals.iter_mut().find(|(tt, _)| *tt == spec.token_type) {
			Some((_, sum)) => {
				*sum = sum
					.checked_add(spec.amount)
					.ok_or(ShieldedCoinSelectionError::ArithmeticOverflow)?;
			},
			None => totals.push((spec.token_type, spec.amount)),
		}
	}

	let mut inputs_info: Vec<Box<dyn BuildInput<DefaultDB>>> = Vec::new();
	let mut outputs_info: Vec<Box<dyn BuildOutput<DefaultDB>>> = Vec::new();

	// User outputs first, in the order they were given.
	for spec in outputs {
		let output: Box<dyn BuildOutput<DefaultDB>> = Box::new(OutputInfo {
			destination: spec.wallet,
			token_type: spec.token_type,
			value: spec.amount,
		});
		outputs_info.push(output);
	}

	// Per token type: select inputs and append a change refund if needed.
	for (token_type, total_required) in totals {
		let (token_inputs, change) = InputInfo::coins_to_cover_value(
			context.clone(),
			funding_seed.clone(),
			total_required,
			token_type,
			coin_selection,
		)?;

		for input in token_inputs {
			let input: Box<dyn BuildInput<DefaultDB>> = Box::new(input);
			inputs_info.push(input);
		}

		if change > 0 {
			let refund: Box<dyn BuildOutput<DefaultDB>> = Box::new(OutputInfo {
				destination: funding_seed.clone(),
				token_type,
				value: change,
			});
			outputs_info.push(refund);
		}
	}

	Ok(OfferInfo { inputs: inputs_info, outputs: outputs_info, transients: vec![] })
}

/// Build the unshielded intents that may contain outputs of multiple distinct
/// token types. UTXOs are selected separately per token type; one change output
/// per token type is appended when needed.
///
/// `input_utxos`, when non-empty, pins the inputs used for the spend. This is
/// only supported when exactly one unshielded token type is used across all
/// outputs (the pinned UTXOs must all share that token type).
pub(crate) fn build_unshielded_intents(
	context: Arc<LedgerContext<DefaultDB>>,
	source_seed: WalletSeed,
	outputs: Vec<UnshieldedOutputSpec>,
	input_utxos: &[UtxoId],
	coin_selection: CoinSelectionStrategy,
) -> Result<HashMap<u16, Box<dyn BuildIntent<DefaultDB>>>, UtxoSelectionError> {
	// Sum amounts per token type, preserving first-seen order.
	let mut totals: Vec<(UnshieldedTokenType, u128)> = Vec::new();
	for spec in &outputs {
		match totals.iter_mut().find(|(tt, _)| *tt == spec.token_type) {
			Some((_, sum)) => {
				*sum =
					sum.checked_add(spec.amount).ok_or(UtxoSelectionError::ArithmeticOverflow)?;
			},
			None => totals.push((spec.token_type, spec.amount)),
		}
	}

	if !input_utxos.is_empty() && totals.len() > 1 {
		panic!(
			"--input-utxo is only supported when a single unshielded token type is used; got {} distinct types",
			totals.len()
		);
	}

	let mut inputs_info: Vec<Box<dyn BuildUtxoSpend<DefaultDB>>> = Vec::new();
	let mut outputs_info: Vec<Box<dyn BuildUtxoOutput<DefaultDB>>> = Vec::new();

	// User outputs first, in the order they were given.
	for spec in outputs {
		let output: Box<dyn BuildUtxoOutput<DefaultDB>> = Box::new(UtxoOutputInfo {
			value: spec.amount,
			owner: spec.wallet,
			token_type: spec.token_type,
		});
		outputs_info.push(output);
	}

	// Per token type: select utxos (or use pinned utxos for the single-token case)
	// and append a change refund if needed.
	for (token_type, total_required) in totals {
		let (token_inputs, remaining) = if input_utxos.is_empty() {
			UtxoSpendInfo::utxos_to_cover_value(
				context.clone(),
				source_seed.clone(),
				total_required,
				token_type,
				coin_selection,
			)?
		} else {
			UtxoSpendInfo::utxos_by_ids(
				context.clone(),
				source_seed.clone(),
				total_required,
				token_type,
				input_utxos,
			)?
		};

		for input in token_inputs {
			let input: Box<dyn BuildUtxoSpend<DefaultDB>> = Box::new(input);
			inputs_info.push(input);
		}

		if remaining > 0 {
			let refund: Box<dyn BuildUtxoOutput<DefaultDB>> = Box::new(UtxoOutputInfo {
				value: remaining,
				owner: source_seed.clone(),
				token_type,
			});
			outputs_info.push(refund);
		}
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
	let boxed_intent: Box<dyn BuildIntent<DefaultDB>> = Box::new(intent_info);

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

		let outputs = vec![
			ShieldedOutputSpec { wallet: wallet1, amount: u128::MAX, token_type },
			ShieldedOutputSpec { wallet: wallet2, amount: u128::MAX, token_type },
		];

		let result = build_shielded_offer(
			context,
			test_seed(),
			outputs,
			CoinSelectionStrategy::default(),
		);

		assert!(matches!(result, Err(ShieldedCoinSelectionError::ArithmeticOverflow)));
	}

	#[test]
	fn build_unshielded_intents_mul_overflow_returns_arithmetic_error() {
		let context = test_context();
		let wallet1 = UnshieldedWallet::default(test_seed());
		let wallet2 = UnshieldedWallet::default(test_seed_2());
		let token_type = UnshieldedTokenType(HashOutput([0u8; 32]));

		let outputs = vec![
			UnshieldedOutputSpec { wallet: wallet1, amount: u128::MAX, token_type },
			UnshieldedOutputSpec { wallet: wallet2, amount: u128::MAX, token_type },
		];

		let result = build_unshielded_intents(
			context,
			test_seed(),
			outputs,
			&[],
			CoinSelectionStrategy::default(),
		);

		assert!(matches!(result, Err(UtxoSelectionError::ArithmeticOverflow)));
	}
}
