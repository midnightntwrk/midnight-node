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
	jsonrpsee::client_for_url, query_ledger_state::QueryLedgerState, query_network::QueryNetwork,
	transactions::Transactions, types::OgmiosUtxo,
};
use std::path::{Path, PathBuf};
use std::time::Duration;
use whisky::csl::{
	Address, AssetName, Assets, ChangeConfig, CoinSelectionStrategyCIP2, Credential, DataCost,
	EnterpriseAddress, MetadataList, MinOutputAdaCalculator, MultiAsset, PrivateKey, ScriptHash,
	Transaction, TransactionHash, TransactionInput, TransactionMetadatum, TransactionOutput,
	TransactionOutputBuilder, TransactionUnspentOutput, TransactionUnspentOutputs, Vkey,
	Vkeywitness, Vkeywitnesses,
};
use whisky::{Protocol, build_tx_builder};

const BRIDGE_TRANSFER_METADATUM_KEY: u64 = 6500973;

#[derive(Args)]
pub struct BridgeTransferArgs {
	/// Path to the Cardano payment signing key file
	#[arg(long)]
	signing_key: PathBuf,

	/// Path to the ICS configuration file (provides ICS address and cNight asset id).
	#[arg(long)]
	ics_config: PathBuf,

	/// Hex-encoded midnight UserAddress
	#[arg(long, conflicts_with_all(["invalid", "reserve"]))]
	recipient_address: Option<String>,

	/// Transfer to Reserve
	#[arg(long, conflicts_with_all(["invalid", "recipient_address"]))]
	reserve: bool,

	/// Makes invalid transfer for tests
	#[arg(long, conflicts_with_all(["reserve", "recipient_address"]))]
	invalid: bool,

	/// Amount of cNight tokens to transfer
	#[arg(long)]
	amount: u64,

	/// URL of the Ogmios server
	#[arg(long, short = 'O', default_value = "ws://localhost:1337", env = "OGMIOS_URL")]
	ogmios_url: String,
}

pub async fn execute(
	args: BridgeTransferArgs,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let ics_config = load_ics_config(&args.ics_config)?;
	let payment_key = read_payment_key(&args.signing_key)?;
	let recipient = parse_recipient_args(&args)?;
	let client = client_for_url(&args.ogmios_url, Duration::from_secs(30))
		.await
		.map_err(|e| format!("Failed to connect to Ogmios at {}: {e}", args.ogmios_url))?;
	let shelley_config = client.shelley_genesis_configuration().await?;
	let protocol_parameters = query_protocol(&client).await?;

	let payment_address = get_payment_key_address(&payment_key, shelley_config);
	let payment_address_bech32 = payment_address.to_bech32(None).map_err(|e| e.to_string())?;
	log::info!("Querying utxos of {payment_address_bech32}");
	let payment_key_utxos = client.query_utxos(&[payment_address_bech32]).await?;
	let tx = build_bridge_transfer_tx(
		ics_config,
		args.amount,
		recipient,
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
	Ok(())
}

fn load_ics_config(path: &Path) -> Result<IcsConfig, Box<dyn std::error::Error + Send + Sync>> {
	let content = std::fs::read_to_string(path)
		.map_err(|e| format!("Could not read ICS config at {}: {e}", path.display()))?;
	let config: IcsConfig = serde_json::from_str(&content)
		.map_err(|e| format!("Invalid ICS config JSON at {}: {e}", path.display()))?;
	Ok(config)
}

/// Parse a Cardano payment signing key file (JSON format with `type` and `cborHex` fields).
fn read_payment_key(path: &Path) -> Result<PrivateKey, Box<dyn std::error::Error + Send + Sync>> {
	let content = std::fs::read_to_string(path)
		.map_err(|e| format!("Could not read key file at {path:?}: {e}"))?;

	#[derive(serde::Deserialize)]
	#[serde(rename_all = "camelCase")]
	struct KeyFile {
		r#type: String,
		cbor_hex: String,
	}

	let key_file: KeyFile = serde_json::from_str(&content)
		.map_err(|e| format!("{path:?} is not a valid Cardano key JSON file: {e}"))?;

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

enum Recipient {
	ToAddress([u8; 32]),
	Reserve,
	Invalid,
}

fn parse_recipient_args(
	args: &BridgeTransferArgs,
) -> Result<Recipient, Box<dyn std::error::Error + Send + Sync + 'static>> {
	if args.invalid {
		Ok(Recipient::Invalid)
	} else if args.reserve {
		Ok(Recipient::Reserve)
	} else {
		let address = args.recipient_address.as_ref().ok_or_else(|| {
			"Either --reserve or --invalid or --recipient-address has to be set".to_string()
		})?;
		let bytes = hex::decode(address)
			.map_err(|e| format!("--recipient-address is not valid hex string: {e}"))?;
		let bytes: [u8; 32] = bytes
			.try_into()
			.map_err(|_| "Invalid --recipient_address length, expected 32 bytes".to_string())?;
		Ok(Recipient::ToAddress(bytes))
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
		max_block_size: pp.max_block_body_size.bytes as i32,
		max_tx_size: pp.max_transaction_size.bytes,
		max_block_header_size: pp.max_block_header_size.bytes as i32,
		key_deposit: pp.stake_credential_deposit.lovelace,
		pool_deposit: pp.stake_pool_deposit.lovelace,
		decentralisation: 0.0,
		min_pool_cost: pp.min_stake_pool_cost.lovelace.to_string(),
		price_mem: *pp.script_execution_prices.memory.numer() as f64
			/ *pp.script_execution_prices.memory.denom() as f64,
		price_step: *pp.script_execution_prices.cpu.numer() as f64
			/ *pp.script_execution_prices.cpu.denom() as f64,
		max_tx_ex_mem: pp.max_execution_units_per_transaction.memory.to_string(),
		max_tx_ex_steps: pp.max_execution_units_per_transaction.cpu.to_string(),
		max_block_ex_mem: pp.max_execution_units_per_block.memory.to_string(),
		max_block_ex_steps: pp.max_execution_units_per_block.cpu.to_string(),
		max_val_size: pp.max_value_size.bytes,
		collateral_percent: pp.collateral_percentage as f64,
		max_collateral_inputs: pp.max_collateral_inputs as i32,
		coins_per_utxo_size: pp.min_utxo_deposit_coefficient,
		min_fee_ref_script_cost_per_byte: pp.min_fee_reference_scripts.base as u64,
	})
}

