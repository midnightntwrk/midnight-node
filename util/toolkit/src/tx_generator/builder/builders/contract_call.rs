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
		BuildContractAction, BuildInput, BuildIntent, BuildOutput, BuildTxs, BuildTxsExt, CallInfo,
		ContractAddress, DefaultDB, DeserializedTransactionsWithContext, HashOutput, InputInfo,
		IntentInfo, IntentToFile, MerkleTreeContract, OfferInfo, OutputInfo, ProofProvider,
		ProofType, SignatureType, TransactionWithContext, Wallet, WalletSeed,
	},
	tx_generator::builder::ContractCallArgs,
	unwrapped_fee_token,
};
use async_trait::async_trait;
use std::{convert::Infallible, marker::PhantomData, sync::Arc};

const CONTRACT_INPUT: u32 = 12;

pub struct ContractCallBuilder {
	call_key: &'static str,
	funding_seed: String,
	contract_address: String,
	rng_seed: Option<[u8; 32]>,
	fee: u128,
}

impl ContractCallBuilder {
	pub fn new(args: ContractCallArgs) -> Self {
		let call_key: &'static str = Box::leak(args.call_key.into_boxed_str());

		Self {
			call_key,
			funding_seed: args.funding_seed,
			contract_address: args.contract_address,
			rng_seed: args.rng_seed,
			fee: args.fee,
		}
	}
}

#[async_trait]
impl IntentToFile for ContractCallBuilder {}

impl BuildTxsExt<Box<dyn BuildIntent<DefaultDB> + Send>> for ContractCallBuilder {
	fn funding_seed(&self) -> WalletSeed {
		Wallet::<DefaultDB>::wallet_seed_decode(&self.funding_seed)
	}

	fn rng_seed(&self) -> Option<[u8; 32]> {
		self.rng_seed
	}

	fn create_intent_info(&self) -> Box<dyn BuildIntent<DefaultDB> + Send> {
		println!("Create intent info for contract call");

		// - Contract Calls
		let contract_address = self.contract_address(&self.contract_address);

		let call_contract: Box<dyn BuildContractAction<DefaultDB> + Send> = Box::new(CallInfo {
			type_: MerkleTreeContract::new(),
			address: ContractAddress(HashOutput(contract_address)),
			key: self.call_key,
			input: Box::new(CONTRACT_INPUT),
			_marker: PhantomData,
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
impl BuildTxs for ContractCallBuilder {
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
