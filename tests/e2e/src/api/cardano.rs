use crate::cfg::*;
use bip39::{Language, Mnemonic, MnemonicType};
use ogmios_client::{
	jsonrpsee::client_for_url, jsonrpsee::OgmiosClients, query_ledger_state::QueryLedgerState,
	transactions::*, types::OgmiosUtxo, OgmiosClientError,
};
use std::fs;
use std::time::Duration;
use whisky::csl::*;
use whisky::data::{constr0, constr1};
use whisky::*;

pub async fn find_utxo_by_tx_id(address: &str, tx_id: &str) -> Option<OgmiosUtxo> {
	let client = get_ogmios_client().await;
	let mut attempts = 0;
	while attempts < 10 {
		let utxos = client.query_utxos(&[address.into()]).await.unwrap();
		println!("Waiting for UTXO with tx_id {}, attempt {}: {:?}", tx_id, attempts + 1, utxos);
		for utxo in &utxos {
			if hex::encode(utxo.transaction.id) == tx_id {
				println!("Found UTXO: {:?}", utxo);
				return Some(utxo.clone());
			}
		}
		attempts += 1;
		tokio::time::sleep(Duration::from_secs(1)).await;
	}
	None
}

/// Waits for a UTXO with the given tx_id to appear, then waits for 3 more Cardano blocks to see if it is spent.
/// Returns true if the UTXO is still unspent after 3 blocks, false if it is spent.
pub async fn wait_utxo_unspent_for_3_blocks(address: &str, tx_id: &str) -> bool {
	let client = get_ogmios_client().await;

	// Get the current block number (slot) as the starting point
	let start_tip = client.get_tip().await.unwrap();
	let start_slot = start_tip.slot;
	println!("Current slot is {}. Waiting for 3 more slots...", start_slot);

	// Wait for 3 more slots (blocks)
	let mut last_slot = start_slot;
	while last_slot < start_slot + 3 {
		let tip = client.get_tip().await.unwrap();
		if tip.slot > last_slot {
			println!("Slot advanced: {} -> {}", last_slot, tip.slot);
			last_slot = tip.slot;
		}
		tokio::time::sleep(Duration::from_secs(1)).await;
	}

	// After 3 slots, check if the UTXO is still present
	let utxos = client.query_utxos(&[address.into()]).await.unwrap();
	let still_unspent = utxos.iter().any(|u| hex::encode(u.transaction.id) == tx_id);
	if still_unspent {
		println!("UTXO {} is still unspent after 3 slots.", tx_id);
	} else {
		println!("UTXO {} was spent within 3 slots.", tx_id);
	}
	still_unspent
}

pub async fn get_ogmios_client() -> OgmiosClients {
	let cfg = load_config();
	let ogmios_url = cfg.ogmios_url.clone();
	client_for_url(&ogmios_url, Duration::from_secs(5)).await.unwrap()
}

pub fn build_asset_vector(ogmios_utxo: &OgmiosUtxo) -> Vec<Asset> {
	let mut input_assets: Vec<Asset> = Vec::new();
	input_assets.push(Asset::new_from_str("lovelace", &ogmios_utxo.value.lovelace.to_string()));
	for (policy_id, assets) in &ogmios_utxo.value.native_tokens {
		let policy_id_str = hex::encode(policy_id);
		for asset in assets {
			input_assets
				.push(Asset::new_from_str(&policy_id_str, asset.amount.to_string().as_str()));
		}
	}
	input_assets
}

