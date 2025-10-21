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
	BuildOutput, CoinInfo, DB, InputInfo, LedgerContext, OfferInfo, OutputInfo, ProofPreimage,
	Segment, StdRng, Transient, WalletSeed,
};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct TransientInfo<O, D> {
	pub input: InputInfo<O>,
	pub output: OutputInfo<D>,
}

pub trait BuildTransient<D: DB + Clone>: Send + Sync {
	fn build(
		&self,
		rng: &mut StdRng,
		context: Arc<LedgerContext<D>>,
	) -> Transient<ProofPreimage, D>;
}

impl<D: DB + Clone> BuildTransient<D> for TransientInfo<WalletSeed, WalletSeed> {
	fn build(
		&self,
		rng: &mut StdRng,
		context: Arc<LedgerContext<D>>,
	) -> Transient<ProofPreimage, D> {
		let inputs = vec![];
		let outputs: Vec<Box<dyn BuildOutput<D>>> = vec![Box::new(self.output)];
		let transients = vec![];

		let mut offer_arg = OfferInfo { inputs, outputs, transients };
		let offer = offer_arg.build(rng, context.clone());

		context.with_wallets_from_seeds(
			self.input.origin,
			self.output.destination,
			|_origin_wallet, destination_wallet| {
				// Apply offer to `destination` to be able to spend later
				let secret_keys = &destination_wallet.shielded.secret_keys();
				let state = &destination_wallet.shielded.state;

				let transient_state = state.apply(secret_keys, &offer);

				//---------- Alternative #1
				let coin_info = CoinInfo::new(rng, self.output.value, self.output.token_type);
				let mt_index = transient_state.first_free - 1; // The output is the latets inserted
				let qualified_coin = coin_info.qualify(mt_index);

				let output = self.output.build(rng, context.clone());

				let (_new_transient_state, transient) = transient_state
					.spend_from_output(
						rng,
						secret_keys,
						&qualified_coin,
						Segment::Guaranteed.into(),
						output,
					)
					.expect("Invalid Transient arguments");

				// transient_wallet = new_transient_state; // ??????? Not sure

				transient

				// //---------- Alternative #2
				// // In this case, `input` field wouldn't be necessary
				// // Update `funding_wallet` which is also `destination`
				// self.input.funding_wallet.clone() = transient_wallet;

				// let input = self.input.build(rng);

				// if output.contract_addrs.is_some() {
				// 	Transient::new_from_contract_owned_output(
				// 		rng,
				// 		&qualified_coin_info,
				// 		outputs[i as usize].clone(),
				// 	)
				// 	.expect("Transient arguments should be valid")
				// } else {
				// 	Transient {
				// 		nullifier: input.nullifier,
				// 		coin_com: output.coin_com,
				// 		value_commitment_input: input.value_commitment,
				// 		value_commitment_output: output.value_commitment,
				// 		contract_address: None,
				// 		ciphertext: output.clone().ciphertext,
				// 		proof_input: input.clone().proof,
				// 		proof_output: output.clone().proof,
				// 	}
			},
		)
	}
}

// TODO: Other possible BuildTransient impl
// impl BuildTransient for TransientInfo<WalletSeed, ContractAddress> {}
// impl BuildTransient for TransientInfo<ContractAddress, WalletSeed> {}
// impl BuildTransient for TransientInfo<ContractAddress, ContractAddress> {}
