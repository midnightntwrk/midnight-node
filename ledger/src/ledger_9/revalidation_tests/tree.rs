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

//! A fixed tree of valid ledger states and the transactions between them.
//!
//! Every state is the one `Bridge` produced, and every transaction applied cleanly where it was
//! built. Sending them anywhere else in the tree is where the interesting verdicts come from.

use super::proofs::{DummyProofProvider, UnverifiedProofs};
use crate::ledger_9::{
	BlockContext, Bridge, TransactionSignature, TxValidationCache, api,
	base_crypto_local::hash::HashOutput,
	ledger_storage_local::{DefaultDB, arena::Sp, storage::default_storage},
	mn_ledger_local::structure::SPECKS_PER_DUST,
};
use midnight_ledger_unsafe_helpers::{
	CoinSelectionStrategy,
	ledger_9::{
		BuildUtxoOutput, BuildUtxoSpend, CNightGeneratesDustActionType, CNightGeneratesDustEvent,
		FromContext, InitialNonce, IntentHash, IntentInfo, LedgerContext, LedgerParameters, NIGHT,
		PureGeneratorPedersen, Segment, SerdeTransaction, StandardTransactionInfo,
		SystemTransaction, Timestamp, Transaction, UnshieldedOfferInfo, Utxo, UtxoOutputInfo,
		UtxoSpendInfo, WalletSeed, deserialize, mn_ledger::structure::UtxoMeta,
	},
};
use midnight_primitives_ledger::LedgerOnchainExt;
use std::sync::Arc;

pub type TreeBridge = Bridge<TransactionSignature, DefaultDB, UnverifiedProofs>;
type Ctx = LedgerContext<DefaultDB>;

pub const RUNTIME_VERSION: u32 = 2;
const BLOCK_SECS: u64 = 6;
const NETWORK: &str = "undeployed";
/// When the funding dust starts generating. The tree starts once it is full.
const FUNDED_AT: u64 = 1_800_000_000;
const STARS: u128 = 1_000_000;
const NIGHT_UTXOS: u8 = 4;
const NIGHT_UTXO_VALUE: u128 = 1_000 * STARS;
const CNIGHT_VALUE: u128 = 1_000 * STARS;
const TRANSFER: u128 = STARS;
/// What every transfer pays, 12–16× its real fee, so fee balancing settles in one proving pass.
/// `raise_fees` must stay far above it, or transfers stop underpaying after a fee raise.
const FEE_ESTIMATE: u128 = 10 * SPECKS_PER_DUST;
/// How stale a wallet's tip is when it builds, so ancestors a few blocks up still pass the TTL
/// check and reach the input checks.
const BUILD_LAG_SECS: u64 = 60;

fn seeds() -> Vec<WalletSeed> {
	(1..=3).map(|i| WalletSeed::Medium([i; 32])).collect()
}

#[derive(Debug, Clone)]
pub enum Edge {
	Transfer { from: usize, to: usize },
	EndBlock,
	Params(fn(&mut LedgerParameters)),
}

/// A run of edges, then the branches that fork from where it ends.
#[derive(Debug, Clone)]
pub struct Branch {
	pub edges: Vec<Edge>,
	pub children: Vec<Branch>,
}

impl Branch {
	pub fn has_transfer(&self) -> bool {
		self.edges.iter().any(|e| matches!(e, Edge::Transfer { .. }))
			|| self.children.iter().any(Branch::has_transfer)
	}
}

pub struct Node {
	pub state_key: Vec<u8>,
	/// The block this state belongs to.
	pub block_context: BlockContext,
}

pub struct TreeTx {
	pub bytes: Vec<u8>,
	/// The node it was applied at.
	pub parent: usize,
}

pub struct Tree {
	pub nodes: Vec<Node>,
	pub txs: Vec<TreeTx>,
}

