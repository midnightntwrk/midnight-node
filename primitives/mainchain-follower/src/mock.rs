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

#[cfg(feature = "std")]
use crate::{MidnightNativeTokenObservationDataSource, PolicyUtxoRow};
#[cfg(feature = "std")]
use async_trait::async_trait;
use chrono::NaiveDateTime;

use sidechain_domain::*;

pub struct NativeTokenObservationDataSourceMock;

impl Default for NativeTokenObservationDataSourceMock {
	fn default() -> Self {
		Self::new()
	}
}

impl NativeTokenObservationDataSourceMock {
	pub fn new() -> Self {
		Self
	}
}

// Mock datum of expected registered user json datum
pub fn mock_datum() -> Vec<(Vec<u8>, Vec<u8>)> {
	vec![(
		vec![
			97, 100, 100, 114, 95, 116, 101, 115, 116, 49, 118, 112, 99, 57, 51, 107, 121, 116,
			116, 117, 112, 97, 113, 104, 120, 115, 103, 114, 104, 101, 114, 119, 110, 56, 108, 100,
			117, 115, 50, 52, 110, 116, 48, 110, 122, 110, 55, 110, 103, 52, 52, 57, 97, 48, 52,
			51, 113, 55, 106, 121, 108, 116, 50,
		],
		vec![
			4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
			4, 4, 4,
		],
	)]
}

fn mock_utxos() -> Vec<PolicyUtxoRow> {
	vec![
		PolicyUtxoRow {
			policy_hex: "03cf16101d110dcad9cacb225f0d1e63a8809979e7feb60426995414".into(),
			asset_name_hex: "".into(),
			quantity: 10,
			holder_address: "addr_test1vznffuwp3z6ffycnhx66pez2p2lquzegjgwuevhzjwt76rq4flmyr"
				.into(),
			ada_lovelace: 1500000,
			creating_tx_hash: "af4184b0cd32a0bdd61e4cbc10380d8b10ef5bcdbfef6a5b39c4bc1fb5d8143a"
				.into(),
			block_no: 3109787,
			time: NaiveDateTime::parse_from_str("2025-04-03T16:24:18", "%Y-%m-%dT%H:%M:%S")
				.unwrap(),
		},
		PolicyUtxoRow {
			policy_hex: "03cf16101d110dcad9cacb225f0d1e63a8809979e7feb60426995414".into(),
			asset_name_hex: "".into(),
			quantity: 10,
			holder_address: "addr_test1vpur8gm3nflg46c8v7wljzk6xchvelh9fy7vw4ftnhwelrcuqhes7"
				.into(),
			ada_lovelace: 1500000,
			creating_tx_hash: "1630415d45a52686043af195134fa1eed3169a9791a612e4e39660f8e765a89b"
				.into(),
			block_no: 3109803,
			time: NaiveDateTime::parse_from_str("2025-04-03T16:33:30", "%Y-%m-%dT%H:%M:%S")
				.unwrap(),
		},
	]
}

#[async_trait]
impl MidnightNativeTokenObservationDataSource for NativeTokenObservationDataSourceMock {
	async fn get_night_generates_dust_registrants_datum(
		&self,
		_address: &str,
		_min_block_no: i64,
		_max_block_no: i64,
	) -> Result<Vec<(Vec<u8>, Vec<u8>)>, Box<dyn std::error::Error + Send + Sync>> {
		Ok(mock_datum())
	}

	async fn get_spends_for_asset_in_block_range(
		&self,
		_policy_id_hex: &str,
		_asset_name: &str,
		_min_block_no: i64,
		_max_block_no: i64,
	) -> Result<Vec<PolicyUtxoRow>, Box<dyn std::error::Error + Send + Sync>> {
		Ok(mock_utxos())
	}

	async fn get_latest_block_no(&self) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
		Ok(3230000)
	}
}
