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

//! On startup, finds this node's Cardano registration by its `crch` key and logs, for each
//! registered session key of the runtime's key types, whether the keystore holds it.
//!
//! Keystore lookups use only public key listings, which never open a key file.

use async_trait::async_trait;
use authority_selection_inherents::{
	AuthoritySelectionDataSource, AuthoritySelectionInputs, CommitteeMember,
};
use log::Level;
use midnight_node_runtime::{
	CrossChainPublic,
	opaque::{Block, SessionKeys},
};
use midnight_primitives_session_info::SessionKeysGenerationApi;
use sidechain_domain::{
	CROSS_CHAIN_KEY_TYPE_ID, CandidateRegistrations, PermissionedCandidateData, ScEpochNumber,
	mainchain_epoch::{MainchainEpochConfig, MainchainEpochDerivation, Timestamp},
};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_core::crypto::KeyTypeId;
use sp_keystore::{Keystore, KeystorePtr};
use sp_session_validator_management::SessionValidatorManagementApi;
use std::sync::Arc;
use time_source::TimeSource;

const LOG_TARGET: &str = "session-keys";

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// The chain data the check needs: the runtime's key types and the candidate lists in effect on
/// Cardano.
#[async_trait]
pub trait Registrations: Send + Sync {
	/// The runtime's session key types at the best block, or `None` when the runtime has no
	/// `SessionKeysGenerationApi`.
	fn key_types(&self) -> Result<Option<Vec<KeyTypeId>>, Error>;

	/// The permissioned candidates and the registered candidates.
	async fn get(
		&self,
	) -> Result<(Vec<PermissionedCandidateData>, Vec<CandidateRegistrations>), Error>;
}

/// [`Registrations`] from the main chain follower, for the current Cardano epoch.
pub struct MainchainRegistrations<C> {
	client: Arc<C>,
	data_source: Arc<dyn AuthoritySelectionDataSource + Send + Sync>,
	mc_epoch_config: MainchainEpochConfig,
	time_source: Arc<dyn TimeSource + Send + Sync>,
}

impl<C> MainchainRegistrations<C> {
	pub fn new(
		client: Arc<C>,
		data_source: Arc<dyn AuthoritySelectionDataSource + Send + Sync>,
		mc_epoch_config: MainchainEpochConfig,
		time_source: Arc<dyn TimeSource + Send + Sync>,
	) -> Self {
		Self { client, data_source, mc_epoch_config, time_source }
	}
}

#[async_trait]
impl<C> Registrations for MainchainRegistrations<C>
where
	C: ProvideRuntimeApi<Block> + HeaderBackend<Block> + Send + Sync + 'static,
	C::Api: SessionValidatorManagementApi<
			Block,
			CommitteeMember<CrossChainPublic, SessionKeys>,
			AuthoritySelectionInputs,
			ScEpochNumber,
		> + SessionKeysGenerationApi<Block>,
{
	fn key_types(&self) -> Result<Option<Vec<KeyTypeId>>, Error> {
		let best_hash = self.client.info().best_hash;
		let api = self.client.runtime_api();
		if !api.has_api::<dyn SessionKeysGenerationApi<Block>>(best_hash)? {
			return Ok(None);
		}
		Ok(Some(api.key_ids(best_hash)?))
	}

	async fn get(
		&self,
	) -> Result<(Vec<PermissionedCandidateData>, Vec<CandidateRegistrations>), Error> {
		let now = Timestamp::from_unix_millis(self.time_source.get_current_time_millis());
		let epoch = self.mc_epoch_config.timestamp_to_mainchain_epoch(now)?;
		// Scoped so the (non-`Send`) runtime API handle is dropped before the awaits below.
		let scripts = {
			let best_hash = self.client.info().best_hash;
			self.client.runtime_api().get_main_chain_scripts(best_hash)?
		};
		let permissioned = self
			.data_source
			.get_ariadne_parameters(
				epoch,
				scripts.d_parameter_policy_id,
				scripts.permissioned_candidates_policy_id,
			)
			.await?
			.permissioned_candidates
			.unwrap_or_default();
		let registered = self
			.data_source
			.get_candidates(epoch, scripts.committee_candidate_address)
			.await?;
		Ok((permissioned, registered))
	}
}

