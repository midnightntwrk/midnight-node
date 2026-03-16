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
	BuildInput, BuildOutput, BuildTransient, DB, Delta, Input, LedgerContext, Offer, Output,
	ProofPreimage, ShieldedTokenType, StdRng, Transient,
};
use std::{collections::HashMap, collections::hash_map::Entry, sync::Arc};

pub trait TokenInfo {
	fn token_type(&self) -> ShieldedTokenType;
	fn value(&self) -> u128;
}

pub type TokensBalance = HashMap<ShieldedTokenType, u128>;

#[derive(Debug, thiserror::Error)]
pub enum OfferBuildError {
	#[error("token value {value} exceeds maximum representable delta (i128::MAX)")]
	DeltaOverflow { value: u128 },
	#[error("delta accumulation overflow")]
	DeltaAccumulationOverflow,
	#[error("balance accumulation overflow")]
	BalanceOverflow,
}

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
	) -> Result<Offer<ProofPreimage, D>, OfferBuildError> {
		let (inputs, inputs_balance) = self.build_inputs(rng, context.clone())?;
		let (outputs, outputs_balance) = self.build_outputs(rng, context.clone())?;
		let transient = self.build_transients(rng, context.clone());

		let inputs = inputs.into();
		let outputs = outputs.into();
		let transient = transient.into();
		let deltas = Self::calculate_offer_deltas(&inputs_balance, &outputs_balance)?.into();

		let mut offer = Offer { inputs, outputs, transient, deltas };

		offer.normalize();
		Ok(offer)
	}

	fn build_inputs(
		&mut self,
		rng: &mut StdRng,
		context: Arc<LedgerContext<D>>,
	) -> Result<(Vec<Input<ProofPreimage, D>>, TokensBalance), OfferBuildError> {
		self.inputs.iter_mut().try_fold(
			(Vec::new(), TokensBalance::default()),
			|(mut inputs, mut tokens_balance), input| {
				inputs.push(input.build(rng, context.clone()));
				let value = input.value();
				match tokens_balance.entry(input.token_type()) {
					Entry::Occupied(mut e) => {
						*e.get_mut() =
							e.get().checked_add(value).ok_or(OfferBuildError::BalanceOverflow)?;
					},
					Entry::Vacant(e) => {
						e.insert(value);
					},
				}
				Ok((inputs, tokens_balance))
			},
		)
	}

	pub fn build_outputs(
		&self,
		rng: &mut StdRng,
		context: Arc<LedgerContext<D>>,
	) -> Result<(Vec<Output<ProofPreimage, D>>, TokensBalance), OfferBuildError> {
		self.outputs.iter().try_fold(
			(Vec::new(), TokensBalance::default()),
			|(mut outputs, mut tokens_balance), output| {
				outputs.push(output.build(rng, context.clone()));
				let value = output.value();
				match tokens_balance.entry(output.token_type()) {
					Entry::Occupied(mut e) => {
						*e.get_mut() =
							e.get().checked_add(value).ok_or(OfferBuildError::BalanceOverflow)?;
					},
					Entry::Vacant(e) => {
						e.insert(value);
					},
				}
				Ok((outputs, tokens_balance))
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
	) -> Result<Vec<Delta>, OfferBuildError> {
		let mut deltas: HashMap<ShieldedTokenType, i128> = HashMap::new();

		for (token, &value) in inputs_balance {
			let signed_value =
				i128::try_from(value).map_err(|_| OfferBuildError::DeltaOverflow { value })?;
			match deltas.entry(*token) {
				Entry::Occupied(mut e) => {
					*e.get_mut() = e
						.get()
						.checked_add(signed_value)
						.ok_or(OfferBuildError::DeltaAccumulationOverflow)?;
				},
				Entry::Vacant(e) => {
					e.insert(signed_value);
				},
			}
		}

		for (token, &value) in outputs_balance {
			let signed_value =
				i128::try_from(value).map_err(|_| OfferBuildError::DeltaOverflow { value })?;
			match deltas.entry(*token) {
				Entry::Occupied(mut e) => {
					*e.get_mut() = e
						.get()
						.checked_sub(signed_value)
						.ok_or(OfferBuildError::DeltaAccumulationOverflow)?;
				},
				Entry::Vacant(e) => {
					let neg_value = signed_value
						.checked_neg()
						.ok_or(OfferBuildError::DeltaAccumulationOverflow)?;
					e.insert(neg_value);
				},
			}
		}

		Ok(deltas
			.into_iter()
			.map(|(token_type, value)| Delta { token_type, value })
			.collect())
	}
}

#[cfg(test)]
mod tests {
	use super::super::{DefaultDB, HashOutput};
	use super::*;

	fn token_a() -> ShieldedTokenType {
		ShieldedTokenType(HashOutput([0u8; 32]))
	}

	type TestOfferInfo = OfferInfo<DefaultDB>;

	#[test]
	fn calculate_deltas_normal_values() {
		let inputs = HashMap::from([(token_a(), 100u128)]);
		let outputs = HashMap::from([(token_a(), 40u128)]);
		let deltas = TestOfferInfo::calculate_offer_deltas(&inputs, &outputs).unwrap();
		assert_eq!(deltas.len(), 1);
		assert_eq!(deltas[0].value, 60);
	}

	#[test]
	fn calculate_deltas_empty_inputs_outputs() {
		let inputs = TokensBalance::default();
		let outputs = TokensBalance::default();
		let deltas = TestOfferInfo::calculate_offer_deltas(&inputs, &outputs).unwrap();
		assert!(deltas.is_empty());
	}

	#[test]
	fn calculate_deltas_input_exceeds_i128_max() {
		let inputs = HashMap::from([(token_a(), u128::MAX)]);
		let outputs = TokensBalance::default();
		let result = TestOfferInfo::calculate_offer_deltas(&inputs, &outputs);
		assert!(matches!(
			result,
			Err(OfferBuildError::DeltaOverflow { value }) if value == u128::MAX
		));
	}

	#[test]
	fn calculate_deltas_output_exceeds_i128_max() {
		let inputs = TokensBalance::default();
		let outputs = HashMap::from([(token_a(), u128::MAX)]);
		let result = TestOfferInfo::calculate_offer_deltas(&inputs, &outputs);
		assert!(matches!(result, Err(OfferBuildError::DeltaOverflow { .. })));
	}

	#[test]
	fn calculate_deltas_at_i128_max_boundary() {
		let max_representable = i128::MAX as u128;
		let inputs = HashMap::from([(token_a(), max_representable)]);
		let outputs = TokensBalance::default();
		let deltas = TestOfferInfo::calculate_offer_deltas(&inputs, &outputs).unwrap();
		assert_eq!(deltas.len(), 1);
		assert_eq!(deltas[0].value, i128::MAX);
	}

	#[test]
	fn calculate_deltas_just_above_i128_max() {
		let just_above = (i128::MAX as u128) + 1;
		let inputs = HashMap::from([(token_a(), just_above)]);
		let outputs = TokensBalance::default();
		let result = TestOfferInfo::calculate_offer_deltas(&inputs, &outputs);
		assert!(matches!(result, Err(OfferBuildError::DeltaOverflow { .. })));
	}

	#[test]
	fn calculate_deltas_zero_values() {
		let inputs = HashMap::from([(token_a(), 0u128)]);
		let outputs = HashMap::from([(token_a(), 0u128)]);
		let deltas = TestOfferInfo::calculate_offer_deltas(&inputs, &outputs).unwrap();
		assert_eq!(deltas.len(), 1);
		assert_eq!(deltas[0].value, 0);
	}

	#[test]
	fn calculate_deltas_outputs_only() {
		let inputs = TokensBalance::default();
		let outputs = HashMap::from([(token_a(), 500u128)]);
		let deltas = TestOfferInfo::calculate_offer_deltas(&inputs, &outputs).unwrap();
		assert_eq!(deltas.len(), 1);
		assert_eq!(deltas[0].value, -500);
	}

	#[test]
	fn calculate_deltas_output_negation_at_i128_max() {
		let max_representable = i128::MAX as u128;
		let inputs = TokensBalance::default();
		let outputs = HashMap::from([(token_a(), max_representable)]);
		let deltas = TestOfferInfo::calculate_offer_deltas(&inputs, &outputs).unwrap();
		assert_eq!(deltas.len(), 1);
		assert_eq!(deltas[0].value, -i128::MAX);
	}
}
