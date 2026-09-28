// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//! Block rewards; operator signatures use recoverable secp256k1 over Blake2b-256(b"midnight:rewards-operator" || skh).
//! Pool members receive floor((S - floor(S*m)) * stake / total snapshot stake); owners' stake stands for the pledge and earns no member share.
//! The reward account receives S minus member shares: the margin, the owners' share and rounding; zero total stake pays it all.
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;
use alloc::vec::Vec;
use frame_support::{pallet_prelude::*, traits::Get};
use frame_system::pallet_prelude::*;
use midnight_primitives_block_rewards::*;
pub use pallet::*;
use sp_runtime::{AccountId32, traits::Keccak256};

pub trait AuthorPool {
	fn pool_for_author(author: &AccountId32) -> Option<PoolId>;
}

/// Verify the operator's stake-key binding and derive its runtime account.
pub fn paired_author(account: &RewardAccount) -> Option<AccountId32> {
	let keys = &account.registration.as_ref()?.operator_keys;
	let key: [u8; 33] =
		keys.iter().find(|(k, _)| k == b"sidechain")?.1.as_slice().try_into().ok()?;
	let sig: [u8; 65] =
		keys.iter().find(|(k, _)| k == b"sidechain_sig")?.1.as_slice().try_into().ok()?;
	let mut message = b"midnight:rewards-operator".to_vec();
	message.extend_from_slice(&account.skh);
	sp_io::crypto::ecdsa_verify(
		&sp_core::ecdsa::Signature::from_raw(sig),
		&message,
		&sp_core::ecdsa::Public::from_raw(key),
	)
	.then(|| AccountId32::new(sp_io::hashing::blake2_256(&key)))
}

