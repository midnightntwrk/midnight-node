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
	BuildContractAction, ContractAddress, ContractMaintenanceAuthority, ContractOperationVersion,
	ContractOperationVersionedVerifierKey, DB, EntryPointBuf, Intent, LedgerContext,
	MaintenanceUpdate, PedersenRandomness, ProofPreimageMarker, SegmentId, Signature, SigningKey,
	SingleUpdate, StdRng,
};

pub struct ContractMaintenanceAuthorityInfo {
	pub new_committee: Vec<SigningKey>,
	pub threshold: u32,
	pub counter: u32,
}

pub enum UpdateInfo {
	ReplaceAuthority(ContractMaintenanceAuthorityInfo),
	VerifierKeyRemove(EntryPointBuf),
	VerifierKeyInsert(EntryPointBuf, ContractOperationVersionedVerifierKey),
}

pub struct MaintenanceUpdateInfo {
	pub address: ContractAddress,
	pub committee: Vec<SigningKey>,
	pub updates: Vec<UpdateInfo>,
	pub counter: u32,
}

#[async_trait]
impl<D: DB + Clone> BuildContractAction<D> for MaintenanceUpdateInfo {
	async fn build(
		&mut self,
		rng: &mut StdRng,
		_context: Arc<LedgerContext<D>>,
		intent: &Intent<Signature, ProofPreimageMarker, PedersenRandomness, D>,
		_segment_id: SegmentId,
	) -> Intent<Signature, ProofPreimageMarker, PedersenRandomness, D> {
		let updates = self
			.updates
			.iter()
			.map(|update| match update {
				UpdateInfo::ReplaceAuthority(info) => {
					SingleUpdate::ReplaceAuthority(ContractMaintenanceAuthority {
						committee: info.new_committee.iter().map(|s| s.verifying_key()).collect(),
						threshold: info.threshold,
						counter: info.counter,
					})
				},
				UpdateInfo::VerifierKeyRemove(k) => {
					SingleUpdate::VerifierKeyRemove(k.clone(), ContractOperationVersion::V2)
				},
				UpdateInfo::VerifierKeyInsert(k, new_key) => {
					SingleUpdate::VerifierKeyInsert(k.clone(), new_key.clone())
				},
			})
			.collect();

		let mut update = MaintenanceUpdate::new(self.address, updates, self.counter);

		// Sign with existing committee
		let data_to_sign = update.data_to_sign();
		for (idx, key) in self.committee.iter().enumerate() {
			let signature = key.sign(rng, &data_to_sign);
			update = update.add_signature(idx as u32, signature)
		}

		intent.add_maintenance_update(update)
	}
}
