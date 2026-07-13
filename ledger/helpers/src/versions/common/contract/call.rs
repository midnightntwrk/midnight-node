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

use std::{any::Any, marker::PhantomData, sync::Arc};

use async_trait::async_trait;

use super::super::{
	BuildContractAction, BuilderContext, Contract, ContractAddress, DB, Intent, PedersenRandomness,
	ProofPreimage, ProofPreimageMarker, Signature, StdRng,
};

pub struct CallInfo<C: Contract<D>, D: DB + Clone> {
	pub type_: C,
	pub address: ContractAddress,
	pub key: &'static str,
	pub input: Box<dyn Any + Send + Sync>,
	pub _marker: PhantomData<D>,
}

#[async_trait]
impl<C: Contract<D>, D: DB + Clone, BC: BuilderContext<D>> BuildContractAction<D, BC>
	for CallInfo<C, D>
{
	async fn build(
		&mut self,
		rng: &mut StdRng,
		context: Arc<BC>,
		intent: &Intent<Signature, ProofPreimageMarker, PedersenRandomness, D>,
	) -> Intent<Signature, ProofPreimageMarker, PedersenRandomness, D> {
		let resolver = self.type_.resolver();
		context.update_resolver(resolver).await;

		let contract_state = context
			.contract_state(self.address)
			.await
			.unwrap_or_else(|| panic!("Contract with address {:?} does not exist", self.address));
		let parameters = context.ledger_parameters().await;

		let call = self.type_.contract_call(
			&self.address,
			self.key,
			&self.input,
			rng,
			&contract_state,
			&parameters,
		);

		intent.add_call::<ProofPreimage>(call)
	}
}
