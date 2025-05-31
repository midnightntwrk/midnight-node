// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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

#![cfg_attr(not(feature = "std"), no_std)]

pub mod idp;

#[cfg(feature = "std")]
pub mod db_model;

#[cfg(feature = "std")]
pub mod db_datum;

#[cfg(feature = "std")]
pub mod data_source;

#[cfg(feature = "std")]
mod mock;

#[cfg(feature = "std")]
use crate::db_model::PolicyUtxoRow;

#[cfg(feature = "std")]
pub use {
	data_source::MidnightNativeTokenObservationDataSourceImpl,
	db_sync_follower,
	idp::MidnightObservationTokenMovement,
	inherent_provider::*,
	mock::{NativeTokenObservationDataSourceMock, mock_datum},
};

use sp_std::boxed::Box;

#[cfg(feature = "std")]
pub mod inherent_provider {
	use super::*;

	#[async_trait::async_trait]
	// Simple wrapper trait for partnerchains native token management trait
	pub trait MidnightNativeTokenObservationDataSource {
		async fn get_night_generates_dust_registrants_datum(
			&self,
			address: &str,
			min_block_no: i64,
			max_block_no: i64,
		) -> Result<Vec<(Vec<u8>, Vec<u8>)>, Box<dyn std::error::Error + Send + Sync>>;

		async fn get_spends_for_asset_in_block_range(
			&self,
			policy_id_hex: &str,
			asset_name: &str,
			min_block_no: i64,
			max_block_no: i64,
		) -> Result<Vec<PolicyUtxoRow>, Box<dyn std::error::Error + Send + Sync>>;

		async fn get_latest_block_no(
			&self,
		) -> Result<i64, Box<dyn std::error::Error + Send + Sync>>;
	}

	#[derive(Clone, Debug)]
	// Extended mainchain scripts
	pub struct MidnightMainChainScripts {
		pub registrants_list_contract: String,
	}
}
