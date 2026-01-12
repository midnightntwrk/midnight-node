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

//! CLI tool for deploying Aiken governance contracts to Cardano.
//!
//! This tool deploys the `council_forever` contract with permissioned candidates
//! by building and submitting a Cardano transaction that:
//! 1. Consumes the council one-shot UTxO
//! 2. Mints the governance NFT using the contract as minting policy
//! 3. Creates an output at the script address with a VersionedMultisig datum

use clap::Parser;
use ogmios_client::jsonrpsee::client_for_url;
use ogmios_client::query_ledger_state::QueryLedgerState;
use ogmios_client::transactions::Transactions;
use ogmios_client::types::OgmiosUtxo;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;
use whisky::csl::{Address, NetworkInfo};
use whisky::{
	apply_double_cbor_encoding, get_script_hash, script_to_address, Asset, Budget, LanguageVersion,
	Network, OfflineTxEvaluator, TxBuilder, WData, WRedeemer,
};

#[derive(Parser, Debug)]
#[command(name = "aiken-deployer")]
#[command(about = "Deploy Aiken governance contracts to Cardano")]
struct Args {
	/// Path to the contract CBOR file (double-encoded hex)
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
}

/// A member in the VersionedMultisig datum.
/// The cardano_hash is the pubkey hash that identifies the signer.
/// The sr25519_key is the sidechain public key mapped to this signer.
#[derive(Debug, serde::Deserialize)]
struct Member {
	cardano_hash: String,
	sr25519_key: String,
}

