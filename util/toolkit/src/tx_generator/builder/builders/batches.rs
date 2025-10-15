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
	BlockContext, SerdeTransaction, ShieldedTokenType, Timestamp, UnshieldedTokenType,
};
use std::{
	collections::HashMap,
	sync::Arc,
	time::{SystemTime, UNIX_EPOCH},
};
use tokio::{sync::Semaphore, task::JoinError};

use crate::{
	Progress, Spin,
	builder::{
		BuildInput, BuildIntent, BuildOutput, BuildTxs, BuildUtxoOutput, DefaultDB,
		DeserializedTransactionsWithContext, DeserializedTransactionsWithContextBatch, FromContext,
		InputInfo, IntentInfo, LedgerContext, OfferInfo, OutputInfo, ProofProvider, ProofType,
		Segment, SignatureType, StandardTrasactionInfo, TransactionWithContext,
		UnshieldedOfferInfo, UtxoOutputInfo, UtxoSpendInfo, Wallet, WalletSeed,
	},
	serde_def::SourceTransactions,
	tx_generator::builder::BatchesArgs,
};

/// The higher the number of transactions per batch, the longer it will take to generate the
/// initial transaction. This is because the time it takes to prove a transaction increases
/// with the number of outputs in the transaction.
pub struct BatchesBuilder {
	funding_seed: String,
	num_txs_per_batch: usize,
	num_batches: usize,
	concurrency: Option<usize>,
	rng_seed: Option<[u8; 32]>,
	coin_amount: u128,
	shielded_token_type: ShieldedTokenType,
	initial_unshielded_intent_value: u128,
	unshielded_token_type: UnshieldedTokenType,
	enable_shielded: bool,
}

impl BatchesBuilder {
	pub fn new(args: BatchesArgs) -> Self {
		Self {
			funding_seed: args.funding_seed,
			num_txs_per_batch: args.num_txs_per_batch,
			num_batches: args.num_batches,
			concurrency: args.concurrency,
			rng_seed: args.rng_seed,
			coin_amount: args.coin_amount,
			shielded_token_type: args.shielded_token_type,
			initial_unshielded_intent_value: args.initial_unshielded_intent_value,
			unshielded_token_type: args.unshielded_token_type,
			enable_shielded: args.enable_shielded,
		}
	}

	fn initial_shielded_offer(
		&self,
		context: Arc<LedgerContext<DefaultDB>>,
		funding_seed: WalletSeed,
		output_wallets: Vec<WalletSeed>,
	) -> OfferInfo<DefaultDB> {
		// Input info
		let input_info = InputInfo {
			origin: funding_seed,
			token_type: self.shielded_token_type,
			value: 1_000_000_000_000_000,
		};

		let inputs_info: Vec<Box<dyn BuildInput<DefaultDB>>> = vec![Box::new(input_info)];

		// Outputs info
		let mut outputs_info: Vec<Box<dyn BuildOutput<DefaultDB>>> = output_wallets
			.iter()
			.map(|wallet_seed| {
				let output: Box<dyn BuildOutput<DefaultDB>> = Box::new(OutputInfo {
					destination: *wallet_seed,
					token_type: self.shielded_token_type,
					value: self.coin_amount,
				});
				output
			})
			.collect();

		// Calculate total coins amount required for future txs to match the spends of the funding wallet
		let total_coins_required = self.coin_amount * self.num_txs_per_batch as u128;

		let funding_wallet = context.clone().wallet_from_seed(funding_seed);
		let already_spent = input_info.min_match_coin(&funding_wallet.shielded.state).value;
		let remaining_coins = already_spent - total_coins_required;

		// Create an `Output` to its self with the remaining coins to avoid spending the whole `Input`
		let output_info_refund: Box<dyn BuildOutput<DefaultDB>> = Box::new(OutputInfo {
			destination: funding_seed,
			token_type: self.shielded_token_type,
			value: remaining_coins,
		});

		outputs_info.push(output_info_refund);

		// Offer info
		OfferInfo { inputs: inputs_info, outputs: outputs_info, transients: vec![] }
	}

