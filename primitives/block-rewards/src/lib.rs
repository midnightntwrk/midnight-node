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
//! Block-production rewards observation and digest types.
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;
use alloc::vec::Vec;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;

pub const ACCOUNTS_INHERENT_IDENTIFIER: sp_inherents::InherentIdentifier = *b"brewarda";
pub type StakeKeyHash = [u8; 28];
pub type PoolId = [u8; 28];
pub const MARGIN_DENOMINATOR: u128 = 1_000_000_000;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct PoolSnapshot {
	pub pool_id: PoolId,
	/// Pool margin in parts per billion.
	pub margin: u32,
	pub reward_account: StakeKeyHash,
	pub owners: Vec<StakeKeyHash>,
	pub delegators: Vec<(StakeKeyHash, u128)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct Registration {
	pub operator_keys: Vec<(Vec<u8>, Vec<u8>)>,
	pub payout_threshold: Option<u128>,
}
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct RewardAccount {
	pub skh: StakeKeyHash,
	pub deposit: u128,
	pub committed: bool,
	pub registration: Option<Registration>,
}
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct RewardAccountsData {
	pub epoch: u64,
	pub accounts: Vec<RewardAccount>,
	pub pool_snapshots: Vec<PoolSnapshot>,
}
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct RewardsDigest {
	pub epoch: u64,
	pub leaf_count: u64,
	pub root: [u8; 32],
	pub min_key: StakeKeyHash,
	pub max_key: StakeKeyHash,
	pub treasury_total: u128,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct BlockRewardsConfig {
	pub virtual_account_policy: Option<[u8; 28]>,
	pub reserve: u128,
	pub rn: u128,
	pub rd: u128,
	pub sn: u128,
	pub sd: u128,
	pub dist_fee: u128,
	pub max_leaves: u32,
	pub funded_floor: u128,
}
impl Default for BlockRewardsConfig {
	fn default() -> Self {
		Self {
			virtual_account_policy: None,
			reserve: 0,
			rn: 7,
			rd: 438_000_000,
			sn: 95,
			sd: 100,
			dist_fee: 1,
			max_leaves: 5000,
			funded_floor: 3_000_000,
		}
	}
}
#[derive(Debug, Encode, Decode)]
pub enum InherentError {
	DecodeFailed,
	AccountsMismatch,
	MissingAccounts,
	DigestMismatch,
}
impl sp_inherents::IsFatalError for InherentError {
	fn is_fatal_error(&self) -> bool {
		true
	}
}
sp_api::decl_runtime_apis! {
	pub trait BlockRewardsApi {
		fn virtual_account_policy() -> Option<[u8; 28]>;
		fn accrued_pools() -> Vec<PoolId>;
	}
}
