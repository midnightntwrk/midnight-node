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

//! User transactions through the real pallet path, checked against the toolkit's own ledger
//! model: transfers are built with the toolkit builder, dispatched to `SimNode` via
//! `pallet-midnight`, and applied to a toolkit `LedgerContext` replayed from the same genesis
//! block; the two ledger state roots must agree byte-for-byte after every step.
//!
//! Requires `--features mock-proofs`: the builder proves with `MockProofServer`.

#![cfg(feature = "mock-proofs")]

use midnight_ledger_unsafe_helpers::{
	DefaultDB, IntoWalletAddress, LedgerContext, MockProofServer, NIGHT, ProofMarker, Signature,
	Sp, SyntheticCost, Transaction, TransactionResult, UnshieldedTokenType, UserAddress,
	WalletAddress, WalletSeed, clamp_and_normalize, compute_overall_fullness, deserialize,
	onchain_runtime::context::BlockContext, serialize_untagged,
	transient_crypto::commitment::PureGeneratorPedersen,
};
use midnight_node_ledger::latest::node_strictness;
use midnight_node_res::networks::{MidnightNetwork, UndeployedNetwork};
use midnight_node_toolkit::{
	cli_parsers::OutputArg,
	serde_def::SourceTransactions,
	tx_generator::{
		builder::{
			BuildTxs, CoinSelectionStrategy, SingleTxArgs, build_fork_aware_context_raw,
			builders::ledger_9::single_tx::SingleTxBuilder,
		},
		source::GetTxsFromFile,
	},
};
use midnight_simulation::{SLOT_DURATION_MS, SimNode, invariants};
use proptest::prelude::*;
use std::sync::{Arc, OnceLock};

const NETWORK_ID: &str = "undeployed";
const SENDER_SEED_HEX: &str = "0000000000000000000000000000000000000000000000000000000000000001";
const RECIPIENT_SEED_HEX: [&str; 2] = [
	"0000000000000000000000000000000000000000000000000000000000000002",
	"0000000000000000000000000000000000000000000000000000000000000003",
];
/// Virtual time for the genesis-registered wallets to accrue DUST for fees.
const DUST_ACCRUAL_MS: u64 = 3_600_000;

fn seed(hex: &str) -> WalletSeed {
	WalletSeed::try_from_hex_str(hex).unwrap()
}

fn seeds() -> Vec<WalletSeed> {
	std::iter::once(SENDER_SEED_HEX).chain(RECIPIENT_SEED_HEX).map(seed).collect()
}

