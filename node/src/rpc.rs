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

//! A collection of node-specific RPC methods.
//! Substrate provides the `sc-rpc` crate, which defines the core RPC layer
//! used by Substrate nodes. This file extends those RPC definitions with
//! capabilities that are specific to this project's runtime configuration.

#![warn(missing_docs)]

use authority_selection_inherents::CommitteeMember;
use authority_selection_inherents::{AuthoritySelectionInputs, CandidateValidationApi};
use jsonrpsee::RpcModule;
use midnight_node_runtime::{
	AccountId, BlockNumber, CrossChainPublic, Hash, Nonce,
	opaque::{Block, SessionKeys},
};
use sc_client_api::{BlockBackend, BlockchainEvents};
use sc_consensus_grandpa::{
	FinalityProofProvider, GrandpaJustificationStream, SharedAuthoritySet, SharedVoterState,
};
use sc_consensus_grandpa_rpc::{Grandpa, GrandpaApiServer};
use sc_rpc::SubscriptionTaskExecutor;
use sc_transaction_pool_api::TransactionPool;
use sidechain_domain::ScEpochNumber;
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder;
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use sp_session_validator_management_query::SessionValidatorManagementQuery;

use crate::main_chain_follower::DataSources;
use pallet_session_validator_management_rpc::*;
use pallet_sidechain_rpc::*;
use sidechain_domain::mainchain_epoch::MainchainEpochConfig;
use time_source::TimeSource;

use pallet_midnight::MidnightRuntimeApi;
use pallet_midnight_rpc::{Midnight, MidnightApiServer};
use sc_consensus_beefy::communication::notification::{
	BeefyBestBlockStream, BeefyVersionedFinalityProofStream,
};
pub use sc_rpc_api::DenyUnsafe;
use sp_consensus_beefy::AuthorityIdBound;
use std::sync::Arc;

/// Extra dependencies for GRANDPA
pub struct GrandpaDeps<B> {
	/// Voting round info.
	pub shared_voter_state: SharedVoterState,
	/// Authority set info.
	pub shared_authority_set: SharedAuthoritySet<Hash, BlockNumber>,
	/// Receives notifications about justification events from Grandpa.
	pub justification_stream: GrandpaJustificationStream<Block>,
	/// Executor to drive the subscription manager in the Grandpa RPC handler.
	pub subscription_executor: SubscriptionTaskExecutor,
	/// Finality proof provider.
	pub finality_provider: Arc<FinalityProofProvider<B, Block>>,
}

/// Dependencies for BEEFY
pub struct BeefyDeps<AuthorityId: AuthorityIdBound> {
	/// Receives notifications about finality proof events from BEEFY.
	pub beefy_finality_proof_stream: BeefyVersionedFinalityProofStream<Block, AuthorityId>,
	/// Receives notifications about best block events from BEEFY.
	pub beefy_best_block_stream: BeefyBestBlockStream<Block>,
	/// Executor to drive the subscription manager in the BEEFY RPC handler.
	pub subscription_executor: sc_rpc::SubscriptionTaskExecutor,
}

/// Full client dependencies.
pub struct FullDeps<C, P, B, T, AuthorityId: AuthorityIdBound> {
	/// The client instance to use.
	pub client: Arc<C>,
	/// Transaction pool instance.
	pub pool: Arc<P>,
	/// GRANDPA specific dependencies.
	pub grandpa: GrandpaDeps<B>,
	/// BEEFY specific dependencies.
	pub beefy: BeefyDeps<AuthorityId>,
	/// Main chain follower data sources.
	pub main_chain_follower_data_sources: DataSources,
	/// Source of system time
	pub time_source: Arc<T>,
	/// Main chain epoch config
	pub main_chain_epoch_config: MainchainEpochConfig,
	/// Backend used by the node.
	pub backend: Arc<B>,
}

