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

//! Library for deploying Aiken governance contracts to Cardano.
//!
//! This crate provides shared functionality for deploying governance contracts
//! (council_forever, tech_auth_forever, federated_ops_forever) to the Cardano network.

use ogmios_client::query_ledger_state::PlutusCostModels;
use ogmios_client::types::OgmiosUtxo;
use thiserror::Error;
use whisky::csl::NetworkInfo;
use whisky::{
	apply_double_cbor_encoding, get_script_hash, script_to_address, Asset, Budget, LanguageVersion,
	Network, OfflineTxEvaluator, TxBuilder, WData, WRedeemer,
};

/// Errors that can occur during contract deployment.
#[derive(Error, Debug)]
pub enum DeployError {
	#[error("Failed to encode CBOR: {0}")]
	CborEncoding(String),
	#[error("Failed to get script hash: {0}")]
	ScriptHash(String),
	#[error("Transaction build error: {0}")]
	TxBuild(String),
	#[error("Transaction signing error: {0}")]
	TxSign(String),
}

/// A member of a governance contract (council, tech_auth).
/// Maps a Cardano key hash to an SR25519 sidechain key.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GovernanceMember {
	/// The Cardano payment key hash (28 bytes, 56 hex chars)
	pub cardano_hash: String,
	/// The SR25519 sidechain public key (32 bytes, 64 hex chars)
	pub sr25519_key: String,
}

/// A candidate for the federated operators contract.
/// Maps an ECDSA cross-chain key to SR25519 AURA and ED25519 GRANDPA keys.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FederatedOpsCandidate {
	/// The ECDSA cross-chain public key (33 bytes compressed, 66 hex chars)
	pub ecdsa_key: String,
	/// The SR25519 AURA public key (32 bytes, 64 hex chars)
	pub aura_key: String,
	/// The ED25519 GRANDPA public key (32 bytes, 64 hex chars)
	pub grandpa_key: String,
}

/// Result of preparing a contract for deployment.
pub struct PreparedContract {
	/// The double-encoded CBOR of the contract
	pub encoded_cbor: String,
	/// The policy ID (script hash)
	pub policy_id: String,
	/// The script address
	pub script_address: String,
}

/// Prepares an Aiken V3 contract for deployment.
///
/// Applies double CBOR encoding (required for V3 scripts) and calculates the policy ID and script address.
pub fn prepare_contract(raw_cbor: &str, network_id: u8) -> Result<PreparedContract, DeployError> {
	let encoded_cbor = apply_double_cbor_encoding(raw_cbor)
		.map_err(|e| DeployError::CborEncoding(format!("{:?}", e)))?;

	let policy_id = get_script_hash(&encoded_cbor, LanguageVersion::V3)
		.map_err(|e| DeployError::ScriptHash(format!("{:?}", e)))?;

	let script_address = script_to_address(network_id, &policy_id, None);

	Ok(PreparedContract { encoded_cbor, policy_id, script_address })
}

/// Builds the VersionedMultisig datum for council/tech_auth contracts.
///
/// Format: `[[total_signers, {signer_key => sr25519_key}], logic_round]`
/// where signer_key is `#"8200581c" + cardano_hash`.
pub fn build_versioned_multisig_datum(members: &[GovernanceMember]) -> serde_json::Value {
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
	serde_json::json!({
		"list": [
			multisig_data,
			{"int": 0}  // logic_round starts at 0
		]
	})
}

/// Builds the redeemer for initial governance contract deployment.
///
/// Format: `{cardano_hash => sr25519_key}`
pub fn build_governance_redeemer(members: &[GovernanceMember]) -> serde_json::Value {
	serde_json::json!({
		"map": members.iter().map(|m| {
			serde_json::json!({
				"k": {"bytes": m.cardano_hash},
				"v": {"bytes": m.sr25519_key}
			})
		}).collect::<Vec<_>>()
	})
}