	fn initial_unshielded_intents(
		&self,
		context: Arc<LedgerContext<DefaultDB>>,
		funding_seed: WalletSeed,
		output_wallets: Vec<WalletSeed>,
		amount_to_send_per_output: u128,
	) -> HashMap<u16, Box<dyn BuildIntent<DefaultDB>>> {
		let utxo_spend_info = UtxoSpendInfo {
			value: self.initial_unshielded_intent_value,
			owner: funding_seed,
			token_type: self.unshielded_token_type,
			intent_hash: None,
			output_number: None,
		};

		let funding_wallet = context.clone().wallet_from_seed(funding_seed);
		let min_match_utxo = utxo_spend_info.min_match_utxo(context, &funding_wallet);

		let input_info = Box::new(utxo_spend_info);

		// Outputs info
		let mut outputs_info: Vec<Box<dyn BuildUtxoOutput<DefaultDB>>> = output_wallets
			.iter()
			.map(|wallet_seed| {
				let output: Box<dyn BuildUtxoOutput<DefaultDB>> = Box::new(UtxoOutputInfo {
					value: amount_to_send_per_output,
					owner: *wallet_seed,
					token_type: self.unshielded_token_type,
				});
				output
			})
			.collect();

		let already_spent = min_match_utxo.value;
		let remaining_nights =
			already_spent - (amount_to_send_per_output * output_wallets.len() as u128);

		// Create an `UtxoOutput` to its self with the remaining nights to avoid spending the whole `UtxoSpend`
		let output_info_refund = Box::new(UtxoOutputInfo {
			value: remaining_nights,
			owner: funding_seed,
			token_type: self.unshielded_token_type,
		});

		if remaining_nights > 0 {
			outputs_info.push(output_info_refund);
		}

		let guaranteed_unshielded_offer_info =
			UnshieldedOfferInfo { inputs: vec![input_info], outputs: outputs_info };

		let intent_info = IntentInfo {
			guaranteed_unshielded_offer: Some(guaranteed_unshielded_offer_info),
			fallible_unshielded_offer: None,
			actions: vec![],
		};
		let boxed_intent: Box<dyn BuildIntent<DefaultDB>> = Box::new(intent_info);

		let mut intents = HashMap::new();
		intents.insert(Segment::Fallible.into(), boxed_intent);

		intents
	}
}

