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

//! The settlement queue: each closed epoch's Treasury entry, the split of its pools into
//! payouts, the release of escrow, and the payouts in reward address order.

use crate::*;
use alloc::vec::Vec;
use frame_support::{
	Hashable,
	pallet_prelude::*,
	storage::{TransactionOutcome, with_transaction},
	traits::Defensive,
};
use midnight_primitives_block_production::OperatorId;

/// The work one settlement chunk hands the ledger.
#[derive(frame_support::DefaultNoBound)]
struct Chunk<T: pallet::Config> {
	/// `L − S` before the chunk.
	available: u128,
	/// Added to `S`: the Treasury payment, the outputs and the escrows.
	spent: u128,
	budget: u32,
	treasury: u128,
	outputs: Vec<RewardPayout>,
	distributed: u128,
	escrowed: u128,
	released: u128,
	/// The epoch whose payouts this chunk paid, and whether it finished them.
	paid_epoch: Option<CardanoEpoch>,
	settled_epoch: Option<CardanoEpoch>,
	events: Vec<pallet::Event<T>>,
	halt: Option<HaltReason>,
}

impl<T: pallet::Config> Chunk<T> {
	fn new(available: u128, budget: u32) -> Self {
		Self { available, budget, ..Default::default() }
	}

	/// Released STAR this chunk can still settle.
	fn room(&self) -> u128 {
		self.available - self.spent
	}

	fn spend(&mut self, amount: u128) {
		self.spent = self.spent.saturating_add(amount);
		self.budget = self.budget.saturating_sub(1);
	}
}

/// What every chunk of a block starts from.
struct Start {
	settled: u128,
	/// `L − S`.
	available: u128,
	/// What the block had used of the ledger's limit before settlement.
	before: BlockUsage,
}

/// What a chunk did, for its weight.
#[derive(Default)]
struct Work {
	updates: u32,
	treasury: bool,
	outputs: u32,
	/// The ledger had no room left in the block for the chunk, which was undone.
	block_full: bool,
}

/// What a settlement step leaves the chunk to do.
enum Next {
	Continue,
	/// Wait for a later chunk: the release does not cover the head yet, or the epoch's payouts
	/// are done.
	Stop,
	Halt(HaltReason),
}

/// The outcome of a chunk's transaction: the chunk, or why the ledger refused it.
type ChunkOutcome<T> = Result<Result<Chunk<T>, (PayoutFailure, HaltReason)>, DispatchError>;

fn rollback<T: pallet::Config>(
	failure: PayoutFailure,
	reason: HaltReason,
) -> TransactionOutcome<ChunkOutcome<T>> {
	TransactionOutcome::Rollback(Ok(Err((failure, reason))))
}

impl<T: Config> Pallet<T> {
	/// Settles in what the block's transactions left, up to `max_updates_per_block` entries and the
	/// `remaining` weight. Runs after the extrinsics and before `pallet_midnight`'s end-of-block
	/// update. A chunk the ledger finds too big for the block is undone and retried at half the
	/// size; what does not fit waits for the next block. Settlement's cost is left out of the
	/// DUST price update and of the producer's utilization.
	pub(crate) fn settle_idle(remaining: Weight) -> Weight {
		let checks = Self::settle_weight(&Work::default());
		if !remaining.all_gte(checks) {
			return Weight::zero();
		}
		let Some(start) = Self::settlement_start() else {
			return checks;
		};
		let mut used = Weight::zero();
		let cap = SettlementBudget::<T>::get()
			.map_or(Self::max_updates(), |budget| budget.min(Self::max_updates()));
		let mut budget = Self::budget_within(remaining, cap);
		let mut tried = 0;
		while budget > 0 {
			tried = budget;
			let work = Self::settle_chunk(&start, budget);
			used = used.saturating_add(Self::settle_weight(&work));
			// A refused chunk of one entry would be refused at any size.
			if !work.block_full || work.outputs.saturating_add(work.treasury.into()) <= 1 {
				break;
			}
			budget = Self::budget_within(remaining.saturating_sub(used), budget / 2);
		}
		if tried > 0 {
			SettlementBudget::<T>::put(tried.saturating_mul(2));
		}
		used.max(checks)
	}

	/// From the current parameters, not the version the queued epoch opened with.
	fn max_updates() -> u32 {
		Self::current_parameters().map_or(0, |parameters| parameters.max_updates_per_block)
	}

