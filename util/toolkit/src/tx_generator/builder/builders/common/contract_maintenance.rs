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

use super::ledger_helpers_local::{
	BuildContractAction, BuildInput, BuildIntent, BuildOutput, BuilderContext, ContractAddress,
	ContractMaintenanceAuthority, ContractMaintenanceAuthorityInfo, ContractOperationVersion,
	ContractOperationVersionedVerifierKey, DefaultDB, EntryPointBuf, IntentInfo,
	MaintenanceUpdateInfo, OfferInfo, ProofProvider, SigningKey, TransactionWithContext,
	UnshieldedWallet, UpdateInfo, VerifyingKey, Wallet, WalletSeed, contract_operation_version_of,
	contract_operation_versioned_verifier_key, maintenance_verifying_key, serialize_untagged,
};
use async_trait::async_trait;
use std::{path::PathBuf, sync::Arc};

use super::build_txs_ext::BuildTxsExt;
use crate::{
	serde_def::SourceTransactions,
	tx_generator::builder::{BuildTxs, ContractMaintenanceArgs},
};
use midnight_node_ledger_helpers::fork::raw_block_data::SerializedTxBatches;

pub struct ContractMaintenanceBuilder<C: BuilderContext<DefaultDB>> {
	context: Arc<C>,
	prover: Arc<dyn ProofProvider<DefaultDB>>,
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

impl<C: BuilderContext<DefaultDB>> ContractMaintenanceBuilder<C> {
	pub fn new(
		args: ContractMaintenanceArgs,
		context: Arc<C>,
		prover: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Self {
		use super::type_convert::{convert_contract_address, convert_wallet_seed};

		let commitee_seeds: Vec<WalletSeed> =
			args.authority_seeds.iter().map(|s| convert_wallet_seed(s.clone())).collect();
		let new_commitee_seeds: Vec<WalletSeed> = args
			.new_authority_seeds
			.iter()
			.map(|s| convert_wallet_seed(s.clone()))
			.collect();

		let current_committee = commitee_seeds
			.iter()
			.map(|s| UnshieldedWallet::default(s.clone()).signing_key().clone())
			.collect();

		let new_committee = new_commitee_seeds
			.iter()
			.map(|s| UnshieldedWallet::default(s.clone()).signing_key().clone())
			.collect();

		Self {
			context,
			prover,
			current_committee,
			new_committee,
			upsert_entrypoints: args.upsert_entrypoints,
			remove_entrypoints: args.remove_entrypoints,
			threshold: args.threshold,
			counter: args.counter,
			funding_seed: args.funding_seed,
			contract_address: convert_contract_address(args.contract_address),
			rng_seed: args.rng_seed,
		}
	}
}

impl<C: BuilderContext<DefaultDB>> BuildTxsExt<C> for ContractMaintenanceBuilder<C> {
	fn funding_seed(&self) -> WalletSeed {
		Wallet::<DefaultDB>::wallet_seed_decode(&self.funding_seed)
	}

	fn rng_seed(&self) -> Option<[u8; 32]> {
		self.rng_seed
	}

	fn context(&self) -> &Arc<C> {
		&self.context
	}