/// Generates `num_txs_per_batch * num_batches` txs. The txs are chained `Offer`s with 1 input and 1 output.
/// where an initital set of `num_txs_per_batch` Wallets, send funds to its +1 derivated version
/// as many times as `num_batches`.
///
/// Steps to generate txs:
/// 1. An `intitial_tx` is created to fund the set of initial Wallets.
///     - As many Wallets as `num_txs_per_batch` are created with `derivation = 0`.
///     - The Wallets are funded accounting for an initial amount to transfer `COIN_AMOUNT`
/// 2. Iterate over `num_batches` (number of chained txs between derivated Wallets):
///     - Each Wallet sends its total funds to its next +1 derivation version
///     - After each batch, the newly derived wallets will need to be updated with `all_transactions`
///       which has been updated with the previous batch txs.
#[async_trait]
impl BuildTxs for BatchesBuilder {
	type Error = JoinError;
	async fn build_txs_from(
		&self,
		received_tx: SourceTransactions<SignatureType, ProofType>,
		prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Result<DeserializedTransactionsWithContext<SignatureType, ProofType>, Self::Error> {
		let now = Timestamp::from_secs(
			SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.expect("time has run backwards")
				.as_secs(),
		);
		// --------------------------------------------------------------
		// Simulates what in the future will be the output of the YAML file based on `num_batches`
		// and `num_txs_per_batch` when https://shielded.atlassian.net/browse/PM-10459 is implemented
		// --------------------------------------------------------------
		let spin = Spin::new("generating initial tx...");
		// - Calculate the funding `WalletSeed` (can be more than one)
		let funding_seed = Wallet::<DefaultDB>::wallet_seed_decode(&self.funding_seed);
		let inputs_wallet_seeds = vec![funding_seed];

		// - Calculate `WalletSeed` to be funded
		// set the initial `wallet_seed`
		let mut wallet_seed_str =
			String::from("0000000000000000000000000000000000000000000000000000000000000010");
		let mut init_output_wallet_seeds = Vec::new();

		// Create outputs `wallet_seed` from the initial one (increments of 1)
		for _ in 0..=self.num_batches {
			for _ in 0..self.num_txs_per_batch {
				init_output_wallet_seeds
					.push(Wallet::<DefaultDB>::wallet_seed_decode(&wallet_seed_str));
				wallet_seed_str = Wallet::<DefaultDB>::increment_seed(&wallet_seed_str);
			}
		}

		// --------------------------------------------------------------
		// Build the Transaction
		// --------------------------------------------------------------
		// - First we need to generate the `LedgerContext`

		// grab the network id from the init transaction
		let network_id = received_tx.network();

		let all_wallet_seeds = [&inputs_wallet_seeds[..], &init_output_wallet_seeds[..]].concat();

		// initialize `LedgerContext` with the wallets
		let context = LedgerContext::new_from_wallet_seeds(network_id, &all_wallet_seeds);

		// update the context applying all existing previous txs queried from source (either genesis or live network)
		for block in received_tx.blocks {
			context.update_from_block(block.transactions, block.context, block.state_root.clone());
		}

		let context_arc = Arc::new(context);

		// - Transaction info
		let mut tx_info = StandardTrasactionInfo::new_from_context(
			context_arc.clone(),
			prover_arc.clone(),
			self.rng_seed,
			Some(now),
		);
		let block_context =
			BlockContext { tblock: now, tblock_err: 30, parent_block_hash: Default::default() };

		// - Initial Tx to fund the first `num_txs_per_batch` wallets of the first batch
		let first_batch_output_wallets =
			init_output_wallet_seeds[0..self.num_txs_per_batch].to_vec();

		if self.enable_shielded {
			let initial_shielded_offer_info = self.initial_shielded_offer(
				context_arc.clone(),
				funding_seed,
				first_batch_output_wallets.clone(),
			);

			tx_info.set_guaranteed_offer(initial_shielded_offer_info);
		}

		// ---------------- UNSHIELDED ------------------------
		let amount_to_send_per_output =
			self.initial_unshielded_intent_value / first_batch_output_wallets.len() as u128;

		let initial_unshielded_offer_intents = self.initial_unshielded_intents(
			context_arc.clone(),
			funding_seed,
			first_batch_output_wallets,
			amount_to_send_per_output,
		);

		tx_info.set_intents(initial_unshielded_offer_intents);

		tx_info.set_wallet_seeds(inputs_wallet_seeds.clone());
		tx_info.use_mock_proofs_for_fees(true);

		let initial_tx = tx_info.prove().await.expect("Balancing TX failed");

		let initial_tx_with_context = TransactionWithContext {
			tx: SerdeTransaction::Midnight(initial_tx),
			block_context: block_context.clone(),
		};

		context_arc.clone().update_from_tx(&initial_tx_with_context.tx, &block_context);

		spin.finish("generated initial tx.");

		// --------------------------------------------------------------
		// Setup to parallelize transactions building per batch
		// --------------------------------------------------------------
		// Progress bar setup
		let (tx_chan, rx_chan) = std::sync::mpsc::channel();

		let num_batches = self.num_batches;
		let num_txs_per_batch = self.num_txs_per_batch;

		std::thread::spawn(move || {
			let total = num_batches * num_txs_per_batch;
			let bar = Progress::new(total, "generating transactions");
			for _i in 0..total {
				let _ = rx_chan.recv().unwrap();
				bar.inc(1);
			}
			bar.finish("generated transactions");
		});

		let concurrency =
			self.concurrency.unwrap_or(std::thread::available_parallelism().unwrap().into());
		let sema = Arc::new(Semaphore::new(concurrency));

		// --------------------------------------------------------------
		// Create Transactions for each batch
		// --------------------------------------------------------------
		// The `output_wallet_seeds` vector should contain `num_txs_per_batch * num_batches` elements.
		// The first slice of size `num_txs_per_batch` from `output_wallet_seeds` will send
		// funds to the next slice, which in turn sends funds to the next, and so on.
		let mut batches = Vec::with_capacity(self.num_batches);

		for batch_num in 0..self.num_batches {
			// Indexes of the `WalletSeed` to fund the txs (inputs)
			let start_input_index = batch_num * self.num_txs_per_batch;
			let end_input_index = start_input_index + self.num_txs_per_batch;

			// Indexes of the `WalletSeed` to be funded (outputs)
			let start_output_index = end_input_index;
			let end_output_index = end_input_index + self.num_txs_per_batch;

			let input_wallet_seeds =
				init_output_wallet_seeds[start_input_index..end_input_index].to_vec();
			let output_wallet_seeds =
				init_output_wallet_seeds[start_output_index..end_output_index].to_vec();

			let tx_tasks: Vec<_> = input_wallet_seeds
				.into_iter()
				.enumerate()
				.map(|(index, seed)| {
					let sema = sema.clone();
					let tx_chan = tx_chan.clone();

					// - Transaction info
					let mut tx_info = StandardTrasactionInfo::new_from_context(
						context_arc.clone(),
						prover_arc.clone(),
						None,
						Some(now),
					);

					let input_seed = seed;
					let output_seed = output_wallet_seeds[index];

					if self.enable_shielded {
						// ---------------- SHIELDED ------------------------
						// Input info
						let input_info = InputInfo {
							origin: input_seed,
							token_type: self.shielded_token_type,
							// All funds that where intially received
							value: self.coin_amount,
						};
						let inputs_info: Vec<Box<dyn BuildInput<DefaultDB>>> =
							vec![Box::new(input_info)];

						// Output info
						let output_info = OutputInfo {
							destination: output_seed,
							token_type: self.shielded_token_type,
							value: self.coin_amount,
						};
						let outputs_info: Vec<Box<dyn BuildOutput<DefaultDB>>> =
							vec![Box::new(output_info)];

						// Offer info
						let offer_info = OfferInfo {
							inputs: inputs_info,
							outputs: outputs_info,
							transients: vec![],
						};

						tx_info.set_guaranteed_offer(offer_info);
					}

					// ---------------- UNSHIELDED ------------------------
					// We pass the whole amount `amount_to_send_per_output` from the wallet of a batch to the next one

					// Utxo Input info
					let input_info = Box::new(UtxoSpendInfo {
						value: amount_to_send_per_output,
						owner: input_seed,
						token_type: self.unshielded_token_type,
						intent_hash: None,
						output_number: None,
					});

					// Utxo Output info
					let output_info = Box::new(UtxoOutputInfo {
						value: amount_to_send_per_output,
						owner: output_seed,
						token_type: self.unshielded_token_type,
					});

					let guaranteed_unshielded_offer_info = UnshieldedOfferInfo {
						inputs: vec![input_info],
						outputs: vec![output_info],
					};

					let intent_info = IntentInfo {
						guaranteed_unshielded_offer: Some(guaranteed_unshielded_offer_info),
						fallible_unshielded_offer: None,
						actions: vec![],
					};

					tx_info.add_intent(Segment::Fallible.into(), Box::new(intent_info));

					// TODO: should the senders pay for this?
					tx_info.set_wallet_seeds(inputs_wallet_seeds.clone());
					tx_info.use_mock_proofs_for_fees(true);

					tokio::task::spawn(async move {
						let _permit = sema.acquire().await.unwrap();

						let tx = tx_info.prove().await;

						tx_chan.send(1).unwrap();

						tx
					})
				})
				.collect();

			let mut txs = Vec::with_capacity(tx_tasks.len());

			for task in tx_tasks {
				let tx = task.await?.expect("Balancing TX failed");
				let tx_with_context = TransactionWithContext {
					tx: SerdeTransaction::Midnight(tx),
					block_context: block_context.clone(),
				};
				context_arc.clone().update_from_tx(&tx_with_context.tx, &block_context);
				txs.push(tx_with_context);
			}

			let batch = DeserializedTransactionsWithContextBatch { txs };
			batches.push(batch);
		}

		Ok(DeserializedTransactionsWithContext { initial_tx: initial_tx_with_context, batches })
	}
}
