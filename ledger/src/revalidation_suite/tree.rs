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

use super::{
	helpers::{
		BuildContractAction, BuildInput, BuildOutput, BuildUtxoOutput, BuildUtxoSpend,
		CNightGeneratesDustActionType, CNightGeneratesDustEvent, CallInfo, ContractAddress,
		ContractDeployInfo, ContractMaintenanceAuthorityInfo, ContractState, EntryPointBuf,
		FromContext, InitialNonce, InputInfo, IntentHash, IntentInfo, LedgerContext,
		LedgerParameters, MaintenanceUpdateInfo, MerkleTreeContract, NIGHT, OfferInfo, OutputInfo,
		PureGeneratorPedersen, Segment, SerdeTransaction, ShieldedTokenType,
		StandardTransactionInfo, SystemTransaction, Timestamp, Transaction, UnshieldedOfferInfo,
		UnshieldedWallet, UpdateInfo, Utxo, UtxoOutputInfo, UtxoSpendInfo, WalletSeed,
		contract_operation_version_of, contract_operation_versioned_verifier_key, deserialize,
		mn_ledger::structure::UtxoMeta, verifier_key,
	},
	ledger::{
		BlockContext, Bridge, TransactionSignature, TxValidationCache, api,
		base_crypto_local::hash::HashOutput,
		ledger_storage_local::{DefaultDB, arena::Sp, storage::default_storage},
		mn_ledger_local::structure::SPECKS_PER_DUST,
	},
	proofs::{DummyProofProvider, UnverifiedProofs},
	shim,
};
use crate::boundary::types::Hash;
use midnight_ledger_unsafe_helpers::CoinSelectionStrategy;
use midnight_primitives_ledger::LedgerOnchainExt;
use std::{marker::PhantomData, sync::Arc};
use storage_sha2::{Digest, Sha256};

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
const SHIELDED: ShieldedTokenType = ShieldedTokenType(HashOutput([0x5e; 32]));
const SHIELDED_COINS: usize = 2;
const SHIELDED_COIN_VALUE: u128 = 1_000 * STARS;
const TRANSFER: u128 = STARS;
/// What every transfer pays, 12–16× its real fee, so fee balancing settles in one proving pass.
/// `raise_fees` must stay far above it, or transfers stop underpaying after a fee raise.
const FEE_ESTIMATE: u128 = 10 * SPECKS_PER_DUST;
/// How stale a wallet's tip is when it builds every other transfer, so ancestors a few blocks up
/// still pass the TTL check and reach the input checks. The rest build at the tip, so their dust
/// `ctime` falls inside the tree, where states across a fork or a block end disagree on the roots.
const BUILD_LAG_SECS: u64 = 60;

fn seeds() -> Vec<WalletSeed> {
	(1..=3).map(|i| WalletSeed::Medium([i; 32])).collect()
}

#[derive(Debug, Clone)]
pub enum Edge {
	Transfer {
		from: usize,
		to: usize,
	},
	/// Its outputs' Merkle root is only spendable after a block end, so the next shielded
	/// transfer on a path needs an `EndBlock` before it.
	ShieldedTransfer {
		from: usize,
		to: usize,
	},
	/// The tree's one contract, its maintenance authority `payer` alone.
	Deploy {
		payer: usize,
	},
	/// A call to the contract's `store`, and its [`TxKind::WrongKey`] twin.
	Store {
		payer: usize,
	},
	/// Signed by the contract's current authority.
	Maintain {
		payer: usize,
		update: Maintenance,
	},
	EndBlock,
	Params(fn(&mut LedgerParameters)),
}

/// What a maintenance edge changes. Nothing can call `store` after a key rotation or removal:
/// its circuit no longer matches its key.
#[derive(Debug, Clone, Copy)]
pub enum Maintenance {
	/// `store`'s verifier key becomes `check`'s.
	RotateKey,
	RemoveKey,
	/// To the next seed's key, cyclically.
	RotateAuthority,
}

/// A run of edges, then the branches that fork from where it ends.
#[derive(Debug, Clone)]
pub struct Branch {
	pub edges: Vec<Edge>,
	pub children: Vec<Branch>,
}

impl Branch {
	pub fn has_tx(&self) -> bool {
		self.edges.iter().any(|e| !matches!(e, Edge::EndBlock | Edge::Params(_)))
			|| self.children.iter().any(Branch::has_tx)
	}
}

