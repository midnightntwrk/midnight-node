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

//! Property tests for keeping intra-block ledger states alive. The properties, and every test of
//! them, are in [`properties`]. The rest is harness: histories of runtime calls, run through the
//! mock runtime against a real world and a model world, and what each call did.

mod properties;
mod world;

use crate::mock::{self, AllPalletsWithSystem, MidnightSystem, RuntimeCall, Test};
use frame_support::storage::with_storage_layer;
use midnight_node_ledger::{
	ephemeral::EphemeralDb,
	ledger_9::suite::{self, arena_storage::storage::default_storage},
	suite_storage::{SuiteDb, SuiteStorageExt},
	types::active_ledger_bridge as LedgerApi,
};
use midnight_node_ledger_helpers::{DustPublicKey, Fr, serialize_untagged};
use midnight_node_res::networks::{MidnightNetwork, UndeployedNetwork};
use midnight_primitives::MidnightSystemTransactionCNightExecutor;
use midnight_primitives_ledger::{LedgerEphemeralExt, LedgerOnchainExt, LedgerStatesExt};
use parity_scale_codec::Encode;
use proptest::{prelude::*, sample::Index};
use sp_core::H256;
use sp_externalities::{Externalities, ExternalitiesExt};
use sp_runtime::{
	DispatchError,
	traits::{BlakeTwo256, Hash as _, Header as _},
	transaction_validity::TransactionSource,
};
use std::{
	collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
	sync::OnceLock,
};
use world::{Hash, Root};

type Block = <Test as frame_system::Config>::Block;
type Header = <Block as sp_runtime::traits::Block>::Header;
type Extrinsic = <Block as sp_runtime::traits::Block>::Extrinsic;
type Executive = frame_executive::Executive<
	Test,
	Block,
	frame_system::ChainContext<Test>,
	Test,
	AllPalletsWithSystem,
>;

/// A system transaction from a small pool, so keys collide often.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemTx {
	/// A `CNightGeneratesDustUpdate`.
	CNight {
		nonce: u8,
	},
	Malformed,
}

/// A `send_mn_transaction` payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserTx {
	/// The undeployed fixture's deploy.
	Deploy,
	Malformed,
}

/// A runtime API call that only loads the ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Read {
	StateRoot,
	Cost(UserTx),
	Validate(UserTx),
}

/// The block a new execution runs on.
#[derive(Debug, Clone, Copy)]
pub enum Parent {
	Genesis,
	Tip(Index),
}

/// One call of a history, inside execution `exec`.
#[derive(Debug, Clone)]
pub enum Call {
	/// On-chain: `initialize_block` and the timestamp inherent on top of `parent`. Off-chain: a
	/// runtime-API instance at `parent`.
	Start {
		exec: usize,
		onchain: bool,
		parent: Parent,
	},
	/// A system-transaction inherent: each transaction through the cNIGHT executor, inside one
	/// storage layer that rolls back when one fails.
	Inherent {
		exec: usize,
		txs: Vec<SystemTx>,
	},
	/// `apply_extrinsic` of an unsigned `send_mn_transaction`. Off-chain, a dry run.
	Extrinsic {
		exec: usize,
		tx: UserTx,
	},
	Read {
		exec: usize,
		read: Read,
	},
	/// `finalize_block`: the post-block update and the flush.
	Finalize {
		exec: usize,
	},
	/// The runtime-API instance and its extensions are dropped.
	End {
		exec: usize,
	},
}

impl Call {
	pub fn exec(&self) -> usize {
		use Call::*;
		match self {
			Start { exec, .. }
			| Inherent { exec, .. }
			| Extrinsic { exec, .. }
			| Read { exec, .. }
			| Finalize { exec }
			| End { exec } => *exec,
		}
	}
}

pub type History = Vec<Call>;

/// A call and its position, for failure messages.
pub type Step<'a> = (usize, &'a Call);

