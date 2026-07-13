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

//! CLI tool for deploying Aiken governance contracts to Cardano.
//!
//! This tool deploys governance contracts (council_forever, tech_auth_forever)
//! by building and submitting a Cardano transaction that:
//! 1. Consumes the one-shot UTxO
//! 2. Mints the governance NFT using the contract as minting policy
//! 3. Creates an output at the script address with a VersionedMultisig datum

use aiken_contracts_lib::{
	build_deploy_transaction, build_federated_ops_datum, build_federated_ops_redeemer,
	build_governance_redeemer, build_versioned_multisig_datum, convert_cost_models,
	prepare_contract, testnet_network_id, DeployParams, FederatedOpsCandidate, GovernanceMember,
};
use clap::{Parser, ValueEnum};
use ogmios_client::jsonrpsee::client_for_url;

/// The type of governance contract being deployed.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum ContractType {
	/// Council governance contract (Versioned<Multisig> datum)
	Council,
	/// Technical authority governance contract (Versioned<Multisig> datum)
	TechAuth,
	/// Federated operators contract (FederatedOps datum with appendix field)
	FederatedOps,
}
use ogmios_client::query_ledger_state::QueryLedgerState;
use ogmios_client::transactions::Transactions;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;
use whisky::csl::Address;

#[derive(Parser, Debug)]
#[command(name = "aiken-deployer")]
#[command(about = "Deploy Aiken governance contracts to Cardano")]
struct Args {
	/// Path to the contract CBOR file (raw hex from Aiken compilation)
	#[arg(long)]
	contract_cbor: PathBuf,

	/// One-shot UTxO reference (format: txhash#index)
	#[arg(long)]
	one_shot_utxo: String,

	/// Path to the payment signing key file (CBOR hex from funded_address.skey)
	#[arg(long)]
	signing_key: PathBuf,

	/// Funded address (bech32)
	#[arg(long)]
	funded_address: String,

	/// Ogmios URL
	#[arg(long, default_value = "http://ogmios:1337")]
	ogmios_url: String,

	/// Path to JSON file with members (array of {cardano_hash, sr25519_key})
	#[arg(long)]
	members_file: PathBuf,

	/// Timeout for Ogmios connection in seconds
	#[arg(long, default_value = "30")]
	timeout: u64,

	/// Type of governance contract being deployed.
	/// Determines the datum structure:
	/// - council/tech-auth: Versioned<Multisig> (2 fields)
	/// - federated-ops: FederatedOps (3 fields with appendix)
	#[arg(long, value_enum, default_value = "council")]
	contract_type: ContractType,

	/// Path to JSON file with permissioned candidates (for federated-ops only)
	/// Array of {ecdsa_key, aura_key} objects
	#[arg(long)]
	candidates_file: Option<PathBuf>,
}

