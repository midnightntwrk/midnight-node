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

//! Logs whether this validator's AURA key is in the committee on each session
//! change.
//!
//! Motivated by an incident where a validator silently failed to produce blocks
//! because its keystore held the wrong AURA key. The standard logs gave no
//! indication. This task watches imported blocks, dedupes by substrate session
//! index, and emits a single INFO (in committee) or WARN (not in committee)
//! line per session.

use futures::StreamExt;
use midnight_node_runtime::{
	committee_keys_migrated, current_committee_storage_key, decode_current_committee,
	opaque::Block, session_committee_management_storage_version_key,
};
use midnight_primitives_session_info::SessionInfoApi;
use sc_client_api::{Backend, BlockchainEvents, StorageProvider};
use sp_api::ProvideRuntimeApi;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::crypto::key_types::AURA as AURA_KEY_TYPE;
use sp_core::storage::StorageKey;
use sp_keystore::{Keystore, KeystorePtr};
use sp_session_validator_management::CommitteeMember as _;
use std::sync::Arc;

const LOG_TARGET: &str = "committee-membership";

pub async fn watch<C, B>(client: Arc<C>, keystore: KeystorePtr)
where
	C: ProvideRuntimeApi<Block>
		+ BlockchainEvents<Block>
		+ StorageProvider<Block, B>
		+ Send
		+ Sync
		+ 'static,
	B: Backend<Block>,
	C::Api: SessionInfoApi<Block>,
{
	let mut notifications = client.import_notification_stream();
	let mut last_session: Option<u32> = None;

	while let Some(notification) = notifications.next().await {
		let block_hash = notification.hash;

		let session_index = match client.runtime_api().current_session_index(block_hash) {
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

		// Read `CurrentCommittee` and the pallet's storage version straight from state, then let
		// the runtime decode the committee in the shape that version implies. Going through the
		// runtime API instead would decode with this binary's `SessionKeys`, which fails while the
		// nodes are updated but the runtime upgrade that adds the beefy key has not run yet.
		let version_bytes = match client
			.storage(block_hash, &StorageKey(session_committee_management_storage_version_key()))
		{
			Ok(bytes) => bytes.unwrap_or_default(),
			Err(err) => {
				log::error!(
					target: LOG_TARGET,
					"Failed to read session-committee-management storage version at {block_hash:?}: {err}",
				);
				continue;
			},
		};
		let committee_bytes =
			match client.storage(block_hash, &StorageKey(current_committee_storage_key())) {
				Ok(bytes) => bytes.unwrap_or_default(),
				Err(err) => {
					log::error!(
						target: LOG_TARGET,
						"Failed to read current committee at {block_hash:?}: {err}",
					);
					continue;
				},
			};

		let migrated = committee_keys_migrated(version_bytes.0.as_slice());
		let (_epoch, committee) = decode_current_committee(committee_bytes.0.as_slice(), migrated);

		let local_aura_keys: Vec<AuraId> = keystore
			.sr25519_public_keys(AURA_KEY_TYPE)
			.into_iter()
			.map(AuraId::from)
			.collect();
		let committee_aura_keys: Vec<AuraId> =
			committee.iter().map(|m| m.authority_keys().aura.clone()).collect();
		let committee_size = committee_aura_keys.len();

		let local_match = local_aura_keys
			.iter()
			.find(|local| committee_aura_keys.iter().any(|c| c == *local));

		match local_match {
			Some(key) => log::info!(
				target: LOG_TARGET,
				"Session {session_index}: this node IS in the committee for this session \
				 (AURA key: 0x{}, committee size: {committee_size})",
				hex::encode(AsRef::<[u8]>::as_ref(key)),
			),
			None => {
				let local_hex: Vec<String> = local_aura_keys
					.iter()
					.map(|k| format!("0x{}", hex::encode(AsRef::<[u8]>::as_ref(k))))
					.collect();
				log::info!(
					target: LOG_TARGET,
					"Session {session_index}: this node IS NOT in the committee \
					 for this session (local AURA keys: {local_hex:?}, committee size: {committee_size})."
				);
			},
		}
	}
}