/// A call's whole return value, encoded, and the execution's `StateKey` after it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
	pub ok: bool,
	pub encoded: Vec<u8>,
	pub state_key: Vec<u8>,
}

impl Outcome {
	fn of<T: Encode, E: Encode>(result: &Result<T, E>) -> Self {
		Self { ok: result.is_ok(), encoded: result.encode(), state_key: vec![] }
	}
}

/// The cases P3 names, as the real world met them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
	/// A block built on a tip another block was already built on.
	Fork,
	/// A post-block tip produced again.
	Reimport,
	/// A storage rollback left `StateKey` at a state from before an apply it held.
	RollbackAfterApply,
	/// An execution holds the same key twice.
	HeldTwice,
	/// An on-chain execution ended without `finalize_block`.
	Abandoned,
	/// A `finalize_block` while another on-chain execution holds states.
	FlushWhileHeld,
	/// An on-chain and an off-chain execution hold the same key.
	SameKeyOnAndOffChain,
	OffchainApply,
	Read,
	Error(Entry),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Entry {
	Inherent,
	Extrinsic,
	Read,
}

pub type Classes = BTreeSet<Class>;

// --- Generator ---

#[derive(Debug, Clone)]
enum Op {
	Inherent(Vec<SystemTx>),
	/// The cNIGHT update rolled back by a failing transaction, then applied again.
	Replay(u8),
	Extrinsic(UserTx),
	Read(Read),
}

/// One execution. Its calls shrink as a unit, so every shrink stays well formed.
#[derive(Debug, Clone)]
struct Plan {
	onchain: bool,
	parent: Parent,
	ops: Vec<Op>,
	/// On-chain, ends with `finalize_block` rather than abandoned.
	finalize: bool,
}

fn system_tx() -> impl Strategy<Value = SystemTx> {
	prop_oneof![
		6 => (0..4u8).prop_map(|nonce| SystemTx::CNight { nonce }),
		1 => Just(SystemTx::Malformed),
	]
}

fn user_tx() -> impl Strategy<Value = UserTx> {
	prop_oneof![2 => Just(UserTx::Deploy), 1 => Just(UserTx::Malformed)]
}

fn read() -> impl Strategy<Value = Read> {
	prop_oneof![
		Just(Read::StateRoot),
		user_tx().prop_map(Read::Cost),
		user_tx().prop_map(Read::Validate),
	]
}

fn ops(onchain: bool) -> BoxedStrategy<Vec<Op>> {
	let op = if onchain {
		prop_oneof![
			3 => prop::collection::vec(system_tx(), 1..3).prop_map(Op::Inherent),
			1 => (0..4u8).prop_map(Op::Replay),
			3 => user_tx().prop_map(Op::Extrinsic),
		]
		.boxed()
	} else {
		prop_oneof![read().prop_map(Op::Read), user_tx().prop_map(Op::Extrinsic)].boxed()
	};
	prop::collection::vec(op, 0..5).boxed()
}

/// Half the executions start at genesis, so their keys collide with other executions'.
fn plan() -> impl Strategy<Value = Plan> {
	let parent = prop_oneof![Just(Parent::Genesis), any::<Index>().prop_map(Parent::Tip)];
	(any::<bool>(), parent, prop::bool::weighted(0.8)).prop_flat_map(
		|(onchain, parent, finalize)| {
			ops(onchain).prop_map(move |ops| Plan { onchain, parent, ops, finalize })
		},
	)
}

fn flatten(plan: &Plan, exec: usize) -> VecDeque<Call> {
	let mut calls =
		VecDeque::from([Call::Start { exec, onchain: plan.onchain, parent: plan.parent }]);
	for op in &plan.ops {
		match op {
			Op::Inherent(txs) => calls.push_back(Call::Inherent { exec, txs: txs.clone() }),
			Op::Replay(nonce) => {
				let tx = SystemTx::CNight { nonce: *nonce };
				calls.push_back(Call::Inherent { exec, txs: vec![tx, SystemTx::Malformed] });
				calls.push_back(Call::Inherent { exec, txs: vec![tx] });
			},
			Op::Extrinsic(tx) => calls.push_back(Call::Extrinsic { exec, tx: *tx }),
			Op::Read(read) => calls.push_back(Call::Read { exec, read: *read }),
		}
	}
	if plan.onchain && plan.finalize {
		calls.push_back(Call::Finalize { exec });
	}
	calls.push_back(Call::End { exec });
	calls
}