#[derive(Error, Debug)]
enum DeployError {
	#[error("IO error: {0}")]
	Io(#[from] std::io::Error),
	#[error("JSON error: {0}")]
	Json(#[from] serde_json::Error),
	#[error("Invalid UTxO format: {0}")]
	InvalidUtxo(String),
	#[error("Ogmios error: {0}")]
	Ogmios(String),
	#[error("Transaction build error: {0}")]
	TxBuild(String),
}

fn parse_utxo_ref(s: &str) -> Result<(String, u32), DeployError> {
	let parts: Vec<&str> = s.split('#').collect();
	if parts.len() != 2 {
		return Err(DeployError::InvalidUtxo(format!("Expected txhash#index, got: {}", s)));
	}
	let index = parts[1]
		.parse::<u32>()
		.map_err(|_| DeployError::InvalidUtxo(format!("Invalid index: {}", parts[1])))?;
	Ok((parts[0].to_string(), index))
}

fn build_asset_vector(utxo: &OgmiosUtxo) -> Vec<Asset> {
	let mut assets: Vec<Asset> = utxo
		.value
		.native_tokens
		.iter()
		.flat_map(|(policy_id, tokens)| {
			let policy_hex = hex::encode(policy_id);
			tokens
				.iter()
				.map(move |token| Asset::new_from_str(&policy_hex, &token.amount.to_string()))
		})
		.collect();

	assets.insert(0, Asset::new_from_str("lovelace", &utxo.value.lovelace.to_string()));
	assets
}

#[tokio::main]
async fn main() -> Result<(), DeployError> {
	let args = Args::parse();

	println!("=== Aiken Governance Contract Deployer ===");

	// Read contract CBOR (raw from file, needs double encoding)
	let raw_contract_cbor = fs::read_to_string(&args.contract_cbor)?;
	let raw_contract_cbor = raw_contract_cbor.trim();
	println!("✓ Loaded contract CBOR ({} chars)", raw_contract_cbor.len());

	// V3 scripts from Aiken need double CBOR encoding
	let contract_cbor = apply_double_cbor_encoding(raw_contract_cbor)
		.map_err(|e| DeployError::TxBuild(format!("Failed to double encode CBOR: {:?}", e)))?;
	println!("✓ Applied double CBOR encoding");

	// Read signing key (expecting CBOR hex format)
	let signing_key_content = fs::read_to_string(&args.signing_key)?;
	let signing_key_cbor = signing_key_content.trim();
	println!("✓ Loaded signing key");

	// Read members
	let members_content = fs::read_to_string(&args.members_file)?;
	let members: Vec<Member> = serde_json::from_str(&members_content)?;
	println!("✓ Loaded {} members", members.len());

	// Parse one-shot UTxO reference
	let (one_shot_hash, one_shot_index) = parse_utxo_ref(&args.one_shot_utxo)?;
	println!("One-shot UTxO: {}#{}", one_shot_hash, one_shot_index);

	// Calculate script hash (policy ID) from double-encoded CBOR
	let policy_id = get_script_hash(&contract_cbor, LanguageVersion::V3)
		.map_err(|e| DeployError::TxBuild(format!("Failed to get script hash: {:?}", e)))?;
	println!("Policy ID: {}", policy_id);

	// Calculate script address (network_id 0 = testnet)
	// For local-environment devnet, use testnet_preview network info
	let network_info = NetworkInfo::testnet_preview();
	let script_address = script_to_address(network_info.network_id(), &policy_id, None);
	println!("Script address: {}", script_address);

	// Connect to Ogmios
	println!("Connecting to Ogmios at {}...", args.ogmios_url);
	let ogmios_client =
		client_for_url(&args.ogmios_url, Duration::from_secs(args.timeout))
			.await
			.map_err(|e| DeployError::Ogmios(format!("Failed to connect to Ogmios: {:?}", e)))?;
	println!("✓ Connected to Ogmios");

	// Query UTxOs at funded address
	let funded_utxos = ogmios_client
		.query_utxos(std::slice::from_ref(&args.funded_address))
		.await
		.map_err(|e| DeployError::Ogmios(format!("Failed to query UTxOs: {:?}", e)))?;

	println!("Found {} UTxOs at funded address", funded_utxos.len());

	// Find the one-shot UTxO
	let one_shot_utxo = funded_utxos
		.iter()
		.find(|u| {
			hex::encode(u.transaction.id) == one_shot_hash && u.index as u32 == one_shot_index
		})
		.ok_or_else(|| DeployError::InvalidUtxo("One-shot UTxO not found on chain".to_string()))?;

	println!("✓ Found one-shot UTxO with {} lovelace", one_shot_utxo.value.lovelace);

	// Find a funding UTxO (pick the one with most lovelace that isn't the one-shot)
	let funding_utxo = funded_utxos
		.iter()
		.filter(|u| {
			!(hex::encode(u.transaction.id) == one_shot_hash && u.index as u32 == one_shot_index)
		})
		.max_by_key(|u| u.value.lovelace)
		.ok_or_else(|| DeployError::InvalidUtxo("No funding UTxO found".to_string()))?;

	println!("✓ Using funding UTxO with {} lovelace", funding_utxo.value.lovelace);

	// Find a collateral UTxO (pick one with at least 5 ADA that isn't used as funding or one-shot)
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
		.ok_or_else(|| DeployError::InvalidUtxo("No collateral UTxO found".to_string()))?;

	println!("✓ Using collateral UTxO with {} lovelace", collateral_utxo.value.lovelace);

	// Extract payment key hash from funded address
	let funded_addr_parsed =
		Address::from_bech32(&args.funded_address).expect("Invalid funded address");
	let payment_keyhash = funded_addr_parsed
		.payment_cred()
		.expect("No payment credential")
		.to_keyhash()
		.expect("Not a keyhash");
	let payment_keyhash_hex = hex::encode(payment_keyhash.to_bytes());

	// Build VersionedMultisig datum matching E2E test format
	// Structure: [[total_signers, {signer_key => sr25519_key}], logic_round]
	// The signer keys are in "created signer" format: #"8200581c" + cardano_hash
	let total_signers = members.len() as u64;

	let multisig_data = serde_json::json!({
		"list": [
			{"int": total_signers},
			{"map": members.iter().map(|m| {
				// The signer key must be in "created signer" format: #"8200581c" + cardano_hash
				let signer_key = format!("8200581c{}", m.cardano_hash);
				serde_json::json!({
					"k": {"bytes": signer_key},
					"v": {"bytes": m.sr25519_key}
				})
			}).collect::<Vec<_>>()}
		]
	});

	// VersionedMultisig is a list: [Multisig, logic_round]
	let datum = serde_json::json!({
		"list": [
			multisig_data,
			{"int": 0}  // logic_round starts at 0
		]
	});

	// Build redeemer (map of cardano_hash => sr25519_key for initial deployment)
	let redeemer = serde_json::json!({
		"map": members.iter().map(|m| {
			serde_json::json!({
				"k": {"bytes": m.cardano_hash},
				"v": {"bytes": m.sr25519_key}
			})
		}).collect::<Vec<_>>()
	});

	println!("Building transaction...");
	println!("  Datum: {}", serde_json::to_string_pretty(&datum).unwrap());

	// Build the transaction
	let send_assets =
		vec![Asset::new_from_str("lovelace", "2000000"), Asset::new_from_str(&policy_id, "1")];

	// Use Preprod network for local testnet (magic 42)
	let network = Network::Preprod;

	let funding_hash = hex::encode(funding_utxo.transaction.id);
	let collateral_hash = hex::encode(collateral_utxo.transaction.id);

	let mut tx_builder = TxBuilder::new_core();
	tx_builder
		.network(network)
		.set_evaluator(Box::new(OfflineTxEvaluator::new()))
		// Funding input
		.tx_in(
			&funding_hash,
			funding_utxo.index.into(),
			&build_asset_vector(funding_utxo),
			&args.funded_address,
		)
		// One-shot input (consumed by minting policy)
		.tx_in(
			&one_shot_hash,
			one_shot_index,
			&build_asset_vector(one_shot_utxo),
			&args.funded_address,
		)
		// Collateral
		.tx_in_collateral(
			&collateral_hash,
			collateral_utxo.index.into(),
			&build_asset_vector(collateral_utxo),
			&args.funded_address,
		)
		// Output to script address with NFT and datum
		.tx_out(&script_address, &send_assets)
		.tx_out_inline_datum_value(&WData::JSON(datum.to_string()))
		// Mint the NFT
		.mint_plutus_script_v3()
		.mint(1, &policy_id, "")
		.minting_script(&contract_cbor)
		.mint_redeemer_value(&WRedeemer {
			data: WData::JSON(redeemer.to_string()),
			ex_units: Budget { mem: 14000000, steps: 10000000000 },
		})
		.change_address(&args.funded_address)
		.required_signer_hash(&payment_keyhash_hex)
		.signing_key(signing_key_cbor)
		.complete_sync(None)
		.map_err(|e| DeployError::TxBuild(format!("Failed to build transaction: {:?}", e)))?;

	// Complete signing
	tx_builder
		.complete_signing()
		.map_err(|e| DeployError::TxBuild(format!("Failed to sign transaction: {:?}", e)))?;

	println!("✓ Transaction built and signed");

	// Get the signed transaction
	let signed_tx_hex = tx_builder.tx_hex();
	let tx_bytes = hex::decode(&signed_tx_hex)
		.map_err(|e| DeployError::TxBuild(format!("Invalid tx hex: {:?}", e)))?;

	println!("Submitting transaction ({} bytes)...", tx_bytes.len());

	let result = ogmios_client
		.submit_transaction(&tx_bytes)
		.await
		.map_err(|e| DeployError::Ogmios(format!("Failed to submit transaction: {:?}", e)))?;

	println!("✓ Transaction submitted successfully!");
	println!("  TX ID: {}", hex::encode(result.transaction.id));

	Ok(())
}
