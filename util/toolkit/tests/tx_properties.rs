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

//! Property tests for `generate-txs single-tx`, run entirely offline.
//!
//! Each case builds a randomly shaped transfer with the toolkit's own builder against a
//! hand-crafted `LedgerState`, then validates and applies it with the real ledger - no node
//! and no proof server. The chain is bootstrapped by injecting UTXOs and shielded coins as a
//! genesis distribution would, and fee money comes from a self-funded DUST registration
//! (see `setup`).
//!
//! Proving uses `MockProofServer`, so DUST proofs are filler and `well_formed` runs with
//! proof verification off; zswap proofs stay real and are always verified by the ledger.
//! Everything else - balancing, signatures, segment structure, UTXO and DUST availability,
//! and the whole of `apply` - is the real ledger.

use async_trait::async_trait;
use midnight_ledger_unsafe_helpers::{
	CoinInfo, CostModel, DefaultDB, HashOutput, IntentHash, IntoWalletAddress, KeyLocation,
	LedgerContext, LedgerState, MockProofServer, NIGHT, Nonce, Offer, PedersenRandomness,
	ProofMarker, ProofPreimage, ProofPreimageMarker, ProvingKeyMaterial, Recipient, Resolver,
	ResolverTrait, SerdeTransaction, ShieldedTokenType, Signature, Sp, StdRng, Timestamp,
	Transaction, TransactionResult, UnshieldedTokenType, UserAddress, Utxo, WalletAddress,
	WalletSeed, deserialize, make_block_context,
	mn_ledger::structure::UtxoMeta,
	onchain_runtime::context::BlockContext,
	strictness_without_proofs,
	transient_crypto::commitment::PureGeneratorPedersen,
	transient_crypto::proofs::{Proof, ProvingProvider},
};
use midnight_node_toolkit::{
	cli_parsers::OutputArg,
	serde_def::SourceTransactions,
	tx_generator::builder::{
		BuildTxs, CoinSelectionStrategy, RegisterDustAddressArgs, SingleTxArgs,
		builders::ledger_9::{RegisterDustAddressBuilder, single_tx::SingleTxBuilder},
	},
};
use proptest::prelude::*;
use std::sync::{Arc, OnceLock};

const NETWORK_ID: &str = "undeployed";
const SENDER_SEED_HEX: &str = "0000000000000000000000000000000000000000000000000000000000000042";
const RECIPIENT_SEED_HEX: [&str; 3] = [
	"0000000000000000000000000000000000000000000000000000000000000101",
	"0000000000000000000000000000000000000000000000000000000000000102",
	"0000000000000000000000000000000000000000000000000000000000000103",
];
const GENESIS_TIME_SECS: u64 = 1_754_395_200;
const REGISTER_AFTER_SECS: u64 = 3_600;
const DUST_ACCRUAL_SECS: u64 = 86_400;

/// Uneven denominations so the two `CoinSelectionStrategy` values pick different inputs.
const NIGHT_VALUES: [u128; 4] = [3_000_000_000, 2_000_000_000, 2_000_000_000, 1_000_000_000];
const CUSTOM_UNSHIELDED_VALUES: [u128; 2] = [1_500_000, 500_000];
const SHIELDED_VALUES: [u128; 2] = [1_500_000, 500_000];
const CUSTOM_UNSHIELDED: [u8; 32] = [0xAA; 32];
const CUSTOM_SHIELDED: [u8; 32] = [0xBB; 32];

type FinalizedTx = Transaction<Signature, ProofMarker, PureGeneratorPedersen, DefaultDB>;

fn sum(values: &[u128]) -> u128 {
	values.iter().sum()
}

/// How much of an asset the sender holds. Fees are paid in DUST, so all of it is spendable.
fn budget(shielded: bool, custom: bool) -> u128 {
	match (shielded, custom) {
		(false, false) => sum(&NIGHT_VALUES),
		(false, true) => sum(&CUSTOM_UNSHIELDED_VALUES),
		(true, _) => sum(&SHIELDED_VALUES),
	}
}

