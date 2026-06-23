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

use crate::cfg::midnight_cfg::invariants::{
	ConsensusConfigCoherenceError, MainchainEpochConfigError, check_mainchain_epoch_invariants,
	check_sidechain_mainchain_coherence,
};
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
use sidechain_slots::ScSlotConfig;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_consensus_aura::{Slot, sr25519::AuthorityPair as AuraPair};
use sp_core::Pair;
use sp_inherents::CreateInherentDataProviders;
use sp_partner_chains_bridge::{
	TokenBridgeDataSource, TokenBridgeIDPRuntimeApi, TokenBridgeInherentDataProvider,
};
use sp_partner_chains_consensus_aura::CurrentSlotProvider;
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
}

impl<T: Send + Sync> CurrentSlotProvider for VerifierCIDP<T> {
	fn slot(&self) -> Slot {
		*timestamp_and_slot_cidp(self.config.slot_duration(), self.config.time_source.clone()).0
	}
}

#[async_trait]
impl<T> CreateInherentDataProviders<Block, (Slot, McBlockHash)> for VerifierCIDP<T>
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
	type InherentDataProviders = (
		sp_timestamp::InherentDataProvider,
		AriadneIDP,
		MidnightCNightObservationInherentDataProvider,
		FederatedAuthorityInherentDataProvider,
		TokenBridgeInherentDataProvider<BridgeRecipient>,
	);

	async fn create_inherent_data_providers(
		&self,
		parent_hash: <Block as BlockT>::Hash,
		(verified_block_slot, mc_hash): (Slot, McBlockHash),
	) -> Result<Self::InherentDataProviders, Box<dyn Error + Send + Sync>> {
		let Self {
			config,
			client,
			mc_hash_data_source,
			authority_selection_data_source,
			cnight_observation_data_source,
			federated_authority_observation_data_source,
			bridge_data_source,
		} = self;

		let CreateInherentDataConfig { mc_epoch_config, sc_slot_config, time_source, .. } = config;

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

		Ok((timestamp, ariadne_data_provider, cnight_observation, federated_authority, bridge))
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

#[derive(Clone)]
pub(crate) struct CreateInherentDataConfig {
	pub mc_epoch_config: MainchainEpochConfig,
	pub sc_slot_config: ScSlotConfig,
	pub time_source: Arc<dyn TimeSource + Send + Sync + 'static>,
}

impl std::fmt::Debug for CreateInherentDataConfig {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		// `time_source` is a trait object without a `Debug` bound, so it is rendered as an opaque
		// placeholder rather than its contents.
		f.debug_struct("CreateInherentDataConfig")
			.field("mc_epoch_config", &self.mc_epoch_config)
			.field("sc_slot_config", &self.sc_slot_config)
			.field("time_source", &"<dyn TimeSource>")
			.finish()
	}
}

/// A timing-configuration coherence failure detected while building [`CreateInherentDataConfig`].
///
/// The constructor is on every path that builds the consensus input — including the subcommand
/// paths that load the configuration with validation disabled — so it enforces both the
/// self-contained mainchain invariants (`I1`–`I4`) and the sidechain↔mainchain cross-field
/// invariant (`I5`). This enum unifies the two underlying error types so the constructor reports a
/// single error regardless of which family of invariant was violated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InherentConfigError {
	/// A self-contained mainchain timing invariant (`I1`–`I4`) was violated.
	MainchainEpoch(MainchainEpochConfigError),
	/// The sidechain↔mainchain cross-field coherence invariant (`I5`) was violated.
	Coherence(ConsensusConfigCoherenceError),
}

impl core::fmt::Display for InherentConfigError {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			Self::MainchainEpoch(e) => e.fmt(f),
			Self::Coherence(e) => e.fmt(f),
		}
	}
}

impl std::error::Error for InherentConfigError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			Self::MainchainEpoch(e) => Some(e),
			Self::Coherence(e) => Some(e),
		}
	}
}

impl From<MainchainEpochConfigError> for InherentConfigError {
	fn from(e: MainchainEpochConfigError) -> Self {
		Self::MainchainEpoch(e)
	}
}

impl From<ConsensusConfigCoherenceError> for InherentConfigError {
	fn from(e: ConsensusConfigCoherenceError) -> Self {
		Self::Coherence(e)
	}
}

impl CreateInherentDataConfig {
	/// Builds the inherent-data configuration, enforcing the mainchain timing invariants.
	///
	/// The self-contained mainchain invariants (`I1`–`I4`) are checked first so the most fundamental
	/// coherence failure is reported before the cross-field check; the sidechain↔mainchain coherence
	/// invariant (`I5`) is checked second because it additionally needs the sidechain slot
	/// configuration. Running both here makes the constructor a uniform backstop on every path that
	/// builds the consensus input, including the subcommand paths that load the configuration with
	/// validation disabled.
	pub fn new(
		mc_epoch_config: MainchainEpochConfig,
		sc_slot_config: ScSlotConfig,
		time_source: Arc<dyn TimeSource + Send + Sync + 'static>,
	) -> Result<Self, InherentConfigError> {
		check_mainchain_epoch_invariants(&mc_epoch_config)?;
		check_sidechain_mainchain_coherence(&mc_epoch_config, &sc_slot_config)?;
		Ok(Self { mc_epoch_config, sc_slot_config, time_source })
	}

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

#[cfg(test)]
mod tests {
	use super::*;
	use sidechain_slots::SlotsPerEpoch;
	use sp_core::offchain::{Duration, Timestamp as OffchainTimestamp};
	use time_source::SystemTimeSource;

