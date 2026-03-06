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

use super::build_txs_ext::{BuildTxsExt, CreateIntentInfo, IntentToFile};
use super::ledger_helpers_local::{
	BuildContractAction, BuildInput, BuildIntent, BuildOutput, ContractDeployInfo, DefaultDB,
	IntentInfo, LedgerContext, MerkleTreeContract, OfferInfo, ProofProvider,
	TransactionWithContext, UnshieldedWallet, VerifyingKey, Wallet, WalletSeed,
};
use crate::{
	serde_def::SourceTransactions,
	tx_generator::builder::{BuildTxs, ContractDeployArgs},
};
use async_trait::async_trait;
use midnight_node_ledger_helpers::fork::raw_block_data::SerializedTxBatches;
use std::{convert::Infallible, marker::PhantomData, sync::Arc};

pub struct ContractDeployBuilder {
	context: Arc<LedgerContext<DefaultDB>>,
	prover: Arc<dyn ProofProvider<DefaultDB>>,
	funding_seed: String,
	committee: Vec<VerifyingKey>,
	committee_threshold: u32,
	rng_seed: Option<[u8; 32]>,
}

impl ContractDeployBuilder {
	pub fn new(
		args: ContractDeployArgs,
		context: Arc<LedgerContext<DefaultDB>>,
		prover: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Self {
		let funding_seed = args.funding_seed;
		let rng_seed = args.rng_seed;
		let committee_threshold = args.authority_threshold;

		let mut committee_seeds: Vec<WalletSeed> = args
			.authority_seeds
			.iter()
			.map(|s| super::type_convert::convert_wallet_seed(*s))
			.collect();

		// Set the funding seed as the committee if none is passed
		if committee_seeds.is_empty() {
			committee_seeds = vec![Wallet::<DefaultDB>::wallet_seed_decode(&funding_seed)];
		}

		let committee: Vec<_> = committee_seeds
			.iter()
			.map(|s| UnshieldedWallet::default(*s).signing_key().verifying_key().clone())
			.collect();

		let committee_threshold = committee_threshold.unwrap_or_else(|| committee.len() as u32);

		Self { context, prover, funding_seed, committee, committee_threshold, rng_seed }
	}
}

#[async_trait]
impl IntentToFile for ContractDeployBuilder {}

impl BuildTxsExt for ContractDeployBuilder {
	fn funding_seed(&self) -> WalletSeed {
		Wallet::<DefaultDB>::wallet_seed_decode(&self.funding_seed)
	}

	fn rng_seed(&self) -> Option<[u8; 32]> {
		self.rng_seed
	}

	fn context(&self) -> &Arc<LedgerContext<DefaultDB>> {
		&self.context
	}

	fn prover(&self) -> &Arc<dyn ProofProvider<DefaultDB>> {
		&self.prover
	}
}

impl CreateIntentInfo for ContractDeployBuilder {
	fn create_intent_info(&self) -> Box<dyn BuildIntent<DefaultDB>> {
		log::info!("Create intent info for contract deploy");
		let deploy_contract: Box<dyn BuildContractAction<DefaultDB>> =
			Box::new(ContractDeployInfo {
				type_: MerkleTreeContract::new(),
				committee: self.committee.clone(),
				committee_threshold: self.committee_threshold,
				_marker: PhantomData,
			});

		let actions: Vec<Box<dyn BuildContractAction<DefaultDB>>> = vec![deploy_contract];

		// - Intents
		let intent_info = IntentInfo {
			guaranteed_unshielded_offer: None,
			fallible_unshielded_offer: None,
			actions,
		};

		Box::new(intent_info)
	}
}

#[async_trait]
impl BuildTxs for ContractDeployBuilder {
	type Error = Infallible;

	async fn build_txs_from(
		&self,
		_received_tx: SourceTransactions,
	) -> Result<SerializedTxBatches, Self::Error> {
		// - LedgerContext and TransactionInfo
		let (_, mut tx_info) = self.context_and_tx_info();

		// - Intents
		let intent_info = self.create_intent_info();
		tx_info.add_intent(1, intent_info);

		//   - Input
		let inputs_info: Vec<Box<dyn BuildInput<DefaultDB>>> = vec![];

		//   - Output
		let outputs_info: Vec<Box<dyn BuildOutput<DefaultDB>>> = vec![];

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