/// Logs the [`report`] for the runtime's session key types.
pub async fn run(registrations: impl Registrations, keystore: KeystorePtr) {
	let key_types = match registrations.key_types() {
		Ok(Some(key_types)) => key_types,
		Ok(None) => {
			log::info!(
				target: LOG_TARGET,
				"The runtime has no SessionKeysGenerationApi: session keys not checked",
			);
			return;
		},
		Err(err) => {
			log::warn!(
				target: LOG_TARGET,
				"Cannot read the runtime's session key types: {err}",
			);
			return;
		},
	};
	match registrations.get().await {
		Ok((permissioned, registered)) => {
			for (level, line) in report(&*keystore, &key_types, &permissioned, &registered) {
				log::log!(target: LOG_TARGET, level, "{line}");
			}
		},
		Err(err) => log::warn!(
			target: LOG_TARGET,
			"Cannot read the Cardano registrations to check the session keys: {err}",
		),
	}
}

/// One log line per session key of `key_types` registered for this node's `crch` key.
pub fn report(
	keystore: &dyn Keystore,
	key_types: &[KeyTypeId],
	permissioned: &[PermissionedCandidateData],
	registered: &[CandidateRegistrations],
) -> Vec<(Level, String)> {
	let own_keys = keystore.keys(CROSS_CHAIN_KEY_TYPE_ID).unwrap_or_default();
	if own_keys.is_empty() {
		return vec![(
			Level::Warn,
			"No crch key in keystore: cannot find this node's Cardano registration".into(),
		)];
	}

	let mut lines = Vec::new();
	for crch in own_keys {
		let entries = permissioned
			.iter()
			.filter(|candidate| candidate.sidechain_public_key.0 == crch)
			.map(|candidate| ("permissioned", &candidate.keys))
			.chain(
				registered
					.iter()
					.flat_map(|candidate| &candidate.registrations)
					.filter(|registration| registration.sidechain_pub_key.0 == crch)
					.map(|registration| ("registered", &registration.keys)),
			);
		let mut found = false;
		for (list, keys) in entries {
			found = true;
			for key_type in key_types {
				let Some(public) = keys.find(*key_type) else { continue };
				let (level, status) = if keystore.keys(*key_type).is_ok_and(|k| k.contains(&public))
				{
					(Level::Info, "present in keystore")
				} else {
					(Level::Warn, "MISSING from keystore")
				};
				lines.push((
					level,
					format!(
						"{list} candidate crch 0x{}: {} 0x{} {status}",
						hex::encode(&crch),
						String::from_utf8_lossy(&key_type.0),
						hex::encode(&public),
					),
				));
			}
		}
		if !found {
			lines.push((
				Level::Warn,
				format!(
					"crch 0x{} is in neither the permissioned nor the registered candidates",
					hex::encode(&crch)
				),
			));
		}
	}
	lines
}

#[cfg(test)]
mod tests {
	use super::*;
	use sc_keystore::LocalKeystore;
	use sidechain_domain::{
		CandidateKey, CandidateKeys, CrossChainPublicKey, CrossChainSignature, MainchainSignature,
		RegistrationData, SidechainPublicKey, SidechainSignature, StakePoolPublicKey, UtxoId,
		UtxoInfo,
	};
	use sp_core::crypto::key_types::{AURA, BABE, GRANDPA};
	use tempfile::TempDir;

	const KEY_TYPES: [KeyTypeId; 3] = [AURA, BABE, GRANDPA];

	struct Node {
		_dir: TempDir,
		keystore: LocalKeystore,
		crch: Vec<u8>,
		aura: Vec<u8>,
	}

	/// A keystore with a `crch` and an `aura` key, whose files are replaced by directories: the
	/// names stay listed, but every read fails, even as root.
	fn node() -> Node {
		let dir = TempDir::new().unwrap();
		let keystore = LocalKeystore::open(dir.path(), None).unwrap();
		let crch = keystore.ecdsa_generate_new(CROSS_CHAIN_KEY_TYPE_ID, None).unwrap().0.to_vec();
		let aura = keystore.sr25519_generate_new(AURA, None).unwrap().0.to_vec();
		for entry in std::fs::read_dir(dir.path()).unwrap() {
			let file = entry.unwrap().path();
			std::fs::remove_file(&file).unwrap();
			std::fs::create_dir(&file).unwrap();
			assert!(std::fs::read(&file).is_err());
		}
		Node { _dir: dir, keystore, crch, aura }
	}

	/// Registered `aura` and `babe` keys; the `babe` one is not in the keystore.
	fn keys(node: &Node) -> CandidateKeys {
		CandidateKeys(vec![
			CandidateKey::new(AURA, node.aura.clone()),
			CandidateKey::new(BABE, vec![7; 32]),
		])
	}

	fn permissioned(crch: &[u8], keys: CandidateKeys) -> PermissionedCandidateData {
		PermissionedCandidateData { sidechain_public_key: SidechainPublicKey(crch.to_vec()), keys }
	}