/// Interleaves the executions' calls in `order`. Executions start in turn. Unless
/// `concurrent_flush`, an on-chain execution starts only once the one before it has ended, so no
/// flush runs while another on-chain execution holds states.
fn interleave(plans: &[Plan], order: &[Index], concurrent_flush: bool) -> History {
	let mut queues: Vec<_> = plans.iter().enumerate().map(|(i, plan)| flatten(plan, i)).collect();
	let (mut started, mut order, mut history) = (0, order.iter(), vec![]);
	loop {
		let running = |i: &usize| !queues[*i].is_empty();
		let onchain_running = (0..started).filter(running).any(|i| plans[i].onchain);
		let mut ready: Vec<usize> = (0..started).filter(running).collect();
		if started < plans.len()
			&& (concurrent_flush || !(plans[started].onchain && onchain_running))
		{
			ready.push(started);
		}
		if ready.is_empty() {
			return history;
		}
		let exec = ready[order.next().map_or(0, |i| i.index(ready.len()))];
		started += usize::from(exec == started);
		history.extend(queues[exec].pop_front());
	}
}

pub fn history(max_execs: usize, concurrent_flush: bool) -> impl Strategy<Value = History> {
	// Enough picks for every call: once they run out, executions run one after another.
	let order = prop::collection::vec(any::<Index>(), max_execs * 12);
	(prop::collection::vec(plan(), 1..=max_execs), order)
		.prop_map(move |(plans, order)| interleave(&plans, &order, concurrent_flush))
}

// --- Worlds ---

/// The fixture deploy and the timestamp of every block: the time its context names.
fn deploy() -> &'static (Vec<u8>, u64) {
	static DEPLOY: OnceLock<(Vec<u8>, u64)> = OnceLock::new();
	DEPLOY.get_or_init(|| {
		let (tx, ctx) = midnight_ledger_unsafe_helpers::ledger_9::extract_tx_with_context(
			midnight_node_res::undeployed::transactions::DEPLOY_TX,
		);
		(tx, ctx.tblock.to_secs() * 1000)
	})
}

fn system_tx_bytes(tx: SystemTx) -> Vec<u8> {
	match tx {
		SystemTx::CNight { nonce } => {
			let owner = serialize_untagged(&DustPublicKey(Fr::from(7u64))).unwrap();
			let event =
				LedgerApi::construct_cnight_generates_dust_event(1_000, &owner, 0, 0, [nonce; 32])
					.unwrap();
			LedgerApi::construct_cnight_generates_dust_system_tx(vec![event]).unwrap()
		},
		SystemTx::Malformed => b"not a system transaction".to_vec(),
	}
}

fn user_tx_bytes(tx: UserTx) -> Vec<u8> {
	match tx {
		UserTx::Deploy => deploy().0.clone(),
		UserTx::Malformed => b"midnight:transaction[v9]:not a transaction".to_vec(),
	}
}

fn send(tx: UserTx) -> Extrinsic {
	let call = pallet_midnight::Call::send_mn_transaction { midnight_tx: user_tx_bytes(tx) };
	Extrinsic::new_bare(RuntimeCall::Midnight(call))
}

struct Exec {
	ext: sp_io::TestExternalities,
	onchain: bool,
	finalized: bool,
}

impl Exec {
	fn state_key(&mut self) -> Vec<u8> {
		self.ext.execute_with(|| pallet_midnight::StateKey::<Test>::get())
	}

