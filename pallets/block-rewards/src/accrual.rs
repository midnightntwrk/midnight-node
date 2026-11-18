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

//! Per-block accrual and epoch close.

use crate::*;
use frame_support::traits::Defensive;
use midnight_primitives_block_production::{OperatorId, Producer};

impl<T: Config> Pallet<T> {
	/// Draws the finalized block's reward and adds it to its pool's bucket and its epoch.
	pub(crate) fn accrue(utilization: Option<Utilization>) {
		let attribution = T::BlockProducer::current();
		let mut epoch = Self::epoch_for(T::CardanoEpoch::get());
		let operator = match attribution.producer {
			Some(Producer::Operator(operator)) => Some(operator),
			_ => None,
		};
		let draw = match operator {
			Some(operator) if Self::in_snapshot(epoch.cardano_epoch, &operator) => {
				let draw = utilization
					.defensive_proof("the utilization of every block after activation is noted")
					.map(|utilization| Self::draw(epoch.parameters_version, utilization))
					.unwrap_or_default();
				Buckets::<T>::mutate(epoch.cardano_epoch, operator, |bucket| {
					*bucket = bucket.saturating_add(draw.producer)
				});
				draw
			},
			_ => BlockDraw::default(),
		};
		ReserveOutstanding::<T>::mutate(|reserve| *reserve = reserve.saturating_sub(draw.base));
		Drawn::<T>::mutate(|drawn| *drawn = drawn.saturating_add(draw.base));
		epoch.treasury = epoch.treasury.saturating_add(draw.treasury);
		epoch.blocks = epoch.blocks.saturating_add(1);
		OpenEpoch::<T>::put(epoch);
		Self::deposit_event(Event::BlockRewarded {
			record: BlockRewardRecord {
				midnight_epoch: attribution.epoch,
				cardano_epoch: epoch.cardano_epoch,
				operator,
				parameters_version: epoch.parameters_version,
				utilization: utilization.unwrap_or_default(),
				draw,
			},
		});
	}

	/// Whether `operator` is a pool of the epoch's complete snapshot with stake. The snapshot
	/// selects the committee, so a block of any other operator is not expected; if one comes, it
	/// draws nothing. A pool without stake is listed but cannot be split, so it earns nothing.
	fn in_snapshot(cardano_epoch: CardanoEpoch, operator: &OperatorId) -> bool {
		T::Snapshot::snapshot_id(cardano_epoch).is_some()
			&& T::Snapshot::pool(cardano_epoch, operator).is_some_and(|pool| pool.total_stake > 0)
	}

	/// The open epoch if it is `cardano_epoch`; otherwise closes it and opens `cardano_epoch`.
	fn epoch_for(cardano_epoch: CardanoEpoch) -> OpenEpochState {
		if let Some(open) = OpenEpoch::<T>::get() {
			if open.cardano_epoch == cardano_epoch {
				return open;
			}
			// Reopening a closed epoch would overwrite its Treasury amount, refill buckets already
			// split and loop the settlement queue: the block stays in the open epoch.
			if cardano_epoch < open.cardano_epoch {
				frame_support::defensive!("the Cardano epoch never goes back");
				return open;
			}
			let closed = ClosedEpoch {
				parameters_version: open.parameters_version,
				blocks: open.blocks,
				treasury: open.treasury,
				snapshot_id: T::Snapshot::snapshot_id(open.cardano_epoch),
			};
			ClosedEpochs::<T>::insert(open.cardano_epoch, closed);
			Self::enqueue(open.cardano_epoch);
			Self::deposit_event(Event::EpochClosed {
				cardano_epoch: open.cardano_epoch,
				epoch: closed,
			});
		}
		OpenEpochState {
			cardano_epoch,
			parameters_version: CurrentParametersVersion::<T>::get().unwrap_or_default(),
			treasury: 0,
			blocks: 0,
		}
	}

	/// Appends a closed epoch to the settlement queue.
	fn enqueue(cardano_epoch: CardanoEpoch) {
		if let Some(last) = LastClosed::<T>::get() {
			NextClosed::<T>::insert(last, cardano_epoch);
		}
		LastClosed::<T>::put(cardano_epoch);
		if !Settlement::<T>::exists() {
			Settlement::<T>::put(SettlementCursor {
				cardano_epoch,
				entry: SettlementEntry::Treasury,
			});
		}
	}

	/// A failed draw is reported and the block draws nothing, so a bug cannot stop the chain.
	fn draw(parameters_version: u32, utilization: Utilization) -> BlockDraw {
		Self::parameters(parameters_version)
			.defensive_proof("an activated chain has its epoch's parameters")
			.and_then(|parameters| {
				midnight_primitives_block_rewards::block_draw(
					ReserveOutstanding::<T>::get(),
					parameters.distribution_rate,
					parameters.fixed_subsidy,
					utilization,
				)
				.map_err(
					|error| log::error!(target: "block-rewards", "block draw failed: {error:?}"),
				)
				.ok()
				.defensive_proof("validated parameters and a utilization within its maximum")
			})
			.unwrap_or_default()
	}
}