/// The genesis block as the toolkit sees it when syncing from a node: its transactions plus
/// the canonical genesis state, which `update_from_block` installs after replaying them because
/// genesis constructor parameters are not observable from the transactions alone.
fn genesis_source() -> SourceTransactions {
	let path =
		concat!(env!("CARGO_MANIFEST_DIR"), "/../../res/genesis/genesis_block_undeployed.mn");
	let batches = GetTxsFromFile::load_single_or_multiple(path).expect("genesis block");
	let mut source = SourceTransactions::from_batches(batches.batches, false, None);
	source.blocks.last_mut().expect("genesis block").state =
		Some(UndeployedNetwork.genesis_state().to_vec());
	source
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Applied {
	Full,
	Partial,
}

/// The toolkit's ledger model, driven in lockstep with a `SimNode`.
struct Oracle {
	ctx: Arc<LedgerContext<DefaultDB>>,
	block: BlockContext,
	fullness: SyntheticCost,
}

impl Oracle {
	fn new(open_block: BlockContext) -> Self {
		let ctx = build_fork_aware_context_raw(&genesis_source(), &seeds())
			.into_ledger9()
			.expect("undeployed genesis is ledger 9");
		let mut oracle =
			Self { ctx: Arc::new(ctx), block: open_block.clone(), fullness: SyntheticCost::ZERO };
		oracle.open_block(open_block);
		oracle
	}

	fn open_block(&mut self, block: BlockContext) {
		*self.ctx.latest_block_context.lock().unwrap() = Some(block.clone());
		self.block = block;
	}

	/// Mirrors `pallet_midnight::on_finalize`: the infallible end-of-block update with the
	/// fullness accrued by the block's transactions.
	fn close_block(&mut self) {
		let fullness = std::mem::replace(&mut self.fullness, SyntheticCost::ZERO);
		let tblock = self.block.tblock;
		self.ctx.with_ledger_state(|state| {
			let limits = state.parameters.limits.block_limits;
			let normalized = clamp_and_normalize(&fullness, &limits, "oracle");
			let overall = compute_overall_fullness(&normalized);
			*state = Sp::new(state.apply_post_block_update(tblock, normalized, overall));
		});
		self.ctx.update_dust_from_block(&self.block);
	}

	/// Mirrors `LedgerApi::apply_transaction` under `mock-proofs`: a partially successful
	/// transaction (failed fallible segment) is applied like the node applies it.
	fn apply(&mut self, raw: &[u8]) -> anyhow::Result<Applied> {
		let tx: Transaction<Signature, ProofMarker, PureGeneratorPedersen, DefaultDB> =
			deserialize(raw)?;
		let tx_context = self.ctx.tx_context(self.block.clone());
		let verified = tx
			.well_formed(&tx_context.ref_state, node_strictness(), self.block.tblock)
			.map_err(|e| anyhow::anyhow!("not well-formed: {e:?}"))?;
		let cost = tx
			.cost(&tx_context.ref_state.parameters, true)
			.map_err(|e| anyhow::anyhow!("cost: {e:?}"))?;
		let (new_state, result) = tx_context.ref_state.apply(&verified, &tx_context);
		let (applied, events) = match result {
			TransactionResult::Success(events) => (Applied::Full, events),
			TransactionResult::PartialSuccess(_, events) => (Applied::Partial, events),
			TransactionResult::Failure(f) => anyhow::bail!("apply failed: {f:?}"),
		};
		self.ctx.with_ledger_state(|state| *state = Sp::new(new_state));
		self.ctx.update_dust_from_events(&events);
		self.fullness = self.fullness + cost;
		Ok(applied)
	}

	/// Same encoding as `LedgerApi::get_ledger_state_root`: the untagged typed arena key.
	fn root(&self) -> Vec<u8> {
		self.ctx
			.with_ledger_state(|state| serialize_untagged(&state.as_typed_key()).expect("root"))
	}

	fn user_address(&self, seed: &WalletSeed) -> UserAddress {
		self.ctx.with_wallet_from_seed(seed.clone(), |w| w.unshielded.user_address)
	}

	fn night_balance(&self, seed: &WalletSeed) -> u128 {
		self.unshielded_balance(self.user_address(seed), NIGHT)
	}

	fn unshielded_balance(&self, owner: UserAddress, token: UnshieldedTokenType) -> u128 {
		self.ctx.with_ledger_state(|state| {
			state
				.utxo
				.utxos
				.iter()
				.filter(|u| u.0.owner == owner && u.0.type_ == token)
				.map(|u| u.0.value)
				.sum()
		})
	}
}

/// A `SimNode` and its oracle at the same block, with DUST accrued for fees.
struct World {
	node: SimNode,
	oracle: Oracle,
	/// NIGHT visible on the unshielded side at genesis; nothing here shields NIGHT, so it must
	/// stay exactly this across every block and transaction.
	genesis_visible_night: u128,
}

impl World {
	fn genesis() -> Self {
		let mut node = SimNode::new();
		let oracle = Oracle::new(node.block_context());
		let genesis_visible_night =
			oracle.ctx.with_ledger_state(|state| invariants::visible_night(state));
		Self { node, oracle, genesis_visible_night }
	}

	fn funded() -> Self {
		let mut world = Self::genesis();
		world.advance(DUST_ACCRUAL_MS);
		world
	}

	fn advance(&mut self, slot_ms: u64) {
		self.oracle.close_block();
		let block = self.node.block_number();
		self.oracle.ctx.with_ledger_state(|state| {
			invariants::assert_holds_at_block_boundary(
				state,
				&format!("at the end of block {block}"),
			)
		});
		self.node.advance_block(slot_ms);
		self.oracle.open_block(self.node.block_context());
	}

	/// Root parity makes the oracle's state a byte-exact copy of the node's, so the invariants
	/// are checked over it.
	fn roots_agree(&mut self, at: &str) -> Result<(), TestCaseError> {
		prop_assert_eq!(
			hex::encode(self.node.ledger_state_root()),
			hex::encode(self.oracle.root()),
			"ledger state root diverged {}",
			at
		);
		let (violations, visible) = self.oracle.ctx.with_ledger_state(|state| {
			(invariants::check(state), invariants::visible_night(state))
		});
		prop_assert!(violations.is_empty(), "ledger invariants violated {}: {:#?}", at, violations);
		prop_assert_eq!(
			visible,
			self.genesis_visible_night,
			"visible NIGHT supply changed {} with no shielded NIGHT traffic",
			at
		);
		Ok(())
	}
}

fn sender_budget() -> u128 {
	static BUDGET: OnceLock<u128> = OnceLock::new();
	*BUDGET.get_or_init(|| {
		let oracle = Oracle::new(SimNode::new().block_context());
		oracle.night_balance(&seed(SENDER_SEED_HEX))
	})
}

#[derive(Clone, Debug)]
struct OutSpec {
	recipient: usize,
	amount: u128,
}

#[derive(Clone, Debug)]
struct TxSpec {
	outputs: Vec<OutSpec>,
	coin_selection: CoinSelectionStrategy,
	rng_seed: [u8; 32],
}

impl TxSpec {
	fn total(&self) -> u128 {
		self.outputs.iter().map(|o| o.amount).sum()
	}
}

fn sequence_total(seq: &[TxSpec]) -> u128 {
	seq.iter().map(TxSpec::total).sum()
}

fn amount_strategy(cap: u128) -> impl Strategy<Value = u128> {
	prop_oneof![
		1 => Just(1u128),
		4 => 2u128..=(cap / 8),
		2 => (cap / 2)..=cap,
	]
}

fn out_spec_strategy() -> impl Strategy<Value = OutSpec> {
	(0usize..RECIPIENT_SEED_HEX.len(), amount_strategy(sender_budget()))
		.prop_map(|(recipient, amount)| OutSpec { recipient, amount })
}

fn coin_selection(largest_first: bool) -> CoinSelectionStrategy {
	if largest_first {
		CoinSelectionStrategy::LargestFirst
	} else {
		CoinSelectionStrategy::SmallestFirst
	}
}

fn tx_spec_strategy(max_outputs: usize) -> impl Strategy<Value = TxSpec> {
	(
		prop::collection::vec(out_spec_strategy(), 1..=max_outputs),
		prop::bool::ANY,
		any::<[u8; 32]>(),
	)
		.prop_map(|(outputs, largest_first, rng_seed)| TxSpec {
			outputs,
			coin_selection: coin_selection(largest_first),
			rng_seed,
		})
}

fn output_args(oracle: &Oracle, spec: &TxSpec) -> Vec<OutputArg> {
	spec.outputs
		.iter()
		.map(|out| {
			let address: WalletAddress =
				oracle.ctx.with_wallet_from_seed(seed(RECIPIENT_SEED_HEX[out.recipient]), |w| {
					w.unshielded.address(NETWORK_ID)
				});
			OutputArg { address, amount: out.amount, token_type: None }
		})
		.collect()
}

/// Builds `spec` from the oracle's current state; `funding` pays the DUST fee (the sender
/// by default).
async fn build(oracle: &Oracle, spec: &TxSpec, funding: Option<&str>) -> anyhow::Result<Vec<u8>> {
	let args = SingleTxArgs {
		outputs: output_args(oracle, spec),
		shielded_amount: vec![],
		shielded_token_type: vec![],
		unshielded_amount: vec![],
		unshielded_token_type: vec![],
		source_seed: SENDER_SEED_HEX.parse().unwrap(),
		funding_seed: funding.map(|s| s.parse().unwrap()),
		destination_address: vec![],
		input_utxos: vec![],
		rng_seed: Some(spec.rng_seed),
		coin_selection: spec.coin_selection,
	};
	let batches = SingleTxBuilder::new(args, oracle.ctx.clone(), Arc::new(MockProofServer))
		.build_txs_from(SourceTransactions::new(vec![], NETWORK_ID))
		.await?;
	Ok(batches.batches[0][0].tx.as_bytes().to_vec())
}

fn build_sync(oracle: &Oracle, spec: &TxSpec, funding: Option<&str>) -> anyhow::Result<Vec<u8>> {
	static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
	RT.get_or_init(|| tokio::runtime::Runtime::new().expect("runtime"))
		.block_on(build(oracle, spec, funding))
}

fn fail(e: anyhow::Error) -> TestCaseError {
	TestCaseError::fail(e.to_string())
}

fn night_balances(oracle: &Oracle) -> Vec<u128> {
	seeds().iter().map(|seed| oracle.night_balance(seed)).collect()
}

/// Two transfers built from the same pre-state with the same coin selection share their
/// first-picked NIGHT input; the second one pays its fee from another wallet so it stays
/// buildable after the first. Returns `(first, second)`.
fn conflicting_pair(
	world: &World,
	a: &TxSpec,
	b: &TxSpec,
) -> Result<(Vec<u8>, Vec<u8>), TestCaseError> {
	let first = build_sync(&world.oracle, a, None).map_err(fail)?;
	let second = build_sync(&world.oracle, b, Some(RECIPIENT_SEED_HEX[0])).map_err(fail)?;
	Ok((first, second))
}

/// Builds and submits `spec` to both sides, asserting the runtime accepted it and the roots
/// agree; returns the transaction bytes.
fn transfer(world: &mut World, spec: &TxSpec) -> Result<Vec<u8>, TestCaseError> {
	let raw = build_sync(&world.oracle, spec, None).map_err(fail)?;
	let outcome = world.node.submit_transaction(&raw);
	prop_assert!(
		outcome.dispatch.is_ok(),
		"runtime rejected a valid transfer: {:?}",
		outcome.dispatch
	);
	prop_assert!(
		outcome.validity.is_ok(),
		"pool rejected a dispatchable transfer: {:?}",
		outcome.validity
	);
	let applied = world.oracle.apply(&raw).map_err(fail)?;
	prop_assert_eq!(applied, Applied::Full, "a valid transfer must apply in full");
	world.roots_agree("after transfer")?;
	Ok(raw)
}

#[test]
fn genesis_roots_agree() {
	let mut world = World::genesis();
	world.roots_agree("at genesis").unwrap();
	world.advance(SLOT_DURATION_MS);
	world.roots_agree("after an empty block").unwrap();
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 40, max_global_rejects: 4096, ..ProptestConfig::default() })]

	/// Every in-budget transfer in a short sequence across successive blocks is accepted by
	/// the pallet, and the toolkit model tracks the runtime's ledger root byte-for-byte after
	/// each transfer and each block boundary.
	#[test]
	fn runtime_and_toolkit_agree(seq in prop::collection::vec(tx_spec_strategy(3), 1..=3)) {
		prop_assume!(sequence_total(&seq) <= sender_budget());
		let mut world = World::funded();
		world.roots_agree("before any transfer")?;
		let sender = seed(SENDER_SEED_HEX);
		let before = world.oracle.night_balance(&sender);
		for spec in &seq {
			transfer(&mut world, spec)?;
			world.advance(SLOT_DURATION_MS);
			world.roots_agree("after block boundary")?;
		}
		// Fees are paid in DUST, so the sender loses exactly what it sent.
		prop_assert_eq!(before - world.oracle.night_balance(&sender), sequence_total(&seq));
	}
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 32, max_global_rejects: 4096, ..ProptestConfig::default() })]

	/// The pool-side verdict predicts the dispatch verdict, for an applicable transfer and for
	/// one whose input was spent by the transaction before it.
	#[test]
	fn validate_unsigned_agrees_with_dispatch(a in tx_spec_strategy(3), b in tx_spec_strategy(3)) {
		prop_assume!(a.total() <= sender_budget() && b.total() <= sender_budget());
		let mut world = World::funded();
		let b = TxSpec { coin_selection: a.coin_selection, ..b };
		let (first, second) = conflicting_pair(&world, &a, &b)?;
		for raw in [first, second] {
			let outcome = world.node.submit_transaction(&raw);
			prop_assert_eq!(
				outcome.validity.is_ok(),
				outcome.dispatch.is_ok(),
				"validity {:?} vs dispatch {:?}",
				outcome.validity,
				outcome.dispatch
			);
		}
	}
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 24, max_global_rejects: 4096, ..ProptestConfig::default() })]

	/// The builder refuses outright overspends at coin selection, so the pallet only ever sees
	/// a conflict as a double spend, and the outcome depends on the segment the builder put
	/// the transfer in. Guaranteed (single input): the pool check and dispatch both reject it
	/// and the root (hence `StateKey`) is untouched. Fallible (several inputs): the pool's
	/// guaranteed-only dry run accepts it and it dispatches as a partial success that charges
	/// the DUST fee but moves no NIGHT. The oracle agrees with the runtime either way.
	#[test]
	fn double_spend_never_moves_funds(a in tx_spec_strategy(3), b in tx_spec_strategy(3)) {
		prop_assume!(a.total() <= sender_budget() && b.total() <= sender_budget());
		let mut world = World::funded();
		let b = TxSpec { coin_selection: a.coin_selection, ..b };
		let (first, second) = conflicting_pair(&world, &a, &b)?;

		let outcome = world.node.submit_transaction(&first);
		prop_assert!(outcome.dispatch.is_ok(), "first transfer rejected: {:?}", outcome.dispatch);
		prop_assert_eq!(world.oracle.apply(&first).map_err(fail)?, Applied::Full);
		world.roots_agree("after the first transfer")?;

		let balances = night_balances(&world.oracle);
		let root = world.node.ledger_state_root();
		let outcome = world.node.submit_transaction(&second);
		match world.oracle.apply(&second) {
			Err(_) => {
				prop_assert!(outcome.validity.is_err(), "pool accepted a guaranteed double spend");
				prop_assert!(outcome.dispatch.is_err(), "guaranteed double spend applied");
				prop_assert_eq!(hex::encode(&outcome.ledger_state_root), hex::encode(&root));
			},
			Ok(Applied::Partial) => {
				prop_assert!(outcome.validity.is_ok(), "pool rejected a fallible double spend: {:?}", outcome.validity);
				prop_assert!(outcome.dispatch.is_ok(), "fallible double spend rejected: {:?}", outcome.dispatch);
				prop_assert_ne!(hex::encode(&outcome.ledger_state_root), hex::encode(&root), "fee not charged");
			},
			Ok(Applied::Full) => {
				return Err(TestCaseError::fail("the second transfer did not conflict with the first"));
			},
		}
		world.roots_agree("after the double spend")?;
		prop_assert_eq!(night_balances(&world.oracle), balances, "a double spend moved NIGHT");
	}
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 24, max_global_rejects: 4096, ..ProptestConfig::default() })]

	/// A valid transaction is accepted once; resubmitting the same bytes in the same block
	/// and in the next one is rejected without touching the ledger root.
	#[test]
	fn replay_is_rejected(spec in tx_spec_strategy(3)) {
		prop_assume!(spec.total() <= sender_budget());
		let mut world = World::funded();
		let raw = transfer(&mut world, &spec)?;
		let root = world.node.ledger_state_root();
		let same_block = world.node.submit_transaction(&raw);
		prop_assert!(same_block.dispatch.is_err(), "replay accepted in the same block");
		prop_assert_eq!(hex::encode(&same_block.ledger_state_root), hex::encode(&root));
		world.advance(SLOT_DURATION_MS);
		let root = world.node.ledger_state_root();
		let next_block = world.node.submit_transaction(&raw);
		prop_assert!(next_block.dispatch.is_err(), "replay accepted in the next block");
		prop_assert_eq!(hex::encode(&next_block.ledger_state_root), hex::encode(&root));
		world.roots_agree("after rejected replays")?;
	}
}

proptest! {
	#![proptest_config(ProptestConfig { cases: 16, max_global_rejects: 4096, ..ProptestConfig::default() })]

	/// Two fresh nodes fed the identical transaction sequence end with identical roots.
	#[test]
	fn dispatch_is_deterministic(seq in prop::collection::vec(tx_spec_strategy(3), 1..=3)) {
		prop_assume!(sequence_total(&seq) <= sender_budget());
		let mut world = World::funded();
		let mut trace = vec![];
		let mut txs = vec![];
		for spec in &seq {
			txs.push(transfer(&mut world, spec)?);
			trace.push(world.node.ledger_state_root());
			world.advance(SLOT_DURATION_MS);
			trace.push(world.node.ledger_state_root());
		}
		drop(world);

		let mut node = SimNode::new();
		node.advance_block(DUST_ACCRUAL_MS);
		let mut replay = vec![];
		for raw in &txs {
			node.submit_transaction(raw).dispatch.expect("replayed sequence must dispatch");
			replay.push(node.ledger_state_root());
			node.advance_block(SLOT_DURATION_MS);
			replay.push(node.ledger_state_root());
		}
		prop_assert_eq!(replay, trace);
	}
}