	/// The largest budget up to `cap`, halving, whose chunk can weigh no more than `weight`.
	fn budget_within(weight: Weight, cap: u32) -> u32 {
		let mut budget = cap;
		while budget > 0
			&& !weight.all_gte(Self::settle_weight(&Work {
				updates: budget,
				treasury: true,
				outputs: budget,
				..Default::default()
			})) {
			budget /= 2;
		}
		budget
	}

	fn settle_weight(work: &Work) -> Weight {
		let mut weight = T::WeightInfo::settle(work.updates);
		if work.treasury {
			weight = weight.saturating_add(T::Ledger::treasury_payment_weight());
		}
		if work.outputs > 0 {
			weight = weight.saturating_add(T::Ledger::distribution_weight(work.outputs));
		}
		weight
	}

	/// The checks every settlement attempt makes, and what the block's chunks start from; `None`
	/// when nothing can be settled in this block.
	fn settlement_start() -> Option<Start> {
		if SettlementHalted::<T>::exists() {
			return None;
		}
		let settled = Settled::<T>::get();
		let Some(available) = T::Released::get().checked_sub(settled) else {
			Self::halt(HaltReason::SettledAboveReleased);
			return None;
		};
		// Checked whether or not anything is queued, so a pool that falls short halts at once.
		let baseline = Activation::<T>::get().map_or(0, |activation| activation.baseline);
		let required = baseline.saturating_add(EscrowTotal::<T>::get()).saturating_add(available);
		if T::Ledger::block_reward_pool()? < required {
			Self::halt(HaltReason::LedgerPoolBelowBaseline);
			return None;
		}
		if !Settlement::<T>::exists() {
			return None;
		}
		// Without a reading, settlement's cost could not be left out of the price update.
		let before = T::Ledger::block_usage()?;
		Some(Start { settled, available, before })
	}

	/// Settles entries from the head of the queue, each whole and only while the released
	/// total covers it, and hands the ledger the chunk's Treasury payment and payouts in one
	/// storage transaction.
	fn settle_chunk(start: &Start, budget: u32) -> Work {
		let Start { settled, available, before } = *start;
		let mut work = Work::default();
		let outcome = with_transaction(|| {
			let mut chunk = Chunk::new(available, budget);
			chunk.halt = Self::fill(&mut chunk);
			work.updates = budget.saturating_sub(chunk.budget);
			if chunk.treasury > 0 {
				work.treasury = true;
				if let Err(failure) = T::Ledger::pay_block_rewards_to_treasury(chunk.treasury) {
					return rollback(failure, HaltReason::TreasuryPaymentFailed);
				}
			}
			if !chunk.outputs.is_empty() {
				let outputs = core::mem::take(&mut chunk.outputs);
				work.outputs = outputs.len() as u32;
				chunk.distributed = outputs.iter().map(|output| output.amount).sum();
				if let Err(failure) = T::Ledger::distribute_night(outputs) {
					return rollback(failure, HaltReason::DistributionFailed);
				}
			}
			Settled::<T>::put(settled.saturating_add(chunk.spent));
			TransactionOutcome::Commit(Ok(Ok(chunk)))
		});
		match outcome {
			Ok(Ok(chunk)) => {
				for event in chunk.events {
					Self::deposit_event(event);
				}
				if let Some(cardano_epoch) = chunk.paid_epoch {
					Self::deposit_event(Event::RewardsDistributed {
						cardano_epoch,
						outputs: work.outputs,
						amount: chunk.distributed,
						escrowed: chunk.escrowed,
						released: chunk.released,
					});
				}
				if let Some(cardano_epoch) = chunk.settled_epoch {
					Self::deposit_event(Event::EpochSettled { cardano_epoch });
				}
				if let Some(reason) = chunk.halt {
					Self::halt(reason);
				}
				// At most one chunk per block commits: `settle_idle` stops after it.
				if let Some(after) = T::Ledger::block_usage() {
					SettlementUsage::<T>::put(after.since(&before));
				}
			},
			Ok(Err((PayoutFailure::Rejected, reason))) => Self::halt(reason),
			Ok(Err((PayoutFailure::BlockFull, _))) => work.block_full = true,
			// A nested-transaction limit: retried later.
			Err(_) => {},
		}
		work
	}

