// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Offline regression tests for self-funded `register-dust-address` (issue #1896).
//!
//! Transactions are built against a hand-crafted `LedgerState` and validated/applied
//! with the real ledger code - no node or proof server needed, since registration
//! transactions carry no ZK proofs.

use async_trait::async_trait;
use midnight_node_ledger_helpers::fork::raw_block_data::SerializedTxBatches;
use midnight_node_ledger_helpers::{
	CostModel, DefaultDB, HashOutput, IntentHash, KeyLocation, LedgerContext, NIGHT,
	PedersenRandomness, ProofMarker, ProofPreimage, ProofPreimageMarker, ProvingKeyMaterial,
	Resolver, ResolverTrait, Signature, Sp, StdRng, Timestamp, Transaction, TransactionResult,
	UserAddress, Utxo, WalletSeed, WellFormedStrictness, deserialize, make_block_context,
	mn_ledger::structure::UtxoMeta,
	onchain_runtime::context::BlockContext,
	transient_crypto::commitment::PureGeneratorPedersen,
	transient_crypto::proofs::{Proof, ProvingProvider},
};
use midnight_node_toolkit::{
	serde_def::SourceTransactions,
	tx_generator::builder::{
		BuildTxs, RegisterDustAddressArgs,
		builders::ledger_9::{RegisterDustAddressBuilder, RegisterDustAddressError},
	},
};
use std::sync::Arc;

const WALLET_SEED_HEX: &str = "0000000000000000000000000000000000000000000000000000000000000042";
/// Mirrors the funding pattern from issue #1896 (4 × 2 NIGHT).
const UTXO_VALUE: u128 = 2_000_000_000;
const GENESIS_TIME_SECS: u64 = 1_754_395_200;

/// `ProvingProvider` for proof-free transactions; fails if proving is ever attempted.
#[derive(Clone)]
struct NoProofs;

impl ProvingProvider for NoProofs {
	async fn check(&self, _preimage: &ProofPreimage) -> Result<Vec<Option<usize>>, anyhow::Error> {
		Ok(vec![])
	}
	async fn prove(
		self,
		_preimage: &ProofPreimage,
		_overwrite_binding_input: Option<midnight_node_ledger_helpers::Fr>,
	) -> Result<Proof, anyhow::Error> {
		anyhow::bail!("register-dust-address transactions must not contain proofs")
	}
	fn split(&mut self) -> Self {
		self.clone()
	}
	fn resolver(&self) -> &impl ResolverTrait {
		self
	}
}

impl ResolverTrait for NoProofs {
	async fn resolve_key(&self, _key: KeyLocation) -> std::io::Result<Option<ProvingKeyMaterial>> {
		Ok(None)
	}
}

struct NoProofsProvider;

#[async_trait]
impl midnight_node_ledger_helpers::ProofProvider<DefaultDB> for NoProofsProvider {
	async fn prove(
		&self,
		tx: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>,
		_rng: StdRng,
		_resolver: &'static Resolver,
		cost_model: CostModel,
	) -> Transaction<Signature, ProofMarker, PedersenRandomness, DefaultDB> {
		// The ledger-9 proving future is !Send (see `LocalProofServer`), so drive it on
		// a blocking-pool thread.
		tokio::task::spawn_blocking(move || {
			futures::executor::block_on(tx.prove(NoProofs, &cost_model))
				.expect("proving of a proof-free tx failed")
		})
		.await
		.expect("proving task panicked")
	}
}

struct TestSetup {
	ctx: Arc<LedgerContext<DefaultDB>>,
	user_address: UserAddress,
	utxos: Vec<Utxo>,
	block_context: BlockContext,
}

/// Build a context whose wallet holds `num_utxos` NIGHT UTXOs created at genesis time,
/// with the chain tip `elapsed_secs` later (retroactive DUST accrues over that interval).
fn setup(num_utxos: u8, elapsed_secs: u64) -> TestSetup {
	let seed = WalletSeed::try_from_hex_str(WALLET_SEED_HEX).unwrap();
	let ctx =
		Arc::new(LedgerContext::<DefaultDB>::new_from_wallet_seeds("undeployed", &[seed.clone()]));
	let user_address = ctx.with_wallet_from_seed(seed, |wallet| wallet.unshielded.user_address);

	let ctime = Timestamp::from_secs(GENESIS_TIME_SECS);
	let utxos: Vec<Utxo> = (0..num_utxos)
		.map(|i| Utxo {
			value: UTXO_VALUE,
			owner: user_address,
			type_: NIGHT,
			intent_hash: IntentHash(HashOutput([i + 1; 32])),
			output_no: 0,
		})
		.collect();

	ctx.with_ledger_state(|state| {
		let mut new_state = (**state).clone();
		let mut utxo_state = (*new_state.utxo).clone();
		for utxo in &utxos {
			utxo_state.utxos = utxo_state.utxos.insert(utxo.clone(), UtxoMeta { ctime });
		}
		new_state.utxo = Sp::new(utxo_state);
		// Keep the NIGHT supply invariant: the injected UTXOs come out of the reserve.
		new_state.reserve_pool -= UTXO_VALUE * num_utxos as u128;
		*state = Sp::new(new_state);
	});

	let now = Timestamp::from_secs(GENESIS_TIME_SECS + elapsed_secs);
	let block_context = make_block_context(now, HashOutput([9u8; 32]), now);
	*ctx.latest_block_context.lock().unwrap() = Some(block_context.clone());

	TestSetup { ctx, user_address, utxos, block_context }
}