#[derive(Error, Debug)]
enum CliError {
	#[error("IO error: {0}")]
	Io(#[from] std::io::Error),
	#[error("JSON error: {0}")]
	Json(#[from] serde_json::Error),
	#[error("Invalid UTxO format: {0}")]
	InvalidUtxo(String),
	#[error("Ogmios error: {0}")]
	Ogmios(String),
	#[error("Deploy error: {0}")]
	Deploy(#[from] aiken_contracts_lib::DeployError),
}

fn parse_utxo_ref(s: &str) -> Result<(String, u32), CliError> {
	let parts: Vec<&str> = s.split('#').collect();
	if parts.len() != 2 {
		return Err(CliError::InvalidUtxo(format!("Expected txhash#index, got: {}", s)));
	}
	let index = parts[1]
		.parse::<u32>()
		.map_err(|_| CliError::InvalidUtxo(format!("Invalid index: {}", parts[1])))?;
	Ok((parts[0].to_string(), index))
}

#[tokio::main]
async fn main() -> Result<(), CliError> {
	let args = Args::parse();

	println!("=== Aiken Governance Contract Deployer ===");

	// Read and prepare contract
	let raw_contract_cbor = fs::read_to_string(&args.contract_cbor)?;
	let raw_contract_cbor = raw_contract_cbor.trim();
	println!("✓ Loaded contract CBOR ({} chars)", raw_contract_cbor.len());

	let contract = prepare_contract(raw_contract_cbor, testnet_network_id())?;
	println!("✓ Applied double CBOR encoding");
	println!("  Policy ID: {}", contract.policy_id);
	println!("  Script address: {}", contract.script_address);

	// Read signing key
	let signing_key_content = fs::read_to_string(&args.signing_key)?;
	let signing_key_cbor = signing_key_content.trim();
	println!("✓ Loaded signing key");

	// Read members
	let members_content = fs::read_to_string(&args.members_file)?;
	let members: Vec<GovernanceMember> = serde_json::from_str(&members_content)?;
	println!("✓ Loaded {} members", members.len());

	// Parse one-shot UTxO reference
	let (one_shot_hash, one_shot_index) = parse_utxo_ref(&args.one_shot_utxo)?;
	println!("One-shot UTxO: {}#{}", one_shot_hash, one_shot_index);

	// Connect to Ogmios
	println!("Connecting to Ogmios at {}...", args.ogmios_url);
	let ogmios_client =
		client_for_url(&args.ogmios_url, Duration::from_secs(args.timeout))
			.await
			.map_err(|e| CliError::Ogmios(format!("Failed to connect to Ogmios: {:?}", e)))?;
	println!("✓ Connected to Ogmios");

	// Query UTxOs at funded address
	let funded_utxos = ogmios_client
		.query_utxos(std::slice::from_ref(&args.funded_address))
		.await
		.map_err(|e| CliError::Ogmios(format!("Failed to query UTxOs: {:?}", e)))?;

	println!("Found {} UTxOs at funded address", funded_utxos.len());

	// Find the one-shot UTxO
	let one_shot_utxo = funded_utxos
		.iter()
		.find(|u| {
			hex::encode(u.transaction.id) == one_shot_hash && u.index as u32 == one_shot_index
		})
		.ok_or_else(|| CliError::InvalidUtxo("One-shot UTxO not found on chain".to_string()))?;

	println!("✓ Found one-shot UTxO with {} lovelace", one_shot_utxo.value.lovelace);

	// Find a funding UTxO (pick the one with most lovelace that isn't the one-shot)
	let funding_utxo = funded_utxos
		.iter()
		.filter(|u| {
			!(hex::encode(u.transaction.id) == one_shot_hash && u.index as u32 == one_shot_index)
		})
		.max_by_key(|u| u.value.lovelace)
		.ok_or_else(|| CliError::InvalidUtxo("No funding UTxO found".to_string()))?;

	println!("✓ Using funding UTxO with {} lovelace", funding_utxo.value.lovelace);

	// Find a collateral UTxO
	let collateral_utxo = funded_utxos
		.iter()
		.find(|u| {
			let is_one_shot =
				hex::encode(u.transaction.id) == one_shot_hash && u.index as u32 == one_shot_index;
			let is_funding = hex::encode(u.transaction.id)
				== hex::encode(funding_utxo.transaction.id)
				&& u.index == funding_utxo.index;
			!is_one_shot && !is_funding && u.value.lovelace >= 5_000_000
		})
		.ok_or_else(|| CliError::InvalidUtxo("No collateral UTxO found".to_string()))?;

	println!("✓ Using collateral UTxO with {} lovelace", collateral_utxo.value.lovelace);

	// Query protocol parameters for cost models
	let protocol_params = ogmios_client
		.query_protocol_parameters()
		.await
		.map_err(|e| CliError::Ogmios(format!("Failed to query protocol parameters: {:?}", e)))?;

	let cost_models = convert_cost_models(&protocol_params.plutus_cost_models);
	println!(
		"✓ Fetched protocol parameters (V3 cost model has {} entries)",
		protocol_params.plutus_cost_models.plutus_v3.len()
	);

	// Extract payment key hash from funded address
	let funded_addr_parsed =
		Address::from_bech32(&args.funded_address).expect("Invalid funded address");
	let payment_keyhash = funded_addr_parsed
		.payment_cred()
		.expect("No payment credential")
		.to_keyhash()
		.expect("Not a keyhash");
	let payment_keyhash_hex = hex::encode(payment_keyhash.to_bytes());

	// Build datum and redeemer using library functions based on contract type
	let (datum, redeemer) = match args.contract_type {
		ContractType::Council | ContractType::TechAuth => {
			// Council and TechAuth use Versioned<Multisig> datum
			let datum = build_versioned_multisig_datum(&members);
			let redeemer = build_governance_redeemer(&members);
			(datum, redeemer)
		},
		ContractType::FederatedOps => {
			// FederatedOps uses a different datum structure with an appendix field
			// Read candidates from file if provided, otherwise use empty list
			let candidates: Vec<FederatedOpsCandidate> =
				if let Some(ref path) = args.candidates_file {
					let content = fs::read_to_string(path)?;
					let candidates: Vec<FederatedOpsCandidate> = serde_json::from_str(&content)?;
					println!("✓ Loaded {} permissioned candidates", candidates.len());
					candidates
				} else {
					println!("No candidates file provided, deploying with empty candidates list");
					vec![]
				};
			let datum = build_federated_ops_datum(&candidates);
			let redeemer = build_federated_ops_redeemer(&candidates);
			(datum, redeemer)
		},
	};

	println!("Building transaction (contract type: {:?})...", args.contract_type);
	println!("  Datum: {}", serde_json::to_string_pretty(&datum).unwrap());

	// Build and sign transaction
	let signed_tx_hex = build_deploy_transaction(DeployParams {
		contract: &contract,
		one_shot_utxo,
		funding_utxo,
		collateral_utxo,
		funded_address: &args.funded_address,
		payment_keyhash: &payment_keyhash_hex,
		signing_key_cbor,
		datum,
		redeemer,
		cost_models,
	})?;

	println!("✓ Transaction built and signed");

	// Submit transaction
	let tx_bytes = hex::decode(&signed_tx_hex)
		.map_err(|e| CliError::Ogmios(format!("Invalid tx hex: {:?}", e)))?;

	println!("Submitting transaction ({} bytes)...", tx_bytes.len());

	let result = ogmios_client
		.submit_transaction(&tx_bytes)
		.await
		.map_err(|e| CliError::Ogmios(format!("Failed to submit transaction: {:?}", e)))?;

	println!("✓ Transaction submitted successfully!");
	println!("  TX ID: {}", hex::encode(result.transaction.id));

	Ok(())
}
