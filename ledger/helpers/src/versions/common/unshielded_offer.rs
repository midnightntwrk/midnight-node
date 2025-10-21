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
	Array, BuildUtxoOutput, BuildUtxoSpend, DB, LedgerContext, Signature, Sp, UnshieldedOffer,
	UtxoOutput, UtxoSpend,
};
use std::sync::Arc;

#[derive(Default)]
pub struct UnshieldedOfferInfo<D: DB + Clone> {
	pub inputs: Vec<Box<dyn BuildUtxoSpend<D>>>,
	pub outputs: Vec<Box<dyn BuildUtxoOutput<D>>>,
}

impl<D: DB + Clone> UnshieldedOfferInfo<D> {
	pub fn build(&self, context: Arc<LedgerContext<D>>) -> Sp<UnshieldedOffer<Signature, D>, D> {
		let inputs = self.build_inputs(context.clone());
		let mut outputs = self.build_outputs(context.clone());

		outputs.sort();

		let unshielded_offer = UnshieldedOffer {
			inputs: inputs.into(),
			outputs: outputs.into(),
			signatures: Array::new(),
		};

		Sp::new(unshielded_offer)
	}

	pub fn build_inputs(&self, context: Arc<LedgerContext<D>>) -> Vec<UtxoSpend> {
		self.inputs.iter().map(|input| input.build(context.clone())).collect()
	}

	pub fn build_outputs(&self, context: Arc<LedgerContext<D>>) -> Vec<UtxoOutput> {
		self.outputs.iter().map(|output| output.build(context.clone())).collect()
	}
}