fn get_payment_key_address(
	payment_key: &PrivateKey,
	shelley_config: ogmios_client::query_network::ShelleyGenesisConfigurationResponse,
) -> Address {
	let network_kind_id = match shelley_config.network {
		sidechain_domain::NetworkType::Mainnet => 1,
		sidechain_domain::NetworkType::Testnet => 0,
	};
	EnterpriseAddress::new(
		network_kind_id,
		&Credential::from_keyhash(&payment_key.to_public().hash()),
	)
	.to_address()
}

fn build_bridge_transfer_tx(
	ics_config: IcsConfig,
	amount: u64,
	recipient: Recipient,
	protocol_parameters: &Protocol,
	payment_key_utxos: &[OgmiosUtxo],
	change_address: &Address,
) -> Result<Transaction, Box<dyn std::error::Error + Send + Sync>> {
	log::info!("Building transaction...");
	let mut tx_builder = build_tx_builder(Some(protocol_parameters.clone()))
		.map_err(|e| format!("Failed to create transaction builder: {e}"))?;

	tx_builder
		.add_metadatum(&BRIDGE_TRANSFER_METADATUM_KEY.into(), &build_metadata_item(recipient)?);

	let ics_address =
		Address::from_bech32(&ics_config.illiquid_circulation_supply_validator_address)
			.map_err(|e| format!("Invalid ICS address in config: {e}"))?;

	// Build output: send cNight tokens to ICS address with minimum ADA
	let output_builder = TransactionOutputBuilder::new()
		.with_address(&ics_address)
		.next()
		.map_err(|e| e.to_string())?;

	let ma = build_cnight_multi_asset(&ics_config, amount)?;
	let min_ada = calculate_min_ada(protocol_parameters, &output_builder, &ma)?;

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

fn build_metadata_item(
	recipient: Recipient,
) -> Result<TransactionMetadatum, Box<dyn std::error::Error + Send + Sync + 'static>> {
	let metadata_item = match recipient {
		Recipient::ToAddress(address_bytes) => {
			let mut metadata_list = MetadataList::new();
			metadata_list.add(
				&TransactionMetadatum::new_bytes(address_bytes.to_vec())
					.map_err(|e| e.to_string())?,
			);
			TransactionMetadatum::new_list(&metadata_list)
		},
		Recipient::Reserve => TransactionMetadatum::new_list(&MetadataList::new()),
		Recipient::Invalid => {
			TransactionMetadatum::new_text("this is invalid bridge tx metadata".to_owned())
				.map_err(|e| e.to_string())?
		},
	};
	Ok(metadata_item)
}

fn build_cnight_multi_asset(
	ics_config: &IcsConfig,
	amount: u64,
) -> Result<MultiAsset, Box<dyn std::error::Error + Send + Sync + 'static>> {
	let mut ma = MultiAsset::new();
	let mut assets = Assets::new();
	let asset_name = AssetName::new(ics_config.asset.asset_name.as_bytes().to_vec())
		.map_err(|e| e.to_string())?;
	assets.insert(&asset_name, &amount.into());
	ma.insert(&ScriptHash::from(ics_config.asset.policy_id.0), &assets);
	Ok(ma)
}

fn calculate_min_ada(
	protocol_parameters: &Protocol,
	output_builder: &whisky::csl::TransactionOutputAmountBuilder,
	ma: &MultiAsset,
) -> Result<whisky::csl::BigNum, Box<dyn std::error::Error + Send + Sync + 'static>> {
	let tmp_output = output_builder
		.with_coin_and_asset(&0u64.into(), ma)
		.build()
		.map_err(|e| e.to_string())?;
	Ok(MinOutputAdaCalculator::new(
		&tmp_output,
		&DataCost::new_coins_per_byte(&protocol_parameters.coins_per_utxo_size.into()),
	)
	.calculate_ada()
	.map_err(|e| e.to_string())?)
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
			for token in tokens {
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

fn sign_transaction(tx: &Transaction, payment_key: &PrivateKey) -> Transaction {
	let tx_body_hash = sp_crypto_hashing::blake2_256(&tx.body().to_bytes());
	let signature = payment_key.sign(&tx_body_hash);
	let mut witness_set = tx.witness_set();
	let mut vkeywitnesses = witness_set.vkeys().unwrap_or_else(Vkeywitnesses::new);
	vkeywitnesses.add(&Vkeywitness::new(&Vkey::new(&payment_key.to_public()), &signature));
	witness_set.set_vkeys(&vkeywitnesses);
	Transaction::new(&tx.body(), &witness_set, tx.auxiliary_data())
}
