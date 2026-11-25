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

//! Probes if node's keystore hold a BABE key that is registered as the `babe` key of a permissioned candidate on Cardano?

use async_trait::async_trait;
use authority_selection_inherents::{
	AuthoritySelectionDataSource, AuthoritySelectionInputs, CommitteeMember,
};
use midnight_node_runtime::{
	CrossChainPublic,
	opaque::{Block, SessionKeys},
};
use sidechain_domain::{
	McEpochNumber, PermissionedCandidateData, ScEpochNumber,
	mainchain_epoch::{MainchainEpochConfig, MainchainEpochDerivation, Timestamp},
};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_core::{crypto::key_types::BABE, sr25519};
use sp_keystore::{Keystore, KeystorePtr};
use sp_session_validator_management::SessionValidatorManagementApi;
use std::sync::Arc;
use time_source::TimeSource;

use super::LOG_TARGET;

/// Error type of the probe and its data sources.
pub type ProbeError = Box<dyn std::error::Error + Send + Sync>;

/// The permissioned candidates in effect on Cardano.
#[async_trait]
pub trait PermissionedCandidatesDataSource: Send + Sync {
	/// Returns the permissioned candidates in effect for the current main chain
	/// epoch. An empty vector means no list is set on the main chain.
	async fn permissioned_candidates(&self) -> Result<Vec<PermissionedCandidateData>, ProbeError>;
}

/// The datasource for probe
pub struct ProbeCandidatesDataSource<C> {
	client: Arc<C>,
	data_source: Arc<dyn AuthoritySelectionDataSource + Send + Sync>,
	mc_epoch_config: MainchainEpochConfig,
	time_source: Arc<dyn TimeSource + Send + Sync>,
}

impl<C> ProbeCandidatesDataSource<C> {
	/// Constructor.
	pub fn new(
		client: Arc<C>,
		data_source: Arc<dyn AuthoritySelectionDataSource + Send + Sync>,
		mc_epoch_config: MainchainEpochConfig,
		time_source: Arc<dyn TimeSource + Send + Sync>,
	) -> Self {
		Self { client, data_source, mc_epoch_config, time_source }
	}

	fn current_mc_epoch(&self) -> Result<McEpochNumber, ProbeError> {
		let now = Timestamp::from_unix_millis(self.time_source.get_current_time_millis());
		Ok(self.mc_epoch_config.timestamp_to_mainchain_epoch(now)?)
	}
}

#[async_trait]
impl<C> PermissionedCandidatesDataSource for ProbeCandidatesDataSource<C>
where
	C: ProvideRuntimeApi<Block> + HeaderBackend<Block> + Send + Sync + 'static,
	C::Api: SessionValidatorManagementApi<
			Block,
			CommitteeMember<CrossChainPublic, SessionKeys>,
			AuthoritySelectionInputs,
			ScEpochNumber,
		>,
{
	async fn permissioned_candidates(&self) -> Result<Vec<PermissionedCandidateData>, ProbeError> {
		let epoch = self.current_mc_epoch()?;
		// Read the policy IDs from the best block. Scoped so the (non-`Send`)
		// runtime API handle is dropped before the await below.
		let scripts = {
			let best_hash = self.client.info().best_hash;
			self.client.runtime_api().get_main_chain_scripts(best_hash)?
		};

		let parameters = self
			.data_source
			.get_ariadne_parameters(
				epoch,
				scripts.d_parameter_policy_id,
				scripts.permissioned_candidates_policy_id,
			)
			.await?;

		match parameters.permissioned_candidates {
			Some(candidates) => Ok(candidates),
			None => Ok(Vec::new()),
		}
	}
}

/// Reports which of this node's BABE keys are registered on Cardano.
pub struct BabeKeyProbe {
	candidates: Arc<dyn PermissionedCandidatesDataSource>,
	keystore: KeystorePtr,
}

impl BabeKeyProbe {
	/// Constructor. `keystore` must be the plain node keystore, not any wrapper with Aura fallback
	pub fn new(
		candidates: Arc<dyn PermissionedCandidatesDataSource>,
		keystore: KeystorePtr,
	) -> Self {
		Self { candidates, keystore }
	}

	/// The node's local BABE keys that are also registered as the `babe` key of a permissioned candidate on Cardano.
	pub async fn matching_babe_keys(&self) -> Result<Vec<sr25519::Public>, ProbeError> {
		let local_keys = self.keystore.sr25519_public_keys(BABE);
		let candidates = self.candidates.permissioned_candidates().await?;
		let babe_key_on_cardano = extract_babe_keys(&candidates);

		log::debug!(
			target: LOG_TARGET,
			"{} local BABE key(s) in keystore, {} of {} permissioned candidate(s) registered a valid BABE key",
			local_keys.len(),
			babe_key_on_cardano.len(),
			candidates.len(),
		);

		Ok(local_keys.into_iter().filter(|key| babe_key_on_cardano.contains(key)).collect())
	}
}