pub async fn send(address: &str, assets: Vec<Asset>) -> String {
	let cfg = load_config();
	let payment_addr = cfg.payment_addr.clone();
	let client = get_ogmios_client().await;
	let utxos = client.query_utxos(std::slice::from_ref(&payment_addr)).await.unwrap();
	assert!(!utxos.is_empty(), "No UTXOs found for funding address");
	let utxo = utxos
		.iter()
		.max_by_key(|u| u.value.lovelace)
		.expect("No UTXO with lovelace found");
	let skey_json =
		fs::read_to_string(&cfg.payment_skey_file).expect("Failed to read payment.skey");
	let skey_value: serde_json::Value =
		serde_json::from_str(&skey_json).expect("Invalid skey JSON");
	let cbor_hex = skey_value["cborHex"].as_str().expect("No cborHex in skey JSON");
	let input_tx_hash = hex::encode(utxo.transaction.id);
	let input_index = utxo.index;
	let input_assets = build_asset_vector(utxo);
	let mut tx_builder = whisky::TxBuilder::new_core();
	tx_builder
		.tx_in(&input_tx_hash, input_index.into(), &input_assets, address)
		.tx_out(address, &assets)
		.change_address(&payment_addr)
		.signing_key(cbor_hex)
		.complete_sync(None)
		.unwrap()
		.complete_signing()
		.unwrap();
	let tx_bytes = hex::decode(tx_builder.tx_hex()).expect("Failed to decode hex string");
	let response = client.submit_transaction(&tx_bytes).await.unwrap();
	hex::encode(response.transaction.id)
}

pub async fn register(
	midnight_address_hex: &str,
	signing_wallet: &Wallet,
	tx_in: &OgmiosUtxo,
	collateral_utxo: &OgmiosUtxo,
) -> [u8; 32] {
	let validator_address = get_mapping_validator_address();
	let datum = serde_json::to_string(&serde_json::json!(
		{
			"constructor": 0,
			"fields": [
				{
					"constructor": 0,
					"fields": [
						{
							"bytes": &signing_wallet.account.public_key.hash().to_hex()
						}
					]
				},
				{
					"bytes": midnight_address_hex
				}
			]
		}
	))
	.unwrap();
	let payment_addr = get_cardano_address_as_bech32(signing_wallet);
	let auth_token_policy_id = get_auth_token_policy_id();
	let send_assets = vec![
		Asset::new_from_str("lovelace", "2000000"),
		Asset::new_from_str(&auth_token_policy_id, "1"),
	];
	let cfg = load_config();
	let minting_script = load_cbor(&cfg.auth_token_policy_file);
	let network = Network::Custom(get_local_env_cost_models());

	let mut tx_builder = whisky::TxBuilder::new_core();
	tx_builder
		.network(network.clone())
		.set_evaluator(Box::new(OfflineTxEvaluator::new()))
		.tx_in(
			&hex::encode(tx_in.transaction.id),
			tx_in.index.into(),
			&build_asset_vector(tx_in),
			&payment_addr,
		)
		.tx_in_collateral(
			&hex::encode(collateral_utxo.transaction.id),
			collateral_utxo.index.into(),
			&build_asset_vector(collateral_utxo),
			&payment_addr,
		)
		.tx_out(&validator_address, &send_assets)
		.tx_out_inline_datum_value(&WData::JSON(datum))
		.mint_plutus_script_v3()
		.mint(1, &auth_token_policy_id, "")
		.minting_script(&minting_script)
		.mint_redeemer_value(&WRedeemer {
			data: WData::JSON(constr0(serde_json::json!([])).to_string()),
			ex_units: Budget { mem: 14000000, steps: 10000000000 },
		})
		.change_address(&payment_addr)
		.required_signer_hash(&signing_wallet.account.public_key.hash().to_hex())
		.complete_sync(None)
		.unwrap();

	let signed_tx = signing_wallet.sign_tx(&tx_builder.tx_hex());
	let tx_bytes = hex::decode(signed_tx.unwrap()).expect("Failed to decode hex string");
	let client = get_ogmios_client().await;
	let response = client.submit_transaction(&tx_bytes).await.unwrap();
	println!("Transaction submitted, response: {:?}", response);
	response.transaction.id
}

#[derive(Debug)]
pub enum DeregisterError {
	Whisky(WError),
	Ogmios(OgmiosClientError),
}

impl From<WError> for DeregisterError {
	fn from(err: WError) -> Self {
		DeregisterError::Whisky(err)
	}
}

impl From<OgmiosClientError> for DeregisterError {
	fn from(err: OgmiosClientError) -> Self {
		DeregisterError::Ogmios(err)
	}
}