pub fn build_tree(spec: &Branch) -> Tree {
	let mut ext = sp_io::TestExternalities::default();
	ext.register_extension(LedgerOnchainExt::default());
	let mut ext = ext.ext();

	let ctx = funded_context();
	let tblock =
		FUNDED_AT + ctx.with_ledger_state(|s| s.parameters.dust.time_to_cap()).as_seconds() as u64;
	let mut genesis = default_storage::<DefaultDB>()
		.arena
		.alloc(api::Ledger::new(ctx.with_ledger_state(|s| (**s).clone())));
	genesis.persist();
	let state_key = api::new().tagged_serialize(&genesis.as_typed_key()).unwrap();

	let mut tree = Tree {
		nodes: vec![Node {
			state_key,
			block_context: BlockContext {
				tblock,
				tblock_err: 30,
				parent_block_hash: vec![1; 32],
				last_block_time: tblock - BLOCK_SECS,
			},
		}],
		txs: vec![],
	};
	build_branch(&mut tree, &mut ext, Arc::new(ctx), 0, spec);
	TreeBridge::flush_storage(&mut ext);
	tree
}

fn build_branch(
	tree: &mut Tree,
	ext: &mut dyn sp_externalities::Externalities,
	ctx: Arc<Ctx>,
	mut at: usize,
	branch: &Branch,
) {
	// Built against a cold cache, so the tree never depends on cache behaviour.
	let cold = || TxValidationCache::new(1);
	for edge in &branch.edges {
		let node = &tree.nodes[at];
		let bc = node.block_context.clone();
		let (state_key, block_context) = match edge {
			Edge::Transfer { from, to } => {
				let tip = BlockContext { tblock: bc.tblock - BUILD_LAG_SECS, ..bc.clone() };
				*ctx.latest_block_context.lock().unwrap() = Some(tip.try_into().unwrap());
				let seeds = seeds();
				let bytes = futures::executor::block_on(transfer(
					ctx.clone(),
					&seeds[*from],
					&seeds[*to],
					tree.txs.len() as u64,
				));
				let applied = TreeBridge::apply_transaction(
					ext,
					&node.state_key,
					&bytes,
					bc.clone(),
					true,
					true,
					RUNTIME_VERSION,
					false,
					&cold(),
				)
				.expect("tree transactions apply");
				assert!(applied.all_applied, "tree transactions apply in full");

				let tx: Transaction<
					TransactionSignature,
					UnverifiedProofs,
					PureGeneratorPedersen,
					DefaultDB,
				> = deserialize(&bytes[..]).unwrap();
				let (events, _) = ctx
					.update_from_tx(
						&SerdeTransaction::Midnight(tx),
						&bc.clone().try_into().unwrap(),
					)
					.unwrap();
				ctx.update_dust_from_events(&events);
				tree.txs.push(TreeTx { bytes, parent: at });
				(applied.state_root, bc)
			},
			Edge::EndBlock => {
				let state_key =
					TreeBridge::post_block_update(ext, &node.state_key, bc.clone()).unwrap();
				ctx.update_dust_from_block(&bc.clone().try_into().unwrap());
				let next = BlockContext {
					tblock: bc.tblock + BLOCK_SECS,
					last_block_time: bc.tblock,
					..bc
				};
				(state_key, next)
			},
			Edge::Params(change) => {
				let mut params = ctx.with_ledger_state(|s| (*s.parameters).clone());
				change(&mut params);
				let tx = api::new()
					.tagged_serialize(&SystemTransaction::OverwriteParameters(params))
					.unwrap();
				let applied =
					TreeBridge::apply_system_transaction(ext, &node.state_key, &tx, bc.clone())
						.unwrap();
				(applied.state_root, bc)
			},
		};
		// Adopt `Bridge`'s state: ctx never runs `post_block_update`, so its fees would drift.
		let ledger = TreeBridge::get_ledger(&api::new(), &state_key).unwrap();
		ctx.with_ledger_state(|s| *s = Sp::new(ledger.state.clone()));
		tree.nodes.push(Node { state_key, block_context });
		at = tree.nodes.len() - 1;
	}
	for child in &branch.children {
		build_branch(tree, ext, Arc::new(ctx.snapshot()), at, child);
	}
}

