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

//! Genesis state inspection and verification module.
//!
//! This module provides functionality to inspect and verify the LedgerState
//! from a chain-spec-raw.json file. It performs the following checks:
//!
//! - DustState verification against cnight-config.json system_tx
//! - Empty state verification for mainnet (utxo, zswap, contract)
//! - Total NIGHT supply invariance (24B = treasury + reserve_pool)
//! - LedgerParameters verification against config file
//! - Genesis timestamp verification in state root histories

use frame_support::traits::Len;
use midnight_node_ledger_helpers::{
	DefaultDB, LedgerParameters, LedgerState, NIGHT, SystemTransaction, Timestamp, TokenType,
	midnight_serialize::tagged_deserialize,
};
use pallet_cnight_observation::config::CNightGenesis;
use std::path::Path;
use thiserror::Error;

/// Maximum NIGHT supply: 24 billion NIGHT with 10^6 atomic units per NIGHT
pub const STARS_PER_NIGHT: u128 = 1_000_000;
pub const MAX_SUPPLY: u128 = 24_000_000_000 * STARS_PER_NIGHT;

const EXPECTED_RESERVE_VALUE: u128 = 6_000_000_000_873_988; // STARS
const EXPECTED_ICS_VALUE: u128 = 1_200_000_000_000_000; // STARS

