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

//! Deterministic simulation testbed.
//!
//! [`SimNode`] runs the real Midnight pallets (`pallet-midnight`, `pallet-midnight-system`,
//! `pallet-cnight-observation`) over the real ledger host functions and a real parity-db
//! ledger arena, inside `sp_io::TestExternalities`. Time is a virtual clock advanced by the
//! harness, Cardano is [`cardano::CardanoSim`] plus a fault-injecting follower, and
//! [`model::ObservationModel`] is an independent re-statement of the observation rules
//! that the tests check the runtime against.

pub mod cardano;
pub mod committee;
pub mod invariants;
pub mod model;

use frame_support::{
	dispatch::DispatchResultWithPostInfo,
	inherent::{InherentData, ProvideInherent},
	traits::Hooks,
};
use midnight_ledger_unsafe_helpers::{
	CNightGeneratesDustEvent, HashOutput, SystemTransaction, Timestamp as LedgerTimestamp,
	deserialize, onchain_runtime::context::BlockContext,
};
use midnight_node_ledger_helpers::fork::raw_block_data::SerializedTxBatches;
use midnight_node_res::networks::{MidnightNetwork, UndeployedNetwork};
use midnight_primitives::LedgerBlockContextProvider;
use midnight_primitives_cnight_observation::{
	CardanoPosition, DustPublicKeyBytes, INHERENT_IDENTIFIER, MidnightObservationTokenMovement,
};
use midnight_primitives_mainchain_follower::ObservedUtxo;
use pallet_cnight_observation_mock::mock::{
	CNightObservation, Midnight, RuntimeCall, RuntimeEvent, System, Test, Timestamp, new_test_ext,
};
use sp_io::TestExternalities;
use sp_runtime::{
	traits::Dispatchable,
	transaction_validity::{TransactionSource, TransactionValidity},
};
use std::{
	collections::BTreeMap,
	sync::{Mutex, MutexGuard, PoisonError},
};

pub const SLOT_DURATION_MS: u64 = 6_000;

/// Snapshot of everything the properties compare between runs and against the model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedState {
	pub mapping: BTreeMap<model::MappingKey, DustPublicKeyBytes>,
	pub next_position: CardanoPosition,
	pub ledger_state_root: Vec<u8>,
}

/// Outcome of feeding one observation inherent to a block.
#[derive(Debug)]
pub struct BlockOutcome {
	pub dispatch: DispatchResultWithPostInfo,
	/// `CNightGeneratesDust` events the ledger system transaction carried, in order.
	pub dust_events: Vec<CNightGeneratesDustEvent>,
	pub check_inherent_ok: bool,
}

/// Outcome of submitting one user transaction to the open block.
#[derive(Debug)]
pub struct TxOutcome {
	/// Pool-side verdict, taken before dispatch.
	pub validity: TransactionValidity,
	pub dispatch: DispatchResultWithPostInfo,
	pub ledger_state_root: Vec<u8>,
}

// The ledger arena is process-global, so nodes must not overlap within one test process.
static ARENA: Mutex<()> = Mutex::new(());

pub struct SimNode {
	ext: TestExternalities,
	block_number: u64,
	/// Virtual wall clock in milliseconds; the timestamp inherent of the next block.
	now_ms: u64,
	_ledger_dir: tempfile::TempDir,
	_arena: MutexGuard<'static, ()>,
}

impl SimNode {
	/// A fresh node on the undeployed-network genesis ledger state with block 1 open.
	pub fn new() -> Self {
		let arena = ARENA.lock().unwrap_or_else(PoisonError::into_inner);
		let genesis_tblock = genesis_block_time_secs();
		let ledger_dir = tempfile::tempdir().expect("tempdir");
		let state_key = midnight_node_ledger::latest::storage::init_storage_paritydb_separate(
			ledger_dir.path(),
			UndeployedNetwork.genesis_state(),
			1024 * 1024,
		);
		let mut ext = new_test_ext();
		let now_ms = genesis_tblock * 1000 + SLOT_DURATION_MS;
		ext.execute_with(|| {
			Midnight::initialize_state(UndeployedNetwork.id(), &state_key);
			System::set_block_number(1);
			Timestamp::set_timestamp(genesis_tblock * 1000);
			<Midnight as Hooks<u64>>::on_initialize(1);
			<CNightObservation as Hooks<u64>>::on_initialize(1);
			Timestamp::set_timestamp(now_ms);
		});
		Self { ext, block_number: 1, now_ms, _ledger_dir: ledger_dir, _arena: arena }
	}

	pub fn block_number(&self) -> u64 {
		self.block_number
	}

	/// Closes the current block and opens the next one `slot_ms` later on the virtual clock,
	/// in the order a real block does it: `on_initialize` sees the parent timestamp, then
	/// the timestamp inherent sets the block's own.
	pub fn advance_block(&mut self, slot_ms: u64) {
		let n = self.block_number;
		let parent_ms = self.now_ms;
		self.now_ms += slot_ms;
		let now_ms = self.now_ms;
		self.ext.execute_with(|| {
			<CNightObservation as Hooks<u64>>::on_finalize(n);
			<Midnight as Hooks<u64>>::on_finalize(n);
			System::set_block_number(n + 1);
			System::reset_events();
			Timestamp::set_timestamp(parent_ms);
			<Midnight as Hooks<u64>>::on_initialize(n + 1);
			<CNightObservation as Hooks<u64>>::on_initialize(n + 1);
			Timestamp::set_timestamp(now_ms);
		});
		self.block_number = n + 1;
	}

