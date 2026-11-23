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

//! Logs whether this validator's producing key is in the committee on each session
//! change.
//!
//! Motivated by an incident where a validator silently failed to produce blocks
//! because its keystore held the wrong AURA key. The standard logs gave no
//! indication. This task watches imported blocks, dedupes by substrate session
//! index, and emits a single INFO (in committee) or WARN (not in committee)
//! line per session.
//!
//! Which authority set to read is [`ConsensusEngineApi::active_engine`]: AURA
//! uses [`AuraApi::authorities`], BABE uses [`BabeApi::current_epoch`]. Both
//! encodings are stable; `SessionKeys` is never decoded on the native side.
//! A runtime older than `ConsensusEngineApi` is treated as AURA.
//!
//! Under BABE a committee key held only as an AURA key counts as in the committee,
//! matching the per-key AURA fallback BABE authoring uses.

use futures::StreamExt;
use midnight_node_runtime::opaque::Block;
use midnight_primitives_consensus_engine::{ActiveEngine, ConsensusEngineApi};
use midnight_primitives_session_info::SessionInfoApi;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_consensus_aura::{AuraApi, sr25519::AuthorityId as AuraId};
use sp_consensus_babe::BabeApi;
use sp_core::{crypto::key_types, sr25519};
use sp_keystore::{Keystore, KeystorePtr};
use sp_runtime::traits::Block as BlockT;
use std::sync::Arc;

const LOG_TARGET: &str = "committee-membership";

pub async fn watch<C>(client: Arc<C>, keystore: KeystorePtr)
where
	C: ProvideRuntimeApi<Block> + sc_client_api::BlockchainEvents<Block> + Send + Sync + 'static,
	C::Api:
		SessionInfoApi<Block> + ConsensusEngineApi<Block> + AuraApi<Block, AuraId> + BabeApi<Block>,
{
	let mut notifications = client.import_notification_stream();
	let mut last_session: Option<u32> = None;
	let mut noted_missing_session_info_api = false;

	while let Some(notification) = notifications.next().await {
		let block_hash = notification.hash;
		let api = client.runtime_api();

		match client.runtime_api().has_api::<dyn SessionInfoApi<Block>>(block_hash) {
			Ok(true) => {},
			Ok(false) => {
				if !noted_missing_session_info_api {
					log::info!(
						target: LOG_TARGET,
						"SessionInfoApi is not exported by the runtime at {block_hash:?}; \
						 committee membership will be reported after a runtime that includes it is enacted",
					);
					noted_missing_session_info_api = true;
				}
				continue;
			},
			Err(err) => {
				log::error!(
					target: LOG_TARGET,
					"Failed to check whether SessionInfoApi is available at {block_hash:?}: {err}",
				);
				continue;
			},
		}

		let session_index = match api.current_session_index(block_hash) {
			Ok(idx) => idx,
			Err(err) => {
				log::error!(
					target: LOG_TARGET,
					"Failed to query session index at {block_hash:?}: {err}",
				);
				continue;
			},
		};

		if last_session == Some(session_index) {
			continue;
		}
		last_session = Some(session_index);

		let engine = api.active_engine(block_hash).unwrap_or(ActiveEngine::Aura);
		let producers = match producing_authorities(&*client, block_hash, engine) {
			Ok(producers) => producers,
			Err(err) => {
				log::error!(
					target: LOG_TARGET,
					"Failed to read block producers at {block_hash:?}: {err}",
				);
				continue;
			},
		};

		let engine_label = match engine {
			ActiveEngine::Aura => "AURA",
			ActiveEngine::Babe => "BABE",
		};
		let committee_size = producers.len();

		match local_producer(&*keystore, &producers, engine) {
			Some(LocalProducer { key, via_aura_fallback }) => {
				let fallback_note =
					if via_aura_fallback { ", held as an AURA key (BABE fallback)" } else { "" };
				log::info!(
					target: LOG_TARGET,
					"Session {session_index}: this node IS in the committee for this session \
					 ({engine_label} key: 0x{}{fallback_note}, committee size: {committee_size})",
					hex::encode(AsRef::<[u8]>::as_ref(&key)),
				)
			},
			None => {
				let local_hex = |key_type| -> Vec<String> {
					keystore
						.sr25519_public_keys(key_type)
						.iter()
						.map(|k| format!("0x{}", hex::encode(AsRef::<[u8]>::as_ref(k))))
						.collect()
				};
				match engine {
					ActiveEngine::Aura => log::info!(
						target: LOG_TARGET,
						"Session {session_index}: this node IS NOT in the committee \
						 for this session (local AURA keys: {:?}, committee size: {committee_size}).",
						local_hex(key_types::AURA),
					),
					ActiveEngine::Babe => log::info!(
						target: LOG_TARGET,
						"Session {session_index}: this node IS NOT in the committee \
						 for this session (local BABE keys: {:?}, local AURA keys: {:?}, \
						 committee size: {committee_size}).",
						local_hex(key_types::BABE),
						local_hex(key_types::AURA),
					),
				}
			},
		}
	}
}

/// A committee key this node can produce blocks with.
#[derive(Debug, PartialEq, Eq)]
struct LocalProducer {
	key: sr25519::Public,
	/// The key is held under the AURA key type and BABE authors with it through the
	/// keystore fallback ([`crate::aura_to_babe_migration_keystore`]).
	via_aura_fallback: bool,
}