#[derive(Clone)]
pub struct Node {
	pub state_key: Vec<u8>,
	/// The block this state belongs to.
	pub block_context: BlockContext,
	pub parent: Option<usize>,
	/// No transaction applied yet in its block, so a skewed call corrects its tblock.
	pub block_start: bool,
	/// How it was reached from `parent`, so the tree can be replayed into another storage.
	pub edge: Option<EdgeRecord>,
}

#[derive(Clone)]
pub enum EdgeRecord {
	/// An index into `Tree::txs`.
	Tx(usize),
	EndBlock,
	/// A serialized system transaction.
	System(Vec<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TxKind {
	Night,
	Shielded,
	Deploy,
	Store,
	Maintain,
	/// A `store` call, identical to the one built next but proved for a key no contract holds, so
	/// valid nowhere. No edge applies it.
	WrongKey,
}

#[derive(Clone)]
pub struct TreeTx {
	pub bytes: Vec<u8>,
	/// The node it was applied at, or built at for a [`TxKind::WrongKey`].
	pub parent: usize,
	/// The node applying it produced. `None` for a [`TxKind::WrongKey`], which no edge applies.
	pub child: Option<usize>,
	/// The cache key's `tx_hash`.
	pub hash: Hash,
	pub kind: TxKind,
}

impl TreeTx {
	fn new(bytes: Vec<u8>, parent: usize, kind: TxKind) -> Self {
		let tx: Transaction<
			TransactionSignature,
			UnverifiedProofs,
			PureGeneratorPedersen,
			DefaultDB,
		> = deserialize(&bytes[..]).unwrap();
		let hash = tx.transaction_hash().0.0;
		// The cache key must cover proofs and signatures too.
		assert_eq!(hash, <[u8; 32]>::from(Sha256::digest(&bytes)), "tx hash is over all bytes");
		Self { bytes, parent, child: None, hash, kind }
	}
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
			parent: None,
			block_start: genesis.is_block_start(),
			edge: None,
		}],
		txs: vec![],
	};
	build_branch(&mut tree, &mut ext, ctx, 0, spec);
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
		let (state_key, block_context, edge) = match edge {
			Edge::EndBlock => {
				let state_key =
					TreeBridge::post_block_update(ext, &node.state_key, bc.clone()).unwrap();
				ctx.update_dust_from_block(&bc.clone().try_into().unwrap());
				let next = BlockContext {
					tblock: bc.tblock + BLOCK_SECS,
					last_block_time: bc.tblock,
					..bc
				};
				(state_key, next, EdgeRecord::EndBlock)
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
				(applied.state_root, bc, EdgeRecord::System(tx))
			},
			_ => {
				let n = tree.txs.len();
				let lag = if n.is_multiple_of(2) { BUILD_LAG_SECS } else { 0 };
				let tip = BlockContext { tblock: bc.tblock - lag, ..bc.clone() };
				*ctx.latest_block_context.lock().unwrap() = Some(tip.try_into().unwrap());
				let build = |ctx, wrong_key| {
					futures::executor::block_on(build_tx(ctx, edge, n as u64, wrong_key))
				};
				if matches!(edge, Edge::Store { .. }) {
					let (_, bytes) = build(Arc::new(ctx.snapshot()), true);
					tree.txs.push(TreeTx::new(bytes, at, TxKind::WrongKey));
				}
				let (kind, bytes) = build(ctx.clone(), false);
				let applied = shim::apply_transaction::<DefaultDB, UnverifiedProofs>(
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

				let tx = TreeTx::new(bytes, at, kind);
				let parsed: Transaction<_, UnverifiedProofs, _, _> =
					deserialize(&tx.bytes[..]).unwrap();
				let (events, _) = ctx
					.update_from_tx(
						&SerdeTransaction::<TransactionSignature, _, _>::Midnight(parsed),
						&bc.clone().try_into().unwrap(),
					)
					.unwrap();
				ctx.update_dust_from_events(&events);
				tree.txs.push(tx);
				(applied.state_root, bc, EdgeRecord::Tx(tree.txs.len() - 1))
			},
		};
		// Adopt `Bridge`'s state: ctx never runs `post_block_update`, so its fees would drift.
		let mut ledger = TreeBridge::get_ledger(&api::new(), &state_key).unwrap();
		ctx.with_ledger_state(|s| *s = Sp::new(ledger.state.clone()));
		// Intra-block states are released when their block ends; sibling branches and steps
		// still start from them.
		ledger.persist();
		tree.nodes.push(Node {
			state_key,
			block_context,
			parent: Some(at),
			block_start: ledger.is_block_start(),
			edge: Some(edge),
		});
		at = tree.nodes.len() - 1;
		if let Some(EdgeRecord::Tx(tx)) = tree.nodes[at].edge {
			tree.txs[tx].child = Some(at);
		}
	}
	for child in &branch.children {
		build_branch(tree, ext, Arc::new(ctx.snapshot()), at, child);
	}
}