	/// Advances the head through as many entries as the chunk allows. Returns a reason to halt
	/// after the steps before the halting one are committed. The halting step is undone, so a
	/// cleared halt retries it from its start.
	fn fill(chunk: &mut Chunk<T>) -> Option<HaltReason> {
		while chunk.budget > 0 {
			let cursor = Settlement::<T>::get()?;
			let cardano_epoch = cursor.cardano_epoch;
			let step = with_transaction(|| {
				let next = match cursor.entry {
					SettlementEntry::Treasury => Self::treasury_step(chunk, cardano_epoch),
					SettlementEntry::Split { operator, stage } => {
						Self::pool_step(chunk, cardano_epoch, operator, stage)
					},
					SettlementEntry::Release { after } => {
						Self::release_step(chunk, cardano_epoch, after)
					},
					SettlementEntry::Payout => Self::payout_step(chunk, cardano_epoch),
				};
				match next {
					Next::Halt(_) => TransactionOutcome::Rollback(Ok::<_, DispatchError>(next)),
					_ => TransactionOutcome::Commit(Ok(next)),
				}
			});
			// A nested-transaction limit: wait for the next block.
			let next = step.unwrap_or(Next::Stop);
			match next {
				Next::Continue => {},
				Next::Stop => return None,
				Next::Halt(reason) => return Some(reason),
			}
		}
		None
	}

	/// The epoch's Treasury amount, whole, if the release covers it.
	fn treasury_step(chunk: &mut Chunk<T>, cardano_epoch: CardanoEpoch) -> Next {
		let amount = ClosedEpochs::<T>::get(cardano_epoch)
			.defensive_proof("a queued epoch is closed")
			.map_or(0, |epoch| epoch.treasury);
		if amount > chunk.room() {
			return Next::Stop;
		}
		chunk.spend(amount);
		chunk.treasury = chunk.treasury.saturating_add(amount);
		if amount > 0 {
			chunk.events.push(Event::TreasuryPaid { cardano_epoch, amount });
		}
		Self::next_pool(cardano_epoch, None);
		Next::Continue
	}

	/// Reads the pool from the epoch's snapshot and takes one [`Self::split_step`].
	fn pool_step(
		chunk: &mut Chunk<T>,
		cardano_epoch: CardanoEpoch,
		operator: OperatorId,
		stage: SplitStage,
	) -> Next {
		// A bucket fills only while its pool is in the epoch's complete snapshot, so a missing
		// pool means the record changed or was dropped.
		if Self::closed_snapshot_served(cardano_epoch) != Ok(true) {
			return Next::Halt(HaltReason::SnapshotInconsistent);
		}
		let Some(pool) = T::Snapshot::pool(cardano_epoch, &operator) else {
			return Next::Halt(HaltReason::SnapshotInconsistent);
		};
		let Ok(split) = PoolSplit::new(
			Buckets::<T>::get(cardano_epoch, operator),
			pool.terms,
			pool.operator_stake,
			pool.total_stake,
		) else {
			return Next::Halt(HaltReason::SnapshotInconsistent);
		};
		match Self::split_step(chunk, cardano_epoch, operator, &pool, &split, stage) {
			Ok(()) => Next::Continue,
			Err(reason) => Next::Halt(reason),
		}
	}

