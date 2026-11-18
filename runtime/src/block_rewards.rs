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

//! What `pallet_block_rewards` needs from the rest of this runtime.

use crate::{Midnight, MidnightSystem, Runtime};
use alloc::vec::Vec;
use frame_support::weights::Weight;
use midnight_node_ledger::types::{
	BlockFullness, FullnessDimension, RewardOutput, RewardPools, active_ledger_bridge as LedgerApi,
};
use midnight_primitives::{LedgerBlockEnd, MidnightSystemTransactionRewardExecutor};
use midnight_primitives_block_production::OperatorId;
use midnight_primitives_block_rewards::{BlockUsage, CardanoEpoch, UsageDimension};
use pallet_block_rewards::{
	ActivationPrerequisites, LockOwner, PayoutFailure, RewardLedger, RewardPayee, RewardPayout,
	SnapshotPayee, SnapshotPool, StakeCredential, StakeSnapshot,
};

/// The ledger, reached through `pallet_midnight` and `pallet_midnight_system`.
pub struct Ledger;

impl RewardLedger for Ledger {
	fn block_reward_pool() -> Option<u128> {
		reward_pools().map(|pools| pools.block_reward_pool)
	}

	fn reserve_pool() -> Option<u128> {
		reward_pools().map(|pools| pools.reserve_pool)
	}

	fn pay_block_rewards_to_treasury(amount: u128) -> Result<(), PayoutFailure> {
		let tx = LedgerApi::construct_pay_block_rewards_to_treasury_system_tx(amount)
			.map_err(|_| PayoutFailure::Rejected)?;
		execute(tx)
	}

	fn treasury_payment_weight() -> Weight {
		pallet_midnight_system::ConfigurableSystemTxWeight::<Runtime>::get()
	}

	fn distribute_night(outputs: Vec<RewardPayout>) -> Result<(), PayoutFailure> {
		let outputs = outputs
			.into_iter()
			.map(|output| RewardOutput {
				amount: output.amount,
				target_address: output.address.0,
				nonce: output.nonce,
			})
			.collect();
		let tx = LedgerApi::construct_distribute_night_reward_system_tx(outputs)
			.map_err(|_| PayoutFailure::Rejected)?;
		execute(tx)
	}

	/// One system transaction plus, per output, the ledger's replay-protection and
	/// unclaimed-rewards updates. An estimate until the reward path is benchmarked.
	fn distribution_weight(outputs: u32) -> Weight {
		pallet_midnight_system::ConfigurableSystemTxWeight::<Runtime>::get().saturating_add(
			<Runtime as frame_system::Config>::DbWeight::get()
				.reads_writes(2, 2)
				.saturating_mul(outputs.into()),
		)
	}

	fn block_usage() -> Option<BlockUsage> {
		Midnight::get_block_fullness()
			.map(|fullness| usage(&fullness))
			.map_err(|error| log::error!("Reading the block fullness failed: {error:?}"))
			.ok()
	}
}

fn execute(tx: Vec<u8>) -> Result<(), PayoutFailure> {
	<MidnightSystem as MidnightSystemTransactionRewardExecutor>::execute_system_transaction(tx)
		.map(|_| ())
		.map_err(|error| {
			if <MidnightSystem as MidnightSystemTransactionRewardExecutor>::is_block_limit_exceeded(
				&error,
			) {
				PayoutFailure::BlockFull
			} else {
				PayoutFailure::Rejected
			}
		})
}

pub struct Prerequisites;

impl ActivationPrerequisites for Prerequisites {
	/// The reserve release observation does not exist yet, so rewards cannot be activated.
	fn reserve_anchor_ms() -> Option<u64> {
		None
	}

	/// Nothing records which Cardano epoch's snapshot selected a committee yet.
	fn cardano_epoch_source_live() -> bool {
		false
	}
}

/// Stands in until the stake snapshot record exists. No operator is a pool of it, so no block
/// draws.
pub struct NoSnapshot;

impl StakeSnapshot for NoSnapshot {
	fn snapshot_id(_: CardanoEpoch) -> Option<[u8; 32]> {
		None
	}

	fn pool(_: CardanoEpoch, _: &OperatorId) -> Option<SnapshotPool> {
		None
	}

	fn lock_owners(_: CardanoEpoch, _: Option<StakeCredential>, _: u32) -> Vec<LockOwner> {
		Vec::new()
	}

	fn payees(
		_: CardanoEpoch,
		_: &OperatorId,
		_: Option<RewardPayee>,
		_: u32,
	) -> Vec<SnapshotPayee> {
		Vec::new()
	}
}

/// Keeps settlement out of the DUST price, and passes each block's usage of the ledger's limit
/// from `pallet_midnight` to `pallet_block_rewards` once rewards are active.
pub struct NoteUtilization;

impl LedgerBlockEnd for NoteUtilization {
	fn left_out_of_price() -> Option<BlockFullness> {
		pallet_block_rewards::SettlementUsage::<Runtime>::get()
			.map(|settlement| fullness_of(&settlement))
	}

	fn note_block_fullness(fullness: BlockFullness) {
		if pallet_block_rewards::Pallet::<Runtime>::is_activated() {
			pallet_block_rewards::Pallet::<Runtime>::note_block_usage(usage(&fullness));
		}
	}
}

fn reward_pools() -> Option<RewardPools> {
	Midnight::get_reward_pools()
		.map_err(|error| log::error!("Reading the ledger's reward pools failed: {error:?}"))
		.ok()
}

/// The inverse of [`usage`].
fn fullness_of(usage: &BlockUsage) -> BlockFullness {
	let [read_time, compute_time, block_usage, bytes_written, bytes_churned] = usage
		.0
		.map(|dimension| FullnessDimension { used: dimension.used, limit: dimension.limit });
	BlockFullness { read_time, compute_time, block_usage, bytes_written, bytes_churned }
}

/// The ledger's block fullness in the order [`BlockUsage`] documents.
pub fn usage(fullness: &BlockFullness) -> BlockUsage {
	BlockUsage(
		[
			fullness.read_time,
			fullness.compute_time,
			fullness.block_usage,
			fullness.bytes_written,
			fullness.bytes_churned,
		]
		.map(|dimension| UsageDimension { used: dimension.used, limit: dimension.limit }),
	)
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::traits::PalletsInfoAccess;

	#[test]
	fn usage_keeps_the_ledgers_dimensions_in_order() {
		let [read_time, compute_time, block_usage, bytes_written, bytes_churned] =
			[1, 2, 3, 4, 5].map(|used| FullnessDimension { used, limit: used * 10 });
		let fullness =
			BlockFullness { read_time, compute_time, block_usage, bytes_written, bytes_churned };
		assert_eq!(
			usage(&fullness).0.map(|dimension| (dimension.used, dimension.limit)),
			[(1, 10), (2, 20), (3, 30), (4, 40), (5, 50)]
		);
		assert_eq!(fullness_of(&usage(&fullness)), fullness);
	}

	/// `on_finalize` runs in declaration order: `Midnight` must note the block's usage before
	/// the ledger resets it and before `BlockRewards` takes it, and the committee must be rotated
	/// before `BlockRewards` reads the producer.
	#[test]
	fn hooks_run_in_the_order_the_reward_draw_needs() {
		let order = crate::AllPalletsWithSystem::infos();
		let position = |name: &str| {
			order
				.iter()
				.position(|info| info.name == name)
				.expect("the pallet is in the runtime")
		};
		assert!(position("Midnight") < position("BlockRewards"));
		assert!(position("Session") < position("BlockRewards"));
	}
}
