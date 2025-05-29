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
use std::{collections::HashMap, sync::Arc};

use crate::builder::{
	BuildContractAction, BuildInput, BuildOutput, BuildTxs, BuilderArgs, ContractAddress,
	ContractMaintenanceAuthorityInfo, DefaultDB, DeserializedTransactionsWithContext, FEE_TOKEN,
	FromContext, HashOutput, InputInfo, IntentInfo, LedgerContext, MaintenanceUpdateInfo,
	OfferInfo, OutputInfo, ProofProvider, ProofType, SignatureType, StandardTrasactionInfo,
	TransactionWithContext, UpdateInfo, VerifyingKey, Wallet,
};

#[allow(dead_code)]
pub struct ContractMaintenanceBuilder {
	committee: Vec<VerifyingKey>,
	threshold: u32,
	counter: u32,
	funding_seed: String,
	contract_address: String,
	rng_seed: Option<[u8; 32]>,
}

impl ContractMaintenanceBuilder {
	pub fn new(args: BuilderArgs) -> Self {
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
impl BuildTxs for ContractMaintenanceBuilder {
	async fn build_txs_from(
		&self,
		received_tx: DeserializedTransactionsWithContext<SignatureType, ProofType>,
		prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Result<
		DeserializedTransactionsWithContext<SignatureType, ProofType>,
		Box<dyn std::error::Error>,
	> {
		// - Calculate the funding `WalletSeed` (can be more than one)
		let funding_seed = Wallet::<DefaultDB>::wallet_seed_decode(&self.funding_seed);
		let inputs_wallet_seeds = vec![funding_seed];

		// initialize `LedgerContext` with the wallets
		let context = LedgerContext::new_from_wallet_seeds(&inputs_wallet_seeds);

		// update the context applying all existing previous txs queried from source (either genesis or live network)
		let previous_txs = received_tx.flat();
		context.update_from_txs(previous_txs);

		let context_arc = Arc::new(context);

		// - Transaction info
		let mut tx_info = StandardTrasactionInfo::new_from_context(
			context_arc.clone(),
			prover_arc.clone(),
			self.rng_seed,
		);

		// - Contract Calls
		let update = UpdateInfo::ReplaceAuthority(ContractMaintenanceAuthorityInfo {
			committee: vec![],
			threshold: self.threshold,
			counter: self.counter + 1,
		});

		let contract_address = self.contract_address(&self.contract_address);

		let call_contract: Box<dyn BuildContractAction<DefaultDB> + Send> =
			Box::new(MaintenanceUpdateInfo {
				address: ContractAddress(HashOutput(contract_address)),
				updates: vec![update],
				counter: self.counter,
			});

		let actions: Vec<Box<dyn BuildContractAction<DefaultDB> + Send>> = vec![call_contract];

		// - Intents
		let mut intents = HashMap::new();
		let intent_info = IntentInfo {
			guaranteed_unshielded_offer: None,
			fallible_unshielded_offer: None,
			actions,
		};
		intents.insert(1, intent_info);

		tx_info.set_intents(intents);

		// - Offer to pay for fees
		let fees = 1_300_000;
		//   - Input
		let input_info = InputInfo { origin: funding_seed, token_type: FEE_TOKEN, value: fees };
		let inputs_info: Vec<Box<dyn BuildInput<DefaultDB> + Send>> = vec![Box::new(input_info)];

		//   - Output
		let funding_wallet = context_arc.clone().wallet_from_seed(funding_seed);
		let already_spent = input_info.min_match_coin(&funding_wallet.shielded.state).value;
		let remaining_coins = already_spent - fees;

		let output_info =
			OutputInfo { destination: funding_seed, token_type: FEE_TOKEN, value: remaining_coins };
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
