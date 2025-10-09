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

use documented::{Documented, DocumentedFields as _};
use serde::{Deserialize, Serialize};
use serde_valid::{Validate, validation};
use sidechain_domain::mainchain_epoch::MainchainEpochConfig;

use super::validation_utils::{maybe, path_exists};
use super::{CfgHelp, HelpField, cfg_help, error::CfgError, util::get_keys};

#[derive(Debug, Clone, Serialize, Deserialize, Default, Validate, Documented)]
#[validate(custom = main_chain_follower_vars)]
/// Parameters specific to Midnight
pub struct MidnightCfg {
	/// On start-up, wipe the chain
	pub wipe_chain_state: bool,

	/// Path to file containing a secret string to use as the AURA seed (32 bytes)
	/// Seed should be either a Phrase, hexadecimal string, or ss58-compatible string.
	/// Docs: https://paritytech.github.io/polkadot-sdk/master/sp_core/crypto/struct.AddressUri.html#structfield.phrase
	pub aura_seed_file: Option<String>,

	/// Path to file containing a secret string to use as the GRANDPA seed (32 bytes)
	/// Seed should be either a Phrase, hexadecimal string, or ss58-compatible string.
	/// Docs: https://paritytech.github.io/polkadot-sdk/master/sp_core/crypto/struct.AddressUri.html#structfield.phrase
	pub grandpa_seed_file: Option<String>,

	/// Path to file containing a secret string to use as the CROSS_CHAIN seed (32 bytes)
	/// Seed should be either a Phrase, hexadecimal string, or ss58-compatible string.
	/// Docs: https://paritytech.github.io/polkadot-sdk/master/sp_core/crypto/struct.AddressUri.html#structfield.phrase
	pub cross_chain_seed_file: Option<String>,

	/// Mock ariadne parameters
	pub use_main_chain_follower_mock: bool,
	/// Required if use_main_chain_follower_mock is true
	/// Used in the sidechains library
	#[validate(custom = |s| maybe(s, path_exists))]
	pub mock_registrations_file: Option<String>,

	/// see partner-chains EpochConfig
	#[serde(rename = "mc__first_epoch_timestamp_millis")]
	pub mc_first_epoch_timestamp_millis: u64,
	/// see partner-chains EpochConfig
	#[serde(rename = "mc__first_epoch_number")]
	pub mc_first_epoch_number: u32,
	/// see partner-chains EpochConfig
	#[serde(rename = "mc__epoch_duration_millis")]
	pub mc_epoch_duration_millis: u64,
	/// see partner-chains EpochConfig
	#[serde(rename = "mc__first_slot_number")]
	pub mc_first_slot_number: u64,
	/// see partner-chains EpochConfig
	#[serde(rename = "mc__slot_duration_millis")]
	pub mc_slot_duration_millis: u64,

	/// see partner-chains ConnectionConfig
	#[doc_tag(secret)]
	pub db_sync_postgres_connection_string: Option<String>,

	/// see partner-chains CandidateDataSourceCacheConfig and DbSyncBlockDataSourceConfig
	pub cardano_security_parameter: Option<u32>,

	/// see partner-chains DbSyncBlockDataSourceConfig
	pub cardano_active_slots_coeff: Option<f64>,

	/// see partner-chains DbSyncBlockDataSourceConfig
	pub block_stability_margin: Option<u32>,

	/// Size of ledger storage cache (number of nodes)
	pub storage_cache_size: usize,
}

fn main_chain_follower_vars(cfg: &MidnightCfg) -> Result<(), validation::Error> {
	let missing = |field: &str| {
		validation::Error::Custom(format!(
			"{field} must be defined if ariadne is enabled (i.e. if use_main_chain_follower_mock is false)"
		))
	};

	if cfg.use_main_chain_follower_mock {
		if cfg.mock_registrations_file.is_none() {
			return Err(validation::Error::Custom(
				"mock_registrations_file must be defined if use_main_chain_follower_mock is true."
					.to_string(),
			));
		}
	} else {
		if cfg.db_sync_postgres_connection_string.is_none() {
			return Err(missing("db_sync_postgres_connection_string"));
		}
		if cfg.cardano_security_parameter.is_none() {
			return Err(missing("cardano_security_parameter"));
		}
		if cfg.cardano_active_slots_coeff.is_none() {
			return Err(missing("cardano_active_slots_coeff"));
		}
		if cfg.block_stability_margin.is_none() {
			return Err(missing("block_stability_margin"));
		}
	}
	Ok(())
}

impl CfgHelp for MidnightCfg {
	fn help(cur_cfg: Option<&config::Config>) -> Result<Vec<HelpField>, CfgError> {
		cfg_help!(cur_cfg, Self)
	}
}

impl From<MidnightCfg> for MainchainEpochConfig {
	fn from(value: MidnightCfg) -> Self {
		MainchainEpochConfig {
			first_epoch_timestamp_millis: sp_core::offchain::Timestamp::from_unix_millis(
				value.mc_first_epoch_timestamp_millis,
			),
			epoch_duration_millis: sp_core::offchain::Duration::from_millis(
				value.mc_epoch_duration_millis,
			),
			first_epoch_number: value.mc_first_epoch_number,
			first_slot_number: value.mc_first_slot_number,
			slot_duration_millis: sp_core::offchain::Duration::from_millis(
				value.mc_slot_duration_millis,
			),
		}
	}
}
