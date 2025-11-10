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

use async_trait::async_trait;
use std::sync::Arc;

use super::super::{
	BuildContractAction, ContractAddress, ContractMaintenanceAuthority, DB, Intent, LedgerContext,
	MaintenanceUpdate, PedersenRandomness, ProofPreimageMarker, Signature, SingleUpdate, StdRng,
	VerifyingKey,
};

pub struct ContractMaintenanceAuthorityInfo {
	pub committee: Vec<VerifyingKey>,
	pub threshold: u32,
	pub counter: u32,
}

pub enum UpdateInfo {
	ReplaceAuthority(ContractMaintenanceAuthorityInfo), // TODO: the rest of Updates
}

pub struct MaintenanceUpdateInfo {
	pub address: ContractAddress,
	pub updates: Vec<UpdateInfo>,
	pub counter: u32,
}

#[async_trait]
impl<D: DB + Clone> BuildContractAction<D> for MaintenanceUpdateInfo {
	async fn build(
		&mut self,
		_rng: &mut StdRng,
		_context: Arc<LedgerContext<D>>,
		intent: &Intent<Signature, ProofPreimageMarker, PedersenRandomness, D>,
	) -> Intent<Signature, ProofPreimageMarker, PedersenRandomness, D> {
		let updates = self
			.updates
			.iter()
			.map(|update| match update {
				UpdateInfo::ReplaceAuthority(info) => {
					SingleUpdate::ReplaceAuthority(ContractMaintenanceAuthority {
						committee: info.committee.to_vec(),
						threshold: info.threshold,
						counter: info.counter,
					})
				},
			})
			.collect();

		let update = MaintenanceUpdate::new(self.address, updates, self.counter);

		intent.add_maintenance_update(update)
	}
}