	/// The keys of the states this execution's extensions hold.
	fn held<const W: u8>(&mut self) -> Vec<Vec<u8>> {
		let mut ext = self.ext.ext();
		let mut ext: &mut dyn Externalities = &mut ext;
		if self.onchain {
			let held = ext.extension::<LedgerStatesExt>().expect("registered at the start");
			held.iter().map(|state| suite::held_key::<W>(&**state)).collect()
		} else {
			let held = ext.extension::<LedgerEphemeralExt>().expect("registered at the start");
			held.iter().map(|(key, _)| key.clone()).collect()
		}
	}
}

/// A storage and the executions running against it. The model (`W = 1`) also keeps every state
/// it produces, persisted, forever.
struct World<const W: u8> {
	disk: midnight_node_ledger::ledger_9::suite::arena_storage::DefaultDB,
	tips: Vec<Vec<u8>>,
	execs: BTreeMap<usize, Exec>,
	/// Every key a call produced, and whether on-chain.
	produced: BTreeSet<(Vec<u8>, bool)>,
	onchain_parents: Vec<Vec<u8>>,
}

impl<const W: u8> World<W> {
	fn new() -> Self {
		// Cached `VerifiedTransaction`s hold nodes of the storage `reset` drops.
		suite::clear_tx_validation_caches();
		let disk = world::reset::<W>();
		let genesis = suite::genesis::<W>(UndeployedNetwork.genesis_state());
		Self {
			disk,
			tips: vec![genesis],
			execs: BTreeMap::new(),
			produced: BTreeSet::new(),
			onchain_parents: vec![],
		}
	}

	fn holds(&mut self, except: usize) -> bool {
		self.execs
			.iter_mut()
			.any(|(id, e)| *id != except && e.onchain && !e.held::<W>().is_empty())
	}

	/// Starts `exec` on `parent`, with the extensions `node/src/extensions.rs` registers.
	fn start(&mut self, onchain: bool, parent: Vec<u8>) -> Exec {
		let mut ext = mock::new_test_ext();
		ext.register_extension(LedgerStatesExt::default());
		ext.register_extension(LedgerEphemeralExt::default());
		ext.register_extension(SuiteStorageExt::from(W));
		if onchain {
			ext.register_extension(LedgerOnchainExt::default());
		}
		let now = deploy().1;
		let parent_hash = BlakeTwo256::hash(&parent);
		ext.execute_with(|| {
			mock::Midnight::initialize_state(UndeployedNetwork.id(), &parent);
			if onchain {
				pallet_timestamp::Now::<Test>::put(now - mock::SLOT_DURATION);
				let header =
					Header::new(1, H256::zero(), H256::zero(), parent_hash, Default::default());
				Executive::initialize_block(&header);
				let set = pallet_timestamp::Call::set { now };
				Executive::apply_extrinsic(Extrinsic::new_bare(RuntimeCall::Timestamp(set)))
					.expect("the timestamp inherent applies")
					.expect("the timestamp inherent dispatches");
			} else {
				// The state after the tip's block, as the blocks here leave it: each runs at `now`, one
				// slot after its parent.
				pallet_timestamp::Now::<Test>::put(now);
				pallet_midnight::ParentTimestamp::<Test>::put((now - mock::SLOT_DURATION) / 1000);
				frame_system::Pallet::<Test>::set_parent_hash(parent_hash);
			}
		});
		Exec { ext, onchain, finalized: false }
	}