/// Wallets holding NIGHT, and full dust from a cNIGHT registration `time_to_cap` ago.
fn funded_context() -> Ctx {
	let seeds = seeds();
	let ctx = Ctx::new_from_wallet_seeds(NETWORK, &seeds);
	let funded_at = Timestamp::from_secs(FUNDED_AT);

	for (i, seed) in seeds.iter().enumerate() {
		let owner = ctx.with_wallet_from_seed(seed.clone(), |w| w.unshielded.user_address);
		ctx.with_ledger_state(|state| {
			let mut new_state = (**state).clone();
			let mut utxo_state = (*new_state.utxo).clone();
			for n in 0..NIGHT_UTXOS {
				let utxo = Utxo {
					value: NIGHT_UTXO_VALUE,
					owner,
					type_: NIGHT,
					intent_hash: IntentHash(HashOutput([i as u8 * 16 + n; 32])),
					output_no: 0,
				};
				utxo_state.utxos = utxo_state.utxos.insert(utxo, UtxoMeta { ctime: funded_at });
			}
			new_state.utxo = Sp::new(utxo_state);
			// Keep the NIGHT supply invariant: the UTXOs come out of the reserve.
			new_state.reserve_pool -= NIGHT_UTXO_VALUE * NIGHT_UTXOS as u128;
			*state = Sp::new(new_state);
		});
	}

	let events = seeds
		.iter()
		.enumerate()
		.map(|(i, seed)| CNightGeneratesDustEvent {
			value: CNIGHT_VALUE,
			owner: ctx.with_wallet_from_seed(seed.clone(), |w| w.dust.public_key),
			time: funded_at,
			action: CNightGeneratesDustActionType::Create,
			nonce: InitialNonce(HashOutput([0x80 + i as u8; 32])),
		})
		.collect();
	let funding_block = BlockContext {
		tblock: FUNDED_AT,
		tblock_err: 30,
		parent_block_hash: vec![1; 32],
		last_block_time: FUNDED_AT - BLOCK_SECS,
	};
	let funding = SerdeTransaction::<TransactionSignature, UnverifiedProofs, DefaultDB>::System(
		SystemTransaction::CNightGeneratesDustUpdate { events },
	);
	let (events, _) = ctx.update_from_tx(&funding, &funding_block.try_into().unwrap()).unwrap();
	ctx.update_dust_from_events(&events);
	ctx
}

/// `TRANSFER` NIGHT from one seed to another, fees paid in dust by the sender.
async fn transfer(ctx: Arc<Ctx>, from: &WalletSeed, to: &WalletSeed, rng_seed: u64) -> Vec<u8> {
	let (inputs, change) = UtxoSpendInfo::utxos_to_cover_value(
		ctx.clone(),
		from.clone(),
		TRANSFER,
		NIGHT,
		CoinSelectionStrategy::SmallestFirst,
	)
	.await
	.expect("the sender holds NIGHT");
	let inputs = inputs
		.into_iter()
		.map(|input| Box::new(input) as Box<dyn BuildUtxoSpend<DefaultDB, Ctx>>)
		.collect();
	let mut outputs: Vec<Box<dyn BuildUtxoOutput<DefaultDB, Ctx>>> =
		vec![Box::new(UtxoOutputInfo { value: TRANSFER, owner: to.clone(), token_type: NIGHT })];
	if change > 0 {
		outputs.push(Box::new(UtxoOutputInfo {
			value: change,
			owner: from.clone(),
			token_type: NIGHT,
		}));
	}
	let intent = IntentInfo {
		guaranteed_unshielded_offer: Some(UnshieldedOfferInfo { inputs, outputs }),
		fallible_unshielded_offer: None,
		actions: vec![],
	};

	let mut rng = [0; 32];
	rng[..8].copy_from_slice(&rng_seed.to_le_bytes());
	let mut info =
		StandardTransactionInfo::new_from_context(ctx, Arc::new(DummyProofProvider), Some(rng));
	info.add_intent(Segment::Fallible.into(), Box::new(intent));
	info.set_funding_seeds(vec![from.clone()]);
	info.fee_estimate = FEE_ESTIMATE;
	let tx = info.build().await.expect("the transfer builds");
	api::new().tagged_serialize(&tx).unwrap()
}