pub async fn deregister(
	signing_wallet: &Wallet,
	tx_in: &OgmiosUtxo,
	register_tx: &OgmiosUtxo,
	collateral_utxo: &OgmiosUtxo,
) -> Result<[u8; 32], DeregisterError> {
	let validator_address = get_mapping_validator_address();
	let datum = serde_json::to_string(&serde_json::json!({"constructor": 0,"fields": []})).unwrap();
	let payment_addr = get_cardano_address_as_bech32(signing_wallet);
	let auth_token_policy_id = get_auth_token_policy_id();
	let send_assets = vec![Asset::new_from_str("lovelace", "2000000")];
	let cfg = load_config();
	let minting_script = load_cbor(&cfg.auth_token_policy_file);
	let network = Network::Custom(get_local_env_cost_models());
	let mapping_validator_cbor = &load_cbor(&cfg.auth_token_policy_file);
	let register_asset_tx_vector = build_asset_vector(register_tx);
	println!("Register tx assets: {:?}", register_asset_tx_vector);
	let script_hash = whisky::get_script_hash(mapping_validator_cbor, LanguageVersion::V2);
	println!("Mapping validator script hash: {:?}", script_hash);

	let mut tx_builder = whisky::TxBuilder::new_core();
	tx_builder
		.network(network.clone())
		.set_evaluator(Box::new(OfflineTxEvaluator::new()))
		.tx_in(
			&hex::encode(tx_in.transaction.id),
			tx_in.index.into(),
			&build_asset_vector(tx_in),
			&payment_addr,
		)
		.spending_plutus_script_v3()
		.tx_in(
			&hex::encode(register_tx.transaction.id),
			register_tx.index.into(),
			&build_asset_vector(register_tx),
			&validator_address,
		)
		.tx_in_inline_datum_present()
		.tx_in_script(mapping_validator_cbor)
		.tx_in_redeemer_value(&WRedeemer {
			data: WData::JSON(datum),
			ex_units: Budget { mem: 3765700, steps: 941562940 },
		})
		.tx_in_collateral(
			&hex::encode(collateral_utxo.transaction.id),
			collateral_utxo.index.into(),
			&build_asset_vector(collateral_utxo),
			&payment_addr,
		)
		.tx_out(&payment_addr, &send_assets)
		.mint_plutus_script_v3()
		.mint(-1, &auth_token_policy_id, "")
		.minting_script(&minting_script)
		.mint_redeemer_value(&WRedeemer {
			data: WData::JSON(constr1(serde_json::json!([])).to_string()),
			ex_units: Budget { mem: 3765700, steps: 941562940 },
		})
		.change_address(&payment_addr)
		.required_signer_hash(&signing_wallet.account.public_key.hash().to_hex())
		.complete_sync(None)?;

	let signed_tx = signing_wallet.sign_tx(&tx_builder.tx_hex())?;
	let tx_bytes = hex::decode(signed_tx).expect("Failed to decode hex string");
	let client = get_ogmios_client().await;
	let response = client.submit_transaction(&tx_bytes).await?;
	println!("Transaction submitted, response: {:?}", response);
	Ok(response.transaction.id)
}

pub fn create_wallet() -> Wallet {
	let mnemonic = Mnemonic::new(MnemonicType::Words24, Language::English);
	let phrase = mnemonic.phrase();
	Wallet::new_mnemonic(phrase).unwrap()
}

pub fn get_cardano_address(wallet: &Wallet) -> Address {
	let pub_key_hash = wallet.account.public_key.hash();
	let payment_cred = whisky::csl::Credential::from_keyhash(&pub_key_hash);
	let network = NetworkInfo::testnet_preview().network_id();
	whisky::csl::RewardAddress::new(network, &payment_cred).to_address()
}

pub fn get_cardano_address_as_bech32(wallet: &Wallet) -> String {
	let address = get_cardano_address(wallet);
	address.to_bech32(None).unwrap()
}

pub fn get_cardano_address_as_bytes(wallet: &Wallet) -> [u8; 29] {
	let address = get_cardano_address(wallet);
	address.to_bytes().try_into().unwrap()
}