fn extract_babe_keys(candidates: &[PermissionedCandidateData]) -> Vec<sr25519::Public> {
	candidates
		.iter()
		.filter_map(|candidate| {
			let bytes = candidate.keys.find(BABE)?;
			match <[u8; 32]>::try_from(bytes.as_slice()) {
				Ok(raw) => Some(sr25519::Public::from_raw(raw)),
				Err(_) => {
					log::warn!(
						target: LOG_TARGET,
						"Permissioned candidate 0x{} has an invalid 'babe' key of {} bytes",
						hex::encode(&candidate.sidechain_public_key.0),
						bytes.len(),
					);
					None
				},
			}
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;
	use sidechain_domain::{CandidateKey, CandidateKeys, SidechainPublicKey};
	use sp_core::crypto::key_types::{AURA, GRANDPA};
	use sp_keystore::testing::MemoryKeystore;

	/// Static [`PermissionedCandidatesDataSource`] mock.
	struct MockCandidates(Result<Vec<PermissionedCandidateData>, String>);

	#[async_trait]
	impl PermissionedCandidatesDataSource for MockCandidates {
		async fn permissioned_candidates(
			&self,
		) -> Result<Vec<PermissionedCandidateData>, ProbeError> {
			self.0.clone().map_err(|e| e.into())
		}
	}

	fn candidate(keys: Vec<CandidateKey>) -> PermissionedCandidateData {
		PermissionedCandidateData {
			sidechain_public_key: SidechainPublicKey(vec![1u8; 33]),
			keys: CandidateKeys(keys),
		}
	}

	fn babe_key(public: &sr25519::Public) -> CandidateKey {
		CandidateKey::new(BABE, public.0.to_vec())
	}

	fn probe(
		candidates: Result<Vec<PermissionedCandidateData>, String>,
		keystore: MemoryKeystore,
	) -> BabeKeyProbe {
		BabeKeyProbe::new(Arc::new(MockCandidates(candidates)), Arc::new(keystore))
	}

	fn keystore_with(
		key_type: sp_core::crypto::KeyTypeId,
		seed: &str,
	) -> (MemoryKeystore, sr25519::Public) {
		let keystore = MemoryKeystore::new();
		let public = keystore.sr25519_generate_new(key_type, Some(seed)).unwrap();
		(keystore, public)
	}

	#[tokio::test]
	async fn reports_local_babe_key_registered_on_cardano() {
		let (keystore, local) = keystore_with(BABE, "//Alice");
		let probe = probe(Ok(vec![candidate(vec![babe_key(&local)])]), keystore);

		assert_eq!(probe.matching_babe_keys().await.unwrap(), vec![local]);
	}

	#[tokio::test]
	async fn reports_nothing_when_keystore_has_no_babe_key() {
		let (keystore, aura) = keystore_with(AURA, "//Alice");
		// The candidate registered a BABE key equal to this node's AURA key: the
		// keystore still has no BABE key, so the node is not ready.
		let probe = probe(Ok(vec![candidate(vec![babe_key(&aura)])]), keystore);

		assert!(probe.matching_babe_keys().await.unwrap().is_empty());
	}

	#[tokio::test]
	async fn reports_nothing_when_candidate_registered_a_different_babe_key() {
		let (keystore, _local) = keystore_with(BABE, "//Alice");
		let other = sr25519::Public::from_raw([7u8; 32]);
		let probe = probe(Ok(vec![candidate(vec![babe_key(&other)])]), keystore);

		assert!(probe.matching_babe_keys().await.unwrap().is_empty());
	}

	#[tokio::test]
	async fn reports_nothing_when_candidates_registered_no_babe_key() {
		let (keystore, local) = keystore_with(BABE, "//Alice");
		// A pre-migration registration: AURA and GRANDPA only.
		let legacy = candidate(vec![
			CandidateKey::new(AURA, local.0.to_vec()),
			CandidateKey::new(GRANDPA, vec![2u8; 32]),
		]);
		let probe = probe(Ok(vec![legacy]), keystore);

		assert!(probe.matching_babe_keys().await.unwrap().is_empty());
	}

	#[tokio::test]
	async fn reports_nothing_when_no_candidate_list_is_set() {
		let (keystore, _local) = keystore_with(BABE, "//Alice");
		let probe = probe(Ok(vec![]), keystore);

		assert!(probe.matching_babe_keys().await.unwrap().is_empty());
	}

	/// A malformed on-chain key must be skipped, not abort the whole probe: the
	/// valid registration of another candidate still has to be found.
	#[tokio::test]
	async fn skips_candidates_with_malformed_babe_key() {
		let (keystore, local) = keystore_with(BABE, "//Alice");
		let malformed = candidate(vec![CandidateKey::new(BABE, vec![0xab; 16])]);
		let probe = probe(Ok(vec![malformed, candidate(vec![babe_key(&local)])]), keystore);

		assert_eq!(probe.matching_babe_keys().await.unwrap(), vec![local]);
	}

	#[tokio::test]
	async fn finds_match_among_several_local_keys() {
		let keystore = MemoryKeystore::new();
		let _first = keystore.sr25519_generate_new(BABE, Some("//Alice")).unwrap();
		let second = keystore.sr25519_generate_new(BABE, Some("//Bob")).unwrap();
		let probe = probe(Ok(vec![candidate(vec![babe_key(&second)])]), keystore);

		assert_eq!(probe.matching_babe_keys().await.unwrap(), vec![second]);
	}

	#[tokio::test]
	async fn propagates_data_source_errors() {
		let (keystore, _local) = keystore_with(BABE, "//Alice");
		let probe = probe(Err("db-sync is down".into()), keystore);

		assert!(probe.matching_babe_keys().await.is_err());
	}
}
