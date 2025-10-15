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
	AuthorityMemberPublicKey, FederatedAuthorityData,
};
use sidechain_domain::McBlockHash;
use sp_core::sr25519::Public;
use sp_keyring::Sr25519Keyring;

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
		mc_block_hash: &McBlockHash,
	) -> Result<FederatedAuthorityData, Box<dyn std::error::Error + Send + Sync>> {
		// Council members
		let dave_public: Public = Sr25519Keyring::Dave.public();
		let dave = AuthorityMemberPublicKey(dave_public.0.to_vec());

		let eve_public: Public = Sr25519Keyring::Eve.public();
		let eve = AuthorityMemberPublicKey(eve_public.0.to_vec());

		let ferdie_public: Public = Sr25519Keyring::Ferdie.public();
		let ferdie = AuthorityMemberPublicKey(ferdie_public.0.to_vec());

		// Technical committee members
		let alice_public: Public = Sr25519Keyring::Alice.public();
		let alice = AuthorityMemberPublicKey(alice_public.0.to_vec());

		let bob_public: Public = Sr25519Keyring::Bob.public();
		let bob = AuthorityMemberPublicKey(bob_public.0.to_vec());

		let charlie_public: Public = Sr25519Keyring::Charlie.public();
		let charlie = AuthorityMemberPublicKey(charlie_public.0.to_vec());

		Ok(FederatedAuthorityData {
			council_authorities: vec![dave, eve, ferdie],
			technical_committee_authorities: vec![alice, bob, charlie],
			mc_block_hash: mc_block_hash.clone(),
		})
	}
}