/// Mark a UTXO as already backing DUST generation, as it would after a prior registration.
/// Only `night_indices` is populated - enough for the availability checks on both sides,
/// but not for applying a transaction that spends the UTXO.
fn mark_backing_generation(ctx: &LedgerContext<DefaultDB>, utxo: &Utxo) {
	ctx.with_ledger_state(|state| {
		let mut new_state = (**state).clone();
		let mut dust_state = (*new_state.dust).clone();
		dust_state.generation.night_indices =
			dust_state.generation.night_indices.insert(utxo.initial_nonce(), 0);
		new_state.dust = Sp::new(dust_state);
		*state = Sp::new(new_state);
	});
}

fn self_funded_args() -> RegisterDustAddressArgs {
	RegisterDustAddressArgs {
		wallet_seed: WALLET_SEED_HEX.parse().unwrap(),
		funding_seed: None,
		destination_dust: None,
		rng_seed: Some([7u8; 32]),
	}
}

async fn build_registration(
	setup: &TestSetup,
) -> Result<SerializedTxBatches, RegisterDustAddressError> {
	RegisterDustAddressBuilder::new(
		self_funded_args(),
		setup.ctx.clone(),
		Arc::new(NoProofsProvider),
	)
	.build_txs_from(SourceTransactions::new(vec![], "undeployed"))
	.await
}

/// Validate and apply the built transaction with the real ledger, and assert the wallet's
/// dust address ends up registered.
fn assert_applies_and_registers(setup: &TestSetup, batches: SerializedTxBatches) {
	let raw = &batches.batches[0][0];
	let tx: Transaction<Signature, ProofMarker, PureGeneratorPedersen, DefaultDB> =
		deserialize(raw.tx.as_bytes()).expect("built tx must deserialize");

	let tx_context = setup.ctx.tx_context(setup.block_context.clone());
	let tx_verified = tx
		.well_formed(
			&tx_context.ref_state,
			WellFormedStrictness::default(),
			setup.block_context.tblock,
		)
		.expect("built tx must be well-formed against the ledger state");
	let (new_state, result) = tx_context.ref_state.apply(&tx_verified, &tx_context);
	assert!(
		matches!(result, TransactionResult::Success(_)),
		"applying the registration failed: {result:?}"
	);
	assert!(
		new_state.dust.generation.address_delegation.contains_key(&setup.user_address),
		"dust address was not registered"
	);
}

/// Issue #1896: with more than one NIGHT UTXO, the requested fee allowance exceeded the
/// ledger's availability and balancing failed with `InsufficientDustForRegistrationFee`.
#[tokio::test]
async fn multi_utxo_wallet_can_self_fund_registration() {
	let setup = setup(4, 3600);
	let batches = build_registration(&setup).await.expect("registration must balance");
	assert_applies_and_registers(&setup, batches);
}

#[tokio::test]
async fn single_utxo_wallet_can_self_fund_registration() {
	let setup = setup(1, 3600);
	let batches = build_registration(&setup).await.expect("registration must balance");
	assert_applies_and_registers(&setup, batches);
}

/// UTXOs already backing DUST generation (e.g. after a register/deregister round-trip)
/// earn no retroactive DUST and must not count towards the fee allowance.
#[tokio::test]
async fn utxos_already_backing_generation_are_excluded_from_allowance() {
	let setup = setup(2, 3600);
	mark_backing_generation(&setup.ctx, &setup.utxos[0]);

	build_registration(&setup).await.expect("registration must balance");
}

/// When every NIGHT UTXO already backs generation, no retroactive DUST can ever accrue,
/// so "wait for more DUST" would mislead - the error must point at minting fresh UTXOs
/// or --funding-seed instead.
#[tokio::test]
async fn all_utxos_backing_generation_fails_with_accurate_guidance() {
	let setup = setup(2, 3600);
	for utxo in &setup.utxos {
		mark_backing_generation(&setup.ctx, utxo);
	}

	let err = build_registration(&setup).await.expect_err("no generationless UTXO must fail");
	assert!(
		matches!(err, RegisterDustAddressError::AllUtxosBackGeneration),
		"unexpected error: {err:?}"
	);
	assert!(err.to_string().contains("--funding-seed"), "error lacks guidance: {err}");
}

/// No time elapsed means no retroactive DUST for the fee: the builder must return an
/// actionable error instead of panicking.
#[tokio::test]
async fn no_accrued_dust_fails_with_actionable_error() {
	let setup = setup(4, 0);
	let err = build_registration(&setup).await.expect_err("balancing must fail without DUST");
	assert!(matches!(err, RegisterDustAddressError::Balancing(_)), "unexpected error: {err:?}");
	assert!(err.to_string().contains("--funding-seed"), "error lacks guidance: {err}");
}

/// A wallet with no NIGHT at all cannot self-fund a registration fee.
#[tokio::test]
async fn empty_wallet_self_funded_fails_with_actionable_error() {
	let setup = setup(0, 3600);
	let err = build_registration(&setup).await.expect_err("balancing must fail without NIGHT");
	assert!(matches!(err, RegisterDustAddressError::Balancing(_)), "unexpected error: {err:?}");
}