pub async fn make_collateral(address: &str) -> OgmiosUtxo {
	let assets = vec![Asset::new_from_str("lovelace", "5000000")];
	let tx_id = send(address, assets).await;
	println!("Collateral transaction ID: {}", tx_id);
	match find_utxo_by_tx_id(address, &tx_id).await {
		Some(utxo) => utxo,
		None => panic!("Collateral UTXO not found after funding"),
	}
}

pub async fn fund_wallet(address: &str, assets: Vec<Asset>) -> OgmiosUtxo {
	let tx_id = send(address, assets).await;
	println!("Funding transaction ID: {}", tx_id);
	match find_utxo_by_tx_id(address, &tx_id).await {
		Some(utxo) => utxo,
		None => panic!("Wallet UTXO not found after funding"),
	}
}

/// Retrieve the pre-created one-shot UTxO from the local environment
///
/// The local-environment creates these UTxOs during Cardano setup in entrypoint.sh
/// The UTxO references are saved to files that we read here
///
/// # Arguments
/// * `governance_type` - Either "council" or "techauth"
pub async fn get_one_shot_utxo(governance_type: &str) -> OgmiosUtxo {
	// Find the workspace root by searching upward for local-environment directory
	let current_dir = std::env::current_dir().expect("Failed to get current directory");
	let mut search_dir = current_dir.as_path();

	let file_path = loop {
		let candidate = search_dir
			.join("local-environment/src/networks/local-env/runtime-values")
			.join(format!("{}.oneshot.utxo", governance_type));
		if candidate.exists() {
			break candidate;
		}

		// Try going up one level
		match search_dir.parent() {
			Some(parent) => search_dir = parent,
			None => panic!(
				"Failed to find local-environment/src/networks/local-env/runtime-values/{}.oneshot.utxo. \
				Searched from {} upward. Make sure local-environment is running and has created the one-shot UTxOs.",
				governance_type,
				current_dir.display()
			),
		}
	};

	let utxo_ref = std::fs::read_to_string(&file_path)
		.unwrap_or_else(|_| panic!("Failed to read one-shot UTxO file at {}. Make sure local-environment is running and has created the one-shot UTxOs.", file_path.display()))
		.trim()
		.to_string();

	println!("Reading {} one-shot UTxO from file: {}", governance_type, utxo_ref);

	let parts: Vec<&str> = utxo_ref.split('#').collect();
	if parts.len() != 2 {
		panic!("Invalid UTxO reference format in file: {}", utxo_ref);
	}

	let tx_hash = parts[0];

	// Query the UTxO from Cardano to get full details
	let client = get_ogmios_client().await;
	let cfg = load_config();
	let payment_addr = cfg.payment_addr.clone();

	let utxos = client.query_utxos(&[payment_addr]).await.expect("Failed to query UTxOs");

	// Find the matching UTxO
	for utxo in utxos {
		if hex::encode(utxo.transaction.id) == tx_hash {
			println!("✓ Found {} one-shot UTxO: {}#{}", governance_type, tx_hash, utxo.index);
			return utxo;
		}
	}

	panic!("One-shot UTxO {} not found on chain. It may have already been spent.", utxo_ref);
}

