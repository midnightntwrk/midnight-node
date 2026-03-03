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

use std::{collections::VecDeque, convert::Infallible, sync::Arc};

use super::ledger_helpers_local::{
	BuildIntent, BuildUtxoOutput, BuildUtxoSpend, DefaultDB, DustRegistrationBuilder, FromContext,
	IntentInfo, LedgerContext, NIGHT, ProofProvider, Segment, StandardTrasactionInfo,
	TransactionWithContext, UnshieldedOfferInfo, UtxoOutputInfo, UtxoSpendInfo, Wallet,
};
use async_trait::async_trait;

use crate::{
	progress::Spin,
	serde_def::SourceTransactions,
	tx_generator::builder::{BuildTxs, DeregisterDustAddressArgs},
};
use midnight_node_ledger_helpers::fork::raw_block_data::SerializedTxBatches;

/// Builder for generating DUST address deregistration transactions.
///
/// This builder creates a transaction that removes the DUST address mapping
/// for a wallet from the Midnight network. The wallet's unshielded UTXOs are
/// spent back to self while the deregistration is processed.
///
/// Deregistration is useful for:
/// - Migrating to a new DUST address
/// - Cleaning up test registrations
/// - Revoking access before rotating wallet keys
pub struct DeregisterDustAddressBuilder {
	context: Arc<LedgerContext<DefaultDB>>,
	prover: Arc<dyn ProofProvider<DefaultDB>>,
	seed: String,
	rng_seed: Option<[u8; 32]>,
	funding_seed: String,
}

impl DeregisterDustAddressBuilder {
	pub fn new(
		args: DeregisterDustAddressArgs,
		context: Arc<LedgerContext<DefaultDB>>,
		prover: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Self {
		Self {
			context,
			prover,
			seed: args.wallet_seed,
			rng_seed: args.rng_seed,
			funding_seed: args.funding_seed,
		}
	}
}

#[async_trait]
impl BuildTxs for DeregisterDustAddressBuilder {
	type Error = Infallible;

	async fn build_txs_from(
		&self,
		_received_tx: SourceTransactions,
	) -> Result<SerializedTxBatches, Self::Error> {
		let spin = Spin::new("building deregister dust address transaction...");

		let seed = Wallet::<DefaultDB>::wallet_seed_decode(&self.seed);
		let funding_seed = Wallet::<DefaultDB>::wallet_seed_decode(&self.funding_seed);

		let context = self.context.clone();

		let mut tx_info = StandardTrasactionInfo::new_from_context(
			context.clone(),
			self.prover.clone(),
			self.rng_seed,
		);

		let inputs = context.with_ledger_state(|ledger_state| {
			context.with_wallet_from_seed(seed, |wallet| {
				wallet
					.unshielded_utxos(ledger_state)
					.iter()
					.filter(|utxo| utxo.type_ == NIGHT)
					.map(|utxo| UtxoSpendInfo {
						value: utxo.value,
						owner: seed,
						token_type: NIGHT,
						intent_hash: Some(utxo.intent_hash),
						output_number: Some(utxo.output_no),
					})
					.collect::<Vec<_>>()
			})
		});

		let mut outputs: VecDeque<Box<dyn BuildUtxoOutput<DefaultDB>>> = inputs
			.iter()
			.map(|input| {
				let output: Box<dyn BuildUtxoOutput<DefaultDB>> = Box::new(UtxoOutputInfo {
					value: input.value,
					owner: input.owner,
					token_type: input.token_type,
				});
				output
			})
			.collect();

		let mut inputs: VecDeque<Box<dyn BuildUtxoSpend<DefaultDB>>> = inputs
			.into_iter()
			.map(|input| {
				let input: Box<dyn BuildUtxoSpend<DefaultDB>> = Box::new(input);
				input
			})
			.collect();

		let guaranteed_inputs = inputs.pop_front().into_iter().collect();
		let guaranteed_outputs = outputs.pop_front().into_iter().collect();
		let guaranteed_unshielded_offer =
			UnshieldedOfferInfo { inputs: guaranteed_inputs, outputs: guaranteed_outputs };

		let fallible_unshielded_offer = if !inputs.is_empty() && !outputs.is_empty() {
			Some(UnshieldedOfferInfo { inputs: inputs.into(), outputs: outputs.into() })
		} else {
			None
		};
		let intent_info = IntentInfo {
			guaranteed_unshielded_offer: Some(guaranteed_unshielded_offer),
			fallible_unshielded_offer,
			actions: vec![],
		};

		let boxed_intent: Box<dyn BuildIntent<DefaultDB>> = Box::new(intent_info);
		tx_info.add_intent(Segment::Fallible.into(), boxed_intent);

		// Deregistration: pass dust_address: None instead of Some(dust_address)
		context.with_wallet_from_seed(seed, |wallet| {
			tx_info.add_dust_registration(DustRegistrationBuilder {
				signing_key: wallet.unshielded.signing_key().clone(),
				dust_address: None,
			});
		});

		tx_info.set_funding_seeds(vec![funding_seed]);
		tx_info.use_mock_proofs_for_fees(true);

		let tx = tx_info.prove().await.expect("Balancing TX failed");

		let tx_with_context = TransactionWithContext::new(tx, None);

		spin.finish("generated tx.");

		Ok(super::tx_serialization::build_single(tx_with_context))
	}
}
