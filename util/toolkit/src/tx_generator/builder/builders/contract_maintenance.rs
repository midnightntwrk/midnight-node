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
use midnight_node_ledger_helpers::{
	BuildIntent, ContractAddress, ContractMaintenanceAuthority,
	ContractOperationVersionedVerifierKey, EntryPointBuf, SigningKey, UnshieldedWallet,
	VerifyingKey, WalletSeed, deserialize, serialize_untagged,
};
use std::{path::PathBuf, sync::Arc};

use crate::{
	builder::{
		BuildContractAction, BuildInput, BuildOutput, BuildTxs, ContractMaintenanceAuthorityInfo,
		DefaultDB, DeserializedTransactionsWithContext, IntentInfo, MaintenanceUpdateInfo,
		OfferInfo, ProofProvider, ProofType, SignatureType, TransactionWithContext, UpdateInfo,
		Wallet,
	},
	serde_def::SourceTransactions,
	tx_generator::builder::{BuildTxsExt, ContractMaintenanceArgs},
};

pub struct ContractMaintenanceBuilder {
	current_committee: Vec<SigningKey>,
	new_committee: Vec<SigningKey>,
	upsert_entrypoints: Vec<PathBuf>,
	remove_entrypoints: Vec<String>,
	threshold: Option<u32>,
	counter: u32,
	funding_seed: String,
	contract_address: ContractAddress,
	rng_seed: Option<[u8; 32]>,
}

impl ContractMaintenanceBuilder {
	pub fn new(args: ContractMaintenanceArgs) -> Self {
		let ContractMaintenanceArgs {
			funding_seed,
			authority_seeds: commitee_seeds,
			new_authority_seeds: new_commitee_seeds,
			contract_address,
			threshold,
			upsert_entrypoints,
			remove_entrypoints,
			counter,
			rng_seed,
		} = args;

		let current_committee = commitee_seeds
			.iter()
			.map(|s| UnshieldedWallet::default(*s).signing_key().clone())
			.collect();

		let new_committee = new_commitee_seeds
			.iter()
			.map(|s| UnshieldedWallet::default(*s).signing_key().clone())
			.collect();

		Self {
			current_committee,
			new_committee,
			upsert_entrypoints,
			remove_entrypoints,
			threshold,
			counter,
			funding_seed,
			contract_address,
			rng_seed,
		}
	}
}

// #[async_trait]
// impl IntentToFile for ContractMaintenanceBuilder {}

impl BuildTxsExt for ContractMaintenanceBuilder {
	fn funding_seed(&self) -> WalletSeed {
		Wallet::<DefaultDB>::wallet_seed_decode(&self.funding_seed)
	}

	fn rng_seed(&self) -> Option<[u8; 32]> {
		self.rng_seed
	}
}

impl ContractMaintenanceBuilder {
	fn create_intent_info(
		&self,
		entrypoints_to_remove: Vec<EntryPointBuf>,
		entrypoints_to_insert: Vec<(EntryPointBuf, ContractOperationVersionedVerifierKey)>,
	) -> Box<dyn BuildIntent<DefaultDB>> {
		println!("Create intent info for Maintenance");

		let mut updates = vec![];

		for entrypoint in entrypoints_to_remove {
			updates.push(UpdateInfo::VerifierKeyRemove(entrypoint));
		}

		for (entrypoint, key) in entrypoints_to_insert {
			updates.push(UpdateInfo::VerifierKeyInsert(entrypoint, key));
		}

		// - Contract Calls
		if self.new_committee.len() > 0 {
			updates.push(UpdateInfo::ReplaceAuthority(ContractMaintenanceAuthorityInfo {
				current_committee: self.current_committee.clone(),
				new_committee: self.new_committee.clone(),
				threshold: self.threshold.unwrap_or(self.new_committee.len() as u32),
				counter: self.counter + 1,
			}));
		}

		let call_contract: Box<dyn BuildContractAction<DefaultDB>> =
			Box::new(MaintenanceUpdateInfo {
				address: self.contract_address,
				updates,
				counter: self.counter,
			});

		let actions: Vec<Box<dyn BuildContractAction<DefaultDB>>> = vec![call_contract];

		// - Intents
		let intent_info = IntentInfo {
			guaranteed_unshielded_offer: None,
			fallible_unshielded_offer: None,
			actions,
		};

		Box::new(intent_info)
	}
}

#[derive(Debug, thiserror::Error)]
pub enum ContractMaintenanceBuilderError {
	#[error("committee provided {0:?} is not a subset of the contract committee {1:?}")]
	ProvidedCommitteeNotSubset(Vec<String>, Vec<String>),
	#[error(
		"not enough committee members provided. Provided {0} < Threshold {1}. Contract commitee: {2:?}"
	)]
	ThresholdMissed(usize, usize, Vec<String>),
	#[error("contract missing")]
	ContractNotPresent(ContractAddress),
	#[error("attempting to remove an entrypoint that doesn't exist")]
	RemovingMissingEntrypoint(String),
	#[error("failed to load keyfile")]
	VerifierKeyLoadError(std::io::Error),
	#[error("failed to deserialize path")]
	DeserializationError(PathBuf, std::io::Error),
	#[error("invalid key-file name - must be <entrypoint>.verifier")]
	InvalidVerifierKeyName(PathBuf),
	#[error("arguments given result in no change to contract")]
	NoChange,
}