/// Finds the first committee key this node can sign with, mirroring how authoring
/// resolves keys.
///
/// Must be called with the *unwrapped* keystore. BABE authoring falls back to the
/// AURA key type per requested public, so a committee key held only as AURA still
/// produces blocks even when unrelated BABE keys are loaded. Listing BABE keys and
/// intersecting them with the committee misses that case (#2295).
fn local_producer(
	keystore: &dyn Keystore,
	producers: &[sr25519::Public],
	engine: ActiveEngine,
) -> Option<LocalProducer> {
	let holds = |key: &sr25519::Public, key_type| {
		keystore.has_keys(&[(AsRef::<[u8]>::as_ref(key).to_vec(), key_type)])
	};
	let (primary, fallback) = match engine {
		ActiveEngine::Aura => (key_types::AURA, None),
		ActiveEngine::Babe => (key_types::BABE, Some(key_types::AURA)),
	};
	producers
		.iter()
		.find(|key| holds(key, primary))
		.map(|key| LocalProducer { key: *key, via_aura_fallback: false })
		.or_else(|| {
			let fallback = fallback?;
			producers
				.iter()
				.find(|key| holds(key, fallback))
				.map(|key| LocalProducer { key: *key, via_aura_fallback: true })
		})
}

fn producing_authorities<C>(
	client: &C,
	block_hash: <Block as BlockT>::Hash,
	engine: ActiveEngine,
) -> Result<Vec<sr25519::Public>, sp_api::ApiError>
where
	C: ProvideRuntimeApi<Block>,
	C::Api: AuraApi<Block, AuraId> + BabeApi<Block>,
{
	let api = client.runtime_api();
	match engine {
		ActiveEngine::Aura => api
			.authorities(block_hash)
			.map(|authorities| authorities.into_iter().map(Into::into).collect()),
		ActiveEngine::Babe => api
			.current_epoch(block_hash)
			.map(|epoch| epoch.authorities.into_iter().map(|(id, _weight)| id.into()).collect()),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::crypto::KeyTypeId;
	use sp_keystore::testing::MemoryKeystore;

	fn gen_key(ks: &MemoryKeystore, key_type: KeyTypeId, seed: &str) -> sr25519::Public {
		ks.sr25519_generate_new(key_type, Some(seed)).unwrap()
	}

	fn stranger(seed: &str) -> sr25519::Public {
		use sp_core::Pair;
		sr25519::Pair::from_string(seed, None).unwrap().public()
	}

	#[test]
	fn babe_key_in_committee() {
		let ks = MemoryKeystore::new();
		let babe = gen_key(&ks, key_types::BABE, "//Babe");
		let producers = [stranger("//Other"), babe];

		assert_eq!(
			local_producer(&ks, &producers, ActiveEngine::Babe),
			Some(LocalProducer { key: babe, via_aura_fallback: false }),
		);
	}

	#[test]
	fn babe_falls_back_to_aura_key_without_babe_keys() {
		let ks = MemoryKeystore::new();
		let aura = gen_key(&ks, key_types::AURA, "//Aura");

		assert_eq!(
			local_producer(&ks, &[aura], ActiveEngine::Babe),
			Some(LocalProducer { key: aura, via_aura_fallback: true }),
		);
	}

	/// Regression for #2295: an unrelated BABE key (e.g. the next key loaded ahead of
	/// rotation) must not hide the AURA key the node is authoring with.
	#[test]
	fn babe_falls_back_to_aura_key_with_unrelated_babe_key_loaded() {
		let ks = MemoryKeystore::new();
		let aura = gen_key(&ks, key_types::AURA, "//Aura");
		let _next_babe = gen_key(&ks, key_types::BABE, "//BabeNext");

		assert_eq!(
			local_producer(&ks, &[stranger("//Other"), aura], ActiveEngine::Babe),
			Some(LocalProducer { key: aura, via_aura_fallback: true }),
		);
	}

	#[test]
	fn babe_prefers_babe_key_over_aura_fallback() {
		let ks = MemoryKeystore::new();
		let aura = gen_key(&ks, key_types::AURA, "//Aura");
		let babe = gen_key(&ks, key_types::BABE, "//Babe");

		assert_eq!(
			local_producer(&ks, &[aura, babe], ActiveEngine::Babe),
			Some(LocalProducer { key: babe, via_aura_fallback: false }),
		);
	}

	#[test]
	fn babe_not_in_committee() {
		let ks = MemoryKeystore::new();
		gen_key(&ks, key_types::AURA, "//Aura");
		gen_key(&ks, key_types::BABE, "//Babe");

		assert_eq!(local_producer(&ks, &[stranger("//Other")], ActiveEngine::Babe), None);
	}

	/// AURA authoring never uses BABE keys.
	#[test]
	fn aura_ignores_babe_keys() {
		let ks = MemoryKeystore::new();
		let babe = gen_key(&ks, key_types::BABE, "//Babe");

		assert_eq!(local_producer(&ks, &[babe], ActiveEngine::Aura), None);
	}

	#[test]
	fn aura_key_in_committee() {
		let ks = MemoryKeystore::new();
		let aura = gen_key(&ks, key_types::AURA, "//Aura");

		assert_eq!(
			local_producer(&ks, &[aura], ActiveEngine::Aura),
			Some(LocalProducer { key: aura, via_aura_fallback: false }),
		);
	}
}