/// Encode the contracts' 45-byte payment leaf.
pub fn reward_leaf(skh: StakeKeyHash, balance: u128) -> [u8; 45] {
	let mut leaf = [0; 45];
	leaf[1..29].copy_from_slice(&skh);
	leaf[29..].copy_from_slice(&balance.to_be_bytes());
	leaf
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	#[pallet::config]
	pub trait Config: frame_system::Config<AccountId = AccountId32> {
		type AuthorPool: AuthorPool;
	}
	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub type VirtualAccountPolicy<T> = StorageValue<_, [u8; 28]>;
	#[pallet::storage]
	pub type Reserve<T> = StorageValue<_, u128, ValueQuery>;
	#[pallet::storage]
	pub type Rate<T> = StorageValue<_, (u128, u128), ValueQuery>;
	#[pallet::storage]
	pub type FixedShare<T> = StorageValue<_, (u128, u128), ValueQuery>;
	#[pallet::storage]
	pub type DistFee<T> = StorageValue<_, u128, ValueQuery>;
	#[pallet::storage]
	pub type MaxLeaves<T> = StorageValue<_, u32, ValueQuery>;
	#[pallet::storage]
	pub type FundedFloor<T> = StorageValue<_, u128, ValueQuery>;
	#[pallet::storage]
	pub type Accounts<T> = StorageValue<_, Vec<RewardAccount>, ValueQuery>;
	#[pallet::storage]
	pub type AuthorAccrued<T> = StorageMap<_, Blake2_128Concat, AccountId32, u128, ValueQuery>;
	#[pallet::storage]
	pub type PoolAccrued<T> = StorageMap<_, Blake2_128Concat, PoolId, u128, ValueQuery>;
	#[pallet::storage]
	pub type Balances<T> = StorageMap<_, Blake2_128Concat, StakeKeyHash, u128, ValueQuery>;
	#[pallet::storage]
	pub type EpochTreasury<T> = StorageValue<_, u128, ValueQuery>;
	#[pallet::storage]
	pub type CurrentAuthor<T> = StorageValue<_, AccountId32>;
	#[pallet::storage]
	pub type RotationCursor<T> = StorageValue<_, StakeKeyHash, ValueQuery>;
	#[pallet::storage]
	pub type EpochLeaves<T> = StorageMap<_, Twox64Concat, u64, Vec<[u8; 45]>, ValueQuery>;
	#[pallet::storage]
	pub type PendingDigest<T> = StorageValue<_, RewardsDigest>;
	#[pallet::storage]
	pub type DigestBlock<T: Config> = StorageMap<_, Twox64Concat, u64, BlockNumberFor<T>>;
	#[pallet::storage]
	pub type ClosingEpoch<T> = StorageValue<_, u64>;
	#[pallet::storage]
	pub type ObservedEpoch<T> = StorageValue<_, u64>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub config: BlockRewardsConfig,
		#[serde(skip)]
		pub _config: core::marker::PhantomData<T>,
	}
	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			let c = &self.config;
			assert!(c.rd > 0 && c.rn <= c.rd && c.sd > 0 && c.sn <= c.sd && c.max_leaves > 0);
			if let Some(policy) = c.virtual_account_policy {
				VirtualAccountPolicy::<T>::put(policy);
			}
			Reserve::<T>::put(c.reserve);
			Rate::<T>::put((c.rn, c.rd));
			FixedShare::<T>::put((c.sn, c.sd));
			DistFee::<T>::put(c.dist_fee);
			MaxLeaves::<T>::put(c.max_leaves);
			FundedFloor::<T>::put(c.funded_floor);
		}
	}
	#[pallet::error]
	pub enum Error<T> {
		UnexpectedDigest,
		UnexpectedObservation,
		InvalidAccounts,
	}
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			T::DbWeight::get().reads_writes(8, 4)
		}
		fn on_finalize(_: BlockNumberFor<T>) {
			if !VirtualAccountPolicy::<T>::exists() {
				if let Some(epoch) = ClosingEpoch::<T>::take() {
					Self::build_tree(epoch, Vec::new());
				}
			}
			let Some(author) = CurrentAuthor::<T>::take() else { return };
			let reserve = Reserve::<T>::get();
			let (rn, rd) = Rate::<T>::get();
			let (sn, sd) = FixedShare::<T>::get();
			let nb = mul_div(reserve, rn, rd);
			let nf = mul_div(nb, sn, sd);
			let normal =
				frame_system::Pallet::<T>::block_weight().get(DispatchClass::Normal).ref_time();
			let maximum = T::BlockWeights::get()
				.get(DispatchClass::Normal)
				.max_total
				.expect("normal block weight limit configured")
				.ref_time();
			let nv = mul_div(nb - nf, normal.into(), maximum.into());
			let na = nf + nv;
			if let Some(pool) = T::AuthorPool::pool_for_author(&author) {
				PoolAccrued::<T>::mutate(pool, |amount| *amount += na);
			} else {
				AuthorAccrued::<T>::mutate(author, |amount| *amount += na);
			}
			EpochTreasury::<T>::mutate(|amount| *amount += nb - na);
			Reserve::<T>::put(reserve - nb);
		}
	}
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Include the completed epoch's rewards digest.
		#[pallet::call_index(0)]
		#[pallet::weight((T::DbWeight::get().reads_writes(1, 2), DispatchClass::Mandatory))]
		pub fn submit_rewards_digest(
			origin: OriginFor<T>,
			epoch: u64,
			leaf_count: u64,
			root: [u8; 32],
			min_key: [u8; 28],
			max_key: [u8; 28],
			treasury_total: u128,
		) -> DispatchResult {
			ensure_none(origin)?;
			let digest =
				RewardsDigest { epoch, leaf_count, root, min_key, max_key, treasury_total };
			ensure!(PendingDigest::<T>::get() == Some(digest), Error::<T>::UnexpectedDigest);
			PendingDigest::<T>::kill();
			DigestBlock::<T>::insert(epoch, frame_system::Pallet::<T>::block_number());
			Ok(())
		}
		/// Observe the virtual accounts at the first block of an epoch.
		#[pallet::call_index(1)]
		#[pallet::weight((T::DbWeight::get().reads_writes(4 + accounts.len() as u64 * 2 + pool_snapshots.iter().map(|pool| 2 + pool.delegators.len() as u64).sum::<u64>(), 4 + accounts.len() as u64 * 2 + pool_snapshots.iter().map(|pool| 2 + pool.delegators.len() as u64).sum::<u64>()), DispatchClass::Mandatory))]
		pub fn note_reward_accounts(
			origin: OriginFor<T>,
			epoch: u64,
			accounts: Vec<RewardAccount>,
			pool_snapshots: Vec<PoolSnapshot>,
		) -> DispatchResult {
			ensure_none(origin)?;
			ensure!(
				ObservedEpoch::<T>::get().is_none_or(|previous| epoch > previous),
				Error::<T>::UnexpectedObservation
			);
			ensure!(accounts.windows(2).all(|w| w[0].skh < w[1].skh), Error::<T>::InvalidAccounts);
			Accounts::<T>::put(accounts);
			ObservedEpoch::<T>::put(epoch);
			if let Some(closing) = ClosingEpoch::<T>::take() {
				Self::build_tree(closing, pool_snapshots);
			}
			Ok(())
		}
	}
	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = InherentError;
		const INHERENT_IDENTIFIER: sp_inherents::InherentIdentifier = ACCOUNTS_INHERENT_IDENTIFIER;
		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			data.get_data::<RewardAccountsData>(&ACCOUNTS_INHERENT_IDENTIFIER)
				.expect("valid reward observation encoding")
				.map(|d| Call::note_reward_accounts {
					epoch: d.epoch,
					accounts: d.accounts,
					pool_snapshots: d.pool_snapshots,
				})
		}
		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::note_reward_accounts { .. } | Call::submit_rewards_digest { .. })
		}
		fn is_inherent_required(data: &InherentData) -> Result<Option<Self::Error>, Self::Error> {
			let data = data
				.get_data::<RewardAccountsData>(&ACCOUNTS_INHERENT_IDENTIFIER)
				.map_err(|_| InherentError::DecodeFailed)?;
			Ok(data.map(|_| InherentError::MissingAccounts))
		}
		fn check_inherent(call: &Self::Call, data: &InherentData) -> Result<(), Self::Error> {
			match call {
				Call::note_reward_accounts { epoch, accounts, pool_snapshots } => {
					let expected = data
						.get_data::<RewardAccountsData>(&ACCOUNTS_INHERENT_IDENTIFIER)
						.map_err(|_| InherentError::DecodeFailed)?
						.ok_or(InherentError::MissingAccounts)?;
					if *epoch != expected.epoch
						|| *accounts != expected.accounts
						|| *pool_snapshots != expected.pool_snapshots
					{
						return Err(InherentError::AccountsMismatch);
					}
				},
				Call::submit_rewards_digest {
					epoch,
					leaf_count,
					root,
					min_key,
					max_key,
					treasury_total,
				} => {
					let digest = RewardsDigest {
						epoch: *epoch,
						leaf_count: *leaf_count,
						root: *root,
						min_key: *min_key,
						max_key: *max_key,
						treasury_total: *treasury_total,
					};
					if PendingDigest::<T>::get() != Some(digest) {
						return Err(InherentError::DigestMismatch);
					}
				},
				_ => (),
			}
			Ok(())
		}
	}
	impl<T: Config> Pallet<T> {
		/// Read the configured virtual-account policy.
		pub fn virtual_account_policy() -> Option<[u8; 28]> {
			VirtualAccountPolicy::<T>::get()
		}
		/// Close an epoch with its boundary observation.
		pub fn close_epoch(epoch: u64) -> Weight {
			ClosingEpoch::<T>::put(epoch);
			T::DbWeight::get().reads_writes(1, 1)
		}
		/// Select the bounded rotation and retain its sorted leaves.
		fn build_tree(epoch: u64, pool_snapshots: Vec<PoolSnapshot>) {
			for pool in pool_snapshots {
				let accrued = PoolAccrued::<T>::take(pool.pool_id);
				let member_total =
					accrued - mul_div(accrued, pool.margin.into(), MARGIN_DENOMINATOR);
				let sigma = pool.delegators.iter().map(|(_, stake)| stake).sum::<u128>();
				let mut operator = accrued;
				if sigma != 0 {
					for (skh, stake) in pool.delegators {
						if pool.owners.binary_search(&skh).is_ok() {
							continue;
						}
						let share = mul_div(member_total, stake, sigma);
						Balances::<T>::mutate(skh, |balance| *balance += share);
						operator -= share;
					}
				}
				Balances::<T>::mutate(pool.reward_account, |balance| *balance += operator);
			}
			let accounts = Accounts::<T>::get();
			for account in &accounts {
				if let Some(author) = paired_author(account) {
					let accrued = AuthorAccrued::<T>::take(author);
					Balances::<T>::mutate(account.skh, |balance| *balance += accrued);
				}
			}
			let cursor = RotationCursor::<T>::get();
			let start = accounts.partition_point(|a| a.skh < cursor);
			let floor = FundedFloor::<T>::get();
			let fee = DistFee::<T>::get();
			let cap = MaxLeaves::<T>::get() as usize;
			let mut selected = Vec::new();
			for (i, account) in accounts.iter().enumerate().cycle().skip(start).take(accounts.len())
			{
				RotationCursor::<T>::put(accounts[(i + 1) % accounts.len()].skh);
				let Some(registration) = &account.registration else { continue };
				let balance = Balances::<T>::get(account.skh);
				if !account.committed
					&& account.deposit >= floor
					&& registration
						.payout_threshold
						.and_then(|threshold| fee.checked_add(threshold))
						.is_some_and(|threshold| balance >= threshold)
				{
					selected.push((account.skh, reward_leaf(account.skh, balance)));
					Balances::<T>::remove(account.skh);
					if selected.len() == cap {
						break;
					}
				}
			}
			selected.sort_unstable_by_key(|(skh, _)| *skh);
			let min_key = selected.first().map(|(key, _)| *key).unwrap_or([0; 28]);
			let max_key = selected.last().map(|(key, _)| *key).unwrap_or([0; 28]);
			let leaves: Vec<_> = selected.into_iter().map(|(_, leaf)| leaf).collect();
			let root = binary_merkle_tree::merkle_root::<Keccak256, _>(&leaves).into();
			let digest = RewardsDigest {
				epoch,
				leaf_count: leaves.len() as u64,
				root,
				min_key,
				max_key,
				treasury_total: EpochTreasury::<T>::take(),
			};
			EpochLeaves::<T>::insert(epoch, leaves);
			PendingDigest::<T>::put(digest);
		}
	}
}
/// Multiply before dividing without overflowing the intermediate product.
fn mul_div(value: u128, numerator: u128, denominator: u128) -> u128 {
	((sp_core::U256::from(value) * sp_core::U256::from(numerator))
		/ sp_core::U256::from(denominator))
	.as_u128()
}
impl<T: Config> pallet_authorship::EventHandler<AccountId32, BlockNumberFor<T>> for Pallet<T> {
	fn note_author(author: AccountId32) {
		CurrentAuthor::<T>::put(author);
	}
}
#[cfg(test)]
mod tests;