	fn prover(&self) -> &Arc<dyn ProofProvider<DefaultDB>> {
		&self.prover
	}
}

impl<C: BuilderContext<DefaultDB>> ContractMaintenanceBuilder<C> {
	fn create_intent_info(
		&self,
		committee: Vec<SigningKey>,
		entrypoints_to_remove: Vec<(EntryPointBuf, ContractOperationVersion)>,
		entrypoints_to_insert: Vec<(EntryPointBuf, ContractOperationVersionedVerifierKey)>,
	) -> Box<dyn BuildIntent<DefaultDB, C>> {
		log::info!("Create intent info for Maintenance");

		let mut updates = vec![];

		for (entrypoint, version) in entrypoints_to_remove {
			updates.push(UpdateInfo::VerifierKeyRemove(entrypoint, version));
		}

		for (entrypoint, key) in entrypoints_to_insert {
			updates.push(UpdateInfo::VerifierKeyInsert(entrypoint, key));
		}

		// - Contract Calls
		if self.new_committee.len() > 0 {
			updates.push(UpdateInfo::ReplaceAuthority(ContractMaintenanceAuthorityInfo {
				new_committee: self.new_committee.clone(),
				threshold: self.threshold.unwrap_or(self.new_committee.len() as u32),
				counter: self.counter + 1,
			}));
		}

		let call_contract: Box<dyn BuildContractAction<DefaultDB, C>> =
			Box::new(MaintenanceUpdateInfo {
				committee,
				address: self.contract_address,
				updates,
				counter: self.counter,
			});

		let actions: Vec<Box<dyn BuildContractAction<DefaultDB, C>>> = vec![call_contract];

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
	if !provided_committee
		.iter()
		.cloned()
		.map(maintenance_verifying_key)
		.all(|c| authority.committee.contains(&c))
	{
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
impl<C: BuilderContext<DefaultDB>> BuildTxs for ContractMaintenanceBuilder<C> {
	type Error = ContractMaintenanceBuilderError;

	async fn build_txs_from(
		&self,
		_received_tx: SourceTransactions,
	) -> Result<SerializedTxBatches, Self::Error> {
		// - LedgerContext and TransactionInfo
		let (context, mut tx_info) = self.context_and_tx_info();

		let contract_state =
			context.contract_state(self.contract_address).await.ok_or_else(|| {
				ContractMaintenanceBuilderError::ContractNotPresent(self.contract_address)
			})?;

		let mut committee = self.current_committee.clone();
		let mut committee_verifying_keys: Vec<_> =
			committee.iter().map(|s| s.verifying_key()).collect();

		let funding_signing_key =
			UnshieldedWallet::default(self.funding_seed()).signing_key().clone();
		let funding_verifying_key = funding_signing_key.verifying_key();
		if !committee_verifying_keys.contains(&funding_verifying_key)
			&& contract_state
				.maintenance_authority
				.committee
				.contains(&maintenance_verifying_key(funding_verifying_key.clone()))
		{
			committee.push(funding_signing_key.clone());
			committee_verifying_keys.push(funding_verifying_key);
		}

		check_committee(&committee_verifying_keys, &contract_state.maintenance_authority)?;

		// Check remove entrypoints. The version (which slot the existing key lives in) is
		// looked up per-entrypoint rather than assumed, since on ledger 9 a key can be in
		// either the legacy `V3` (v6) or `V4` (v7) slot depending on what compiled it.
		let mut entrypoints_to_remove = vec![];
		for e in &self.remove_entrypoints {
			let entrypoint = EntryPointBuf(e.as_bytes().into());
			let op = contract_state.operations.get(&entrypoint).ok_or_else(|| {
				ContractMaintenanceBuilderError::RemovingMissingEntrypoint(e.clone())
			})?;
			entrypoints_to_remove.push((entrypoint, contract_operation_version_of(&op)));
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

			// The maintenance-update variant is version- (and, on ledger 9, key-format-)
			// dependent: pre-ledger-9 ledgers expose only `V3` (2.x key), while ledger 9
			// accepts either a legacy 2.x key (`V3`) or a 3.x/zk-stdlib-v2 key (`V4`) and
			// picks the right one by peeking the key file's tag.
			// `contract_operation_versioned_verifier_key` selects the right variant/type
			// for the active ledger generation.
			let versioned_key = contract_operation_versioned_verifier_key(key_bytes)
				.map_err(|e| ContractMaintenanceBuilderError::DeserializationError(p.clone(), e))?;

			if let Some(op) = contract_state.operations.get(&entrypoint) {
				entrypoints_to_remove
					.push((entrypoint.clone(), contract_operation_version_of(&op)));
			}
			entrypoints_to_insert.push((entrypoint, versioned_key));
		}

		if entrypoints_to_remove.is_empty()
			&& entrypoints_to_insert.is_empty()
			&& self.new_committee.is_empty()
		{
			return Err(ContractMaintenanceBuilderError::NoChange);
		}

		// - Intents
		let intent_info =
			self.create_intent_info(committee, entrypoints_to_remove, entrypoints_to_insert);
		tx_info.add_intent(1, intent_info);

		//   - Input
		let inputs_info: Vec<Box<dyn BuildInput<DefaultDB, C>>> = vec![];

		//   - Output
		let outputs_info: Vec<Box<dyn BuildOutput<DefaultDB, C>>> = vec![];

		let offer_info =
			OfferInfo { inputs: inputs_info, outputs: outputs_info, transients: vec![] };

		tx_info.set_guaranteed_offer(offer_info);

		tx_info.set_funding_seeds(vec![self.funding_seed()]);
		tx_info.use_mock_proofs_for_fees(true);

		#[cfg(not(feature = "erase-proof"))]
		let tx = tx_info.prove().await.expect("Balancing TX failed");

		#[cfg(feature = "erase-proof")]
		let tx = tx_info.erase_proof().await.expect("Balancing TX failed");

		let tx_with_context = TransactionWithContext::new(tx, None);

		Ok(super::tx_serialization::build_single(tx_with_context))
	}
}
