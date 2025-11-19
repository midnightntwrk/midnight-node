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

use crate::FederatedAuthorityObservationDataSource;
use midnight_primitives_federated_authority_observation::{
	AuthorityMemberPublicKey, FederatedAuthorityData, FederatedAuthorityObservationConfig,
	ed25519_to_mainchain_member,
};
use sidechain_domain::McBlockHash;
use sp_core::sr25519::Public;
use sp_keyring::{Ed25519Keyring, Sr25519Keyring};

#[derive(Clone, Debug, Default)]
pub struct FederatedAuthorityObservationDataSourceMock;

impl FederatedAuthorityObservationDataSourceMock {
	pub fn new() -> Self {
		Self
	}
}

#[async_trait::async_trait]
impl FederatedAuthorityObservationDataSource for FederatedAuthorityObservationDataSourceMock {
	async fn get_federated_authority_data(
		&self,
		_config: &FederatedAuthorityObservationConfig,
		mc_block_hash: &McBlockHash,
	) -> Result<FederatedAuthorityData, Box<dyn std::error::Error + Send + Sync>> {
		// Council members - using Sr25519 for authority keys and Ed25519 for mainchain identifiers
		let dave_sr25519: Public = Sr25519Keyring::Dave.public();
		let dave = AuthorityMemberPublicKey(dave_sr25519.0.to_vec());
		let dave_mainchain = ed25519_to_mainchain_member(Ed25519Keyring::Dave.public());

		let eve_sr25519: Public = Sr25519Keyring::Eve.public();
		let eve = AuthorityMemberPublicKey(eve_sr25519.0.to_vec());
		let eve_mainchain = ed25519_to_mainchain_member(Ed25519Keyring::Eve.public());

		let ferdie_sr25519: Public = Sr25519Keyring::Ferdie.public();
		let ferdie = AuthorityMemberPublicKey(ferdie_sr25519.0.to_vec());
		let ferdie_mainchain = ed25519_to_mainchain_member(Ed25519Keyring::Ferdie.public());

		// Technical committee members - using Sr25519 for authority keys and Ed25519 for mainchain identifiers
		let alice_sr25519: Public = Sr25519Keyring::Alice.public();
		let alice = AuthorityMemberPublicKey(alice_sr25519.0.to_vec());
		let alice_mainchain = ed25519_to_mainchain_member(Ed25519Keyring::Alice.public());

		let bob_sr25519: Public = Sr25519Keyring::Bob.public();
		let bob = AuthorityMemberPublicKey(bob_sr25519.0.to_vec());
		let bob_mainchain = ed25519_to_mainchain_member(Ed25519Keyring::Bob.public());

		let charlie_sr25519: Public = Sr25519Keyring::Charlie.public();
		let charlie = AuthorityMemberPublicKey(charlie_sr25519.0.to_vec());
		let charlie_mainchain = ed25519_to_mainchain_member(Ed25519Keyring::Charlie.public());

		Ok(FederatedAuthorityData {
			council_authorities: vec![
				(dave, dave_mainchain),
				(eve, eve_mainchain),
				(ferdie, ferdie_mainchain),
			],
			technical_committee_authorities: vec![
				(alice, alice_mainchain),
				(bob, bob_mainchain),
				(charlie, charlie_mainchain),
			],
			mc_block_hash: mc_block_hash.clone(),
		})
	}
}
