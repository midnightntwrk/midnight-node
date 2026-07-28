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
	Array, BuildUtxoOutput, BuildUtxoSpend, BuilderContext, DB, Signature, Sp, UnshieldedOffer,
	UtxoOutput, UtxoSpend,
};
use std::sync::Arc;

#[derive(Default)]
pub struct UnshieldedOfferInfo<D: DB + Clone, C: BuilderContext<D>> {
	pub inputs: Vec<Box<dyn BuildUtxoSpend<D, C>>>,
	pub outputs: Vec<Box<dyn BuildUtxoOutput<D, C>>>,
}

impl<D: DB + Clone, C: BuilderContext<D>> UnshieldedOfferInfo<D, C> {
	pub async fn build(&self, context: Arc<C>) -> Sp<UnshieldedOffer<Signature, D>, D> {
		let inputs = self.build_inputs(context.clone()).await;
		let outputs = self.build_outputs(context.clone());

		let unshielded_offer = UnshieldedOffer {
			inputs: inputs.into(),
			outputs: outputs.into(),
			signatures: Array::new(),
		};

		Sp::new(unshielded_offer)
	}

	pub async fn build_inputs(&self, context: Arc<C>) -> Vec<UtxoSpend> {
		let mut inputs: Vec<UtxoSpend> = Vec::with_capacity(self.inputs.len());
		for input in self.inputs.iter() {
			inputs.push(input.build(context.clone()).await);
		}
		inputs.sort();
		inputs
	}

	pub fn build_outputs(&self, context: Arc<C>) -> Vec<UtxoOutput> {
		let mut outputs: Vec<_> =
			self.outputs.iter().map(|output| output.build(context.clone())).collect();
		outputs.sort();
		outputs
	}
}
