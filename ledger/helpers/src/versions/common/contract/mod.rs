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
use std::{any::Any, sync::Arc};

use super::super::{
	AlignedValue, ContractAddress, ContractCallPrototype, ContractDeploy, ContractOperation, DB,
	Intent, LedgerContext, Op, PedersenRandomness, ProofPreimageMarker, Resolver, ResultModeGather,
	ResultModeVerify, Signature, Sp, StdRng, Transcripts,
};

// Re-export types needed by submodules
pub use super::super::{
	ChargedState, ContractMaintenanceAuthority, ContractState, DUST_EXPECTED_FILES, DustResolver,
	EntryPointBuf, FetchMode, HashMapStorage, HistoricMerkleTree_check_root,
	HistoricMerkleTree_insert, Key, KeyLocation, MerkleTree, MidnightDataProvider, OutputMode,
	PUBLIC_PARAMS, PreTranscript, QueryContext, Rng, StateValue, ValueReprAlignedValue, key,
	leaf_hash, partition_transcripts, stval, verifier_key,
};

#[cfg(feature = "test-utils")]
pub use super::super::test_resolver;

mod call;
mod contracts;
mod deploy;
mod maintenance;

pub use call::*;
pub use contracts::*;
pub use deploy::*;
pub use maintenance::*;

#[async_trait]
pub trait Contract<D: DB + Clone>: Send + Sync {
	async fn deploy(&self, rng: &mut StdRng) -> ContractDeploy<D>;

	fn resolver(&self) -> &'static Resolver;

	fn transcript(
		&self,
		key: &str,
		input: &Box<dyn Any + Send + Sync>,
		address: &ContractAddress,
		context: Arc<LedgerContext<D>>,
	) -> (AlignedValue, Vec<AlignedValue>, Vec<Transcripts<D>>);

	fn operation(
		&self,
		key: &str,
		address: &ContractAddress,
		context: Arc<LedgerContext<D>>,
	) -> Sp<ContractOperation, D>;

	fn program_with_results(
		prog: &[Op<ResultModeGather, D>],
		results: &[AlignedValue],
	) -> Vec<Op<ResultModeVerify, D>>;

	fn contract_call(
		&self,
		address: &ContractAddress,
		key: &'static str,
		input: &Box<dyn Any + Send + Sync>,
		rng: &mut StdRng,
		context: Arc<LedgerContext<D>>,
	) -> ContractCallPrototype<D>;
}

#[async_trait]
pub trait BuildContractAction<D: DB + Clone>: Send + Sync {
	async fn build(
		&mut self,
		rng: &mut StdRng,
		context: Arc<LedgerContext<D>>,
		intent: &Intent<Signature, ProofPreimageMarker, PedersenRandomness, D>,
	) -> Intent<Signature, ProofPreimageMarker, PedersenRandomness, D>;
}