	/// Runs `call`, recording the cases it meets into `classes`.
	fn call(&mut self, call: &Call, classes: &mut Classes) -> Outcome {
		let mut see = |class| {
			classes.insert(class);
		};
		let exec = call.exec();
		// The soft cache is keyed by the transaction alone, so the model would read the real
		// world's answer. And the strict cache's `VerifiedTransaction`s reach the disk on a
		// flush: a separate leak from the one these properties are about.
		suite::clear_tx_validation_caches();
		let mut outcome = match call {
			Call::Start { onchain, parent, .. } => {
				let parent = match parent {
					Parent::Genesis => self.tips[0].clone(),
					Parent::Tip(i) => i.get(&self.tips).clone(),
				};
				if *onchain {
					if self.onchain_parents.contains(&parent) {
						see(Class::Fork);
					}
					self.onchain_parents.push(parent.clone());
				}
				let e = self.start(*onchain, parent);
				self.execs.insert(exec, e);
				Outcome::default()
			},
			Call::Inherent { txs, .. } => {
				let e = self.execs.get_mut(&exec).expect("a running execution");
				let held = e.held::<W>().len();
				let result = e.ext.execute_with(|| {
					with_storage_layer(|| {
						txs.iter().try_for_each(|tx| {
							<MidnightSystem as MidnightSystemTransactionCNightExecutor>::execute_system_transaction(
								system_tx_bytes(*tx),
							)
							.map(drop)
						})
					})
				});
				if result.is_err() {
					see(Class::Error(Entry::Inherent));
					if e.held::<W>().len() > held {
						see(Class::RollbackAfterApply);
					}
				}
				Outcome::of::<(), DispatchError>(&result)
			},
			Call::Extrinsic { tx, .. } => {
				let e = self.execs.get_mut(&exec).expect("a running execution");
				let result = e.ext.execute_with(|| Executive::apply_extrinsic(send(*tx)));
				let applied = matches!(result, Ok(Ok(())));
				if !applied {
					see(Class::Error(Entry::Extrinsic));
				} else if !e.onchain {
					see(Class::OffchainApply);
				}
				Outcome::of(&result)
			},
			Call::Read { read, .. } => {
				let e = self.execs.get_mut(&exec).expect("a running execution");
				see(Class::Read);
				let outcome = e.ext.execute_with(|| match *read {
					Read::StateRoot => Outcome::of(&mock::Midnight::get_ledger_state_root()),
					Read::Cost(tx) => {
						Outcome::of(&mock::Midnight::get_transaction_cost(&user_tx_bytes(tx)))
					},
					Read::Validate(tx) => {
						let at = frame_system::Pallet::<Test>::parent_hash();
						let validity = Executive::validate_transaction(
							TransactionSource::External,
							send(tx),
							at,
						);
						Outcome::of(&validity)
					},
				});
				if !outcome.ok {
					see(Class::Error(Entry::Read));
				}
				outcome
			},
			Call::Finalize { .. } => {
				if self.holds(exec) {
					see(Class::FlushWhileHeld);
				}
				let e = self.execs.get_mut(&exec).expect("a running execution");
				let header = e.ext.execute_with(Executive::finalize_block);
				e.finalized = true;
				let tip = e.state_key();
				if self.tips.contains(&tip) {
					see(Class::Reimport);
				}
				self.tips.push(tip);
				Outcome { ok: true, encoded: header.encode(), state_key: vec![] }
			},
			Call::End { .. } => {
				let e = self.execs.remove(&exec).expect("a running execution");
				if e.onchain && !e.finalized {
					see(Class::Abandoned);
				}
				return Outcome::default();
			},
		};
		self.observe(exec, classes);
		outcome.state_key = self.execs.get_mut(&exec).expect("a running execution").state_key();
		outcome
	}

	/// Records the keys `exec` produced and holds. The model persists each new one.
	fn observe(&mut self, exec: usize, classes: &mut Classes) {
		let e = self.execs.get_mut(&exec).expect("a running execution");
		let (onchain, state_key, held) = (e.onchain, e.state_key(), e.held::<W>());
		if held.len() != held.iter().collect::<HashSet<_>>().len() {
			classes.insert(Class::HeldTwice);
		}
		for key in held.iter().chain([&state_key]) {
			if self.produced.insert((key.clone(), onchain)) && W == 1 {
				suite::persist::<W>(key, onchain);
			}
		}
		let others = self.execs.iter_mut().filter(|(id, e)| **id != exec && e.onchain != onchain);
		let others: HashSet<_> = others.flat_map(|(_, e)| e.held::<W>()).collect();
		if held.iter().any(|key| others.contains(key)) {
			classes.insert(Class::SameKeyOnAndOffChain);
		}
	}

