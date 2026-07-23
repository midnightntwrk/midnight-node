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

use std::{collections::HashMap, sync::Arc};

use super::ledger_helpers_local::{
	BuilderContext, CoinSelectionStrategy, DefaultDB, FromContext as _, ProofProvider,
	ShieldedCoinSelectionError, ShieldedTokenType, ShieldedWallet, StandardTrasactionInfo,
	TransactionWithContext, UnshieldedTokenType, UnshieldedWallet, UtxoSelectionError,
	WalletAddress,
};
use super::output_spec::{ShieldedOutputSpec, UnshieldedOutputSpec};
use super::single_tx::{MAX_GUARANTEED_OUTPUTS, build_shielded_offer, build_unshielded_intents};
use async_trait::async_trait;
use futures::stream::StreamExt;

use crate::{Progress, serde_def::SourceTransactions, tx_generator::builder::BatchSingleTxArgs};
use midnight_node_ledger_helpers::fork::raw_block_data::SerializedTxBatches;

use crate::tx_generator::builder::{BuildTxs, TransferSpec};

#[derive(Debug, thiserror::Error)]
enum BatchTransferError {
	#[error("{0}")]
	UtxoSelection(#[from] UtxoSelectionError),
	#[error("{0}")]
	ShieldedCoinSelection(#[from] ShieldedCoinSelectionError),
	#[error("proving failed: {0}")]
	ProvingFailed(String),
}

pub struct BatchSingleTxBuilder<C: BuilderContext<DefaultDB>> {
	context: Arc<C>,
	prover: Arc<dyn ProofProvider<DefaultDB>>,
	transfers: Vec<TransferSpec>,
	concurrency: usize,
	coin_selection: CoinSelectionStrategy,
}

impl<C: BuilderContext<DefaultDB>> BatchSingleTxBuilder<C> {
	pub fn new(
		args: BatchSingleTxArgs,
		context: Arc<C>,
		prover: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Self {
		let coin_selection = args.coin_selection;
		let transfers = args.get_transfer_specs();
		let concurrency = args
			.concurrency
			.unwrap_or_else(|| std::thread::available_parallelism().unwrap().into());

		Self { context, prover, transfers, concurrency, coin_selection }
	}

	async fn build_single_transfer(
		context: Arc<C>,
		prover: Arc<dyn ProofProvider<DefaultDB>>,
		spec: &TransferSpec,
		coin_selection: CoinSelectionStrategy,
	) -> Result<
		TransactionWithContext<
			super::ledger_helpers_local::Signature,
			super::ledger_helpers_local::ProofMarker,
			DefaultDB,
		>,
		BatchTransferError,
	> {
		use super::type_convert::*;

		// The scheme half of each resolved pair is applied at context build time (see
		// `Builder::relevant_wallet_schemes`); here we only need the seed value.
		let (source_seed, _) = spec.resolve_source();
		let source_seed = convert_wallet_seed(source_seed);
		let funding_seed = spec
			.resolve_funding()
			.map(|(s, _)| convert_wallet_seed(s))
			.unwrap_or(source_seed.clone());

		let rng_seed: Option<[u8; 32]> = spec.rng_seed.as_ref().map(|s| {
			let bytes = hex::decode(s).expect("invalid rng_seed hex");
			bytes.try_into().expect("rng_seed must be 32 bytes")
		});

		let dest_address: WalletAddress = convert_wallet_address(
			&spec.destination_address.parse().expect("invalid destination_address"),
		);

		let mut tx_info =
			StandardTrasactionInfo::new_from_context(context.clone(), prover, rng_seed);

		if let Some(amount) = spec.unshielded_amount {
			let hash = parse_hash_output(spec.unshielded_token_type.as_deref());
			let token_type: UnshieldedTokenType = convert_unshielded_token_type(
				midnight_node_ledger_helpers::UnshieldedTokenType(hash),
			);

			let dest_wallet: UnshieldedWallet = (&dest_address)
				.try_into()
				.expect("destination is not a valid unshielded address");

			let intents = build_unshielded_intents(
				context.clone(),
				source_seed.clone(),
				vec![UnshieldedOutputSpec { wallet: dest_wallet, amount, token_type }],
				&[],
				coin_selection,
			)
			.await?;
			tx_info.set_intents(intents);
		}

		if let Some(amount) = spec.shielded_amount {
			let hash = parse_hash_output(spec.shielded_token_type.as_deref());
			let token_type: ShieldedTokenType =
				convert_shielded_token_type(midnight_node_ledger_helpers::ShieldedTokenType(hash));

			let dest_wallet: ShieldedWallet<DefaultDB> =
				(&dest_address).try_into().expect("destination is not a valid shielded address");

			let offer = build_shielded_offer(
				context,
				source_seed,
				vec![ShieldedOutputSpec { wallet: dest_wallet, amount, token_type }],
				coin_selection,
			)?;

			if offer.outputs.len() > MAX_GUARANTEED_OUTPUTS {
				tx_info.set_fallible_offers(HashMap::from([(1, offer)]));
			} else {
				tx_info.set_guaranteed_offer(offer);
			}
		}

		tx_info.set_funding_seeds(vec![funding_seed]);
		tx_info.use_mock_proofs_for_fees(true);

		if tx_info.is_empty() {
			panic!(
				"transfer to {} is empty — must specify shielded_amount or unshielded_amount",
				spec.destination_address
			);
		}

		// Proving now self-offloads onto the blocking pool (see `ProofProvider::prove`), so await it
		// directly rather than wrapping it in a second `spawn_blocking`.
		let tx = tx_info
			.prove()
			.await
			.map_err(|e| BatchTransferError::ProvingFailed(format!("{e}")))?;

		Ok(TransactionWithContext::new(tx, None))
	}
}

fn parse_hash_output(hex_str: Option<&str>) -> midnight_node_ledger_helpers::HashOutput {
	let hex_str =
		hex_str.unwrap_or("0000000000000000000000000000000000000000000000000000000000000000");
	midnight_node_ledger_helpers::HashOutput(
		hex::decode(hex_str)
			.expect("invalid token_type hex")
			.try_into()
			.expect("token_type must be 32 bytes"),
	)
}

#[async_trait]
impl<C: BuilderContext<DefaultDB>> BuildTxs for BatchSingleTxBuilder<C> {
	type Error = BatchSingleTxError;

	async fn build_txs_from(
		&self,
		_received_tx: SourceTransactions,
	) -> Result<SerializedTxBatches, Self::Error> {
		let total = self.transfers.len();
		log::info!("Building {} transfers from batch spec...", total);

		let progress = Progress::new(total, "generating batch-single-tx transfers");

		let mut succeeded = 0usize;
		let mut failed = 0usize;

		let num_transfers = self.transfers.len();
		let futures: Vec<_> = self
			.transfers
			.iter()
			.map(|spec| {
				let context = self.context.clone();
				let prover = self.prover.clone();
				let spec = spec.clone();
				let coin_selection = self.coin_selection;
				async move {
					let result =
						Self::build_single_transfer(context, prover, &spec, coin_selection)
							.await
							.map(|tx_with_ctx| {
								let serialized = super::tx_serialization::build_single(tx_with_ctx);
								serialized
									.batches
									.into_iter()
									.next()
									.and_then(|b| b.into_iter().next())
									.expect("build_single should produce exactly one tx")
							});
					result
				}
			})
			.collect();
		let mut stream = futures::stream::iter(futures).buffered(self.concurrency);

		let mut txs = Vec::with_capacity(num_transfers);
		let mut index_iter = (1..=num_transfers).into_iter();
		while let Some(result) = stream.next().await {
			let index = index_iter.next().unwrap();
			match result {
				Ok(tx) => {
					tracing::info!(
						index = index,
						total = num_transfers,
						"Built tx {} ",
						hex::encode(tx.tx_hash)
					);
					txs.push(tx);
					succeeded += 1;
				},
				Err(e) => {
					tracing::error!(
						index = index,
						total = num_transfers,
						"Failed to build tx: {}",
						e
					);
					failed += 1;
				},
			}
			progress.inc(1);
		}

		progress.finish(format!("batch-single-tx: {} succeeded, {} failed", succeeded, failed));

		if failed > 0 {
			return Err(BatchSingleTxError::PartialFailure { succeeded, failed });
		}

		Ok(SerializedTxBatches { batches: vec![txs] })
	}
}

#[derive(Debug, thiserror::Error)]
pub enum BatchSingleTxError {
	#[error("{failed} of {} transfers failed", succeeded + failed)]
	PartialFailure { succeeded: usize, failed: usize },
}
