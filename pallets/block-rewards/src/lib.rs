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

//! Native NIGHT block rewards.
//!
//! Holds the reward parameters and their history, the activation record with the ledger's pool
//! baseline, the reserve counters `B_o` (outstanding), `D` (drawn) and `S` (settled), the
//! settlement halt and the settlement queue. The symbols are listed in
//! [`midnight_primitives_block_rewards`].
//!
//! ```text
//! B_o + D = reserve at activation           every draw moves STAR from B_o to D
//! S ≤ D                                     nothing is paid before it is drawn
//! S ≤ L                                     nothing is paid before the Cardano Reserve releases it
//! D − S = buckets + payouts + Treasury amounts not yet settled
//! ledger block_reward_pool ≥ baseline + escrow + (L − S)
//! ```
//!
//! Rewards are accounted per Cardano epoch: the epoch whose stake snapshot selected the block's
//! committee. Many Midnight epochs (committee sessions) share one Cardano epoch.
//!
//! From the block after activation, every block draws from the reserve in `on_finalize`: an
//! operator's block adds the producer's share to its `(Cardano epoch, operator)` bucket and the
//! rest to the open epoch's Treasury amount; a permissioned block draws nothing. Every block emits
//! a [`BlockRewardRecord`]. Must run after `pallet_session` (see `BlockProducer`) and after the
//! pallet that captures the block's utilization.
//!
//! Closed epochs join the settlement queue in the order they close (`settlement`). For each epoch,
//! settlement takes in turn: its Treasury amount, the split of each pool into the epoch's payouts,
//! the release of escrow, and the payouts in reward address order. Entries settle whole, only while
//! the released total covers them (`L − S`), up to `max_updates_per_block` a block. Settlement runs
//! in `on_idle`, in what the block's transactions left of the ledger's limit; its cost is left out
//! of the DUST price update and of the producer's utilization. An inconsistency between `L`, `S`,
//! the ledger's pool and the snapshot, or a payment the ledger rejects, halts settlement until
//! governance clears it.
//!
//! Monetary parameters have no governance call: they change only with a runtime upgrade, whose
//! migration calls [`Pallet::install_parameters`].

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

pub mod runtime_api;
pub mod weights;
pub use runtime_api::*;
pub use weights::WeightInfo;

mod accrual;
mod interfaces;
mod invariants;
#[cfg(test)]
mod mock;
mod settlement;
#[cfg(test)]
mod tests;
mod types;

pub use interfaces::*;
pub use types::*;