	/// One step of a pool's split: summing a page of shares, or crediting the operator and a
	/// page of payees to [`Payouts`].
	fn split_step(
		chunk: &mut Chunk<T>,
		cardano_epoch: CardanoEpoch,
		operator: OperatorId,
		pool: &SnapshotPool,
		split: &PoolSplit,
		stage: SplitStage,
	) -> Result<(), HaltReason> {
		let inconsistent = |_| HaltReason::SnapshotInconsistent;
		let stage = match stage {
			SplitStage::Summing { after, mut shares, mut stake } => {
				let requested = chunk.budget;
				let page = Self::payees(cardano_epoch, &operator, after, requested);
				chunk.budget = chunk.budget.saturating_sub((page.len() as u32).max(1));
				for payee in &page {
					shares = shares.saturating_add(split.share(payee.stake).map_err(inconsistent)?);
					stake =
						stake.checked_add(payee.stake).ok_or(HaltReason::SnapshotInconsistent)?;
				}
				match page.last() {
					Some(last) if page.len() as u32 == requested => {
						SplitStage::Summing { after: Some(last.payee), shares, stake }
					},
					_ => {
						// A total stake above the rows' would hand the difference's shares to the
						// operator as rounding.
						if pool.operator_stake.checked_add(stake) != Some(pool.total_stake) {
							return Err(HaltReason::SnapshotInconsistent);
						}
						SplitStage::Crediting { after: None, payee_shares: shares, credited: 0 }
					},
				}
			},
			SplitStage::Crediting { mut after, payee_shares, mut credited } => {
				let operator_amount = split.operator(payee_shares).map_err(inconsistent)?;
				if after.is_none() {
					Self::credit(
						cardano_epoch,
						Destination::Address(pool.reward_address),
						operator_amount,
					);
					credited = credited.saturating_add(operator_amount);
					after = Some(RewardPayee::Operator);
					chunk.budget = chunk.budget.saturating_sub(1);
				}
				let requested = chunk.budget;
				let page = if requested > 0 {
					Self::payees(cardano_epoch, &operator, after, requested)
				} else {
					Vec::new()
				};
				chunk.budget = chunk.budget.saturating_sub(page.len() as u32);
				for payee in &page {
					let amount = split.share(payee.stake).map_err(inconsistent)?;
					let destination = match (payee.reward_address, payee.payee) {
						(Some(address), _) => Destination::Address(address),
						(None, RewardPayee::Delegator { stake_credential }) => {
							Destination::Escrow(stake_credential)
						},
						(None, _) => return Err(HaltReason::SnapshotInconsistent),
					};
					Self::credit(cardano_epoch, destination, amount);
					credited = credited.saturating_add(amount);
					after = Some(payee.payee);
				}
				if requested > 0 && (page.len() as u32) < requested {
					let pool_total = Buckets::<T>::take(cardano_epoch, operator);
					if credited != pool_total {
						return Err(HaltReason::SnapshotInconsistent);
					}
					chunk.events.push(Event::PoolSplit {
						cardano_epoch,
						operator,
						pool_total,
						operator_amount,
					});
					Self::next_pool(cardano_epoch, Some(&operator));
					return Ok(());
				}
				SplitStage::Crediting { after, payee_shares, credited }
			},
		};
		Settlement::<T>::put(SettlementCursor {
			cardano_epoch,
			entry: SettlementEntry::Split { operator, stage },
		});
		Ok(())
	}

	/// Moves to an address the escrow of each lock owner, after `after`, whose locks all name it,
	/// with any escrow this epoch's split gave the same credential.
	fn release_step(
		chunk: &mut Chunk<T>,
		cardano_epoch: CardanoEpoch,
		mut after: Option<StakeCredential>,
	) -> Next {
		let requested = chunk.budget;
		let mut owners = match Self::closed_snapshot_served(cardano_epoch) {
			Ok(true) => T::Snapshot::lock_owners(cardano_epoch, after, requested),
			// Closed without a snapshot: no lock names an address in it.
			Ok(false) => Vec::new(),
			Err(reason) => return Next::Halt(reason),
		};
		owners.truncate(requested as usize);
		chunk.budget = chunk.budget.saturating_sub((owners.len() as u32).max(1));
		for owner in &owners {
			after = Some(owner.credential);
			let Some(address) = owner.reward_address else { continue };
			let held = Escrow::<T>::take(owner.credential);
			// Escrowed by this epoch's split, not yet settled: settles to the address.
			let pending =
				Payouts::<T>::take(cardano_epoch, Destination::Escrow(owner.credential)).settle;
			if held > 0 || pending > 0 {
				Payouts::<T>::mutate(cardano_epoch, Destination::Address(address), |payout| {
					payout.settle = payout.settle.saturating_add(pending);
					payout.release = payout.release.saturating_add(held);
				});
			}
		}
		let entry = if (owners.len() as u32) < requested {
			SettlementEntry::Payout
		} else {
			SettlementEntry::Release { after }
		};
		Settlement::<T>::put(SettlementCursor { cardano_epoch, entry });
		Next::Continue
	}

