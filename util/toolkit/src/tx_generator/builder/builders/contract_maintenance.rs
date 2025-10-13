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
use midnight_node_ledger_helpers::{BuildIntent, ContractAddress, WalletSeed};
use std::{convert::Infallible, sync::Arc};

use crate::{
	builder::{
		BuildContractAction, BuildInput, BuildOutput, BuildTxs, ContractMaintenanceAuthorityInfo,
		DefaultDB, DeserializedTransactionsWithContext, IntentInfo, MaintenanceUpdateInfo,
		OfferInfo, ProofProvider, ProofType, SignatureType, TransactionWithContext, UpdateInfo,
		VerifyingKey, Wallet,
	},
	serde_def::SourceTransactions,
	tx_generator::builder::{BuildTxsExt, ContractMaintenanceArgs, CreateIntentInfo, IntentToFile},
};

#[allow(dead_code)]
pub struct ContractMaintenanceBuilder {
	committee: Vec<VerifyingKey>,
	threshold: u32,
	counter: u32,
	funding_seed: String,
	contract_address: ContractAddress,
	rng_seed: Option<[u8; 32]>,
}

impl ContractMaintenanceBuilder {
	pub fn new(args: ContractMaintenanceArgs) -> Self {
		Self {
			committee: vec![],
			threshold: args.threshold,
			counter: args.counter,
			funding_seed: args.funding_seed,
			contract_address: args.contract_address,
			rng_seed: args.rng_seed,
		}
	}
}

#[async_trait]
impl IntentToFile for ContractMaintenanceBuilder {}

impl BuildTxsExt for ContractMaintenanceBuilder {
	fn funding_seed(&self) -> WalletSeed {
		Wallet::<DefaultDB>::wallet_seed_decode(&self.funding_seed)
	}

	fn rng_seed(&self) -> Option<[u8; 32]> {
		self.rng_seed
	}
}

impl CreateIntentInfo for ContractMaintenanceBuilder {
	fn create_intent_info(&self) -> Box<dyn BuildIntent<DefaultDB>> {
		println!("Create intent info for Maintenance");

		// - Contract Calls
		let update = UpdateInfo::ReplaceAuthority(ContractMaintenanceAuthorityInfo {
			committee: vec![],
			threshold: self.threshold,
			counter: self.counter + 1,
		});

		let call_contract: Box<dyn BuildContractAction<DefaultDB>> =
			Box::new(MaintenanceUpdateInfo {
				address: self.contract_address,
				updates: vec![update],
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

#[async_trait]
impl BuildTxs for ContractMaintenanceBuilder {
	type Error = Infallible;

	async fn build_txs_from(
		&self,
		received_tx: SourceTransactions<SignatureType, ProofType>,
		prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Result<DeserializedTransactionsWithContext<SignatureType, ProofType>, Self::Error> {
		// - LedgerContext and TransactionInfo
		let (_, mut tx_info) = self.context_and_tx_info(received_tx, prover_arc);

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
