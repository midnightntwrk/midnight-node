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

use documented::{Documented, DocumentedFields as _};
use serde::{Deserialize, Serialize};
use serde_valid::{Validate, validation};
use sidechain_domain::mainchain_epoch::MainchainEpochConfig;

use super::validation_utils::{maybe, path_exists};
use super::{CfgHelp, HelpField, cfg_help, error::CfgError, util::get_keys};

pub mod invariants;
use invariants::check_mainchain_epoch_invariants;

#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum StorageSeparation {
	#[default]
	Separate,
	Unified,
}

/// Default for `cnight_observation_window_size` when not present in config.
/// Applied by serde on deserialization (the path that builds a live config);
/// the derived `Default` is only used by a key-enumeration test helper.
fn default_cnight_observation_window_size() -> u32 {
	midnight_primitives_mainchain_follower::data_source::DEFAULT_WINDOW_SIZE
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Validate, Documented)]
#[validate(custom = main_chain_follower_vars)]
#[validate(custom = mainchain_epoch_invariants)]
/// Parameters specific to Midnight
pub struct MidnightCfg {
	/// On start-up, wipe the chain
	pub wipe_chain_state: bool,

	/// Path to file containing a secret string to use as the AURA seed (32 bytes)
	/// Seed should be either a Phrase, hexadecimal string, or ss58-compatible string.
	/// Docs: https://paritytech.github.io/polkadot-sdk/master/sp_core/crypto/struct.AddressUri.html#structfield.phrase
	pub aura_seed_file: Option<String>,

	/// Path to file containing a secret string to use as the BABE seed (32 bytes)
	/// Seed should be either a Phrase, hexadecimal string, or ss58-compatible string.
	/// Docs: https://paritytech.github.io/polkadot-sdk/master/sp_core/crypto/struct.AddressUri.html#structfield.phrase
	pub babe_seed_file: Option<String>,

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

	/// Path to federated authority config file (contains council and technical committee addresses and policy IDs)
	#[validate(custom = |s| maybe(s, path_exists))]
	pub federated_authority_config_file: Option<String>,

	/// Cardano blocks to keep in the cNIGHT observation sliding window.
	/// Bigger = fewer cache misses during sync but more memory; smaller =
	/// less memory but more db-fallback calls. Defaults to
	/// `DEFAULT_WINDOW_SIZE` (100k) when unset.
	#[serde(default = "default_cnight_observation_window_size")]
	pub cnight_observation_window_size: u32,

	/// Size of ledger storage cache (number of nodes)
	pub storage_cache_size: usize,

	/// Whether substrate and midnight storage should be separate or unified
	pub storage_separation: StorageSeparation,

	/// Deprecated: plaintext database connections are no longer permitted.
	/// This flag is ignored — all connections use TLS. It will be removed in a future release.
	pub allow_non_ssl: bool,

	/// Path to SSL root certificate for database connections.
	/// When set, connections use PgSslMode::VerifyFull (certificate + hostname validation).
	/// When absent, connections use PgSslMode::Require (encrypted but no certificate validation).
	#[validate(custom = |s| maybe(s, path_exists))]
	pub ssl_root_cert: Option<String>,

	/// URL of the Prometheus Remote Write endpoint to push metrics to.
	/// Example: https://thanos.example.com/api/v1/receive
	/// Supports Thanos Receive, Cortex, Mimir, and other remote write compatible endpoints.
	/// If not set, metrics will only be available via the pull endpoint.
	pub prometheus_push_endpoint: Option<String>,

	/// Interval in seconds between metric pushes to the remote write endpoint.
	/// Default: 15 seconds
	pub prometheus_push_interval_secs: Option<u64>,

	/// Job name label to include with pushed metrics.
	/// Default: "midnight-node"
	pub prometheus_push_job_name: Option<String>,
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

/// Validates the self-contained mainchain timing invariants (I1–I4, I6) at config-parse time, so an
/// internally-incoherent mainchain config fails as a `CfgError` before any service or runtime
/// construction. The cross-field sidechain↔mainchain invariant (I5) is not checked here: it needs
/// the sidechain slot configuration, which is only available at service construction.
fn mainchain_epoch_invariants(cfg: &MidnightCfg) -> Result<(), validation::Error> {
	let epoch_config: MainchainEpochConfig = cfg.clone().into();
	check_mainchain_epoch_invariants(&epoch_config)
		.map_err(|e| validation::Error::Custom(e.to_string()))
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

#[cfg(test)]
mod tests {
	use super::*;

	/// A `MidnightCfg` whose mainchain timing fields are internally coherent (1000 ms slots, a
	/// 5-day epoch divisible by the slot duration and at least one second). Other fields are left at
	/// their defaults; the mainchain-invariant validator reads only the `mc_*` fields.
	fn good_cfg() -> MidnightCfg {
		MidnightCfg {
			mc_first_epoch_timestamp_millis: 1_596_059_091_000,
			mc_first_epoch_number: 208,
			mc_epoch_duration_millis: 432_000_000,
			mc_first_slot_number: 4_492_800,
			mc_slot_duration_millis: 1000,
			..Default::default()
		}
	}

	#[test]
	fn validator_accepts_coherent_mainchain_config() {
		assert!(mainchain_epoch_invariants(&good_cfg()).is_ok());
	}

	#[test]
	fn validator_rejects_zero_epoch_duration() {
		let mut cfg = good_cfg();
		cfg.mc_epoch_duration_millis = 0;
		assert!(mainchain_epoch_invariants(&cfg).is_err());
	}

	#[test]
	fn validator_rejects_zero_slot_duration() {
		let mut cfg = good_cfg();
		cfg.mc_slot_duration_millis = 0;
		assert!(mainchain_epoch_invariants(&cfg).is_err());
	}

	#[test]
	fn validator_rejects_sub_second_epoch_duration() {
		let mut cfg = good_cfg();
		cfg.mc_epoch_duration_millis = 999;
		cfg.mc_slot_duration_millis = 1;
		assert!(mainchain_epoch_invariants(&cfg).is_err());
	}

	#[test]
	fn validator_rejects_non_divisible_epoch_slot_pair() {
		let mut cfg = good_cfg();
		cfg.mc_epoch_duration_millis = 10_000;
		cfg.mc_slot_duration_millis = 3000;
		assert!(mainchain_epoch_invariants(&cfg).is_err());
	}

	#[test]
	fn validator_rejects_non_1000ms_slot_duration() {
		// epoch 432_000_000 ms is an exact multiple of a 2000 ms slot, so this passes the I4
		// divisibility check yet is rejected by I6 because the vendored upstream `slots_per_epoch`
		// hardcodes a 1000 ms slot.
		let mut cfg = good_cfg();
		cfg.mc_slot_duration_millis = 2000;
		assert!(mainchain_epoch_invariants(&cfg).is_err());
	}

	#[test]
	fn serde_valid_validate_surfaces_mainchain_invariant_violation() {
		// The aggregate `Validate::validate()` path (run by `Cfg::new()` → `cfg.validate()`,
		// surfacing as a `CfgError`) reports the mainchain-invariant violation. Mock follower vars
		// are set so the unrelated `main_chain_follower_vars` validator does not mask this failure.
		let mut cfg = good_cfg();
		cfg.use_main_chain_follower_mock = true;
		cfg.mock_registrations_file = Some("/dev/null".to_string());
		cfg.mc_epoch_duration_millis = 0;

		let err = cfg.validate().expect_err("zero epoch duration must be rejected");
		assert!(
			err.to_string().contains("mc__epoch_duration_millis"),
			"validation error should name the offending parameter, got: {err}"
		);
	}
}