#[derive(Debug, Error)]
pub enum VerifyLedgerStateGenesisError {
	#[error("Failed to read file: {0}")]
	IoError(#[from] std::io::Error),

	#[error("Failed to parse JSON: {0}")]
	JsonError(#[from] serde_json::Error),

	#[error("Missing genesis_state in chain spec properties")]
	MissingGenesisState,

	#[error("Invalid genesis_state hex encoding: {0}")]
	InvalidHex(#[from] hex::FromHexError),

	#[error("Failed to deserialize LedgerState: {0}")]
	DeserializationError(String),
}

#[derive(Debug, Clone)]
pub struct VerificationResult {
	pub dust_state_ok: bool,
	pub dust_state_message: String,
	pub empty_state_ok: bool,
	pub empty_state_message: String,
	pub supply_invariant_ok: bool,
	pub supply_invariant_message: String,
	pub ledger_parameters_ok: bool,
	pub ledger_parameters_message: String,
	pub timestamp_in_state_ok: bool,
	pub timestamp_in_state_message: String,
}

impl VerificationResult {
	pub fn all_passed(&self) -> bool {
		self.dust_state_ok
			&& self.empty_state_ok
			&& self.supply_invariant_ok
			&& self.ledger_parameters_ok
			&& self.timestamp_in_state_ok
	}

	pub fn print_summary(&self) {
		// Print machine-parseable markers for the shell script
		if self.dust_state_ok {
			println!("DUST_STATE_OK");
		}
		if self.empty_state_ok {
			println!("EMPTY_STATE_OK");
		}
		if self.supply_invariant_ok {
			println!("SUPPLY_INVARIANT_OK");
		}
		if self.ledger_parameters_ok {
			println!("LEDGER_PARAMETERS_OK");
		}
		if self.timestamp_in_state_ok {
			println!("GENESIS_TIMESTAMP_IN_STATE_OK");
		}

		// Print human-readable details
		println!("\n=== Genesis State Verification Results ===\n");

		println!(
			"Dust State: {} - {}",
			if self.dust_state_ok { "PASS" } else { "FAIL" },
			self.dust_state_message
		);

		println!(
			"Empty State: {} - {}",
			if self.empty_state_ok { "PASS" } else { "FAIL" },
			self.empty_state_message
		);

		println!(
			"Supply Invariant: {} - {}",
			if self.supply_invariant_ok { "PASS" } else { "FAIL" },
			self.supply_invariant_message
		);

		println!(
			"Ledger Parameters: {} - {}",
			if self.ledger_parameters_ok { "PASS" } else { "FAIL" },
			self.ledger_parameters_message
		);

		println!(
			"Genesis Timestamp in State: {} - {}",
			if self.timestamp_in_state_ok { "PASS" } else { "FAIL" },
			self.timestamp_in_state_message
		);
	}
}

/// Extract genesis_state from chain-spec-raw.json and deserialize it
fn load_ledger_state(
	chain_spec_path: &Path,
) -> Result<LedgerState<DefaultDB>, VerifyLedgerStateGenesisError> {
	let content = std::fs::read_to_string(chain_spec_path)?;
	let spec: serde_json::Value = serde_json::from_str(&content)?;

	let genesis_state_hex = spec
		.get("properties")
		.and_then(|p| p.get("genesis_state"))
		.and_then(|s| s.as_str())
		.ok_or(VerifyLedgerStateGenesisError::MissingGenesisState)?;

	// Remove 0x prefix if present
	let hex_str = genesis_state_hex.strip_prefix("0x").unwrap_or(genesis_state_hex);
	let genesis_state_bytes = hex::decode(hex_str)?;

	let state: LedgerState<DefaultDB> = tagged_deserialize(&mut genesis_state_bytes.as_slice())
		.map_err(|e| VerifyLedgerStateGenesisError::DeserializationError(e.to_string()))?;

	Ok(state)
}

/// Load cnight-config.json and extract the system_tx
fn load_cnight_system_tx(
	cnight_config_path: &Path,
) -> Result<Option<SystemTransaction>, VerifyLedgerStateGenesisError> {
	let content = std::fs::read_to_string(cnight_config_path)?;
	let config: CNightGenesis = serde_json::from_str(&content)?;

	if let Some(system_tx) = config.system_tx {
		let tx: SystemTransaction = tagged_deserialize(&mut system_tx.0.as_slice())
			.map_err(|e| VerifyLedgerStateGenesisError::DeserializationError(e.to_string()))?;
		Ok(Some(tx))
	} else {
		Ok(None)
	}
}

/// Load ledger-parameters-config.json
fn load_ledger_parameters(
	ledger_params_path: &Path,
) -> Result<LedgerParameters, VerifyLedgerStateGenesisError> {
	let content = std::fs::read_to_string(ledger_params_path)?;
	let params: LedgerParameters = serde_json::from_str(&content)?;
	Ok(params)
}

/// Verify that DustState matches the expected state after applying cnight system_tx
///
/// For mainnet (no faucet wallets), DustState should exactly match the system_tx result.
/// For testnets with faucet wallets, the genesis DustState will include additional entries
/// from faucet wallet DUST registrations and rewards claiming, so we only verify that:
/// - The system_tx entries are a subset (genesis >= expected counts)
/// - The system_tx was successfully applied (no errors)
fn verify_dust_state(
	state: &LedgerState<DefaultDB>,
	cnight_config_path: Option<&Path>,
	network: Option<&str>,
	genesis_timestamp_arg: u64,
) -> (bool, String) {
	let Some(path) = cnight_config_path else {
		return (false, "No cnight-config.json provided".to_string());
	};

	let is_mainnet = network == Some("mainnet");

	match load_cnight_system_tx(path) {
		Ok(Some(system_tx)) => {
			// Re-apply the system_tx to a fresh LedgerState and compare the resulting DustState
			// with the genesis state's DustState.
			let fresh_state = LedgerState::<DefaultDB>::new(&state.network_id);

			let genesis_timestamp = Timestamp::from_secs(genesis_timestamp_arg);

			match fresh_state.apply_system_tx(&system_tx, genesis_timestamp) {
				Ok((expected_state, _events)) => {
					// Compare the DustState from the genesis state with the expected state
					let genesis_dust = &state.dust;
					let expected_dust = &expected_state.dust;

					let genesis_delegations_count =
						genesis_dust.generation.address_delegation.len();
					let expected_delegations_count =
						expected_dust.generation.address_delegation.len();

					let genesis_tree_first_free =
						genesis_dust.generation.generating_tree_first_free;
					let expected_tree_first_free =
						expected_dust.generation.generating_tree_first_free;

					if is_mainnet {
						// For mainnet, require exact match (no faucet wallets)
						let mut issues = Vec::new();

						if genesis_delegations_count != expected_delegations_count {
							issues.push(format!(
								"address_delegation count mismatch: genesis={}, expected={}",
								genesis_delegations_count, expected_delegations_count
							));
						}

						if genesis_tree_first_free != expected_tree_first_free {
							issues.push(format!(
								"generating_tree_first_free mismatch: genesis={}, expected={}",
								genesis_tree_first_free, expected_tree_first_free
							));
						}

						let genesis_hash = genesis_dust.hash();
						let expected_hash = expected_dust.hash();

						if genesis_hash != expected_hash {
							issues.push(format!(
								"DustState hash mismatch:\n  genesis:  {:?}\n  expected: {:?}",
								genesis_hash, expected_hash
							));
						}

						if issues.is_empty() {
							(
								true,
								format!(
									"DustState exactly matches system_tx. Delegations: {}, TreeFirstFree: {}",
									genesis_delegations_count, genesis_tree_first_free
								),
							)
						} else {
							(false, format!("DustState mismatch:\n{}", issues.join("\n")))
						}
					} else {
						// For testnets with faucet wallets, verify system_tx is a subset
						// Genesis state will have additional entries from faucet wallet setup
						let mut issues = Vec::new();

						if genesis_tree_first_free < expected_tree_first_free {
							issues.push(format!(
								"generating_tree_first_free is less than expected from system_tx: genesis={}, expected>={}",
								genesis_tree_first_free, expected_tree_first_free
							));
						}

						if issues.is_empty() {
							(
								true,
								format!(
									"DustState includes system_tx data. Genesis: delegations={}, tree_first_free={}. \
									 System_tx baseline: delegations={}, tree_first_free={}",
									genesis_delegations_count,
									genesis_tree_first_free,
									expected_delegations_count,
									expected_tree_first_free
								),
							)
						} else {
							(
								false,
								format!("DustState verification failed:\n{}", issues.join("\n")),
							)
						}
					}
				},
				Err(e) => (false, format!("Failed to apply system_tx to fresh state: {:?}", e)),
			}
		},
		Ok(None) => {
			// No system_tx in cnight-config.json
			let dust_state = &state.dust;
			let has_delegations = !dust_state.generation.address_delegation.is_empty();
			let has_generating_entries = dust_state.generation.generating_tree_first_free > 0;

			if is_mainnet {
				// For mainnet without system_tx, DustState should be empty
				if has_delegations || has_generating_entries {
					(
						false,
						format!(
							"No system_tx but DustState is not empty. Delegations: {}, TreeFirstFree: {}",
							dust_state.generation.address_delegation.len(),
							dust_state.generation.generating_tree_first_free
						),
					)
				} else {
					(
						true,
						"No system_tx in cnight-config.json - DustState is correctly empty"
							.to_string(),
					)
				}
			} else {
				// For testnets, DustState may have faucet wallet entries even without system_tx
				(
					true,
					format!(
						"No system_tx in cnight-config.json. DustState has delegations={}, tree_first_free={} (from faucet wallets)",
						dust_state.generation.address_delegation.len(),
						dust_state.generation.generating_tree_first_free
					),
				)
			}
		},
		Err(e) => (false, format!("Failed to load cnight-config.json: {}", e)),
	}
}

/// Verify that utxo, zswap, and contract states are empty (mainnet only)
fn verify_empty_state(state: &LedgerState<DefaultDB>, network: Option<&str>) -> (bool, String) {
	let Some(net) = network else {
		return (false, "No network specified".to_string());
	};

	if net != "mainnet" {
		return (true, format!("Skipped - empty state check only applies to mainnet, got {}", net));
	}

	let mut issues = Vec::new();

	// Check UTXO state is empty
	if !state.utxo.utxos.is_empty() {
		issues.push("UTXO state is not empty".to_string());
	}

	// Check UTXO NIGHT value is zero (no funded seed wallets)
	let utxo_value = state.utxo.utxos.ann().value;
	if utxo_value != 0 {
		issues.push(format!("UTXO NIGHT value is not zero: {}", utxo_value));
	}

	// Check zswap state is empty (nullifier set)
	if !state.zswap.nullifiers.is_empty() {
		issues.push("Zswap nullifiers is not empty".to_string());
	}

	// Check contract state is empty
	if !state.contract.is_empty() {
		issues.push("Contract state is not empty".to_string());
	}

	// Check contract NIGHT value is zero
	let contract_value = state.contract.ann().value;
	if contract_value != 0 {
		issues.push(format!("Contract NIGHT value is not zero: {}", contract_value));
	}

	if issues.is_empty() {
		(true, "All state components are empty (no faucet funding)".to_string())
	} else {
		(false, format!("State is not empty: {}", issues.join("; ")))
	}
}

/// Verify total NIGHT supply invariance: all pools + UTXOs + contracts = MAX_SUPPLY (24B)
/// Also verifies that reserve_pool and treasury (ICS) match their expected values.
fn verify_supply_invariant(state: &LedgerState<DefaultDB>) -> (bool, String) {
	// Get treasury balance for NIGHT token
	let night_token = TokenType::Unshielded(NIGHT);
	let treasury_balance = state.treasury.get(&night_token).copied().unwrap_or(0);

	// Get reserve pool balance
	let reserve_pool = state.reserve_pool;

	// Get block reward pool balance
	let block_reward_pool = state.block_reward_pool;

	// Get locked pool balance
	let locked_pool = state.locked_pool;

	// Calculate total unclaimed block rewards
	let mut unclaimed_rewards: u128 = 0;
	for (_, amount) in state.unclaimed_block_rewards.iter() {
		unclaimed_rewards += *amount;
	}

	// Get UTXO value (NIGHT held in UTXOs) - uses the annotation which sums all UTXO values
	let utxo_value = state.utxo.utxos.ann().value;

	// Get contract value (NIGHT held in contracts) - uses the annotation
	let contract_value = state.contract.ann().value;

	// Total supply should equal MAX_SUPPLY
	// The invariant matches the ledger's check_night_balance_invariant:
	// utxo_value + locked_pool + reserve_pool + block_reward_pool + treasury + unclaimed_rewards + contract_value = MAX_SUPPLY
	//
	// Note: bridge_receiving is NOT included in the ledger's invariant check

	let total = utxo_value
		.saturating_add(locked_pool)
		.saturating_add(reserve_pool)
		.saturating_add(block_reward_pool)
		.saturating_add(treasury_balance)
		.saturating_add(unclaimed_rewards)
		.saturating_add(contract_value);

	let mut issues = Vec::new();

	if total != MAX_SUPPLY {
		issues.push(format!("NIGHT supply mismatch! Expected {}, got {}", MAX_SUPPLY, total));
	}

	if reserve_pool != EXPECTED_RESERVE_VALUE {
		issues.push(format!(
			"Reserve pool mismatch: expected {}, got {}",
			EXPECTED_RESERVE_VALUE, reserve_pool
		));
	}

	if treasury_balance != EXPECTED_ICS_VALUE {
		issues.push(format!(
			"Treasury (ICS) mismatch: expected {}, got {}",
			EXPECTED_ICS_VALUE, treasury_balance
		));
	}

	let detail = format!(
		"Reserve: {}, Treasury: {}, UTXOs: {}, Contracts: {}, Block Rewards: {}, Unclaimed: {}, Locked: {}",
		reserve_pool,
		treasury_balance,
		utxo_value,
		contract_value,
		block_reward_pool,
		unclaimed_rewards,
		locked_pool
	);

	if issues.is_empty() {
		(true, format!("Total NIGHT supply = {} (24B). {}", MAX_SUPPLY, detail))
	} else {
		(false, format!("{}\n{}", issues.join("\n"), detail))
	}
}

/// Verify LedgerParameters match the config file
fn verify_ledger_parameters(
	state: &LedgerState<DefaultDB>,
	ledger_params_path: Option<&Path>,
) -> (bool, String) {
	let Some(path) = ledger_params_path else {
		return (false, "No ledger-parameters-config.json provided".to_string());
	};

	match load_ledger_parameters(path) {
		Ok(expected_params) => {
			let state_params = &*state.parameters;

			// Compare key fields
			let mut issues = Vec::new();

			// NOTE: fee_prices are dynamically adjusted during genesis generation.
			// The post_block_update() function calls fee_prices.update_from_fullness()
			// which modifies the fee prices based on block fullness. The config file
			// contains the initial fee prices, while the genesis state contains the
			// adjusted values after post_block_update. We allow up to 5% deviation.
			{
				const MAX_DEVIATION_PCT: f64 = 5.0;
				let fee_fields: &[(&str, f64, f64)] = &[
					(
						"overall_price",
						f64::from(state_params.fee_prices.overall_price),
						f64::from(expected_params.fee_prices.overall_price),
					),
					(
						"read_factor",
						f64::from(state_params.fee_prices.read_factor),
						f64::from(expected_params.fee_prices.read_factor),
					),
					(
						"compute_factor",
						f64::from(state_params.fee_prices.compute_factor),
						f64::from(expected_params.fee_prices.compute_factor),
					),
					(
						"block_usage_factor",
						f64::from(state_params.fee_prices.block_usage_factor),
						f64::from(expected_params.fee_prices.block_usage_factor),
					),
					(
						"write_factor",
						f64::from(state_params.fee_prices.write_factor),
						f64::from(expected_params.fee_prices.write_factor),
					),
				];

				for (name, actual, expected) in fee_fields {
					let deviation_pct = if *expected == 0.0 {
						if *actual == 0.0 { 0.0 } else { f64::INFINITY }
					} else {
						((actual - expected) / expected).abs() * 100.0
					};

					if deviation_pct > MAX_DEVIATION_PCT {
						issues.push(format!(
							"fee_prices.{} deviation {:.2}% exceeds {}% threshold:\n  state:    {}\n  expected: {}",
							name, deviation_pct, MAX_DEVIATION_PCT, actual, expected
						));
					}
				}
			}

			// Compare limits
			if state_params.limits != expected_params.limits {
				issues.push(format!(
					"limits mismatch:\n  state:    {:?}\n  expected: {:?}",
					state_params.limits, expected_params.limits
				));
			}

			// Compare dust parameters
			if state_params.dust != expected_params.dust {
				issues.push(format!(
					"dust parameters mismatch:\n  state:    {:?}\n  expected: {:?}",
					state_params.dust, expected_params.dust
				));
			}

			// Compare cost model
			if state_params.cost_model != expected_params.cost_model {
				issues.push(format!(
					"cost_model mismatch:\n  state:    {:?}\n  expected: {:?}",
					state_params.cost_model, expected_params.cost_model
				));
			}

			// Compare global TTL
			if state_params.global_ttl != expected_params.global_ttl {
				issues.push(format!(
					"global_ttl mismatch:\n  state:    {:?}\n  expected: {:?}",
					state_params.global_ttl, expected_params.global_ttl
				));
			}

			// Compare bridge fee
			if state_params.cardano_to_midnight_bridge_fee_basis_points
				!= expected_params.cardano_to_midnight_bridge_fee_basis_points
			{
				issues.push(format!(
					"cardano_to_midnight_bridge_fee_basis_points mismatch:\n  state:    {:?}\n  expected: {:?}",
					state_params.cardano_to_midnight_bridge_fee_basis_points,
					expected_params.cardano_to_midnight_bridge_fee_basis_points
				));
			}

			// Compare c_to_m_bridge_min_amount
			if state_params.c_to_m_bridge_min_amount != expected_params.c_to_m_bridge_min_amount {
				issues.push(format!(
					"c_to_m_bridge_min_amount mismatch:\n  state:    {:?}\n  expected: {:?}",
					state_params.c_to_m_bridge_min_amount, expected_params.c_to_m_bridge_min_amount
				));
			}

			if issues.is_empty() {
				(true, "All LedgerParameters match".to_string())
			} else {
				(false, format!("Parameter mismatches:\n{}", issues.join("\n")))
			}
		},
		Err(e) => (false, format!("Failed to load ledger-parameters-config.json: {}", e)),
	}
}

/// Verify that the genesis timestamp is recorded in the LedgerState root histories.
///
/// During genesis generation, `post_block_update(tblock, ...)` inserts `tblock` as a key into
/// three `TimeFilterMap`s: `zswap.past_roots`, `dust.utxo.root_history`, and
/// `dust.generation.root_history`. For the genesis block, each should contain exactly one entry
/// at the expected timestamp. We verify this by checking that `get(expected_ts)` returns `Some`
/// and `get(expected_ts - 1)` returns `None` (confirming no predecessor, i.e. the entry is
/// exactly at the expected timestamp).
fn verify_genesis_timestamp_in_state(
	state: &LedgerState<DefaultDB>,
	genesis_timestamp_arg: u64,
) -> (bool, String) {
	let expected_secs = genesis_timestamp_arg;
	let expected_ts = Timestamp::from_secs(expected_secs);
	let before_ts = Timestamp::from_secs(expected_secs - 1);

	let checks: [(&str, bool, bool); 3] = [
		(
			"zswap.past_roots",
			state.zswap.past_roots.get(expected_ts).is_some(),
			state.zswap.past_roots.get(before_ts).is_some(),
		),
		(
			"dust.utxo.root_history",
			state.dust.utxo.root_history.get(expected_ts).is_some(),
			state.dust.utxo.root_history.get(before_ts).is_some(),
		),
		(
			"dust.generation.root_history",
			state.dust.generation.root_history.get(expected_ts).is_some(),
			state.dust.generation.root_history.get(before_ts).is_some(),
		),
	];

	let mut issues = Vec::new();
	for (name, has_entry, has_predecessor) in &checks {
		if !has_entry {
			issues.push(format!("{}: no entry at timestamp {}", name, expected_secs));
		} else if *has_predecessor {
			issues.push(format!(
				"{}: unexpected predecessor before timestamp {} (entry may not be exact)",
				name, expected_secs
			));
		}
	}

	if issues.is_empty() {
		(
			true,
			format!(
				"Genesis timestamp {} confirmed in zswap.past_roots, dust.utxo.root_history, dust.generation.root_history",
				expected_secs
			),
		)
	} else {
		(false, format!("Timestamp verification failed:\n{}", issues.join("\n")))
	}
}

/// Inspect and verify the genesis state
pub fn verify_ledger_state_genesis(
	chain_spec_path: &Path,
	cnight_config_path: Option<&Path>,
	ledger_params_path: Option<&Path>,
	network: Option<&str>,
	genesis_timestamp: u64,
) -> Result<VerificationResult, VerifyLedgerStateGenesisError> {
	log::info!("Loading LedgerState from {}", chain_spec_path.display());
	let state = load_ledger_state(chain_spec_path)?;

	log::info!("LedgerState loaded successfully. Network ID: {}", state.network_id);

	// Run all verifications
	let (dust_state_ok, dust_state_message) =
		verify_dust_state(&state, cnight_config_path, network, genesis_timestamp);
	let (empty_state_ok, empty_state_message) = verify_empty_state(&state, network);
	let (supply_invariant_ok, supply_invariant_message) = verify_supply_invariant(&state);
	let (ledger_parameters_ok, ledger_parameters_message) =
		verify_ledger_parameters(&state, ledger_params_path);
	let (timestamp_in_state_ok, timestamp_in_state_message) =
		verify_genesis_timestamp_in_state(&state, genesis_timestamp);

	Ok(VerificationResult {
		dust_state_ok,
		dust_state_message,
		empty_state_ok,
		empty_state_message,
		supply_invariant_ok,
		supply_invariant_message,
		ledger_parameters_ok,
		ledger_parameters_message,
		timestamp_in_state_ok,
		timestamp_in_state_message,
	})
}