/// Wallets holding NIGHT, and full dust from a cNIGHT registration `time_to_cap` ago.
fn funded_context() -> Arc<Ctx> {
	let seeds = seeds();
	let ctx = Arc::new(Ctx::new_from_wallet_seeds(NETWORK, &seeds));
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
	let (events, _) = ctx
		.update_from_tx(&funding, &funding_block.clone().try_into().unwrap())
		.unwrap();
	ctx.update_dust_from_events(&events);
	fund_shielded(&ctx, &seeds, funding_block);
	ctx
}

/// `SHIELDED_COINS` coins per seed, minted as the toolkit's genesis does: an offer of outputs
/// only, applied unbalanced. The treasury, which a system tx would pay from, is disabled.
fn fund_shielded(ctx: &Arc<Ctx>, seeds: &[WalletSeed], block: BlockContext) {
	let outputs = seeds
		.iter()
		.flat_map(|seed| std::iter::repeat_n(seed, SHIELDED_COINS))
		.map(|seed| {
			Box::new(OutputInfo {
				destination: seed.clone(),
				token_type: SHIELDED,
				value: SHIELDED_COIN_VALUE,
			}) as Box<dyn BuildOutput<DefaultDB, Ctx>>
		})
		.collect();
	let mut info = StandardTransactionInfo::new_from_context(
		ctx.clone(),
		Arc::new(DummyProofProvider::default()),
		Some([0x5e; 32]),
	);
	info.set_guaranteed_offer(OfferInfo { inputs: vec![], outputs, transients: vec![] });
	let tx = futures::executor::block_on(info.build()).expect("the funding builds");
	let tx = deserialize(&api::new().tagged_serialize(&tx).unwrap()[..]).unwrap();
	// A zero parent hash marks a genesis block, where balancing is off.
	let genesis = BlockContext { parent_block_hash: vec![0; 32], ..block.clone() };
	ctx.update_from_tx(
		&SerdeTransaction::<TransactionSignature, UnverifiedProofs, DefaultDB>::Midnight(tx),
		&genesis.try_into().unwrap(),
	)
	.expect("the funding applies");
	// Spends prove membership against a root the block end recorded.
	ctx.with_ledger_state(|s| {
		let mut state = (**s).clone();
		state.zswap =
			Sp::new(shim::zswap_post_block_update(&state, Timestamp::from_secs(block.tblock)));
		*s = Sp::new(state);
	});
	for seed in seeds {
		let coins =
			ctx.with_wallet_from_seed(seed.clone(), |w| w.shielded.state.coins.iter().count());
		assert_eq!(coins, SHIELDED_COINS, "the wallet sees its shielded funding");
	}
}

/// The tx `edge` applies, built from `ctx`'s state, fees paid in dust by its sender or payer.
async fn build_tx(ctx: Arc<Ctx>, edge: &Edge, rng_seed: u64, wrong_key: bool) -> (TxKind, Vec<u8>) {
	let seeds = seeds();
	let mut rng = [0; 32];
	rng[..8].copy_from_slice(&rng_seed.to_le_bytes());
	let mut info = StandardTransactionInfo::new_from_context(
		ctx.clone(),
		Arc::new(DummyProofProvider { wrong_key }),
		Some(rng),
	);
	let actions = |action: Box<dyn BuildContractAction<DefaultDB, Ctx>>| {
		Box::new(IntentInfo {
			guaranteed_unshielded_offer: None,
			fallible_unshielded_offer: None,
			actions: vec![action],
		})
	};
	let (kind, payer) = match *edge {
		Edge::Transfer { from, to } => {
			let intent = night_intent(ctx, &seeds[from], &seeds[to]).await;
			info.add_intent(Segment::Fallible.into(), Box::new(intent));
			(TxKind::Night, from)
		},
		Edge::ShieldedTransfer { from, to } => {
			info.set_guaranteed_offer(shielded_offer(ctx, &seeds[from], &seeds[to]));
			(TxKind::Shielded, from)
		},
		Edge::Deploy { payer } => {
			let deploy = ContractDeployInfo {
				type_: MerkleTreeContract::new(),
				committee: vec![UnshieldedWallet::default(seeds[payer].clone())],
				committee_threshold: 1,
				_marker: PhantomData,
			};
			info.add_intent(Segment::Fallible.into(), actions(Box::new(deploy)));
			(TxKind::Deploy, payer)
		},
		Edge::Store { payer } => {
			let call = CallInfo {
				type_: MerkleTreeContract::new(),
				address: contract(&ctx).0,
				key: "store",
				input: Box::new(rng_seed as u32),
				_marker: PhantomData,
			};
			info.add_intent(Segment::Fallible.into(), actions(Box::new(call)));
			(if wrong_key { TxKind::WrongKey } else { TxKind::Store }, payer)
		},
		Edge::Maintain { payer, update } => {
			let update = maintenance(&ctx, update).await;
			info.add_intent(Segment::Fallible.into(), actions(Box::new(update)));
			(TxKind::Maintain, payer)
		},
		Edge::EndBlock | Edge::Params(_) => unreachable!("not a transaction"),
	};
	info.set_funding_seeds(vec![seeds[payer].clone()]);
	info.fee_estimate = FEE_ESTIMATE;
	let tx = info.build().await.expect("tree transactions build");
	(kind, api::new().tagged_serialize(&tx).unwrap())
}

