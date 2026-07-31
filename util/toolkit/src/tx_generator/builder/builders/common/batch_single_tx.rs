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
	WalletAddress, WalletSeed,
};
use super::output_spec::{ShieldedOutputSpec, UnshieldedOutputSpec};
use super::single_tx::{MAX_GUARANTEED_OUTPUTS, build_shielded_offer, build_unshielded_intents};
use async_trait::async_trait;
use futures::stream::StreamExt;
use tracing::Instrument as _;

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

/// Per-wallet-seed locks serializing transfers that touch the same wallet, so a
/// failed transfer's wallet mutations can be rolled back without racing
/// concurrent transfers built from the same wallet.
type SeedLocks = std::sync::Mutex<HashMap<WalletSeed, Arc<tokio::sync::Mutex<()>>>>;

/// Deterministic global ordering for acquiring multiple seed locks (deadlock avoidance).
fn seed_lock_order(seed: &WalletSeed) -> (u8, &[u8]) {
	match seed {
		WalletSeed::Short(bytes) => (0, bytes.as_slice()),
		WalletSeed::Medium(bytes) => (1, bytes.as_slice()),
		WalletSeed::Long(bytes) => (2, bytes.as_slice()),
	}
}

pub struct BatchSingleTxBuilder<C: BuilderContext<DefaultDB>> {
	context: Arc<C>,
	prover: Arc<dyn ProofProvider<DefaultDB>>,
	transfers: Vec<TransferSpec>,
	concurrency: usize,
	coin_selection: CoinSelectionStrategy,
	emit_partial_batch: bool,
}

impl<C: BuilderContext<DefaultDB>> BatchSingleTxBuilder<C> {
	pub fn new(
		args: BatchSingleTxArgs,
		context: Arc<C>,
		prover: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Self {
		let coin_selection = args.coin_selection;
		let emit_partial_batch = args.emit_partial_batch;
		let transfers = args.get_transfer_specs();
		let concurrency = args
			.concurrency
			.unwrap_or_else(|| std::thread::available_parallelism().unwrap().into());

		Self { context, prover, transfers, concurrency, coin_selection, emit_partial_batch }
	}

	/// Builds one transfer while holding the locks of every wallet it mutates
	/// (source and funding), and restores those wallets to their pre-transfer
	/// state if the build fails. Failures after input selection (e.g. proving
	/// or validation errors) would otherwise leave coins marked spent — and
	/// change coins pending — in the shared context for a tx that is never
	/// emitted, so later transfers built from the same wallet could depend on
	/// state that will never exist on-chain.
	async fn build_single_transfer(
		context: Arc<C>,
		prover: Arc<dyn ProofProvider<DefaultDB>>,
		spec: &TransferSpec,
		coin_selection: CoinSelectionStrategy,
		seed_locks: Arc<SeedLocks>,
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

		let mut seeds = vec![source_seed.clone()];
		if funding_seed != source_seed {
			seeds.push(funding_seed.clone());
		}
		seeds.sort_by(|a, b| seed_lock_order(a).cmp(&seed_lock_order(b)));

		let mut guards = Vec::with_capacity(seeds.len());
		let mut snapshots = Vec::with_capacity(seeds.len());
		for seed in seeds {
			let lock = seed_locks
				.lock()
				.expect("seed locks mutex poisoned")
				.entry(seed.clone())
				.or_default()
				.clone();
			guards.push(lock.lock_owned().await);
			snapshots.push((seed.clone(), context.with_wallet_from_seed(seed, |w| w.clone())));
		}

		let result = Self::build_transfer_inner(
			context.clone(),
			prover,
			spec,
			coin_selection,
			source_seed,
			funding_seed,
		)
		.await;

		if result.is_err() {
			for (seed, snapshot) in snapshots {
				context.with_wallet_from_seed(seed, |wallet| *wallet = snapshot);
			}
		}

		result
	}

	async fn build_transfer_inner(
		context: Arc<C>,
		prover: Arc<dyn ProofProvider<DefaultDB>>,
		spec: &TransferSpec,
		coin_selection: CoinSelectionStrategy,
		source_seed: WalletSeed,
		funding_seed: WalletSeed,
	) -> Result<
		TransactionWithContext<
			super::ledger_helpers_local::Signature,
			super::ledger_helpers_local::ProofMarker,
			DefaultDB,
		>,
		BatchTransferError,
	> {
		use super::type_convert::*;

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
		let seed_locks: Arc<SeedLocks> = Arc::default();
		let futures: Vec<_> = self
			.transfers
			.iter()
			.enumerate()
			.map(|(i, spec)| {
				let context = self.context.clone();
				let prover = self.prover.clone();
				let spec = spec.clone();
				let coin_selection = self.coin_selection;
				let seed_locks = seed_locks.clone();
				let index = i + 1;
				async move {
					let result = Self::build_single_transfer(
						context,
						prover,
						&spec,
						coin_selection,
						seed_locks,
					)
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
				// Tags every nested `[perf]` phase log (select/build_offer/pay_fees/prove_tx/
				// serialize, emitted from single_tx.rs/transaction.rs/tx_serialization.rs) with
				// this transfer's index, so a report script can correlate per-phase timings back
				// to a single tx even though transfers build concurrently on one thread.
				.instrument(tracing::debug_span!("transfer", index, total = num_transfers))
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
			if succeeded == 0 {
				return Err(BatchSingleTxError::AllFailed { failed });
			}
			// With --emit-partial-batch, emit every tx that built successfully instead of
			// discarding the whole batch: throwing away the successful txs because a few
			// transfers failed starves downstream load appliers. Without the flag, keep the
			// strict all-or-nothing behavior.
			if !self.emit_partial_batch {
				return Err(BatchSingleTxError::PartialFailure { succeeded, failed });
			}
			tracing::warn!(
				failed,
				total,
				succeeded,
				"Partial batch: {failed} of {total} transfers failed; emitting {succeeded} successful txs"
			);
		}

		Ok(SerializedTxBatches { batches: vec![txs] })
	}
}

#[derive(Debug, thiserror::Error)]
pub enum BatchSingleTxError {
	#[error(
		"{failed} of {} transfers failed; pass --emit-partial-batch to emit the {succeeded} successful txs instead",
		.succeeded + .failed
	)]
	PartialFailure { succeeded: usize, failed: usize },
	#[error("all {failed} transfers failed")]
	AllFailed { failed: usize },
}
