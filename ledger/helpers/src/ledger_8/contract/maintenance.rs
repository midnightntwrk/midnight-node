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

//! Contract maintenance module.

use std::sync::Arc;

use async_trait::async_trait;

use super::BuildContractAction;
use crate::ledger_8::{
	BuilderContext, ContractAddress, ContractMaintenanceAuthority, ContractOperationVersion,
	ContractOperationVersionedVerifierKey, DB, EntryPointBuf, Intent, MaintenanceUpdate,
	PedersenRandomness, ProofPreimageMarker, Signature, SingleUpdate, StdRng, UnshieldedWallet,
};

/// A committee member is a full [`UnshieldedWallet`] rather than a bare Schnorr key, so its
/// signature scheme (Schnorr or ledger-9 ECDSA) travels with it: the verifying key and the
/// signature are produced via the wallet's scheme-agnostic accessors.
pub struct ContractMaintenanceAuthorityInfo {
	pub new_committee: Vec<UnshieldedWallet>,
	pub threshold: u32,
	pub counter: u32,
}

pub enum UpdateInfo {
	ReplaceAuthority(ContractMaintenanceAuthorityInfo),
	VerifierKeyRemove(EntryPointBuf, ContractOperationVersion),
	VerifierKeyInsert(EntryPointBuf, ContractOperationVersionedVerifierKey),
}

pub struct MaintenanceUpdateInfo {
	pub address: ContractAddress,
	pub committee: Vec<UnshieldedWallet>,
	pub updates: Vec<UpdateInfo>,
	pub counter: u32,
}

#[async_trait]
impl<D: DB + Clone, C: BuilderContext<D>> BuildContractAction<D, C> for MaintenanceUpdateInfo {
	async fn build(
		&mut self,
		rng: &mut StdRng,
		_context: Arc<C>,
		intent: &Intent<Signature, ProofPreimageMarker, PedersenRandomness, D>,
	) -> Intent<Signature, ProofPreimageMarker, PedersenRandomness, D> {
		let updates = self
			.updates
			.iter()
			.map(|update| match update {
				UpdateInfo::ReplaceAuthority(info) => {
					SingleUpdate::ReplaceAuthority(ContractMaintenanceAuthority {
						committee: info
							.new_committee
							.iter()
							.map(|w| {
								w.maintenance_verifying_key()
									.expect("committee member must carry key material")
							})
							.collect(),
						threshold: info.threshold,
						counter: info.counter,
					})
				},
				UpdateInfo::VerifierKeyRemove(k, version) => {
					SingleUpdate::VerifierKeyRemove(k.clone(), version.clone())
				},
				UpdateInfo::VerifierKeyInsert(k, new_key) => {
					SingleUpdate::VerifierKeyInsert(k.clone(), new_key.clone())
				},
			})
			.collect();

		let mut update = MaintenanceUpdate::new(self.address, updates, self.counter);

		// Sign with existing committee. `UnshieldedWallet::sign` already returns this generation's
		// wrapped signature type (Schnorr or ECDSA), so no per-scheme wrapping is needed here.
		let data_to_sign = update.data_to_sign();
		for (idx, wallet) in self.committee.iter().enumerate() {
			let signature = wallet.sign(rng, &data_to_sign);
			update = update.add_signature(idx as u32, signature)
		}

		intent.add_maintenance_update(update)
	}
}