/// The tree's one contract.
fn contract(ctx: &Ctx) -> (ContractAddress, ContractState<DefaultDB>) {
	ctx.with_ledger_state(|s| {
		let (address, state) = s.contract.iter().next().expect("the contract is deployed");
		(address, (*state).clone())
	})
}

async fn maintenance(ctx: &Ctx, update: Maintenance) -> MaintenanceUpdateInfo {
	let (address, state) = contract(ctx);
	let authority = &state.maintenance_authority;
	let member = |i: usize| UnshieldedWallet::default(seeds()[i % 3].clone());
	let current = (0..3)
		.find(|&i| member(i).maintenance_verifying_key().as_ref() == authority.committee.first())
		.expect("the authority is a seed's");
	let store = EntryPointBuf(b"store".to_vec());
	let store_version = contract_operation_version_of(&state.operations.get(&store).unwrap());
	let updates = match update {
		Maintenance::RotateKey => {
			let resolver = MerkleTreeContract::new().resolver;
			let check = verifier_key(resolver, "check").await.expect("check has a key");
			vec![
				UpdateInfo::VerifierKeyRemove(store.clone(), store_version),
				UpdateInfo::VerifierKeyInsert(
					store,
					contract_operation_versioned_verifier_key(check.0).unwrap(),
				),
			]
		},
		Maintenance::RemoveKey => vec![UpdateInfo::VerifierKeyRemove(store, store_version)],
		Maintenance::RotateAuthority => {
			vec![UpdateInfo::ReplaceAuthority(ContractMaintenanceAuthorityInfo {
				new_committee: vec![member(current + 1)],
				threshold: 1,
				counter: authority.counter + 1,
			})]
		},
	};
	MaintenanceUpdateInfo {
		address,
		committee: vec![member(current)],
		updates,
		counter: authority.counter,
	}
}

async fn night_intent(
	ctx: Arc<Ctx>,
	from: &WalletSeed,
	to: &WalletSeed,
) -> IntentInfo<DefaultDB, Ctx> {
	let (inputs, change) = UtxoSpendInfo::utxos_to_cover_value(
		ctx,
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
	IntentInfo {
		guaranteed_unshielded_offer: Some(UnshieldedOfferInfo { inputs, outputs }),
		fallible_unshielded_offer: None,
		actions: vec![],
	}
}

fn shielded_offer(ctx: Arc<Ctx>, from: &WalletSeed, to: &WalletSeed) -> OfferInfo<DefaultDB, Ctx> {
	let (inputs, change) = InputInfo::coins_to_cover_value(
		ctx,
		from.clone(),
		TRANSFER,
		SHIELDED,
		CoinSelectionStrategy::SmallestFirst,
	)
	.expect("the sender holds shielded coins");
	let inputs = inputs
		.into_iter()
		.map(|input| Box::new(input) as Box<dyn BuildInput<DefaultDB, Ctx>>)
		.collect();
	let output = |destination: &WalletSeed, value| {
		Box::new(OutputInfo { destination: destination.clone(), token_type: SHIELDED, value })
			as Box<dyn BuildOutput<DefaultDB, Ctx>>
	};
	let mut outputs = vec![output(to, TRANSFER)];
	if change > 0 {
		outputs.push(output(from, change));
	}
	OfferInfo { inputs, outputs, transients: vec![] }
}