	fn registered(crch: &[u8], keys: CandidateKeys) -> CandidateRegistrations {
		CandidateRegistrations::new(
			StakePoolPublicKey([0; 32]),
			None,
			vec![RegistrationData {
				registration_utxo: UtxoId::default(),
				sidechain_signature: SidechainSignature(vec![]),
				mainchain_signature: MainchainSignature([0; 64]),
				cross_chain_signature: CrossChainSignature(vec![]),
				sidechain_pub_key: SidechainPublicKey(crch.to_vec()),
				cross_chain_pub_key: CrossChainPublicKey(crch.to_vec()),
				utxo_info: UtxoInfo::default(),
				tx_inputs: vec![],
				keys,
			}],
		)
	}

	fn other_crch() -> Vec<u8> {
		vec![2; 33]
	}

	fn assert_reports_own_keys(lines: &[(Level, String)], list: &str, node: &Node) {
		let aura = format!("aura 0x{}", hex::encode(&node.aura));
		let babe = format!("babe 0x{}", hex::encode([7u8; 32]));
		assert_eq!(lines.len(), 2, "{lines:?}");
		assert!(lines.iter().any(|(level, line)| *level == Level::Info
			&& line.contains(list)
			&& line.contains(&aura)
			&& line.contains("present in keystore")));
		assert!(lines.iter().any(|(level, line)| *level == Level::Warn
			&& line.contains(list)
			&& line.contains(&babe)
			&& line.contains("MISSING from keystore")));
	}

	#[test]
	fn reports_keys_of_the_own_permissioned_entry() {
		let node = node();
		let candidates = [
			permissioned(
				&other_crch(),
				CandidateKeys(vec![CandidateKey::new(GRANDPA, vec![3; 32])]),
			),
			permissioned(&node.crch, keys(&node)),
		];

		let lines = report(&node.keystore, &KEY_TYPES, &candidates, &[]);

		assert_reports_own_keys(&lines, "permissioned", &node);
	}

	#[test]
	fn reports_keys_of_the_own_registration() {
		let node = node();
		let registrations = [
			registered(&other_crch(), CandidateKeys(vec![CandidateKey::new(GRANDPA, vec![3; 32])])),
			registered(&node.crch, keys(&node)),
		];

		let lines = report(&node.keystore, &KEY_TYPES, &[], &registrations);

		assert_reports_own_keys(&lines, "registered", &node);
	}

	#[test]
	fn reports_only_key_types_of_the_runtime() {
		let node = node();
		let candidates = [permissioned(&node.crch, keys(&node))];

		let lines = report(&node.keystore, &[AURA], &candidates, &[]);

		assert_eq!(lines.len(), 1, "{lines:?}");
		assert!(lines[0].1.contains("aura"));
	}

	#[test]
	fn logs_when_the_keystore_has_no_cross_chain_key() {
		let dir = TempDir::new().unwrap();
		let keystore = LocalKeystore::open(dir.path(), None).unwrap();
		let candidates = [permissioned(&other_crch(), CandidateKeys(vec![]))];

		let lines = report(&keystore, &KEY_TYPES, &candidates, &[]);

		assert_eq!(lines.len(), 1, "{lines:?}");
		assert!(lines[0].1.contains("No crch key in keystore"));
	}

	struct Failing;

	#[async_trait]
	impl Registrations for Failing {
		fn key_types(&self) -> Result<Option<Vec<KeyTypeId>>, Error> {
			Ok(Some(KEY_TYPES.to_vec()))
		}

		async fn get(
			&self,
		) -> Result<(Vec<PermissionedCandidateData>, Vec<CandidateRegistrations>), Error> {
			Err("db-sync is down".into())
		}
	}

	// The check returns instead of failing the node.
	#[tokio::test]
	async fn returns_when_registrations_are_unavailable() {
		let dir = TempDir::new().unwrap();
		run(Failing, Arc::new(LocalKeystore::open(dir.path(), None).unwrap())).await;
	}

	struct RuntimeWithoutApi;

	#[async_trait]
	impl Registrations for RuntimeWithoutApi {
		fn key_types(&self) -> Result<Option<Vec<KeyTypeId>>, Error> {
			Ok(None)
		}

		async fn get(
			&self,
		) -> Result<(Vec<PermissionedCandidateData>, Vec<CandidateRegistrations>), Error> {
			panic!("registrations read although the runtime has no key types to check")
		}
	}

	#[tokio::test]
	async fn skips_the_check_on_a_runtime_without_the_generation_api() {
		let dir = TempDir::new().unwrap();
		run(RuntimeWithoutApi, Arc::new(LocalKeystore::open(dir.path(), None).unwrap())).await;
	}
}