/// Instantiate all full RPC extensions.
#[allow(deprecated)] // GetSidechainStatus is deprecated but required
pub fn create_full<C, P, B, T, AuthorityId: AuthorityIdBound>(
	deps: FullDeps<C, P, B, T, AuthorityId>,
) -> Result<RpcModule<()>, Box<dyn std::error::Error + Send + Sync>>
where
	C: ProvideRuntimeApi<Block>,
	C: HeaderBackend<Block> + HeaderMetadata<Block, Error = BlockChainError> + 'static,
	C: BlockBackend<Block>,
	C: BlockchainEvents<Block>,
	C: Send + Sync + 'static,
	C::Api: substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
	C::Api: BlockBuilder<Block>,
	C::Api: MidnightRuntimeApi<Block>,
	C::Api: sp_consensus_aura::AuraApi<Block, sp_consensus_aura::sr25519::AuthorityId>,
	C::Api: sp_consensus_beefy::BeefyApi<Block, AuthorityId>,
	C::Api: mmr_rpc::MmrRuntimeApi<Block, <Block as sp_runtime::traits::Block>::Hash, BlockNumber>,
	C::Api: sidechain_slots::SlotApi<Block>,
	C::Api: sp_sidechain::GetGenesisUtxo<Block>,
	C::Api: sp_sidechain::GetSidechainStatus<Block>,
	C::Api: sp_session_validator_management::SessionValidatorManagementApi<
			Block,
			CommitteeMember<CrossChainPublic, SessionKeys>,
			AuthoritySelectionInputs,
			ScEpochNumber,
		>,
	C::Api: CandidateValidationApi<Block>,
	P: TransactionPool + 'static,
	B: sc_client_api::Backend<Block> + Send + Sync + 'static,
	B::State: sc_client_api::backend::StateBackend<sp_runtime::traits::HashingFor<Block>>,
	T: TimeSource + Send + Sync + 'static,
{
	use mmr_rpc::{Mmr, MmrApiServer};
	use sc_consensus_beefy_rpc::{Beefy, BeefyApiServer};
	use substrate_frame_rpc_system::{System, SystemApiServer};

	let mut module = RpcModule::new(());
	let FullDeps {
		client,
		pool,
		grandpa,
		beefy,
		main_chain_follower_data_sources,
		time_source,
		main_chain_epoch_config,
		backend,
	} = deps;

	module.merge(System::new(client.clone(), pool).into_rpc())?;
	module.merge(
		SidechainRpc::new(
			client.clone(),
			main_chain_epoch_config,
			main_chain_follower_data_sources.sidechain_rpc.clone(),
			time_source.clone(),
		)
		.into_rpc(),
	)?;

	let GrandpaDeps {
		shared_voter_state,
		shared_authority_set,
		justification_stream,
		subscription_executor,
		finality_provider,
	} = grandpa;
	module.merge(
		Grandpa::new(
			subscription_executor,
			shared_authority_set.clone(),
			shared_voter_state,
			justification_stream,
			finality_provider,
		)
		.into_rpc(),
	)?;

	// Making synchronous calls in light client freezes the browser currently,
	// more context: https://github.com/paritytech/substrate/pull/3480
	// These RPCs should use an asynchronous caller instead.
	module.merge(
		Mmr::new(
			client.clone(),
			backend
				.offchain_storage()
				.ok_or("Backend doesn't provide an offchain storage")?,
		)
		.into_rpc(),
	)?;
	let BeefyDeps { beefy_finality_proof_stream, beefy_best_block_stream, subscription_executor } =
		beefy;
	module.merge(
		Beefy::<Block, AuthorityId>::new(
			beefy_finality_proof_stream,
			beefy_best_block_stream,
			subscription_executor,
		)?
		.into_rpc(),
	)?;

	module.merge(
		SessionValidatorManagementRpc::new(Arc::new(SessionValidatorManagementQuery::new(
			client.clone(),
			main_chain_follower_data_sources.authority_selection.clone(),
		)))
		.into_rpc(),
	)?;
	module.merge(Midnight::new(client).into_rpc())?;

	// Extend this RPC with a custom API by using the following syntax.
	// `YourRpcStruct` should have a reference to a client, which is needed
	// to call into the runtime.
	// `module.merge(YourRpcTrait::into_rpc(YourRpcStruct::new(ReferenceToClient, ...)))?;`

	Ok(module)
}
