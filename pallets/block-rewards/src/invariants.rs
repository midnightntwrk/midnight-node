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

//! The accounting identities: the counters' after every block, the rest in `try-state`.

use crate::*;

impl<T: Config> Pallet<T> {
	/// `B_o + D` equals the reserve at activation, `S ≤ D` and `S ≤ L`. A block that leaves
	/// them broken is invalid.
	pub(crate) fn check_counters(
		activation: &ActivationRecord<frame_system::pallet_prelude::BlockNumberFor<T>>,
	) -> Result<(), &'static str> {
		let drawn = Drawn::<T>::get();
		let settled = Settled::<T>::get();
		if ReserveOutstanding::<T>::get().checked_add(drawn) != Some(activation.reserve) {
			return Err(
				"B_o + D (reserve_outstanding + drawn) differs from the reserve at activation",
			);
		}
		if settled > drawn {
			return Err("S (settled) exceeds D (drawn)");
		}
		if settled > T::Released::get() {
			return Err("S (settled) exceeds L (released)");
		}
		Ok(())
	}

	/// The counters' identities, and every drawn STAR not yet settled is in a bucket, an unpaid payout, the open
	/// epoch's Treasury amount or the Treasury amount of a closed epoch still in the queue. A
	/// pool being credited is in both its bucket and the payouts, once. Unless settlement is
	/// halted, the ledger's pool is the baseline, the escrow and `L − S`.
	#[cfg(any(feature = "try-runtime", test))]
	pub fn do_try_state() -> Result<(), &'static str> {
		let Some(activation) = Activation::<T>::get() else {
			return Ok(());
		};
		Self::check_counters(&activation)?;
		let drawn = Drawn::<T>::get();
		let settled = Settled::<T>::get();
		let buckets = Buckets::<T>::iter_values().try_fold(0u128, u128::checked_add);
		let open = OpenEpoch::<T>::get().map_or(0, |epoch| epoch.treasury);
		let payouts = Payouts::<T>::iter_values()
			.try_fold(0u128, |sum, payout| sum.checked_add(payout.settle));
		let releasing = Payouts::<T>::iter_values()
			.try_fold(0u128, |sum, payout| sum.checked_add(payout.release));
		let escrow = Escrow::<T>::iter_values().try_fold(0u128, u128::checked_add);
		if escrow.zip(releasing).and_then(|(held, out)| held.checked_add(out))
			!= Some(EscrowTotal::<T>::get())
		{
			return Err("escrow balances and releases differ from EscrowTotal");
		}
		let crediting = match Settlement::<T>::get().map(|cursor| cursor.entry) {
			Some(SettlementEntry::Split {
				stage: SplitStage::Crediting { credited, .. }, ..
			}) => credited,
			_ => 0,
		};
		let unsettled = buckets
			.and_then(|sum| sum.checked_add(payouts?))
			.and_then(|sum| sum.checked_add(open))
			.and_then(|sum| sum.checked_add(Self::treasury_queued()))
			.and_then(|sum| sum.checked_sub(crediting));
		if unsettled != Some(drawn - settled) {
			return Err(
				"D − S (drawn − settled) differs from buckets, payouts and Treasury amounts",
			);
		}
		if Self::queue_length() != QueuedEpochs::<T>::get() {
			return Err("QueuedEpochs differs from the closed epochs in the queue");
		}
		let unpaid = T::Released::get().saturating_sub(settled);
		let required = activation
			.baseline
			.saturating_add(EscrowTotal::<T>::get())
			.saturating_add(unpaid);
		if !SettlementHalted::<T>::exists()
			&& T::Ledger::block_reward_pool().is_some_and(|pool| pool != required)
		{
			return Err("the ledger pool is not baseline + escrow + (L − S)");
		}
		Ok(())
	}

	/// Closed epochs from the head of the queue to its end.
	#[cfg(any(feature = "try-runtime", test))]
	fn queue_length() -> u32 {
		let mut next = Settlement::<T>::get().map(|cursor| cursor.cardano_epoch);
		let mut length = 0u32;
		while let Some(cardano_epoch) = next {
			length = length.saturating_add(1);
			next = NextClosed::<T>::get(cardano_epoch);
		}
		length
	}

	/// Treasury amounts of closed epochs whose Treasury entry is not settled yet.
	#[cfg(any(feature = "try-runtime", test))]
	fn treasury_queued() -> u128 {
		let mut next = Settlement::<T>::get().and_then(|cursor| match cursor.entry {
			SettlementEntry::Treasury => Some(cursor.cardano_epoch),
			_ => NextClosed::<T>::get(cursor.cardano_epoch),
		});
		let mut sum = 0u128;
		while let Some(cardano_epoch) = next {
			sum = sum.saturating_add(
				ClosedEpochs::<T>::get(cardano_epoch).map_or(0, |epoch| epoch.treasury),
			);
			next = NextClosed::<T>::get(cardano_epoch);
		}
		sum
	}
}