	fn mc_config(epoch_duration_millis: u64, slot_duration_millis: u64) -> MainchainEpochConfig {
		MainchainEpochConfig {
			epoch_duration_millis: Duration::from_millis(epoch_duration_millis),
			slot_duration_millis: Duration::from_millis(slot_duration_millis),
			first_epoch_timestamp_millis: OffchainTimestamp::from_unix_millis(1_596_059_091_000),
			first_epoch_number: 208,
			first_slot_number: 4_492_800,
		}
	}

	fn sc_config(slots_per_epoch: u32, slot_duration_millis: u64) -> ScSlotConfig {
		ScSlotConfig {
			slots_per_epoch: SlotsPerEpoch(slots_per_epoch),
			slot_duration: SlotDuration::from_millis(slot_duration_millis),
		}
	}

	#[test]
	fn new_accepts_coherent_pair() {
		// Mainchain epoch 432_000_000 ms; sidechain epoch 60 * 6000 = 360_000 ms; divides evenly.
		let cfg = CreateInherentDataConfig::new(
			mc_config(432_000_000, 1000),
			sc_config(60, 6000),
			Arc::new(SystemTimeSource),
		);
		assert!(cfg.is_ok());
	}

	#[test]
	fn new_rejects_incoherent_pair() {
		// Sidechain epoch 60 * 7000 = 420_000 ms does not divide the mainchain epoch.
		let result = CreateInherentDataConfig::new(
			mc_config(432_000_000, 1000),
			sc_config(60, 7000),
			Arc::new(SystemTimeSource),
		);
		assert!(result.is_err());
	}

	#[test]
	fn new_rejects_zero_mc_slot_duration_i2() {
		// A zero mainchain slot duration violates I2. The I5 coherence check never inspects the
		// mainchain slot duration, so before the constructor enforced I1–I4 this slipped through.
		let err = CreateInherentDataConfig::new(
			mc_config(432_000_000, 0),
			sc_config(60, 6000),
			Arc::new(SystemTimeSource),
		)
		.expect_err("zero mainchain slot duration must be rejected");
		assert_eq!(
			err,
			InherentConfigError::MainchainEpoch(MainchainEpochConfigError::SlotDurationZero)
		);
	}

	#[test]
	fn new_rejects_non_divisible_mc_epoch_slot_pair_i4() {
		// 10_000 ms epoch is not an exact multiple of a 3000 ms slot (I4). Both values are nonzero
		// and the epoch is at least 1000 ms, so I1–I3 pass. The sidechain epoch here is
		// 1 * 10_000 = 10_000 ms, which divides the mainchain epoch evenly, so the I5 coherence
		// relation would not have caught this — the I4 mainchain check is what rejects it.
		let err = CreateInherentDataConfig::new(
			mc_config(10_000, 3000),
			sc_config(1, 10_000),
			Arc::new(SystemTimeSource),
		)
		.expect_err("non-divisible mainchain epoch/slot pair must be rejected");
		assert_eq!(
			err,
			InherentConfigError::MainchainEpoch(
				MainchainEpochConfigError::EpochNotDivisibleBySlot {
					epoch_duration_millis: 10_000,
					slot_duration_millis: 3000,
				}
			)
		);
	}

	#[test]
	fn new_rejects_non_1000ms_mc_slot_duration_i6() {
		// 432_000_000 ms epoch is an exact multiple of a 2000 ms slot, so I1–I4 pass, and the
		// sidechain epoch 60 * 6000 = 360_000 ms divides the mainchain epoch so I5 passes too. Only
		// the I6 guard — enforced through the shared check_mainchain_epoch_invariants on this
		// construction path — rejects the non-1000 ms mainchain slot duration.
		let err = CreateInherentDataConfig::new(
			mc_config(432_000_000, 2000),
			sc_config(60, 6000),
			Arc::new(SystemTimeSource),
		)
		.expect_err("non-1000ms mainchain slot duration must be rejected");
		assert_eq!(
			err,
			InherentConfigError::MainchainEpoch(
				MainchainEpochConfigError::UnsupportedSlotDuration { slot_duration_millis: 2000 }
			)
		);
	}

	#[test]
	fn coherence_error_is_convertible_to_service_error() {
		// Mirrors the propagation at the service.rs construction sites: the coherence error maps
		// to a sc_service::error::Error carrying the operator-facing message.
		let err = CreateInherentDataConfig::new(
			mc_config(432_000_000, 1000),
			sc_config(60, 7000),
			Arc::new(SystemTimeSource),
		)
		.expect_err("incoherent pair must be rejected");

		let service_error =
			sc_service::Error::Other(format!("incoherent consensus timing configuration: {err}"));
		assert!(service_error.to_string().contains("incoherent consensus timing configuration"));
	}
}