fn unshielded_token(custom: bool) -> UnshieldedTokenType {
	if custom { UnshieldedTokenType(HashOutput(CUSTOM_UNSHIELDED)) } else { NIGHT }
}

fn shielded_token(custom: bool) -> ShieldedTokenType {
	if custom {
		ShieldedTokenType(HashOutput(CUSTOM_SHIELDED))
	} else {
		ShieldedTokenType::default()
	}
}

fn seed(hex: &str) -> WalletSeed {
	WalletSeed::try_from_hex_str(hex).unwrap()
}

#[derive(Clone)]
struct NoProofs;

impl ProvingProvider for NoProofs {
	async fn check(&self, _preimage: &ProofPreimage) -> Result<Vec<Option<usize>>, anyhow::Error> {
		Ok(vec![])
	}
	async fn prove(
		self,
		_preimage: &ProofPreimage,
		_overwrite_binding_input: Option<midnight_ledger_unsafe_helpers::Fr>,
	) -> Result<Proof, anyhow::Error> {
		anyhow::bail!("the registration transaction must not contain proofs")
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
impl midnight_ledger_unsafe_helpers::ProofProvider<DefaultDB> for NoProofsProvider {
	async fn prove(
		&self,
		tx: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>,
		_rng: StdRng,
		_resolver: &'static Resolver,
		cost_model: CostModel,
	) -> Transaction<Signature, ProofMarker, PedersenRandomness, DefaultDB> {
		// The ledger-9 proving future is !Send.
		tokio::task::spawn_blocking(move || {
			futures::executor::block_on(tx.prove(NoProofs, &cost_model))
				.expect("proving of a proof-free tx failed")
		})
		.await
		.expect("proving task panicked")
	}
}

struct World {
	ctx: Arc<LedgerContext<DefaultDB>>,
	block_context: BlockContext,
	recipients: Vec<WalletSeed>,
}

fn user_address(ctx: &LedgerContext<DefaultDB>, seed: &WalletSeed) -> UserAddress {
	ctx.with_wallet_from_seed(seed.clone(), |w| w.unshielded.user_address)
}

fn fund_unshielded(
	ctx: &LedgerContext<DefaultDB>,
	owner: UserAddress,
	token: UnshieldedTokenType,
	values: &[u128],
	tag: u8,
	ctime: Timestamp,
) {
	ctx.with_ledger_state(|state| {
		let mut new_state = (**state).clone();
		let mut utxo_state = (*new_state.utxo).clone();
		for (i, value) in values.iter().enumerate() {
			let utxo = Utxo {
				value: *value,
				owner,
				type_: token,
				intent_hash: IntentHash(HashOutput([tag + i as u8; 32])),
				output_no: 0,
			};
			utxo_state.utxos = utxo_state.utxos.insert(utxo, UtxoMeta { ctime });
		}
		new_state.utxo = Sp::new(utxo_state);
		if token == NIGHT {
			// Keep the NIGHT supply invariant: injected UTXOs come out of the reserve.
			new_state.reserve_pool -= sum(values);
		}
		*state = Sp::new(new_state);
	});
}

/// Put shielded coins in the wallet and their commitments in the chain's zswap tree, as a
/// genesis distribution would. The resulting root only becomes spendable after the next
/// `post_block_update`, which `setup` performs.
fn fund_shielded(
	ctx: &LedgerContext<DefaultDB>,
	seed: &WalletSeed,
	token: ShieldedTokenType,
	values: &[u128],
	tag: u8,
) {
	let cpk = ctx.with_wallet_from_seed(seed.clone(), |w| w.shielded.coin_public_key);
	for (i, value) in values.iter().enumerate() {
		let coin =
			CoinInfo { nonce: Nonce(HashOutput([tag + i as u8; 32])), type_: token, value: *value };
		let commitment = coin.commitment(&Recipient::User(cpk));
		ctx.with_ledger_state(|state| {
			let mut new_state = (**state).clone();
			let mut zswap = (*new_state.zswap).clone();
			let index = zswap.first_free;
			zswap.coin_coms = zswap
				.coin_coms
				.try_update_hash(index, commitment.0, None)
				.expect("zswap tree update");
			zswap.coin_coms_set = zswap.coin_coms_set.insert(commitment, ());
			zswap.first_free = index + 1;
			new_state.zswap = Sp::new(zswap);
			*state = Sp::new(new_state);
		});
		ctx.with_wallet_from_seed(seed.clone(), |w| {
			let updated = w
				.shielded
				.state
				.insert_coin(w.shielded.secret_keys(), coin)
				.expect("wallet coin insert");
			w.shielded.state = updated;
		});
	}
}

/// A funded sender, three recipients and a chain tip with DUST available for fees.
///
/// Every case needs its own `World`: building a transaction speculatively marks the DUST it
/// spends, and that is only cleared by a new block.
async fn setup() -> World {
	let sender = seed(SENDER_SEED_HEX);
	let recipients: Vec<WalletSeed> = RECIPIENT_SEED_HEX.iter().map(|hex| seed(hex)).collect();
	let seeds: Vec<WalletSeed> =
		std::iter::once(sender.clone()).chain(recipients.clone()).collect();

	let ctx = Arc::new(LedgerContext::<DefaultDB>::new_from_wallet_seeds(NETWORK_ID, &seeds));
	let sender_addr = user_address(&ctx, &sender);
	let ctime = Timestamp::from_secs(GENESIS_TIME_SECS);

	fund_unshielded(&ctx, sender_addr, unshielded_token(false), &NIGHT_VALUES, 0x01, ctime);
	fund_unshielded(
		&ctx,
		sender_addr,
		unshielded_token(true),
		&CUSTOM_UNSHIELDED_VALUES,
		0x20,
		ctime,
	);
	fund_shielded(&ctx, &sender, shielded_token(false), &SHIELDED_VALUES, 0x40);
	fund_shielded(&ctx, &sender, shielded_token(true), &SHIELDED_VALUES, 0x60);

	// A self-funded DUST registration is how fee money is bootstrapped offline: it carries no
	// proofs and pays itself out of the retroactive DUST the genesis NIGHT accrued.
	let register_time = Timestamp::from_secs(GENESIS_TIME_SECS + REGISTER_AFTER_SECS);
	let register_bc = make_block_context(register_time, HashOutput([9u8; 32]), register_time);
	*ctx.latest_block_context.lock().unwrap() = Some(register_bc.clone());

	let batches = RegisterDustAddressBuilder::new(
		RegisterDustAddressArgs {
			wallet_seed: SENDER_SEED_HEX.parse().unwrap(),
			funding_seed: None,
			destination_dust: None,
			rng_seed: Some([7u8; 32]),
		},
		ctx.clone(),
		Arc::new(NoProofsProvider),
	)
	.build_txs_from(SourceTransactions::new(vec![], NETWORK_ID))
	.await
	.expect("registration must balance");

	let reg_tx: FinalizedTx =
		deserialize(batches.batches[0][0].tx.as_bytes()).expect("registration must deserialize");
	let (events, _cost) = ctx
		.update_from_tx(&SerdeTransaction::Midnight(reg_tx), &register_bc)
		.expect("registration must apply");
	ctx.update_dust_from_events(&events);

	// An empty block at the tip runs `post_block_update`, publishing the zswap and DUST merkle
	// roots that the builder's inputs reference; without it the ledger rejects both.
	let now = Timestamp::from_secs(GENESIS_TIME_SECS + REGISTER_AFTER_SECS + DUST_ACCRUAL_SECS);
	let block_context = make_block_context(now, HashOutput([10u8; 32]), now);
	let empty: Vec<SerdeTransaction<Signature, ProofMarker, DefaultDB>> = vec![];
	ctx.update_from_block(&empty, &block_context, None, None)
		.expect("empty block must apply");
	*ctx.latest_block_context.lock().unwrap() = Some(block_context.clone());
	ctx.update_dust_from_block(&block_context);

	World { ctx, block_context, recipients }
}

#[derive(Clone, Debug)]
struct OutSpec {
	recipient: usize,
	shielded: bool,
	custom_token: bool,
	amount: u128,
}

#[derive(Clone, Debug)]
struct TxSpec {
	outputs: Vec<OutSpec>,
	coin_selection: CoinSelectionStrategy,
	rng_seed: [u8; 32],
}

impl TxSpec {
	fn sent(&self, matches: impl Fn(&OutSpec) -> bool) -> u128 {
		self.outputs.iter().filter(|o| matches(o)).map(|o| o.amount).sum()
	}

	fn within_budget(&self) -> bool {
		[(false, false), (false, true), (true, false), (true, true)].into_iter().all(
			|(shielded, custom)| {
				self.sent(|o| o.shielded == shielded && o.custom_token == custom)
					<= budget(shielded, custom)
			},
		)
	}
}

fn coin_selection_strategy() -> impl Strategy<Value = CoinSelectionStrategy> {
	prop_oneof![
		Just(CoinSelectionStrategy::LargestFirst),
		Just(CoinSelectionStrategy::SmallestFirst)
	]
}

fn amount_strategy(cap: u128) -> impl Strategy<Value = u128> {
	prop_oneof![
		1 => Just(1u128),
		4 => 2u128..=(cap / 8),
		2 => (cap / 2)..=cap,
	]
}

fn out_spec_strategy(allow_shielded: bool) -> impl Strategy<Value = OutSpec> {
	let side = if allow_shielded { prop::bool::ANY.boxed() } else { Just(false).boxed() };
	(0usize..RECIPIENT_SEED_HEX.len(), side, prop::bool::ANY).prop_flat_map(
		|(recipient, shielded, custom_token)| {
			amount_strategy(budget(shielded, custom_token)).prop_map(move |amount| OutSpec {
				recipient,
				shielded,
				custom_token,
				amount,
			})
		},
	)
}

fn tx_spec_strategy(allow_shielded: bool, max_outputs: usize) -> impl Strategy<Value = TxSpec> {
	(
		prop::collection::vec(out_spec_strategy(allow_shielded), 1..=max_outputs),
		coin_selection_strategy(),
		any::<[u8; 32]>(),
	)
		.prop_map(|(outputs, coin_selection, rng_seed)| TxSpec {
			outputs,
			coin_selection,
			rng_seed,
		})
}

fn output_args(world: &World, spec: &TxSpec) -> Vec<OutputArg> {
	spec.outputs
		.iter()
		.map(|out| {
			let seed = world.recipients[out.recipient].clone();
			let address: WalletAddress = world.ctx.with_wallet_from_seed(seed, |w| {
				if out.shielded {
					w.shielded.address(NETWORK_ID)
				} else {
					w.unshielded.address(NETWORK_ID)
				}
			});
			let token_type = match (out.shielded, out.custom_token) {
				(false, true) => Some(CUSTOM_UNSHIELDED),
				(true, true) => Some(CUSTOM_SHIELDED),
				(_, false) => None,
			};
			OutputArg { address, amount: out.amount, token_type }
		})
		.collect()
}

async fn build(world: &World, spec: &TxSpec) -> anyhow::Result<Vec<u8>> {
	let args = SingleTxArgs {
		outputs: output_args(world, spec),
		shielded_amount: vec![],
		shielded_token_type: vec![],
		unshielded_amount: vec![],
		unshielded_token_type: vec![],
		source_seed: SENDER_SEED_HEX.parse().unwrap(),
		funding_seed: None,
		destination_address: vec![],
		input_utxos: vec![],
		rng_seed: Some(spec.rng_seed),
		coin_selection: spec.coin_selection,
	};
	let batches = SingleTxBuilder::new(args, world.ctx.clone(), Arc::new(MockProofServer))
		.build_txs_from(SourceTransactions::new(vec![], NETWORK_ID))
		.await?;
	Ok(batches.batches[0][0].tx.as_bytes().to_vec())
}

fn apply(world: &World, raw: &[u8]) -> anyhow::Result<LedgerState<DefaultDB>> {
	let tx: FinalizedTx = deserialize(raw)?;
	let tx_context = world.ctx.tx_context(world.block_context.clone());
	let verified = tx
		.well_formed(&tx_context.ref_state, strictness_without_proofs(), world.block_context.tblock)
		.map_err(|e| anyhow::anyhow!("not well-formed: {e:?}"))?;
	let (new_state, result) = tx_context.ref_state.apply(&verified, &tx_context);
	match result {
		TransactionResult::Success(_) => Ok(new_state),
		other => Err(anyhow::anyhow!("apply did not succeed: {other:?}")),
	}
}

fn unshielded_balance(
	state: &LedgerState<DefaultDB>,
	owner: UserAddress,
	token: UnshieldedTokenType,
) -> u128 {
	state
		.utxo
		.utxos
		.iter()
		.filter(|u| u.0.owner == owner && u.0.type_ == token)
		.map(|u| u.0.value)
		.sum()
}

fn shielded_offers(tx: &FinalizedTx) -> Vec<Offer<Proof, DefaultDB>> {
	let Transaction::Standard(stx) = tx else { return vec![] };
	let mut offers = vec![];
	if let Some(guaranteed) = &stx.guaranteed_coins {
		offers.push((**guaranteed).clone());
	}
	for entry in stx.fallible_coins.iter() {
		offers.push((*entry.1).clone());
	}
	offers
}

/// Recipients only see shielded coins addressed to them once the offers are replayed into them.
fn replay_offers_into_recipients(world: &World, raw: &[u8]) {
	let tx: FinalizedTx = deserialize(raw).expect("built tx must deserialize");
	let offers = shielded_offers(&tx);
	for seed in &world.recipients {
		world
			.ctx
			.with_wallet_from_seed(seed.clone(), |w| w.update_state_from_offers(&offers));
	}
}

fn shielded_balance(world: &World, seed: &WalletSeed, token: ShieldedTokenType) -> u128 {
	world.ctx.with_wallet_from_seed(seed.clone(), |w| {
		w.shielded
			.state
			.coins
			.iter()
			.filter(|c| c.1.type_ == token)
			.map(|c| c.1.value)
			.sum()
	})
}

fn delta(
	before: &LedgerState<DefaultDB>,
	after: &LedgerState<DefaultDB>,
	owner: UserAddress,
	token: UnshieldedTokenType,
) -> i128 {
	unshielded_balance(after, owner, token) as i128
		- unshielded_balance(before, owner, token) as i128
}

fn fail(e: anyhow::Error) -> TestCaseError {
	TestCaseError::fail(e.to_string())
}

/// Proptest bodies are synchronous; one shared runtime drives the async builder.
fn block_on<F: std::future::Future>(future: F) -> F::Output {
	static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
	RT.get_or_init(|| tokio::runtime::Runtime::new().expect("runtime"))
		.block_on(future)
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 96, max_global_rejects: 4096, ..ProptestConfig::default() })]

	#[test]
	fn every_generated_transfer_applies(spec in tx_spec_strategy(false, 4)) {
		prop_assume!(spec.within_budget());
		let outcome: Result<(), TestCaseError> = block_on(async {
			let world = setup().await;
			let raw = build(&world, &spec).await.map_err(fail)?;
			let _ = apply(&world, &raw).map_err(fail)?;
			Ok(())
		});
		outcome?;
	}
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 64, max_global_rejects: 4096, ..ProptestConfig::default() })]

	#[test]
	fn value_is_conserved(spec in tx_spec_strategy(false, 4)) {
		prop_assume!(spec.within_budget());
		let outcome: Result<(), TestCaseError> = block_on(async {
			let world = setup().await;
			let sender = user_address(&world.ctx, &seed(SENDER_SEED_HEX));
			let before = world.ctx.with_ledger_state(|s| (**s).clone());
			let raw = build(&world, &spec).await.map_err(fail)?;
			let after = apply(&world, &raw).map_err(fail)?;

			for custom in [false, true] {
				let token = unshielded_token(custom);
				let sent = spec.sent(|o| !o.shielded && o.custom_token == custom);

				// Fees are paid in DUST, so the sender loses exactly what it sent.
				prop_assert_eq!(
					delta(&before, &after, sender, token),
					-(sent as i128),
					"sender delta, custom token: {:?}",
					custom
				);

				for (i, recipient) in world.recipients.iter().enumerate() {
					let expected =
						spec.sent(|o| !o.shielded && o.custom_token == custom && o.recipient == i);
					prop_assert_eq!(
						delta(&before, &after, user_address(&world.ctx, recipient), token),
						expected as i128,
						"recipient {} delta, custom token: {:?}",
						i,
						custom
					);
				}
			}
			Ok(())
		});
		outcome?;
	}
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 32, max_global_rejects: 4096, ..ProptestConfig::default() })]

	/// The same spec built twice from identical state, with the same `--rng-seed`, must
	/// produce the same bytes on the wire.
	#[test]
	fn building_is_deterministic(spec in tx_spec_strategy(false, 3)) {
		prop_assume!(spec.within_budget());
		let outcome: Result<(), TestCaseError> = block_on(async {
			let first = build(&setup().await, &spec).await.map_err(fail)?;
			let second = build(&setup().await, &spec).await.map_err(fail)?;
			prop_assert_eq!(first, second);
			Ok(())
		});
		outcome?;
	}
}