/// Builds the FederatedOps datum for federated_ops_forever contract.
///
/// Format: `[data, appendix, logic_round]`
/// - data: empty list (constructor 0 with no fields)
/// - appendix: list of `[partner_chains_key, keys]` where:
///   - partner_chains_key: ECDSA cross-chain key
///   - keys: list of `[id, bytes]` pairs (e.g., `[aura_id, aura_key]`)
/// - logic_round: 1 (required for partner-chains SDK compatibility - maps to version=1 parsing)
pub fn build_federated_ops_datum(candidates: &[FederatedOpsCandidate]) -> serde_json::Value {
	let aura_id = "61757261"; // "aura" in hex
	let gran_id = "6772616e"; // "gran" in hex

	let appendix: Vec<serde_json::Value> = candidates
		.iter()
		.map(|c| {
			// Each PermissionedCandidateDatumV1 is [partner_chains_key, keys]
			// keys is a list of [key_id, key_bytes] pairs
			serde_json::json!({
				"list": [
					{"bytes": c.ecdsa_key},
					{"list": [
						{"list": [
							{"bytes": aura_id},
							{"bytes": c.aura_key}
						]},
						{"list": [
							{"bytes": gran_id},
							{"bytes": c.grandpa_key}
						]}
					]}
				]
			})
		})
		.collect();

	serde_json::json!({
		"list": [
			{"list": []},           // data: empty
			{"list": appendix},     // appendix: list of candidates
			{"int": 1}              // logic_round: 1 (SDK parses as version=1 for V1 appendix format)
		]
	})
}

/// Builds the redeemer for initial FederatedOps contract deployment.
///
/// Format: empty list for initialization.
pub fn build_federated_ops_redeemer(_candidates: &[FederatedOpsCandidate]) -> serde_json::Value {
	// Empty list redeemer for initialization
	serde_json::json!({"list": []})
}

/// Builds an asset vector from a UTxO for use with TxBuilder.
pub fn build_asset_vector(utxo: &OgmiosUtxo) -> Vec<Asset> {
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

/// Parameters for deploying a governance contract.
pub struct DeployParams<'a> {
	/// The prepared contract (double-encoded CBOR, policy ID, script address)
	pub contract: &'a PreparedContract,
	/// The one-shot UTxO to consume (ensures single minting)
	pub one_shot_utxo: &'a OgmiosUtxo,
	/// The funding UTxO (for fees)
	pub funding_utxo: &'a OgmiosUtxo,
	/// The collateral UTxO (for script execution)
	pub collateral_utxo: &'a OgmiosUtxo,
	/// The funded address (bech32) that owns all inputs
	pub funded_address: &'a str,
	/// The payment key hash of the funded address (hex)
	pub payment_keyhash: &'a str,
	/// The signing key CBOR (hex)
	pub signing_key_cbor: &'a str,
	/// The datum JSON value
	pub datum: serde_json::Value,
	/// The redeemer JSON value
	pub redeemer: serde_json::Value,
	/// The cost models from protocol parameters (for script integrity hash)
	pub cost_models: Vec<Vec<i64>>,
}

/// Converts Ogmios PlutusCostModels to the whisky format (Vec<Vec<i64>>).
///
/// The output is a vector of three cost model vectors: [V1, V2, V3].
pub fn convert_cost_models(cost_models: &PlutusCostModels) -> Vec<Vec<i64>> {
	vec![
		cost_models.plutus_v1.iter().map(|&v| v as i64).collect(),
		cost_models.plutus_v2.iter().map(|&v| v as i64).collect(),
		cost_models.plutus_v3.iter().map(|&v| v as i64).collect(),
	]
}