	/// Feeds one cNIGHT observation inherent to the open block, exactly as the node does:
	/// the call is produced by `create_inherent` from inherent data and dispatched with
	/// `None` origin. Also reports whether `check_inherent` (the verifier side) accepts it.
	pub fn observe(&mut self, utxos: Vec<ObservedUtxo>, next: CardanoPosition) -> BlockOutcome {
		let data = inherent_data(utxos, next);
		self.ext.execute_with(|| {
			let call = CNightObservation::create_inherent(&data).expect("inherent call");
			let check_inherent_ok = CNightObservation::check_inherent(&call, &data).is_ok();
			let events_before = System::events().len();
			let dispatch =
				RuntimeCall::CNightObservation(call).dispatch(frame_system::RawOrigin::None.into());
			let dust_events = System::events()
				.into_iter()
				.skip(events_before)
				.filter_map(|r| match r.event {
					RuntimeEvent::MidnightSystem(
						pallet_midnight_system::Event::SystemTransactionApplied(e),
					) => Some(e.serialized_system_transaction),
					_ => None,
				})
				.flat_map(|bytes| {
					let tx: SystemTransaction = deserialize(&bytes[..]).expect("system tx");
					match tx {
						SystemTransaction::CNightGeneratesDustUpdate { events } => events,
						other => panic!("unexpected system tx {other:?}"),
					}
				})
				.collect();
			BlockOutcome { dispatch, dust_events, check_inherent_ok }
		})
	}

	/// Verifier-side check of an inherent call against (possibly different) inherent data.
	pub fn check_inherent(
		&mut self,
		call_utxos: Vec<ObservedUtxo>,
		call_next: CardanoPosition,
		data_utxos: Vec<ObservedUtxo>,
		data_next: CardanoPosition,
	) -> bool {
		let call = pallet_cnight_observation::Call::<Test>::process_tokens {
			utxos: call_utxos,
			next_cardano_position: call_next,
		};
		let data = inherent_data(data_utxos, data_next);
		self.ext
			.execute_with(|| CNightObservation::check_inherent(&call, &data).is_ok())
	}

	pub fn state(&mut self) -> ObservedState {
		self.ext.execute_with(|| ObservedState {
			mapping: pallet_cnight_observation::Mapping::<Test>::iter()
				.map(|(addr, utxo, pk)| ((addr, utxo.tx_hash.0, utxo.index.0), pk))
				.collect(),
			next_position: pallet_cnight_observation::NextCardanoPosition::<Test>::get(),
			ledger_state_root: ledger_state_root(),
		})
	}

	pub fn utxo_owner_count(&mut self) -> usize {
		self.ext
			.execute_with(|| pallet_cnight_observation::UtxoOwners::<Test>::iter().count())
	}

	/// Submits a serialized ledger transaction to the open block the way the node does: the
	/// pool-side `validate_unsigned` verdict is recorded, then the bare call is dispatched.
	#[allow(deprecated)] // `ValidateUnsigned` is what `pallet-midnight` implements today.
	pub fn submit_transaction(&mut self, raw_tx: &[u8]) -> TxOutcome {
		self.ext.execute_with(|| {
			let call =
				pallet_midnight::Call::<Test>::send_mn_transaction { midnight_tx: raw_tx.to_vec() };
			let validity = <Midnight as sp_runtime::traits::ValidateUnsigned>::validate_unsigned(
				TransactionSource::External,
				&call,
			);
			let dispatch =
				RuntimeCall::Midnight(call).dispatch(frame_system::RawOrigin::None.into());
			TxOutcome { validity, dispatch, ledger_state_root: ledger_state_root() }
		})
	}

	/// The ledger block context the pallet applies transactions (and the end-of-block update)
	/// with in the open block, in the helpers' representation.
	pub fn block_context(&mut self) -> BlockContext {
		let ctx = self
			.ext
			.execute_with(<Midnight as LedgerBlockContextProvider>::get_block_context);
		BlockContext {
			tblock: LedgerTimestamp::from_secs(ctx.tblock),
			tblock_err: ctx.tblock_err,
			parent_block_hash: HashOutput(
				ctx.parent_block_hash.try_into().expect("32-byte parent hash"),
			),
			last_block_time: LedgerTimestamp::from_secs(ctx.last_block_time),
		}
	}

	pub fn ledger_state_root(&mut self) -> Vec<u8> {
		self.ext.execute_with(ledger_state_root)
	}
}

impl Default for SimNode {
	fn default() -> Self {
		Self::new()
	}
}

fn ledger_state_root() -> Vec<u8> {
	Midnight::get_ledger_state_root().expect("ledger state root")
}

fn inherent_data(utxos: Vec<ObservedUtxo>, next_cardano_position: CardanoPosition) -> InherentData {
	let mut data = InherentData::new();
	data.put_data(
		INHERENT_IDENTIFIER,
		&MidnightObservationTokenMovement { utxos, next_cardano_position },
	)
	.expect("inherent data");
	data
}

/// `tblock` of the genesis block's transactions; the virtual clock starts one slot later.
pub fn genesis_block_time_secs() -> u64 {
	let genesis: SerializedTxBatches =
		serde_json::from_slice(UndeployedNetwork.genesis_block()).expect("genesis block");
	let first = genesis.batches.iter().flatten().next().expect("genesis tx");
	first.context.tblock.to_secs()
}
