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

use clap::Args;
use midnight_primitives_ics_observation::IcsConfig;
use ogmios_client::{
	jsonrpsee::client_for_url, query_ledger_state::QueryLedgerState, transactions::Transactions,
	types::OgmiosUtxo,
};
use std::path::PathBuf;
use std::time::Duration;
use whisky::csl::{
	Address, AssetName, Assets, BigNum, ChangeConfig, CoinSelectionStrategyCIP2, Credential,
	DataCost, EnterpriseAddress, MetadataList, MinOutputAdaCalculator, MultiAsset, NetworkIdKind,
	PrivateKey, ScriptHash, Transaction, TransactionHash, TransactionInput, TransactionMetadatum,
	TransactionOutput, TransactionOutputBuilder, TransactionUnspentOutput,
	TransactionUnspentOutputs, Vkey, Vkeywitness, Vkeywitnesses,
};
use whisky::{Network, Protocol, build_tx_builder};

const BRIDGE_TRANSFER_METADATUM_KEY: u64 = 6500973;

#[derive(Args)]
pub struct BridgeTransferArgs {
	/// Path to the Cardano payment signing key file
	#[arg(long, short = 'w')]
	wallet: String,

	/// Amount of cNight tokens to transfer
	#[arg(long)]
	amount: u64,

	/// Path to the ICS configuration file (provides ICS address and cNight token identity)
	#[arg(long)]
	ics_config: PathBuf,

	/// Hex-encoded target address (included in transaction metadata)
	#[arg(long)]
	target_address: String,

	/// URL of the Ogmios server
	#[arg(long, short = 'O', default_value = "ws://localhost:1337", env = "OGMIOS_URL")]
	ogmios_url: String,

	/// Cardano network (mainnet, preprod, preview)
	#[arg(long, default_value = "preprod")]
	network: String,
}

pub async fn execute(
	args: BridgeTransferArgs,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let ics_config = load_ics_config(&args.ics_config)?;

	let payment_key = read_payment_key(&args.wallet)?;

	let target_address_bytes = hex::decode(&args.target_address)
		.map_err(|e| format!("Invalid target address hex: {e}"))?;

	let policy_id_bytes = ics_config.asset.policy_id.0;
	let asset_name_bytes = ics_config.asset.asset_name.as_bytes().to_vec();

	let network: Network = args
		.network
		.clone()
		.try_into()
		.map_err(|e: serde_json::Error| format!("Invalid network '{}': {e}", args.network))?;

	let network_id = match &network {
		Network::Mainnet => NetworkIdKind::Mainnet,
		_ => NetworkIdKind::Testnet,
	};

	let client = client_for_url(&args.ogmios_url, Duration::from_secs(180))
		.await
		.map_err(|e| format!("Failed to connect to Ogmios at {}: {e}", args.ogmios_url))?;

	let protocol_parameters = query_protocol(&client).await?;

	let pub_key_hash = payment_key.to_public().hash();
	let payment_address =
		EnterpriseAddress::new(network_id as u8, &Credential::from_keyhash(&pub_key_hash))
			.to_address();
	let payment_address_bech32 = payment_address.to_bech32(None).map_err(|e| e.to_string())?;
	let payment_key_utxos = client.query_utxos(&[payment_address_bech32]).await?;

	let ics_address =
		Address::from_bech32(&ics_config.illiquid_circulation_supply_validator_address)
			.map_err(|e| format!("Invalid ICS address in config: {e}"))?;

	let tx = build_bridge_transfer_tx(
		&ics_address,
		&policy_id_bytes,
		&asset_name_bytes,
		args.amount,
		&target_address_bytes,
		&protocol_parameters,
		&payment_key_utxos,
		&payment_address,
	)?;

	let signed_tx = sign_transaction(&tx, &payment_key);
	let signed_tx_bytes = signed_tx.to_bytes();

	let res = client.submit_transaction(&signed_tx_bytes).await.map_err(|e| {
		format!(
			"Bridge transfer transaction submission failed: {e}, tx bytes: {}",
			hex::encode(&signed_tx_bytes)
		)
	})?;

	let tx_id = hex::encode(res.transaction.id);
	log::info!("Bridge transfer transaction submitted: {tx_id}");
	println!("Bridge transfer transaction submitted: {tx_id}");

	Ok(())
}