fn check_committee(
	provided_committee: &[VerifyingKey],
	authority: &ContractMaintenanceAuthority,
) -> Result<(), ContractMaintenanceBuilderError> {
	if !provided_committee.iter().all(|c| authority.committee.contains(&c)) {
		let provided_committee_display: Vec<String> = provided_committee
			.iter()
			.map(|v| hex::encode(serialize_untagged(&v).unwrap()))
			.collect();
		let current_committee_display: Vec<String> = authority
			.committee
			.iter()
			.map(|v| hex::encode(serialize_untagged(&v).unwrap()))
			.collect();
		return Err(ContractMaintenanceBuilderError::ProvidedCommitteeNotSubset(
			provided_committee_display,
			current_committee_display,
		));
	}

	if provided_committee.len() < authority.threshold as usize {
		let current_committee_display: Vec<String> = authority
			.committee
			.iter()
			.map(|v| hex::encode(serialize_untagged(&v).unwrap()))
			.collect();
		return Err(ContractMaintenanceBuilderError::ThresholdMissed(
			provided_committee.len(),
			authority.threshold as usize,
			current_committee_display,
		));
	}

	Ok(())
}

#[async_trait]
impl BuildTxs for ContractMaintenanceBuilder {
	type Error = ContractMaintenanceBuilderError;

	async fn build_txs_from(
		&self,
		received_tx: SourceTransactions<SignatureType, ProofType>,
		prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Result<DeserializedTransactionsWithContext<SignatureType, ProofType>, Self::Error> {
		// - LedgerContext and TransactionInfo
		let (context, mut tx_info) = self.context_and_tx_info(received_tx, prover_arc);

		let contract_state = context.with_ledger_state(|ref_state| {
			Ok(ref_state
				.index(self.contract_address)
				.ok_or_else(|| {
					ContractMaintenanceBuilderError::ContractNotPresent(self.contract_address)
				})?
				.clone())
		})?;

		let provided_committee: Vec<VerifyingKey> =
			self.current_committee.iter().map(|s| s.verifying_key()).collect();

		check_committee(&provided_committee, &contract_state.maintenance_authority)?;

		// Check remove entrypoints
		let mut entrypoints_to_remove: Vec<_> = self
			.remove_entrypoints
			.iter()
			.map(|e| EntryPointBuf(e.as_bytes().into()))
			.collect();
		let existing_entrypoints: Vec<_> = contract_state.operations.keys().collect();
		for entrypoint in &entrypoints_to_remove {
			if !existing_entrypoints.contains(entrypoint) {
				return Err(ContractMaintenanceBuilderError::RemovingMissingEntrypoint(
					String::from_utf8_lossy(&entrypoint.0).to_string(),
				));
			}
		}

		let mut entrypoints_to_insert = vec![];

		for p in &self.upsert_entrypoints {
			if p.extension().map(|s| s.as_encoded_bytes()) != Some(b"verifier") {
				return Err(ContractMaintenanceBuilderError::InvalidVerifierKeyName(p.clone()));
			}
			let entrypoint = p
				.file_stem()
				.map(|e| EntryPointBuf(e.as_encoded_bytes().into()))
				.ok_or(ContractMaintenanceBuilderError::InvalidVerifierKeyName(p.clone()))?;

			let key_bytes =
				std::fs::read(&p).map_err(ContractMaintenanceBuilderError::VerifierKeyLoadError)?;

			let key: ContractOperationVersionedVerifierKey = deserialize(&mut &key_bytes[..])
				.map_err(|e| ContractMaintenanceBuilderError::DeserializationError(p.clone(), e))?;

			if existing_entrypoints.contains(&entrypoint) {
				entrypoints_to_remove.push(entrypoint.clone());
			}
			entrypoints_to_insert.push((entrypoint, key));
		}

		if entrypoints_to_remove.is_empty()
			&& entrypoints_to_insert.is_empty()
			&& self.new_committee.is_empty()
		{
			return Err(ContractMaintenanceBuilderError::NoChange);
		}

		// - Intents
		let intent_info = self.create_intent_info(entrypoints_to_remove, entrypoints_to_insert);
		tx_info.add_intent(1, intent_info);

		//   - Input
		let inputs_info: Vec<Box<dyn BuildInput<DefaultDB>>> = vec![];

		//   - Output
		let outputs_info: Vec<Box<dyn BuildOutput<DefaultDB>>> = vec![];

		let offer_info =
			OfferInfo { inputs: inputs_info, outputs: outputs_info, transients: vec![] };

		tx_info.set_guaranteed_offer(offer_info);

		tx_info.set_wallet_seeds(vec![self.funding_seed()]);
		tx_info.use_mock_proofs_for_fees(true);

		#[cfg(not(feature = "erase-proof"))]
		let tx = tx_info.prove().await.expect("Balancing TX failed");

		#[cfg(feature = "erase-proof")]
		let tx = tx_info.erase_proof().await.expect("Balancing TX failed");

		let tx_with_context = TransactionWithContext::new(tx, None);

		Ok(DeserializedTransactionsWithContext { initial_tx: tx_with_context, batches: vec![] })
	}
}