	/// The keys the node may still load, each with whether the storage that loads it is
	/// on-chain: every tip, and each execution's `StateKey`. With `held`, every key a live
	/// execution holds too.
	fn live(&mut self, held: bool) -> Vec<(Vec<u8>, bool)> {
		let mut live: Vec<_> = self.tips.iter().map(|key| (key.clone(), true)).collect();
		for e in self.execs.values_mut() {
			live.push((e.state_key(), e.onchain));
			if held {
				let onchain = e.onchain;
				live.extend(e.held::<W>().into_iter().map(|key| (key, onchain)));
			}
		}
		live
	}

	/// Every live key, and whether it resolves in the storage that loads it.
	fn live_keys(&mut self) -> Vec<(Vec<u8>, bool)> {
		let live = self.live(false).into_iter().map(|(key, onchain)| {
			let (root, mut found) = (suite::root(&key), HashSet::new());
			let resolves = if onchain {
				world::walk::<SuiteDb<W>>(&root, &mut found)
			} else {
				world::walk::<EphemeralDb<SuiteDb<W>>>(&root, &mut found)
			};
			(key, resolves)
		});
		live.collect()
	}

	/// The arena's root counts, and how many times each tip was produced.
	fn roots_and_tips(&self) -> (HashMap<Hash, u32>, HashMap<Hash, u32>) {
		let roots = default_storage::<SuiteDb<W>>().with_backend(|backend| backend.get_roots());
		let mut tips = HashMap::new();
		for tip in &self.tips {
			*tips.entry(suite::root(tip).hash().clone()).or_default() += 1;
		}
		(roots, tips)
	}

	/// The nodes of released states that still load: those under a key a call produced, in the
	/// storage it was produced in, that no live key reaches.
	fn released(&mut self) -> Vec<Hash> {
		let roots = |keys: &mut dyn Iterator<Item = &Vec<u8>>| -> Vec<Root> {
			keys.map(|key| suite::root(key)).collect()
		};
		let produced = |side| {
			let keys = self.produced.iter().filter(move |(_, onchain)| *onchain == side);
			roots(&mut keys.map(|(key, _)| key))
		};
		let (onchain, offchain) = (produced(true), produced(false));
		let live = self.live(true);
		let onchain_live = roots(&mut live.iter().filter(|(_, o)| *o).map(|(key, _)| key));
		// The off-chain storage reads through the shared one, so every live key reaches nodes there.
		let all_live = roots(&mut live.iter().map(|(key, _)| key));
		let mut leftover = world::leftover::<SuiteDb<W>>(&onchain_live, &onchain);
		leftover.extend(world::leftover::<EphemeralDb<SuiteDb<W>>>(&all_live, &offchain));
		leftover
	}
}

/// Runs `history` in the real world and the model, checking P1 after every call, P4 after every
/// `finalize_block`, and P5 per call. Records the cases it meets into `classes`.
pub fn check_history(history: &History, classes: &mut Classes) -> Result<(), TestCaseError> {
	properties::history_is_well_formed(history)?;
	let _lock = world::lock();
	let mut real = World::<0>::new();
	let mut model = World::<1>::new();
	for (i, call) in history.iter().enumerate() {
		let step = (i, call);
		let outcome = real.call(call, classes);
		properties::same_results(step, &outcome, &model.call(call, &mut Classes::new()))?;
		properties::live_keys_resolve(step, &real.live_keys())?;
		let (roots, tips) = real.roots_and_tips();
		properties::roots_are_tips(step, &roots, &tips)?;
		if matches!(call, Call::Finalize { .. }) {
			properties::released_states_gone(step, &real.released())?;
			properties::disk_reachable_from_roots(step, &world::unreachable(&real.disk))?;
		}
	}
	Ok(())
}
