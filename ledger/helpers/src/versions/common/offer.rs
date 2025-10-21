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

use super::{
	BuildInput, BuildOutput, BuildTransient, DB, Delta, Input, LedgerContext, Offer, Output,
	ProofPreimage, ShieldedTokenType, StdRng, Transient,
};
use std::{collections::HashMap, sync::Arc};

pub trait TokenInfo {
	fn token_type(&self) -> ShieldedTokenType;
	fn value(&self) -> u128;
}

pub type TokensBalance = HashMap<ShieldedTokenType, u128>;

#[derive(Default)]
pub struct OfferInfo<D: DB + Clone> {
	pub inputs: Vec<Box<dyn BuildInput<D>>>,
	pub outputs: Vec<Box<dyn BuildOutput<D>>>,
	pub transients: Vec<Box<dyn BuildTransient<D>>>,
}

impl<D: DB + Clone> OfferInfo<D> {
	pub fn build(
		&mut self,
		rng: &mut StdRng,
		context: Arc<LedgerContext<D>>,
	) -> Offer<ProofPreimage, D> {
		let (inputs, inputs_balance) = self.build_inputs(rng, context.clone());
		let (outputs, outputs_balance) = self.build_outputs(rng, context.clone());
		let transient = self.build_transients(rng, context.clone());

		let inputs = inputs.into();
		let outputs = outputs.into();
		let transient = transient.into();
		let deltas = Self::calculate_offer_deltas(&inputs_balance, &outputs_balance).into();

		let mut offer = Offer { inputs, outputs, transient, deltas };

		offer.normalize();
		offer
	}

	fn build_inputs(
		&mut self,
		rng: &mut StdRng,
		context: Arc<LedgerContext<D>>,
	) -> (Vec<Input<ProofPreimage, D>>, TokensBalance) {
		self.inputs.iter_mut().fold(
			(Vec::new(), TokensBalance::default()),
			|(mut inputs, mut tokens_balance), input| {
				inputs.push(input.build(rng, context.clone()));
				tokens_balance
					.entry(input.token_type())
					.and_modify(|balance| *balance += input.value())
					.or_insert(input.value());

				(inputs, tokens_balance)
			},
		)
	}

	pub fn build_outputs(
		&self,
		rng: &mut StdRng,
		context: Arc<LedgerContext<D>>,
	) -> (Vec<Output<ProofPreimage, D>>, TokensBalance) {
		self.outputs.iter().fold(
			(Vec::new(), TokensBalance::default()),
			|(mut outputs, mut tokens_balance), output| {
				outputs.push(output.build(rng, context.clone()));
				tokens_balance
					.entry(output.token_type())
					.and_modify(|balance| *balance += output.value())
					.or_insert(output.value());

				(outputs, tokens_balance)
			},
		)
	}

	pub fn build_transients(
		&self,
		rng: &mut StdRng,
		context: Arc<LedgerContext<D>>,
	) -> Vec<Transient<ProofPreimage, D>> {
		self.transients
			.iter()
			.map(|transient| transient.build(rng, context.clone()))
			.collect()
	}

	fn calculate_offer_deltas(
		inputs_balance: &TokensBalance,
		outputs_balance: &TokensBalance,
	) -> Vec<Delta> {
		let mut deltas = HashMap::new();

		// Process input balances (adding to deltas)
		for (token, &value) in inputs_balance {
			deltas
				.entry(*token)
				.and_modify(|e| *e += value as i128)
				.or_insert(value as i128);
		}

		// Process output balances (subtracting from deltas)
		for (token, &value) in outputs_balance {
			deltas
				.entry(*token)
				.and_modify(|e| *e -= value as i128)
				.or_insert(-(value as i128));
		}

		// Convert HashMap into a Vec of `Delta`
		deltas
			.into_iter()
			.map(|(token_type, value)| Delta { token_type, value })
			.collect()
	}
}