fn load_ics_config(path: &PathBuf) -> Result<IcsConfig, Box<dyn std::error::Error + Send + Sync>> {
	let content = std::fs::read_to_string(path)
		.map_err(|e| format!("Could not read ICS config at {}: {e}", path.display()))?;
	let config: IcsConfig = serde_json::from_str(&content)
		.map_err(|e| format!("Invalid ICS config JSON at {}: {e}", path.display()))?;
	Ok(config)
}

/// Parse a Cardano payment signing key file (JSON format with `type` and `cborHex` fields).
fn read_payment_key(path: &str) -> Result<PrivateKey, Box<dyn std::error::Error + Send + Sync>> {
	let content = std::fs::read_to_string(path)
		.map_err(|e| format!("Could not read key file at {path}: {e}"))?;

	#[derive(serde::Deserialize)]
	#[serde(rename_all = "camelCase")]
	struct KeyFile {
		r#type: String,
		cbor_hex: String,
	}

	let key_file: KeyFile = serde_json::from_str(&content)
		.map_err(|e| format!("{path} is not a valid Cardano key JSON file: {e}"))?;

	// Strip CBOR prefix (first 4 hex chars = 2 bytes)
	let raw_hex = key_file.cbor_hex.get(4..).ok_or("cborHex too short")?;

	let raw_bytes = hex::decode(raw_hex).map_err(|e| format!("Invalid cborHex: {e}"))?;

	match key_file.r#type.as_str() {
		"PaymentSigningKeyShelley_ed25519" => PrivateKey::from_normal_bytes(&raw_bytes)
			.map_err(|e| format!("Failed to parse normal signing key: {e}").into()),
		"PaymentExtendedSigningKeyShelley_ed25519_bip32" => {
			let prefix = &raw_bytes[..64];
			PrivateKey::from_extended_bytes(prefix)
				.map_err(|e| format!("Failed to parse extended signing key: {e}").into())
		},
		other => Err(format!("Unsupported key type: {other}").into()),
	}
}

async fn query_protocol<C: QueryLedgerState>(
	client: &C,
) -> Result<Protocol, Box<dyn std::error::Error + Send + Sync>> {
	let pp = client.query_protocol_parameters().await?;
	Ok(Protocol {
		epoch: 0,
		min_fee_a: pp.min_fee_coefficient.into(),
		min_fee_b: pp.min_fee_constant.lovelace,
		max_block_size: 0,
		max_tx_size: pp.max_transaction_size.bytes,
		max_block_header_size: 0,
		key_deposit: pp.stake_credential_deposit.lovelace,
		pool_deposit: pp.stake_pool_deposit.lovelace,
		decentralisation: 0.0,
		min_pool_cost: "0".to_string(),
		price_mem: *pp.script_execution_prices.memory.numer() as f64
			/ *pp.script_execution_prices.memory.denom() as f64,
		price_step: *pp.script_execution_prices.cpu.numer() as f64
			/ *pp.script_execution_prices.cpu.denom() as f64,
		max_tx_ex_mem: "16000000".to_string(),
		max_tx_ex_steps: "10000000000".to_string(),
		max_block_ex_mem: "80000000".to_string(),
		max_block_ex_steps: "40000000000".to_string(),
		max_val_size: pp.max_value_size.bytes,
		collateral_percent: pp.collateral_percentage as f64,
		max_collateral_inputs: pp.max_collateral_inputs as i32,
		coins_per_utxo_size: pp.min_utxo_deposit_coefficient,
		min_fee_ref_script_cost_per_byte: pp.min_fee_reference_scripts.base as u64,
	})
}

