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

use crate::{mock::*, *};
use frame_support::traits::Hooks;
use frame_support::weights::Weight;
use frame_support::{assert_noop, assert_ok};
use midnight_primitives_block_rewards::{
	CardanoEpoch, Ratio, RewardAddress, StakeCredential, UtxoId,
};
use sp_runtime::DispatchError;

fn last_event() -> crate::Event<Test> {
	System::events()
		.into_iter()
		.rev()
		.find_map(|record| match record.event {
			RuntimeEvent::BlockRewards(event) => Some(event),
			_ => None,
		})
		.expect("an event")
}

fn records() -> Vec<BlockRewardRecord> {
	System::events()
		.into_iter()
		.filter_map(|record| match record.event {
			RuntimeEvent::BlockRewards(Event::BlockRewarded { record }) => Some(record),
			_ => None,
		})
		.collect()
}

fn payments() -> Vec<u128> {
	TREASURY_PAYMENTS.with_borrow(Clone::clone)
}

fn address(byte: u8) -> RewardAddress {
	RewardAddress([byte; 32])
}

fn credential(byte: u8) -> StakeCredential {
	StakeCredential { kind: 0, hash: [byte; 28] }
}

fn delegator(byte: u8, stake: u128, to: Option<u8>) -> SnapshotPayee {
	SnapshotPayee {
		payee: RewardPayee::Delegator { stake_credential: credential(byte) },
		stake,
		reward_address: to.map(address),
	}
}

fn lock(byte: u8, stake: u128, to: Option<u8>) -> SnapshotPayee {
	SnapshotPayee {
		payee: RewardPayee::Lock { utxo: UtxoId { transaction: [byte; 32], index: 0 } },
		stake,
		reward_address: to.map(address),
	}
}

/// Records operator `byte` in epoch `epoch`'s snapshot as a sole pool paying `address(byte)`.
fn record_sole_pool(epoch: u32, byte: u8) {
	let pool = SnapshotPool { reward_address: address(byte), ..sole_pool() };
	SNAPSHOT.with_borrow_mut(|pools| {
		pools.insert((CardanoEpoch(epoch), operator(byte)), (pool, Vec::new()));
	});
}

fn installed() -> sp_io::TestExternalities {
	let mut ext = new_test_ext();
	ext.execute_with(|| {
		assert_ok!(BlockRewards::install_parameters(parameters(1)));
	});
	ext
}

fn finalize(block: u64) {
	System::set_block_number(block);
	BlockRewards::note_block_usage(usage_of(UTILIZATION.get()));
	BlockRewards::on_finalize(block);
}

/// The settlement of block `block`: its start, its extrinsics' use of the ledger, and `on_idle`.
fn initialize(block: u64) {
	System::set_block_number(block);
	BLOCK_USED.set(0);
	BlockRewards::on_initialize(block);
	BLOCK_USED.set(BLOCK_USED.get() + EXTRINSICS_USE.get());
	BlockRewards::on_idle(block, Weight::MAX);
}

mod accrual;
mod activation;
mod halt;
mod parameters;
mod settlement;
mod treasury;

/// Reports `total` as released and credits the ledger pool with the difference, as the bridge
/// does in one block.
fn release(total: u128) {
	LEDGER_POOL.set(LEDGER_POOL.get() + total - RELEASED.get());
	RELEASED.set(total);
}
