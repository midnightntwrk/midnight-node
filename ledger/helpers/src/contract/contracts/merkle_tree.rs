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

use async_trait::async_trait;
use lazy_static::lazy_static;
use std::{any::Any, borrow::Cow, sync::Arc};

use crate::{
	AlignedValue, Array, Contract, ContractAddress, ContractCallPrototype, ContractDeploy,
	ContractMaintenanceAuthority, ContractOperation, ContractState, DB, DUMMY_COST_MODEL,
	EntryPointBuf, HashMapStorage as HashMap, HistoricMerkleTree_check_root,
	HistoricMerkleTree_insert, Key, KeyLocation, LedgerContext, MerkleTree, Op, QueryContext,
	Resolver, ResultModeGather, ResultModeVerify, Rng, Sp, StateValue, StdRng, Transcript,
	Transcripts, ValueReprAlignedValue, key, leaf_hash, stval, test_resolver, verifier_key,
};

lazy_static! {
	static ref RESOLVER: Resolver = test_resolver("simple-merkle-tree");
}

pub struct MerkleTreeContract {
	pub resolver: &'static Resolver,
}

impl MerkleTreeContract {
	pub fn new() -> Self {
		Self { resolver: &RESOLVER }
	}
}

impl Default for MerkleTreeContract {
	fn default() -> Self {
		Self::new()
	}
}

#[async_trait]
impl<D: DB + Clone> Contract<D> for MerkleTreeContract {
	async fn deploy(&self, rng: &mut StdRng) -> ContractDeploy<D> {
		let root = MerkleTree::<()>::blank(10).root();
		let store_op = ContractOperation::new(verifier_key(self.resolver, "store").await);
		let check_op = ContractOperation::new(verifier_key(self.resolver, "check").await);

		let contract = ContractState {
			data: stval!([[{MT(10) {}}, (0u64), {root => null}]]),
			operations: HashMap::new()
				.insert(b"store"[..].into(), store_op.clone())
				.insert(b"check"[..].into(), check_op.clone()),
			maintenance_authority: ContractMaintenanceAuthority {
				committee: vec![],
				threshold: 0,
				counter: 0,
			},
			balance: HashMap::new(),
		};

		ContractDeploy::new(rng, contract)
	}

	fn resolver(&self) -> &'static Resolver {
		self.resolver
	}

	fn transcript(
		&self,
		key: &str,
		input: &Box<dyn Any + Send>,
		address: &ContractAddress,
		context: Arc<LedgerContext<D>>,
	) -> (AlignedValue, Vec<AlignedValue>, Transcripts<D>) {
		context.with_ledger_state(|ledger_state| {
			let contract_state = ledger_state
				.index(*address)
				.unwrap_or_else(|| panic!("Contract with address {:?} does not exist", *address));

			let input = *input.downcast_ref::<u32>().expect("Contract Call input should exist");

			match key {
				"store" => {
					#[allow(deprecated)]
					let transcripts = Transcript::new(
						&QueryContext::new(contract_state.data, *address),
						&HistoricMerkleTree_insert!([key!(0u8)], false, 10, u32, input),
						&DUMMY_COST_MODEL,
					)
					.expect("Transcript arguments should be valid");

					let merkle_path = vec![];

					(input.into(), merkle_path, transcripts)
				},
				"check" => {
					let path = match &contract_state.data {
						StateValue::Array(arr) => match &arr[0] {
							StateValue::Array(arr) => match &arr[0] {
								StateValue::BoundedMerkleTree(tree) => tree
									.find_path_for_leaf(input)
									.expect("Path not found for leaf in MerkleTree contract"),
								_ => panic!(),
							},
							_ => panic!(),
						},
						_ => panic!(),
					};

					#[allow(deprecated)]
					let transcripts = Transcript::new(
						&QueryContext::new(contract_state.data, *address),
						&Self::program_with_results(
							&HistoricMerkleTree_check_root!(
								[key!(0u8)],
								false,
								10,
								u32,
								path.root()
							),
							&[true.into()],
						),
						&DUMMY_COST_MODEL,
					)
					.expect("Transcript arguments should be valid");

					let private_outputs = vec![path.into()];

					(input.into(), private_outputs, transcripts)
				},
				_ => panic!("Key doesn't exist for Merkle Tree Contract"),
			}
		})
	}

	fn operation(
		&self,
		key: &str,
		address: &ContractAddress,
		context: Arc<LedgerContext<D>>,
	) -> Sp<ContractOperation, D> {
		context.with_ledger_state(|ledger_state| {
			let contract_state = ledger_state
				.index(*address)
				.unwrap_or_else(|| panic!("Contract with address {:?} does not exist", *address));

			contract_state
				.operations
				.get(&EntryPointBuf(key.as_bytes().to_vec()))
				.expect("Contract Operation argments should be valid")
				.clone()
		})
	}

	fn program_with_results(
		prog: &[Op<ResultModeGather, D>],
		results: &[AlignedValue],
	) -> Vec<Op<ResultModeVerify, D>> {
		let mut res_iter = results.iter();
		prog.iter()
			.map(|op| op.clone().translate(|()| res_iter.next().unwrap().clone()))
			.collect()
	}

	fn contract_call(
		&self,
		address: &ContractAddress,
		key: &'static str,
		input: &Box<dyn Any + Send>,
		rng: &mut StdRng,
		context: Arc<LedgerContext<D>>,
	) -> ContractCallPrototype<D> {
		let (input, private_transcript_outputs, transcripts) =
			self.transcript(key, input, address, context.clone());

		ContractCallPrototype {
			address: *address,
			entry_point: key.as_bytes().into(),
			op: (*self.operation(key, address, context)).clone(),
			guaranteed_public_transcript: transcripts.0,
			fallible_public_transcript: transcripts.1,
			private_transcript_outputs,
			input,
			output: ().into(),
			communication_commitment_rand: rng.r#gen(),
			key_location: KeyLocation(Cow::Borrowed(key)),
		}
	}
}
