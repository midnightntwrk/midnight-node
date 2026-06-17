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

use async_trait::async_trait;
use authority_selection_inherents::CommitteeMember;
use authority_selection_inherents::{
	AriadneInherentDataProvider as AriadneIDP, AuthoritySelectionDataSource,
	AuthoritySelectionInputs,
};
use derive_new::new;
use midnight_node_runtime::{
	CrossChainPublic,
	opaque::{Block, SessionKeys},
};
use midnight_primitives::BridgeRecipient;
use midnight_primitives_cnight_observation::CNightObservationApi;
use midnight_primitives_federated_authority_observation::FederatedAuthorityObservationApi;
use sc_consensus_aura::{SlotDuration, find_pre_digest};
use sc_service::Arc;
use sidechain_domain::{McBlockHash, ScEpochNumber, mainchain_epoch::MainchainEpochConfig};
use sidechain_mc_hash::McHashDataSource;
use sidechain_mc_hash::McHashInherentDataProvider as McHashIDP;
use sidechain_mc_hash::McHashInherentDigest;
use sp_partner_chains_consensus::VerificationContextSink;
use std::sync::Mutex;
use sidechain_slots::ScSlotConfig;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_consensus_aura::{Slot, sr25519::AuthorityPair as AuraPair};
use sp_core::Pair;
use sp_inherents::CreateInherentDataProviders;
use sp_partner_chains_bridge::{
	TokenBridgeDataSource, TokenBridgeIDPRuntimeApi, TokenBridgeInherentDataProvider,
};
use sp_runtime::traits::{Block as BlockT, Header, Zero};
use sp_session_validator_management::SessionValidatorManagementApi;
use sp_timestamp::Timestamp;
use std::error::Error;
use time_source::TimeSource;

use midnight_primitives_mainchain_follower::{
	FederatedAuthorityObservationDataSource, MidnightCNightObservationDataSource,
	idp::{FederatedAuthorityInherentDataProvider, MidnightCNightObservationInherentDataProvider},
};

//#[cfg(feature = "experimental")]
//use {midnight_node_runtime::BeneficiaryId, sp_block_rewards::BlockBeneficiaryInherentProvider};
#[allow(clippy::too_many_arguments)]
#[derive(new)]
pub(crate) struct ProposalCIDP<T> {
	config: CreateInherentDataConfig,
	client: Arc<T>,
	mc_hash_data_source: Arc<dyn McHashDataSource + Send + Sync>,
	authority_selection_data_source: Arc<dyn AuthoritySelectionDataSource + Send + Sync>,
	cnight_observation_data_source: Arc<dyn MidnightCNightObservationDataSource + Send + Sync>,
	federated_authority_observation_data_source:
		Arc<dyn FederatedAuthorityObservationDataSource + Send + Sync>,
	bridge_data_source: Arc<dyn TokenBridgeDataSource<BridgeRecipient> + Send + Sync>,
}

