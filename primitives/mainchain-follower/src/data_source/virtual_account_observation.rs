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

use crate::{
	VirtualAccountObservationDataSource,
	db::queries::virtual_account_observation::get_virtual_account_utxos,
};
use cardano_serialization_lib::{PlutusData, PlutusList};
use lru::LruCache;
use midnight_primitives_block_rewards::{Registration, RewardAccount};
use sidechain_domain::McBlockHash;
use sqlx::PgPool;
use std::{collections::BTreeMap, error::Error, num::NonZeroUsize, sync::Mutex};

type ObservationError = Box<dyn Error + Send + Sync>;
type CacheKey = (McBlockHash, [u8; 28]);

pub struct VirtualAccountObservationDataSourceImpl {
	pool: PgPool,
	cache: Mutex<LruCache<CacheKey, Vec<RewardAccount>>>,
}

impl VirtualAccountObservationDataSourceImpl {
	pub fn new(pool: PgPool, cache_size: NonZeroUsize) -> Self {
		Self { pool, cache: Mutex::new(LruCache::new(cache_size)) }
	}
}

#[async_trait::async_trait]
impl VirtualAccountObservationDataSource for VirtualAccountObservationDataSourceImpl {
	async fn get_reward_accounts(
		&self,
		policy: &[u8; 28],
		mc_block_hash: &McBlockHash,
	) -> Result<Vec<RewardAccount>, ObservationError> {
		let key = (mc_block_hash.clone(), *policy);
		if let Some(accounts) = self.cache.lock().unwrap().get(&key) {
			return Ok(accounts.clone());
		}
		let block = crate::db::get_block_by_hash(&self.pool, mc_block_hash.clone())
			.await?
			.ok_or("Virtual account observation block not found")?;
		let utxos = get_virtual_account_utxos(&self.pool, policy, block.block_number.0).await?;
		let mut deposits = BTreeMap::new();
		let mut registrations = BTreeMap::new();
		for utxo in utxos {
			let skh: [u8; 28] = utxo.asset_name[1..].try_into()?;
			if skh == [0xff; 28] {
				continue;
			}
			match utxo.asset_name[0] {
				0 => {
					let fields = constructor(&utxo.full_datum.0, 1, 3)?;
					let committed = fields
						.get(2)
						.as_constr_plutus_data()
						.ok_or("Expected deposit commitment option")?;
					let committed = match u64::from(committed.alternative()) {
						0 => true,
						1 => false,
						_ => return Err("Invalid deposit commitment option".into()),
					};
					deposits.insert(skh, (utxo.lovelace.parse::<u128>()?, committed));
				},
				1 => {
					let fields = constructor(&utxo.full_datum.0, 2, 4)?;
					let keys = fields.get(2).as_map().ok_or("Expected operator key map")?;
					let mut operator_keys = Vec::new();
					for key in keys.keys().into_iter() {
						let values = keys.get(key).ok_or("Missing operator key value")?;
						for index in 0..values.len() {
							let value = values.get(index).ok_or("Missing operator key value")?;
							operator_keys.push((
								key.as_bytes().ok_or("Expected operator key name bytes")?,
								value.as_bytes().ok_or("Expected operator key value bytes")?,
							));
						}
					}
					let payout_threshold = payout_threshold(&fields.get(3))?;
					registrations.insert(skh, Registration { operator_keys, payout_threshold });
				},
				_ => unreachable!("Query selects deposit and registration NFTs"),
			}
		}
		let accounts: Vec<_> = deposits
			.into_iter()
			.map(|(skh, (deposit, committed))| RewardAccount {
				skh,
				deposit,
				committed,
				registration: registrations.remove(&skh),
			})
			.collect();
		self.cache.lock().unwrap().put(key, accounts.clone());
		Ok(accounts)
	}
}

/// Decode the expected account datum constructor at the observation boundary.
fn constructor(
	data: &PlutusData,
	alternative: u64,
	length: usize,
) -> Result<PlutusList, ObservationError> {
	let constructor = data.as_constr_plutus_data().ok_or("Expected account datum constructor")?;
	let fields = constructor.data();
	if u64::from(constructor.alternative()) != alternative || fields.len() != length {
		return Err("Invalid account datum constructor or field count".into());
	}
	Ok(fields)
}

#[derive(Clone, Debug, Default)]
pub struct VirtualAccountObservationDataSourceMock;

#[async_trait::async_trait]
impl VirtualAccountObservationDataSource for VirtualAccountObservationDataSourceMock {
	async fn get_reward_accounts(
		&self,
		_policy: &[u8; 28],
		_mc_block_hash: &McBlockHash,
	) -> Result<Vec<RewardAccount>, ObservationError> {
		Ok(vec![])
	}
}

/// Preserve thresholds above the largest representable balance as ineligible.
fn payout_threshold(data: &PlutusData) -> Result<Option<u128>, ObservationError> {
	let integer = data.as_integer().ok_or("Expected payout threshold integer")?;
	match integer.to_str().parse::<u128>() {
		Ok(value) => Ok(Some(value)),
		Err(error) if *error.kind() == std::num::IntErrorKind::PosOverflow => Ok(None),
		Err(error) => Err(error.into()),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use cardano_serialization_lib::BigInt;

	#[test]
	fn threshold_preserves_the_balance_boundary() {
		let datum = |value: &str| PlutusData::new_integer(&BigInt::from_str(value).unwrap());
		assert_eq!(payout_threshold(&datum("0")).unwrap(), Some(0));
		assert_eq!(payout_threshold(&datum(&u128::MAX.to_string())).unwrap(), Some(u128::MAX));
		assert_eq!(
			payout_threshold(&datum("340282366920938463463374607431768211456")).unwrap(),
			None
		);
		assert!(payout_threshold(&datum("-1")).is_err());
	}
}