use frame_support::pallet_prelude::*;
use midnight_primitives_block_production::{BlockProducer, OperatorId};
pub use midnight_primitives_block_rewards::{
	BlockDraw, BlockRewardRecord, BlockUsage, CardanoEpoch, LockOwner, ParameterError, PoolSplit,
	RewardAddress, RewardParameters, RewardParametersV1, RewardPayee, SnapshotPayee, SnapshotPool,
	SplitError, StakeCredential, Utilization,
};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type GovernanceOrigin: EnsureOrigin<Self::RuntimeOrigin>;
		type Ledger: RewardLedger;
		type Prerequisites: ActivationPrerequisites;
		type BlockProducer: BlockProducer;
		/// The current block's timestamp.
		type Time: frame_support::traits::UnixTime;
		/// Cardano epoch whose snapshot selected the current committee. Read only after
		/// activation, which requires [`ActivationPrerequisites::cardano_epoch_source_live`].
		type CardanoEpoch: Get<CardanoEpoch>;
		type Snapshot: StakeSnapshot;
		/// The chain's slot duration in milliseconds; `γ` must count the blocks of a year at it.
		type SlotDurationMs: Get<u64>;
		/// `L`: NIGHT the Cardano Reserve has released for rewards so far, in STAR. Counts only
		/// NIGHT already credited to the ledger's `block_reward_pool`: the release is credited
		/// first and reported here after.
		type Released: Get<u128>;
		type WeightInfo: WeightInfo;
	}

	/// Every installed parameter version; records name the version they were computed with.
	#[pallet::storage]
	pub type ParameterSets<T> = StorageMap<_, Twox64Concat, u32, RewardParameters, OptionQuery>;

	#[pallet::storage]
	pub type CurrentParametersVersion<T> = StorageValue<_, u32, OptionQuery>;

	#[pallet::storage]
	pub type Activation<T: Config> =
		StorageValue<_, ActivationRecord<BlockNumberFor<T>>, OptionQuery>;

	#[pallet::storage]
	pub type SettlementHalted<T> = StorageValue<_, HaltReason, OptionQuery>;

	/// `B_o`: reserve not yet drawn by blocks.
	#[pallet::storage]
	pub type ReserveOutstanding<T> = StorageValue<_, u128, ValueQuery>;

	/// `D`: drawn by blocks so far.
	#[pallet::storage]
	pub type Drawn<T> = StorageValue<_, u128, ValueQuery>;

	/// `S`: paid to the ledger or held in escrow so far.
	#[pallet::storage]
	pub type Settled<T> = StorageValue<_, u128, ValueQuery>;

	#[pallet::storage]
	pub type OpenEpoch<T> = StorageValue<_, OpenEpochState, OptionQuery>;

	/// Closed epochs not settled yet; removed when the epoch's payouts are done.
	#[pallet::storage]
	pub type ClosedEpochs<T> = StorageMap<_, Twox64Concat, CardanoEpoch, ClosedEpoch, OptionQuery>;

	/// The epoch that closed after the key: the settlement queue's order.
	#[pallet::storage]
	pub type NextClosed<T> = StorageMap<_, Twox64Concat, CardanoEpoch, CardanoEpoch, OptionQuery>;

	#[pallet::storage]
	pub type LastClosed<T> = StorageValue<_, CardanoEpoch, OptionQuery>;

	/// The head of the settlement queue; `None` when every closed epoch is settled.
	#[pallet::storage]
	pub type Settlement<T> = StorageValue<_, SettlementCursor, OptionQuery>;

	/// What the block being executed used of the ledger's block limit, noted before the ledger's
	/// end-of-block update and removed in `on_finalize`.
	#[pallet::storage]
	pub type BlockUtilization<T> = StorageValue<_, BlockUsage, OptionQuery>;

	/// What the block's settlement used of the ledger's block limit, left out of the producer's
	/// utilization and of the DUST price update. Written in `on_idle`, removed in `on_finalize`.
	#[pallet::storage]
	pub type SettlementUsage<T> = StorageValue<_, BlockUsage, OptionQuery>;

	/// Where the next block's settlement starts: twice the last chunk tried, so a block that fits
	/// fewer entries than `max_updates_per_block` does not halve down to them every block.
	#[pallet::storage]
	pub type SettlementBudget<T> = StorageValue<_, u32, OptionQuery>;

	/// Producer shares accrued per operator in an epoch, in STAR.
	#[pallet::storage]
	pub type Buckets<T> = StorageDoubleMap<
		_,
		Twox64Concat,
		CardanoEpoch,
		// Unhashed, so an epoch's pools iterate in operator order.
		Identity,
		OperatorId,
		u128,
		ValueQuery,
	>;

	/// What each destination receives for an epoch, summed over the epoch's pools, until paid.
	#[pallet::storage]
	pub type Payouts<T> = StorageDoubleMap<
		_,
		Twox64Concat,
		CardanoEpoch,
		// Unhashed, so payouts iterate in destination order, which the ledger's own maps share.
		// Reward addresses are chosen by delegators, so long shared prefixes can deepen this
		// prefix's trie; every entry still needs a non-zero share of real stake.
		Identity,
		Destination,
		PayoutAmount,
		ValueQuery,
	>;

	/// Settled to a delegator that names no single reward address. It stays in the ledger's
	/// `block_reward_pool` and counts in `S` until released.
	#[pallet::storage]
	pub type Escrow<T> = StorageMap<_, Blake2_128Concat, StakeCredential, u128, ValueQuery>;

	#[pallet::storage]
	pub type EscrowTotal<T> = StorageValue<_, u128, ValueQuery>;

	/// Development and test networks install their first parameter version at genesis; live
	/// networks leave it empty and install parameters with a runtime upgrade.
	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub parameters: Option<RewardParametersV1>,
		#[serde(skip)]
		pub _marker: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			if let Some(parameters) = &self.parameters {
				Pallet::<T>::install_parameters(parameters.clone())
					.expect("genesis reward parameters are version 1 and valid");
			}
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		ParametersInstalled {
			version: u32,
		},
		Activated {
			parameters_version: u32,
			anchor_ms: u64,
			reserve: u128,
			baseline: u128,
		},
		SettlementHalted {
			reason: HaltReason,
		},
		/// `reconciliation` is the hash of the published reconciliation that justified clearing.
		HaltCleared {
			reason: HaltReason,
			reconciliation: [u8; 32],
		},
		BlockRewarded {
			record: BlockRewardRecord,
		},
		EpochClosed {
			cardano_epoch: CardanoEpoch,
			epoch: ClosedEpoch,
		},
		TreasuryPaid {
			cardano_epoch: CardanoEpoch,
			amount: u128,
		},
		/// A pool's total for the epoch is credited to its operator and payees.
		PoolSplit {
			cardano_epoch: CardanoEpoch,
			operator: OperatorId,
			pool_total: u128,
			operator_amount: u128,
		},
		/// One chunk of the epoch's payouts: its `DistributeNight` outputs, what went to escrow
		/// and what came out of it.
		RewardsDistributed {
			cardano_epoch: CardanoEpoch,
			outputs: u32,
			amount: u128,
			escrowed: u128,
			released: u128,
		},
		EpochSettled {
			cardano_epoch: CardanoEpoch,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		AlreadyActivated,
		NoParameters,
		ReserveObservationNotLive,
		CardanoEpochSourceNotLive,
		NotHalted,
		/// The ledger's `block_reward_pool` or `reserve_pool` could not be read.
		LedgerPoolUnreadable,
		/// The ledger's `block_reward_pool` holds less than the released total `L`.
		LedgerPoolBelowReleased,
		/// The ledger's `reserve_pool` plus the released total `L` is not `initial_reward_reserve_star`.
		ReservePoolMismatch,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			if !Self::is_activated() {
				// `Activation`, read again by `on_finalize`; the activation block's own takes are
				// in `activate`'s weight.
				return T::DbWeight::get().reads(1);
			}
			T::WeightInfo::accrue()
		}

		fn on_idle(_: BlockNumberFor<T>, remaining: Weight) -> Weight {
			let check = T::DbWeight::get().reads(1);
			if !remaining.all_gte(check) || !Self::is_activated() {
				return Weight::zero();
			}
			check.saturating_add(Self::settle_idle(remaining.saturating_sub(check)))
		}

		fn on_finalize(now: BlockNumberFor<T>) {
			// Nothing writes the two usages before activation.
			let Some(activation) = Activation::<T>::get() else {
				return;
			};
			let settlement = SettlementUsage::<T>::take().unwrap_or_default();
			let utilization =
				BlockUtilization::<T>::take().map(|usage| usage.utilization_after(&settlement));
			if activation.at < now && Self::now_ms() >= activation.anchor_ms {
				Self::accrue(utilization);
			}
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(
			_: BlockNumberFor<T>,
		) -> Result<(), frame_support::sp_runtime::TryRuntimeError> {
			Self::do_try_state().map_err(Into::into)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Starts rewards with the current parameters, from the first later block at or after the
		/// reserve release anchor.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::activate())]
		pub fn activate(origin: OriginFor<T>) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			ensure!(Activation::<T>::get().is_none(), Error::<T>::AlreadyActivated);
			let version = CurrentParametersVersion::<T>::get().ok_or(Error::<T>::NoParameters)?;
			let parameters = Self::parameters(version).ok_or(Error::<T>::NoParameters)?;
			let anchor_ms = T::Prerequisites::reserve_anchor_ms()
				.ok_or(Error::<T>::ReserveObservationNotLive)?;
			ensure!(
				T::Prerequisites::cardano_epoch_source_live(),
				Error::<T>::CardanoEpochSourceNotLive
			);

			// What the pool holds besides released rewards stays out of settlement's reach.
			let released = T::Released::get();
			let pool = T::Ledger::block_reward_pool().ok_or(Error::<T>::LedgerPoolUnreadable)?;
			let baseline = pool.checked_sub(released).ok_or(Error::<T>::LedgerPoolBelowReleased)?;
			let reserve = parameters.initial_reward_reserve_star;
			let reserve_pool = T::Ledger::reserve_pool().ok_or(Error::<T>::LedgerPoolUnreadable)?;
			ensure!(
				reserve_pool.checked_add(released) == Some(reserve),
				Error::<T>::ReservePoolMismatch
			);
			Activation::<T>::put(ActivationRecord {
				at: frame_system::Pallet::<T>::block_number(),
				anchor_ms,
				reserve,
				baseline,
			});
			ReserveOutstanding::<T>::put(reserve);
			Drawn::<T>::put(0);
			Settled::<T>::put(0);
			Self::deposit_event(Event::Activated {
				parameters_version: version,
				anchor_ms,
				reserve,
				baseline,
			});
			Ok(())
		}

		/// Lets settlement retry the step that halted it. A halt whose cause is in storage or in
		/// the snapshot (a pool's payees that do not match its bucket, a lock without an address,
		/// a pool missing from a complete snapshot) halts again until a runtime upgrade repairs
		/// that state; `reconciliation` names the off-chain record of what was checked or repaired.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::clear_halt())]
		pub fn clear_halt(origin: OriginFor<T>, reconciliation: [u8; 32]) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			let reason = SettlementHalted::<T>::take().ok_or(Error::<T>::NotHalted)?;
			Self::deposit_event(Event::HaltCleared { reason, reconciliation });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Validates and stores the next parameter version and makes it current. For runtime
		/// upgrade migrations.
		pub fn install_parameters(parameters: RewardParametersV1) -> Result<(), InstallError> {
			let expected = CurrentParametersVersion::<T>::get().map_or(1, |v| v.saturating_add(1));
			if parameters.version != expected {
				return Err(InstallError::VersionNotNext { expected });
			}
			parameters.validate(T::SlotDurationMs::get()).map_err(InstallError::Invalid)?;
			if Self::current_parameters().is_some_and(|current| {
				current.initial_reward_reserve_star != parameters.initial_reward_reserve_star
			}) {
				return Err(InstallError::ReserveChanged);
			}
			ParameterSets::<T>::insert(expected, RewardParameters::V1(parameters));
			CurrentParametersVersion::<T>::put(expected);
			Self::deposit_event(Event::ParametersInstalled { version: expected });
			Ok(())
		}

		pub fn current_parameters() -> Option<RewardParametersV1> {
			CurrentParametersVersion::<T>::get().and_then(Self::parameters)
		}

		pub fn parameters(version: u32) -> Option<RewardParametersV1> {
			ParameterSets::<T>::get(version).map(RewardParameters::into_v1)
		}

		pub fn is_activated() -> bool {
			Activation::<T>::exists()
		}

		pub(crate) fn now_ms() -> u64 {
			u64::try_from(<T::Time as frame_support::traits::UnixTime>::now().as_millis())
				.unwrap_or(u64::MAX)
		}

		pub fn state() -> RewardState<BlockNumberFor<T>> {
			RewardState {
				activation: Activation::<T>::get(),
				parameters_version: CurrentParametersVersion::<T>::get(),
				reserve_outstanding: ReserveOutstanding::<T>::get(),
				drawn: Drawn::<T>::get(),
				settled: Settled::<T>::get(),
				released: T::Released::get(),
				settlement: Settlement::<T>::get(),
				escrow_total: EscrowTotal::<T>::get(),
				halted: SettlementHalted::<T>::get(),
				open_epoch: OpenEpoch::<T>::get(),
			}
		}

		/// Records what the block being executed used of the ledger's block limit, for its
		/// `on_finalize`.
		pub fn note_block_usage(usage: BlockUsage) {
			BlockUtilization::<T>::put(usage);
		}

		/// Stops payouts until governance clears it; accrual continues. The first reason is kept
		/// while halted.
		pub(crate) fn halt(reason: HaltReason) {
			if SettlementHalted::<T>::exists() {
				return;
			}
			SettlementHalted::<T>::put(reason);
			Self::deposit_event(Event::SettlementHalted { reason });
		}
	}
}