/// A spec whose total for one asset exceeds the sender's holding of it.
fn overspend_strategy() -> impl Strategy<Value = TxSpec> {
	(
		0usize..RECIPIENT_SEED_HEX.len(),
		prop::bool::ANY,
		1u128..=3,
		1u128..=100_000,
		coin_selection_strategy(),
		any::<[u8; 32]>(),
	)
		.prop_map(|(recipient, custom_token, multiple, excess, coin_selection, rng_seed)| {
			TxSpec {
				outputs: vec![OutSpec {
					recipient,
					shielded: false,
					custom_token,
					amount: budget(false, custom_token) * multiple + excess,
				}],
				coin_selection,
				rng_seed,
			}
		})
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 48, ..ProptestConfig::default() })]

	#[test]
	fn overspending_is_rejected(spec in overspend_strategy()) {
		prop_assert!(!spec.within_budget());
		let outcome: Result<(), TestCaseError> = block_on(async {
			let world = setup().await;
			if let Ok(raw) = build(&world, &spec).await {
				prop_assert!(
					apply(&world, &raw).is_err(),
					"an overspending transaction applied successfully: {:?}",
					spec
				);
			}
			Ok(())
		});
		outcome?;
	}
}

proptest! {
	// Shielded outputs carry real zswap proofs (seconds per transaction), so this axis runs
	// with a much smaller case count than the unshielded properties.
	#![proptest_config(ProptestConfig { cases: 12, max_global_rejects: 4096, ..ProptestConfig::default() })]

	/// Shielded and mixed shielded/unshielded transfers apply, and each shielded recipient
	/// ends up seeing exactly what was sent to it.
	#[test]
	fn shielded_transfers_apply_and_conserve_value(spec in tx_spec_strategy(true, 3)) {
		prop_assume!(spec.within_budget());
		prop_assume!(spec.outputs.iter().any(|o| o.shielded));
		let outcome: Result<(), TestCaseError> = block_on(async {
			let world = setup().await;
			let raw = build(&world, &spec).await.map_err(fail)?;
			let _ = apply(&world, &raw).map_err(fail)?;
			replay_offers_into_recipients(&world, &raw);

			for (i, recipient) in world.recipients.iter().enumerate() {
				for custom in [false, true] {
					let expected =
						spec.sent(|o| o.shielded && o.custom_token == custom && o.recipient == i);
					prop_assert_eq!(
						shielded_balance(&world, recipient, shielded_token(custom)),
						expected,
						"shielded recipient {} balance, custom token: {:?}",
						i,
						custom
					);
				}
			}
			Ok(())
		});
		outcome?;
	}
}