/// Deploy a governance contract and mint the NFT with multisig datum
///
/// # Arguments
/// * `tx_in` - Input UTxO to fund the transaction (must be owned by funded_address)
/// * `collateral_utxo` - Collateral UTxO for script execution (must be owned by funded_address)
/// * `one_shot_utxo` - The one-shot UTxO to consume (ensures single minting, owned by funded_address)
/// * `script_cbor` - The compiled contract CBOR
/// * `script_address` - The script address to send the NFT to
/// * `sr25519_pubkeys` - Map of Cardano pubkey hash to Sr25519 public key (hex strings)
/// * `total_signers` - Total number of required signers
#[allow(clippy::too_many_arguments)]
pub async fn deploy_governance_contract(
	tx_in: &OgmiosUtxo,
	collateral_utxo: &OgmiosUtxo,
	one_shot_utxo: &OgmiosUtxo,
	script_cbor: &str,
	script_address: &str,
	policy_id: &str,
	sr25519_pubkeys: Vec<(String, String)>, // (cardano_pubkey_hash, sr25519_pubkey)
	total_signers: u64,
) -> [u8; 32] {
	// Load the funded_address credentials (owner of all inputs)
	let cfg = load_config();
	let funded_addr = cfg.payment_addr.clone();
	let skey_json =
		fs::read_to_string(&cfg.payment_skey_file).expect("Failed to read payment.skey");
	let skey_value: serde_json::Value =
		serde_json::from_str(&skey_json).expect("Invalid skey JSON");
	let funded_skey_cbor = skey_value["cborHex"].as_str().expect("No cborHex in skey JSON");

	// Extract the verification key hash from the funded address for required signatories
	// The address format is: payment credential hash (28 bytes)
	// For enterprise addresses: addr_test + network_tag + payment_keyhash
	let funded_addr_parsed = Address::from_bech32(&funded_addr).expect("Invalid funded address");
	let payment_keyhash = funded_addr_parsed
		.payment_cred()
		.expect("No payment credential in address")
		.to_keyhash()
		.expect("Payment credential is not a keyhash");
	let payment_keyhash_hex = hex::encode(payment_keyhash.to_bytes());

	// Build the Multisig datum
	let datum = serde_json::json!({
		"list": [
			{"int": total_signers},
			{"map": sr25519_pubkeys.iter().map(|(cardano_hash, sr25519_key)| {
				// The signer keys must be in "created signer" format: #"8200581c" + cardano_hash
				let signer_key = format!("8200581c{}", cardano_hash);
				serde_json::json!({
					"k": {"bytes": signer_key},
					"v": {"bytes": sr25519_key}
				})
			}).collect::<Vec<_>>()}
		]
	});

	// Build the redeemer
	let redeemer = serde_json::json!({
		"map": sr25519_pubkeys.iter().map(|(cardano_hash, sr25519_key)| {
			serde_json::json!({
				"k": {"bytes": cardano_hash},
				"v": {"bytes": sr25519_key}
			})
		}).collect::<Vec<_>>()
	});

	// Validation: Verify script hash matches policy ID
	let calculated_hash = whisky::get_script_hash(script_cbor, LanguageVersion::V3);
	if let Ok(hash) = calculated_hash {
		if hash != policy_id {
			println!("WARNING: Script hash mismatch!");
			println!("  Expected (policy_id): {}", policy_id);
			println!("  Calculated from script: {}", hash);
			println!("  This transaction may fail validation!");
		}
	}

	println!("Deploying governance contract");
	println!("  Script address: {}", script_address);
	println!("  Policy ID: {}", policy_id);
	println!("  Total signers: {}", total_signers);
	println!(
		"  One-shot UTXO: {}#{}",
		hex::encode(one_shot_utxo.transaction.id),
		one_shot_utxo.index
	);
	println!("  Datum: {}", serde_json::to_string_pretty(&datum).unwrap());
	println!("  Redeemer: {}", serde_json::to_string_pretty(&redeemer).unwrap());

	let send_assets = vec![
		Asset::new_from_str("lovelace", "2000000"), // 2 ADA
		Asset::new_from_str(policy_id, "1"),        // The governance NFT
	];

	let network = Network::Custom(get_local_env_cost_models());

	let mut tx_builder = whisky::TxBuilder::new_core();
	tx_builder
		.network(network.clone())
		.set_evaluator(Box::new(OfflineTxEvaluator::new()))
		// Add regular input for fees
		.tx_in(
			&hex::encode(tx_in.transaction.id),
			tx_in.index.into(),
			&build_asset_vector(tx_in),
			&funded_addr,
		)
		// Add one-shot input (consumed by minting policy)
		.tx_in(
			&hex::encode(one_shot_utxo.transaction.id),
			one_shot_utxo.index.into(),
			&build_asset_vector(one_shot_utxo),
			&funded_addr,
		)
		.tx_in_collateral(
			&hex::encode(collateral_utxo.transaction.id),
			collateral_utxo.index.into(),
			&build_asset_vector(collateral_utxo),
			&funded_addr,
		)
		// Output to script address with NFT and datum
		.tx_out(script_address, &send_assets)
		.tx_out_inline_datum_value(&WData::JSON(datum.to_string()))
		// Mint the NFT
		.mint_plutus_script_v3()
		.mint(1, policy_id, "")
		.minting_script(script_cbor)
		.mint_redeemer_value(&WRedeemer {
			data: WData::JSON(redeemer.to_string()),
			// Using generous ex_units to rule out budget issues
			// Max values from protocol params: mem: 14000000, steps: 10000000000
			ex_units: Budget { mem: 14000000, steps: 10000000000 },
		})
		.change_address(&funded_addr)
		.required_signer_hash(&payment_keyhash_hex)
		.signing_key(funded_skey_cbor)
		.complete_sync(None)
		.map_err(|e| {
			panic!("Transaction building failed: {:?}", e);
		})
		.unwrap()
		.complete_signing()
		.map_err(|e| {
			panic!("Transaction signing failed: {:?}", e);
		})
		.unwrap();

	println!("✓ Transaction Built Successfully");

	let signed_tx_hex = tx_builder.tx_hex();

	let tx_bytes = hex::decode(&signed_tx_hex).expect("Failed to decode hex string");
	let client = get_ogmios_client().await;

	let response = client
		.submit_transaction(&tx_bytes)
		.await
		.map_err(|e| {
			panic!("Transaction submission failed: {:?}", e);
		})
		.unwrap();

	println!(
		"✓ Governance contract deployed successfully, transaction ID: {:?}",
		hex::encode(response.transaction.id)
	);
	response.transaction.id
}