	/// Pays the epoch's payouts, whole and in order, while the release and the budget cover them.
	/// Reads one entry past the last it pays, at most.
	fn payout_step(chunk: &mut Chunk<T>, cardano_epoch: CardanoEpoch) -> Next {
		let mut payouts = Payouts::<T>::iter_prefix(cardano_epoch);
		let complete = loop {
			if chunk.budget == 0 {
				break false;
			}
			// Removing the entry just read does not disturb the iterator, which looks up the key
			// after it.
			let Some((destination, payout)) = payouts.next() else { break true };
			if payout.settle > chunk.room() {
				return Next::Stop;
			}
			match destination {
				Destination::Address(address) => chunk.outputs.push(RewardPayout {
					address,
					amount: payout.settle.saturating_add(payout.release),
					nonce: Self::nonce(cardano_epoch, &address),
				}),
				Destination::Escrow(credential) => {
					Escrow::<T>::mutate(credential, |held| {
						*held = held.saturating_add(payout.settle)
					});
					EscrowTotal::<T>::mutate(|total| *total = total.saturating_add(payout.settle));
					chunk.escrowed = chunk.escrowed.saturating_add(payout.settle);
				},
			}
			if payout.release > 0 {
				EscrowTotal::<T>::mutate(|total| *total = total.saturating_sub(payout.release));
				chunk.released = chunk.released.saturating_add(payout.release);
			}
			Payouts::<T>::remove(cardano_epoch, destination);
			chunk.spend(payout.settle);
			chunk.paid_epoch = Some(cardano_epoch);
		};
		if complete {
			chunk.settled_epoch = Some(cardano_epoch);
			Self::next_epoch(cardano_epoch);
			// One `DistributeNight` never mixes epochs.
			return Next::Stop;
		}
		Next::Continue
	}

	/// Forgets the settled epoch and moves the head to the next epoch's first entry, or empties
	/// the queue.
	fn next_epoch(cardano_epoch: CardanoEpoch) {
		ClosedEpochs::<T>::remove(cardano_epoch);
		match NextClosed::<T>::take(cardano_epoch) {
			Some(cardano_epoch) => Settlement::<T>::put(SettlementCursor {
				cardano_epoch,
				entry: SettlementEntry::Treasury,
			}),
			None => {
				LastClosed::<T>::kill();
				Settlement::<T>::kill();
			},
		}
	}

	/// Moves the head to the split of the epoch's first pool after `after`, or to its payouts.
	fn next_pool(cardano_epoch: CardanoEpoch, after: Option<&OperatorId>) {
		let next = match after {
			Some(operator) => Buckets::<T>::iter_key_prefix_from(
				cardano_epoch,
				Buckets::<T>::hashed_key_for(cardano_epoch, operator),
			)
			.next(),
			None => Buckets::<T>::iter_key_prefix(cardano_epoch).next(),
		};
		let entry = match next {
			Some(operator) => SettlementEntry::Split {
				operator,
				stage: SplitStage::Summing { after: None, shares: 0, stake: 0 },
			},
			None if EscrowTotal::<T>::get() == 0 && !Self::escrows_pending(cardano_epoch) => {
				SettlementEntry::Payout
			},
			None => SettlementEntry::Release { after: None },
		};
		Settlement::<T>::put(SettlementCursor { cardano_epoch, entry });
	}

	/// Whether the epoch's split put anything in escrow.
	fn escrows_pending(cardano_epoch: CardanoEpoch) -> bool {
		// Every escrow destination sorts after the largest address.
		let last_address = Destination::Address(RewardAddress([u8::MAX; 32]));
		Payouts::<T>::iter_key_prefix_from(
			cardano_epoch,
			Payouts::<T>::hashed_key_for(cardano_epoch, last_address),
		)
		.next()
		.is_some()
	}

	/// Whether the epoch closed with a snapshot (`true`) or without one (`false`). Halts if the
	/// provider now serves a different record for the epoch, which settlement must not use.
	fn closed_snapshot_served(cardano_epoch: CardanoEpoch) -> Result<bool, HaltReason> {
		match ClosedEpochs::<T>::get(cardano_epoch).and_then(|epoch| epoch.snapshot_id) {
			None => Ok(false),
			Some(id) if T::Snapshot::snapshot_id(cardano_epoch) == Some(id) => Ok(true),
			Some(_) => Err(HaltReason::SnapshotInconsistent),
		}
	}

	/// A page of the pool's payees, at most `limit` long whatever the snapshot returns.
	fn payees(
		cardano_epoch: CardanoEpoch,
		operator: &OperatorId,
		after: Option<RewardPayee>,
		limit: u32,
	) -> Vec<SnapshotPayee> {
		let mut page = T::Snapshot::payees(cardano_epoch, operator, after, limit);
		page.truncate(limit as usize);
		page
	}

	fn nonce(cardano_epoch: CardanoEpoch, address: &RewardAddress) -> [u8; 32] {
		(*b"block-rewards", cardano_epoch, *address).blake2_256()
	}

	fn credit(cardano_epoch: CardanoEpoch, destination: Destination, amount: u128) {
		if amount > 0 {
			Payouts::<T>::mutate(cardano_epoch, destination, |payout| {
				payout.settle = payout.settle.saturating_add(amount)
			});
		}
	}
}