#[allow(clippy::too_many_arguments)]
fn build_bridge_transfer_tx(
	ics_address: &Address,
	policy_id_bytes: &[u8; 28],
	asset_name_bytes: &[u8],
	amount: u64,
	target_address_bytes: &[u8],
	protocol_parameters: &Protocol,
	payment_key_utxos: &[OgmiosUtxo],
	change_address: &Address,
) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync>> {
	let mut tx_builder = build_tx_builder(Some(protocol_parameters.clone()))
		.map_err(|e| format!("Failed to create transaction builder: {e}"))?;

	// Add metadata: key 1234321, value = list with one item (bytes of target address)
	let mut metadata_list = MetadataList::new();
	metadata_list.add(
		&TransactionMetadatum::new_bytes(target_address_bytes.to_vec())
			.map_err(|e| e.to_string())?,
	);
	tx_builder.add_metadatum(
		&BRIDGE_TRANSFER_METADATUM_KEY.into(),
		&TransactionMetadatum::new_list(&metadata_list),
	);

	// Build output: send cNight tokens to ICS address with minimum ADA
	let output_builder = TransactionOutputBuilder::new()
		.with_address(ics_address)
		.with_plutus_data(&whisky::csl::PlutusData::new_empty_constr_plutus_data(&BigNum::zero()))
		.next()
		.map_err(|e| e.to_string())?;

	let mut ma = MultiAsset::new();
	let mut assets = Assets::new();
	let asset_name = AssetName::new(asset_name_bytes.to_vec()).map_err(|e| e.to_string())?;
	assets.insert(&asset_name, &amount.into());
	ma.insert(&ScriptHash::from(*policy_id_bytes), &assets);

	let min_ada = {
		let tmp_output = output_builder
			.with_coin_and_asset(&0u64.into(), &ma)
			.build()
			.map_err(|e| e.to_string())?;
		MinOutputAdaCalculator::new(
			&tmp_output,
			&DataCost::new_coins_per_byte(&protocol_parameters.coins_per_utxo_size.into()),
		)
		.calculate_ada()
		.map_err(|e| e.to_string())?
	};

	let output = output_builder
		.with_coin_and_asset(&min_ada, &ma)
		.build()
		.map_err(|e| e.to_string())?;
	tx_builder.add_output(&output).map_err(|e| e.to_string())?;

	// Add inputs from wallet UTxOs and set change address
	let utxos = ogmios_utxos_to_csl(payment_key_utxos)?;
	tx_builder
		.add_inputs_from_and_change(
			&utxos,
			CoinSelectionStrategyCIP2::LargestFirstMultiAsset,
			&ChangeConfig::new(change_address),
		)
		.map_err(|e| format!("Could not balance transaction: {e}"))?;

	tx_builder.build_tx().map_err(|e| e.to_string().into())
}

fn sign_transaction(tx: &Transaction, payment_key: &PrivateKey) -> Transaction {
	let tx_body_hash = sp_crypto_hashing::blake2_256(&tx.body().to_bytes());
	let signature = payment_key.sign(&tx_body_hash);
	let mut witness_set = tx.witness_set();
	let mut vkeywitnesses = witness_set.vkeys().unwrap_or_else(Vkeywitnesses::new);
	vkeywitnesses.add(&Vkeywitness::new(&Vkey::new(&payment_key.to_public()), &signature));
	witness_set.set_vkeys(&vkeywitnesses);
	Transaction::new(&tx.body(), &witness_set, tx.auxiliary_data())
}

fn ogmios_utxos_to_csl(
	utxos: &[OgmiosUtxo],
) -> Result<TransactionUnspentOutputs, Box<dyn std::error::Error + Send + Sync>> {
	let mut result = TransactionUnspentOutputs::new();
	for utxo in utxos {
		let input =
			TransactionInput::new(&TransactionHash::from(utxo.transaction.id), utxo.index.into());
		let output = TransactionOutput::new(
			&Address::from_bech32(&utxo.address).map_err(|e| e.to_string())?,
			&ogmios_value_to_csl(&utxo.value)?,
		);
		result.add(&TransactionUnspentOutput::new(&input, &output));
	}
	Ok(result)
}

fn ogmios_value_to_csl(
	value: &ogmios_client::types::OgmiosValue,
) -> Result<whisky::csl::Value, Box<dyn std::error::Error + Send + Sync>> {
	if !value.native_tokens.is_empty() {
		let mut multiasset = MultiAsset::new();
		for (policy_id, tokens) in value.native_tokens.iter() {
			let mut csl_assets = Assets::new();
			for token in tokens.iter() {
				let asset_name = AssetName::new(token.name.clone()).map_err(|e| e.to_string())?;
				csl_assets.insert(&asset_name, &token.amount.into());
			}
			multiasset.insert(&ScriptHash::from(*policy_id), &csl_assets);
		}
		Ok(whisky::csl::Value::new_with_assets(&value.lovelace.into(), &multiasset))
	} else {
		Ok(whisky::csl::Value::new(&value.lovelace.into()))
	}
}