#[async_trait]
impl<T> CreateInherentDataProviders<Block, ()> for ProposalCIDP<T>
where
	T: ProvideRuntimeApi<Block> + Send + Sync + 'static,
	T: HeaderBackend<Block>,
	T::Api: SessionValidatorManagementApi<
			Block,
			CommitteeMember<CrossChainPublic, SessionKeys>,
			AuthoritySelectionInputs,
			ScEpochNumber,
		>,
	T::Api: CNightObservationApi<Block>,
	T::Api: FederatedAuthorityObservationApi<Block>,
	T::Api: TokenBridgeIDPRuntimeApi<Block>,
{
	type InherentDataProviders = (
		sp_consensus_aura::inherents::InherentDataProvider,
		sp_timestamp::InherentDataProvider,
		McHashIDP,
		AriadneIDP,
		//BlockBeneficiaryInherentProvider<BeneficiaryId>,
		MidnightCNightObservationInherentDataProvider,
		FederatedAuthorityInherentDataProvider,
		TokenBridgeInherentDataProvider<BridgeRecipient>,
	);

	async fn create_inherent_data_providers(
		&self,
		parent_hash: <Block as BlockT>::Hash,
		_extra_args: (),
	) -> Result<Self::InherentDataProviders, Box<dyn std::error::Error + Send + Sync>> {
		let Self {
			config,
			client,
			mc_hash_data_source,
			authority_selection_data_source,
			cnight_observation_data_source,
			federated_authority_observation_data_source,
			bridge_data_source,
		} = self;

		let CreateInherentDataConfig { mc_epoch_config, sc_slot_config, time_source } = config;

		let (slot, timestamp) =
			timestamp_and_slot_cidp(sc_slot_config.slot_duration, time_source.clone());

		let parent_header = client
			.header(parent_hash)?
			.ok_or_else(|| format!("Missing parent header for {parent_hash:?}"))?;

		let mc_hash = McHashIDP::new_proposal(
			parent_header,
			mc_hash_data_source.as_ref(),
			*slot,
			sc_slot_config.slot_duration,
		)
		.await
		.map_err(|e| {
			log::warn!("Failed to create mc_hash inherent data for proposal: {e}");
			e
		})?;

		let ariadne_data_provider = AriadneIDP::new(
			client.as_ref(),
			sc_slot_config,
			mc_epoch_config,
			parent_hash,
			*slot,
			authority_selection_data_source.as_ref(),
			mc_hash.mc_epoch(),
		)
		.await
		.map_err(|e| {
			log::warn!("Failed to create authority_selection inherent data for proposal: {e}");
			e
		})?;
		/*
		#[cfg(feature = "experimental")]
		let block_beneficiary_provider = BlockBeneficiaryInherentProvider::<BeneficiaryId>::from_env(
			"SIDECHAIN_BLOCK_BENEFICIARY",
		)?;
		 */

		let cnight_observation = MidnightCNightObservationInherentDataProvider::new(
			client.clone(),
			cnight_observation_data_source.as_ref(),
			parent_hash,
			mc_hash.mc_hash(),
		)
		.await
		.map_err(|e| {
			log::warn!("Failed to create cnight_observation inherent data for proposal: {e}");
			e
		})?;

		let federated_authority = FederatedAuthorityInherentDataProvider::new(
			client.clone(),
			federated_authority_observation_data_source.as_ref(),
			parent_hash,
			&mc_hash.mc_hash(),
		)
		.await
		.map_err(|e| {
			log::warn!("Failed to create federated_authority inherent data for proposal: {e}");
			e
		})?;

		let bridge = TokenBridgeInherentDataProvider::new(
			client.as_ref(),
			parent_hash,
			mc_hash.mc_hash(),
			bridge_data_source.as_ref(),
		)
		.await
		.map_err(|e| {
			log::warn!("Failed to create bridge inherent data for proposal: {e}");
			e
		})?;

		Ok((
			slot,
			timestamp,
			mc_hash,
			ariadne_data_provider,
			//#[cfg(feature = "experimental")]
			//block_beneficiary_provider,
			cnight_observation,
			federated_authority,
			bridge,
		))
	}
}

#[allow(clippy::too_many_arguments)]
#[derive(new)]
pub struct VerifierCIDP<T> {
	config: CreateInherentDataConfig,
	client: Arc<T>,
	mc_hash_data_source: Arc<dyn McHashDataSource + Send + Sync>,
	authority_selection_data_source: Arc<dyn AuthoritySelectionDataSource + Send + Sync>,
	cnight_observation_data_source: Arc<dyn MidnightCNightObservationDataSource + Send + Sync>,
	federated_authority_observation_data_source:
		Arc<dyn FederatedAuthorityObservationDataSource + Send + Sync>,
	bridge_data_source: Arc<dyn TokenBridgeDataSource<BridgeRecipient> + Send + Sync>,
	/// Slot and mainchain reference hash of the block currently being verified/imported,
	/// injected by the Partner Chains wrappers (`PartnerChainsVerifier` /
	/// `PartnerChainsBlockImport`) via [`VerificationContextSink`] immediately before they
	/// delegate to the inner consensus gadget. Behind interior mutability because
	/// `create_inherent_data_providers` takes `&self`; block verification/import is sequential,
	/// so the value set for a block is the one its inherent-data creation reads.
	#[new(default)]
	verification_context: Arc<Mutex<Option<(Slot, McBlockHash)>>>,
}

impl<T> VerificationContextSink<McHashInherentDigest> for VerifierCIDP<T>
where
	T: Send + Sync,
{
	fn set_verification_context(&self, slot: Slot, mc_hash: McBlockHash) {
		*self
			.verification_context
			.lock()
			.expect("VerifierCIDP verification context mutex poisoned") = Some((slot, mc_hash));
	}
}

