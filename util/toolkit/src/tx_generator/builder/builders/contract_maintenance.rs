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

use crate::{
	builder::{
		BuildContractAction, BuildInput, BuildIntent, BuildOutput, BuildTxs, BuildTxsExt,
		ContractAddress, ContractMaintenanceAuthorityInfo, DefaultDB,
		DeserializedTransactionsWithContext, HashOutput, InputInfo, IntentInfo, IntentToFile,
		MaintenanceUpdateInfo, OfferInfo, OutputInfo, ProofProvider, ProofType, SignatureType,
		TransactionWithContext, UpdateInfo, VerifyingKey, Wallet, WalletSeed,
	},
	tx_generator::builder::ContractMaintenanceArgs,
	unwrapped_fee_token,
};
use async_trait::async_trait;
use std::{convert::Infallible, sync::Arc};

#[allow(dead_code)]
pub struct ContractMaintenanceBuilder {
	committee: Vec<VerifyingKey>,
	threshold: u32,
	counter: u32,
	funding_seed: String,
	contract_address: String,
	rng_seed: Option<[u8; 32]>,
	fee: u128,
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
			fee: args.fee,
		}
	}
}

#[async_trait]
impl IntentToFile for ContractMaintenanceBuilder {}

impl BuildTxsExt<Box<dyn BuildIntent<DefaultDB> + Send>> for ContractMaintenanceBuilder {
	fn funding_seed(&self) -> WalletSeed {
		Wallet::<DefaultDB>::wallet_seed_decode(&self.funding_seed)
	}

	fn rng_seed(&self) -> Option<[u8; 32]> {
		self.rng_seed
	}

	fn create_intent_info(&self) -> Box<dyn BuildIntent<DefaultDB> + Send> {
		println!("Create intent info for Maintenance");
		let contract_address = self.contract_address(&self.contract_address);

		// - Contract Calls
		let update = UpdateInfo::ReplaceAuthority(ContractMaintenanceAuthorityInfo {
			committee: vec![],
			threshold: self.threshold,
			counter: self.counter + 1,
		});

		let call_contract: Box<dyn BuildContractAction<DefaultDB> + Send> =
			Box::new(MaintenanceUpdateInfo {
				address: ContractAddress(HashOutput(contract_address)),
				updates: vec![update],
				counter: self.counter,
			});

		let actions: Vec<Box<dyn BuildContractAction<DefaultDB> + Send>> = vec![call_contract];

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
		received_tx: DeserializedTransactionsWithContext<SignatureType, ProofType>,
		prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Result<DeserializedTransactionsWithContext<SignatureType, ProofType>, Self::Error> {
		// - LedgerContext and TransactionInfo
		let (context_arc, mut tx_info) = self.context_and_tx_info(received_tx, prover_arc);

		// - Intents
		let intent_info = self.create_intent_info();
		tx_info.add_intent(1, intent_info);

		let funding_seed = self.funding_seed();

		//   - Input
		let input_info =
			InputInfo { origin: funding_seed, token_type: unwrapped_fee_token(), value: self.fee };
		let inputs_info: Vec<Box<dyn BuildInput<DefaultDB> + Send>> = vec![Box::new(input_info)];

		//   - Output
		let funding_wallet = context_arc.clone().wallet_from_seed(funding_seed);
		let already_spent = input_info.min_match_coin(&funding_wallet.shielded.state).value;
		let remaining_coins = already_spent - self.fee;

		let output_info = OutputInfo {
			destination: funding_seed,
			token_type: unwrapped_fee_token(),
			value: remaining_coins,
		};
		let outputs_info: Vec<Box<dyn BuildOutput<DefaultDB> + Send>> = vec![Box::new(output_info)];

		let offer_info =
			OfferInfo { inputs: inputs_info, outputs: outputs_info, transients: vec![] };

		tx_info.set_guaranteed_coins(offer_info);

		#[cfg(not(feature = "erase-proof"))]
		let tx = tx_info.prove().await;

		#[cfg(feature = "erase-proof")]
		let tx = tx_info.erase_proof().await;

		let tx_with_context = TransactionWithContext::new(tx, None);

		Ok(DeserializedTransactionsWithContext { initial_tx: tx_with_context, batches: vec![] })
	}
}