/// Builds and signs a governance contract deployment transaction.
///
/// Returns the signed transaction hex.
pub fn build_deploy_transaction(params: DeployParams<'_>) -> Result<String, DeployError> {
	// Use 5 ADA to ensure minimum UTXO requirement is met with large datums
	let send_assets = vec![
		Asset::new_from_str("lovelace", "5000000"),
		Asset::new_from_str(&params.contract.policy_id, "1"),
	];

	let funding_hash = hex::encode(params.funding_utxo.transaction.id);
	let collateral_hash = hex::encode(params.collateral_utxo.transaction.id);
	let one_shot_hash = hex::encode(params.one_shot_utxo.transaction.id);

	// Use Network::Custom with cost models from protocol parameters
	// This ensures the script integrity hash matches what the ledger computes
	let network = Network::Custom(params.cost_models);

	let mut tx_builder = TxBuilder::new_core();
	tx_builder
		.network(network)
		.set_evaluator(Box::new(OfflineTxEvaluator::new()))
		// Funding input
		.tx_in(
			&funding_hash,
			params.funding_utxo.index.into(),
			&build_asset_vector(params.funding_utxo),
			params.funded_address,
		)
		// One-shot input (consumed by minting policy)
		.tx_in(
			&one_shot_hash,
			params.one_shot_utxo.index.into(),
			&build_asset_vector(params.one_shot_utxo),
			params.funded_address,
		)
		// Collateral
		.tx_in_collateral(
			&collateral_hash,
			params.collateral_utxo.index.into(),
			&build_asset_vector(params.collateral_utxo),
			params.funded_address,
		)
		// Output to script address with NFT and datum
		.tx_out(&params.contract.script_address, &send_assets)
		.tx_out_inline_datum_value(&WData::JSON(params.datum.to_string()))
		// Mint the NFT
		.mint_plutus_script_v3()
		.mint(1, &params.contract.policy_id, "")
		.minting_script(&params.contract.encoded_cbor)
		.mint_redeemer_value(&WRedeemer {
			data: WData::JSON(params.redeemer.to_string()),
			ex_units: Budget { mem: 14000000, steps: 10000000000 },
		})
		.change_address(params.funded_address)
		.required_signer_hash(params.payment_keyhash)
		.signing_key(params.signing_key_cbor)
		.complete_sync(None)
		.map_err(|e| DeployError::TxBuild(format!("{:?}", e)))?;

	tx_builder
		.complete_signing()
		.map_err(|e| DeployError::TxSign(format!("{:?}", e)))?;

	Ok(tx_builder.tx_hex())
}

/// Returns the network ID for testnet (preview/preprod).
pub fn testnet_network_id() -> u8 {
	NetworkInfo::testnet_preview().network_id()
}

/// Returns the network ID for mainnet.
pub fn mainnet_network_id() -> u8 {
	NetworkInfo::mainnet().network_id()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_build_versioned_multisig_datum() {
		let members = vec![
			GovernanceMember {
				cardano_hash: "e8c300330fe315531ca89d4a2e7d0c80211bc70b473b1ed4979dff2a"
					.to_string(),
				sr25519_key: "d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"
					.to_string(),
			},
			GovernanceMember {
				cardano_hash: "e8c300330fe315531ca89d4a2e7d0c80211bc70b473b1ed4979dff2b"
					.to_string(),
				sr25519_key: "e659a7a1628cdd93febc04a4e0646ea20e9f5f0ce097d9a05290d4a9e054df4e"
					.to_string(),
			},
		];

		let datum = build_versioned_multisig_datum(&members);

		// Verify structure
		let list = datum.get("list").expect("should have list");
		assert!(list.is_array());
		let outer = list.as_array().unwrap();
		assert_eq!(outer.len(), 2); // [multisig_data, logic_round]

		// Check logic_round is 0
		let logic_round = outer[1].get("int").expect("should have int");
		assert_eq!(logic_round.as_u64(), Some(0));
	}

	#[test]
	fn test_build_federated_ops_datum() {
		let candidates = vec![FederatedOpsCandidate {
			ecdsa_key: "020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a1"
				.to_string(),
			aura_key: "d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"
				.to_string(),
			grandpa_key: "88dc3417d5058ec4b4503e0c12ea1a0a89be200fe98922423d4334014fa6b0ee"
				.to_string(),
		}];

		let datum = build_federated_ops_datum(&candidates);

		// Verify structure: [data, appendix, logic_round]
		let list = datum.get("list").expect("should have list");
		assert!(list.is_array());
		let outer = list.as_array().unwrap();
		assert_eq!(outer.len(), 3); // [data, appendix, logic_round]

		// Check logic_round is 1 (required for SDK V1 parsing)
		let logic_round = outer[2].get("int").expect("should have int");
		assert_eq!(logic_round.as_u64(), Some(1));
	}
}