pub async fn mint_tokens(
	wallet: &Wallet,
	policy_id: &str,
	asset_name: &str,
	amount: &str,
	minting_script: &str,
) -> [u8; 32] {
	let network = Network::Custom(get_local_env_cost_models());
	let payment_addr = get_cardano_address_as_bech32(wallet);
	let collateral_utxo = make_collateral(&payment_addr).await;

	let client = get_ogmios_client().await;
	let utxos = client.query_utxos(std::slice::from_ref(&payment_addr)).await.unwrap();
	assert!(!utxos.is_empty(), "No UTXOs found for payment address {}", payment_addr);
	let utxo = utxos
		.iter()
		.max_by_key(|u| u.value.lovelace)
		.expect("No UTXO with lovelace found");
	let input_tx_hash = hex::encode(utxo.transaction.id);
	let input_index = utxo.index;
	let input_assets = build_asset_vector(utxo);

	let assets =
		vec![Asset::new_from_str("lovelace", "1500000"), Asset::new_from_str(policy_id, amount)];

	let mut tx_builder = whisky::TxBuilder::new_core();
	tx_builder
		.network(network.clone())
		.set_evaluator(Box::new(OfflineTxEvaluator::new()))
		.tx_in(&input_tx_hash, input_index.into(), &input_assets, &payment_addr)
		.tx_in_collateral(
			&hex::encode(collateral_utxo.transaction.id),
			collateral_utxo.index.into(),
			&build_asset_vector(&collateral_utxo),
			&payment_addr,
		)
		.tx_out(&payment_addr, &assets)
		.mint_plutus_script_v2()
		.mint(amount.parse().unwrap(), policy_id, asset_name)
		.minting_script(minting_script)
		.mint_redeemer_value(&WRedeemer {
			data: WData::JSON(constr0(serde_json::json!([])).to_string()),
			ex_units: Budget { mem: 376570, steps: 94156294 },
		})
		.change_address(&payment_addr)
		.complete_sync(None)
		.unwrap();

	let signed_tx = wallet.sign_tx(&tx_builder.tx_hex());
	let tx_bytes = hex::decode(signed_tx.unwrap()).expect("Failed to decode hex string");
	let response = client.submit_transaction(&tx_bytes).await.unwrap();
	println!("Transaction submitted, response: {:?}", response);
	response.transaction.id
}