#[async_trait]
impl<T> CreateInherentDataProviders<Block, ()> for VerifierCIDP<T>
where
	T: ProvideRuntimeApi<Block> + Send + Sync + HeaderBackend<Block> + 'static,
	T::Api: SessionValidatorManagementApi<
			Block,
			CommitteeMember<CrossChainPublic, SessionKeys>,
			AuthoritySelectionInputs,
			ScEpochNumber,
		>,
	T::Api: CNightObservationApi<Block>,
	T::Api: FederatedAuthorityObservationApi<Block>,
	T::Api: TokenBridgeIDPRuntimeApi<Block>,
{
	// The full inherent set the inner consensus gadget (Aura/BABE) recreates during its own
	// inherent check — the Partner Chains wrappers no longer perform the check themselves.
	// The slot and mainchain reference hash come from the header (injected via
	// `set_verification_context`), the rest from the Partner Chains data sources.
	type InherentDataProviders = (
		sp_consensus_aura::inherents::InherentDataProvider,
		sp_timestamp::InherentDataProvider,
		McHashIDP,
		AriadneIDP,
		MidnightCNightObservationInherentDataProvider,
		FederatedAuthorityInherentDataProvider,
		TokenBridgeInherentDataProvider<BridgeRecipient>,
	);

	async fn create_inherent_data_providers(
		&self,
		parent_hash: <Block as BlockT>::Hash,
		_extra_args: (),
	) -> Result<Self::InherentDataProviders, Box<dyn Error + Send + Sync>> {
		// Slot and mainchain reference hash of the block being verified/imported, injected by
		// the wrapper immediately before this call (see `VerificationContextSink`).
		let (verified_block_slot, mc_hash) = self
			.verification_context
			.lock()
			.expect("VerifierCIDP verification context mutex poisoned")
			.clone()
			.ok_or(
				"VerifierCIDP: verification context (slot, mc_hash) was not set before \
				 create_inherent_data_providers; the block must be verified/imported through \
				 PartnerChainsVerifier or PartnerChainsBlockImport",
			)?;

		let Self {
			config,
			client,
			mc_hash_data_source,
			authority_selection_data_source,
			cnight_observation_data_source,
			federated_authority_observation_data_source,
			bridge_data_source,
			..
		} = self;

		let CreateInherentDataConfig { mc_epoch_config, sc_slot_config, time_source, .. } = config;

		let slot = sp_consensus_aura::inherents::InherentDataProvider::new(verified_block_slot);

		let timestamp = sp_timestamp::InherentDataProvider::new(Timestamp::new(
			time_source.get_current_time_millis(),
		));
		let parent_header = client.expect_header(parent_hash)?;
		let parent_slot = slot_from_predigest(&parent_header)?;
		let mc_state_reference = McHashIDP::new_verification(
			parent_header,
			parent_slot,
			verified_block_slot,
			mc_hash.clone(),
			config.slot_duration(),
			mc_hash_data_source.as_ref(),
		)
		.await
		.map_err(|e| {
			log::warn!("Failed to create mc_hash inherent data for verification: {e}");
			e
		})?;

		let ariadne_data_provider = AriadneIDP::new(
			client.as_ref(),
			sc_slot_config,
			mc_epoch_config,
			parent_hash,
			verified_block_slot,
			authority_selection_data_source.as_ref(),
			mc_state_reference.epoch,
		)
		.await
		.map_err(|e| {
			log::warn!("Failed to create authority_selection inherent data for verification: {e}");
			e
		})?;

		let cnight_observation = MidnightCNightObservationInherentDataProvider::new(
			client.clone(),
			cnight_observation_data_source.as_ref(),
			parent_hash,
			mc_hash.clone(),
		)
		.await
		.map_err(|e| {
			log::warn!("Failed to create cnight_observation inherent data for verification: {e}");
			e
		})?;

		let federated_authority = FederatedAuthorityInherentDataProvider::new(
			client.clone(),
			federated_authority_observation_data_source.as_ref(),
			parent_hash,
			&mc_hash,
		)
		.await
		.map_err(|e| {
			log::warn!("Failed to create federated_authority inherent data for verification: {e}");
			e
		})?;

		let bridge = TokenBridgeInherentDataProvider::new(
			client.as_ref(),
			parent_hash,
			mc_hash,
			bridge_data_source.as_ref(),
		)
		.await
		.map_err(|e| {
			log::warn!("Failed to create bridge inherent data for verification: {e}");
			e
		})?;

		Ok((
			slot,
			timestamp,
			mc_state_reference,
			ariadne_data_provider,
			cnight_observation,
			federated_authority,
			bridge,
		))
	}
}

pub fn slot_from_predigest(
	header: &<Block as BlockT>::Header,
) -> Result<Option<Slot>, Box<dyn Error + Send + Sync>> {
	if header.number().is_zero() {
		// genesis block doesn't have a slot
		Ok(None)
	} else {
		Ok(Some(find_pre_digest::<Block, <AuraPair as Pair>::Signature>(header)?))
	}
}

#[derive(new, Clone)]
pub(crate) struct CreateInherentDataConfig {
	pub mc_epoch_config: MainchainEpochConfig,
	// TODO ETCM-4079 make sure that this struct can be instantiated only if sidechain epoch duration is divisible by slot_duration
	pub sc_slot_config: ScSlotConfig,
	pub time_source: Arc<dyn TimeSource + Send + Sync + 'static>,
}

impl CreateInherentDataConfig {
	pub fn slot_duration(&self) -> SlotDuration {
		self.sc_slot_config.slot_duration
	}
}

fn timestamp_and_slot_cidp(
	slot_duration: SlotDuration,
	time_source: Arc<dyn TimeSource + Send + Sync>,
) -> (sp_consensus_aura::inherents::InherentDataProvider, sp_timestamp::InherentDataProvider) {
	let timestamp = sp_timestamp::InherentDataProvider::new(Timestamp::new(
		time_source.get_current_time_millis(),
	));
	let slot = sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
		*timestamp,
		slot_duration,
	);
	(slot, timestamp)
}
