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

use futures::StreamExt;
use midnight_node_runtime::opaque::Block;
use midnight_primitives_consensus_engine::{ActiveEngine, ConsensusEngineApi};
use midnight_primitives_session_info::SessionInfoApi;
use sp_api::ProvideRuntimeApi;
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

	while let Some(notification) = notifications.next().await {
		let block_hash = notification.hash;
		let api = client.runtime_api();

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

		let (key_type, engine_label) = match engine {
			ActiveEngine::Aura => (key_types::AURA, "AURA"),
			ActiveEngine::Babe => (key_types::BABE, "BABE"),
		};
		let local_keys = keystore.sr25519_public_keys(key_type);
		let committee_size = producers.len();
		let local_match = local_keys.iter().find(|local| producers.iter().any(|p| p == *local));

		match local_match {
			Some(key) => log::info!(
				target: LOG_TARGET,
				"Session {session_index}: this node IS in the committee for this session \
				 ({engine_label} key: 0x{}, committee size: {committee_size})",
				hex::encode(AsRef::<[u8]>::as_ref(key)),
			),
			None => {
				let local_hex: Vec<String> = local_keys
					.iter()
					.map(|k| format!("0x{}", hex::encode(AsRef::<[u8]>::as_ref(k))))
					.collect();
				log::info!(
					target: LOG_TARGET,
					"Session {session_index}: this node IS NOT in the committee \
					 for this session (local {engine_label} keys: {local_hex:?}, committee size: {committee_size})."
				);
			},
		}
	}
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
