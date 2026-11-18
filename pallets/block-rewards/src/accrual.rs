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
use midnight_primitives_block_production::Producer;

impl<T: Config> Pallet<T> {
	/// Draws the finalized block's reward and adds it to its pool's bucket and its epoch. A block
	/// whose draw fails or overflows a counter is invalid.
	pub(crate) fn accrue(utilization: Option<Utilization>) {
		let attribution = T::BlockProducer::current();
		let mut epoch = Self::epoch_for(T::CardanoEpoch::get());
		let operator = match attribution.producer {
			Some(Producer::Operator(operator)) => Some(operator),
			_ => None,
		};
		let draw = match operator {
			// Every registered producer is eligible once the record is complete; what the record
			// holds for its pool is checked by the split, which halts on what it cannot use.
			Some(operator) if T::Snapshot::snapshot_id(epoch.cardano_epoch).is_some() => {
				let utilization =
					utilization.unwrap_or_else(|| invalid("the block's utilization is not noted"));
				let draw = Self::draw(epoch.parameters_version, utilization);
				if draw.producer > 0 {
					Buckets::<T>::mutate(epoch.cardano_epoch, operator, |bucket| {
						*bucket = checked(bucket.checked_add(draw.producer))
					});
				}
				draw
			},
			_ => BlockDraw::default(),
		};
		ReserveOutstanding::<T>::mutate(|reserve| {
			*reserve = checked(reserve.checked_sub(draw.base))
		});
		Drawn::<T>::mutate(|drawn| *drawn = checked(drawn.checked_add(draw.base)));
		epoch.treasury = checked(epoch.treasury.checked_add(draw.treasury));
		epoch.blocks = checked(epoch.blocks.checked_add(1));
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
		QueuedEpochs::<T>::mutate(|queued| *queued = queued.saturating_add(1));
		if !Settlement::<T>::exists() {
			Settlement::<T>::put(SettlementCursor {
				cardano_epoch,
				entry: SettlementEntry::Treasury,
			});
		}
	}

	/// A draw that fails, including on a utilization above its maximum, makes the block invalid.
	fn draw(parameters_version: u32, utilization: Utilization) -> BlockDraw {
		let parameters = Self::parameters(parameters_version)
			.unwrap_or_else(|| invalid("the epoch's parameter version is not installed"));
		midnight_primitives_block_rewards::block_draw(
			ReserveOutstanding::<T>::get(),
			parameters.distribution_rate,
			parameters.fixed_subsidy,
			utilization,
		)
		.unwrap_or_else(|error| panic!("block rewards: the block draw failed: {error:?}"))
	}
}

/// Rejects the block being executed.
pub(crate) fn invalid(reason: &str) -> ! {
	panic!("block rewards: {reason}")
}

fn checked<N>(value: Option<N>) -> N {
	value.unwrap_or_else(|| invalid("a reward counter overflows"))
}
