use crate::configuration::{local_env_cost_models, OgmiosClientSettings, TestFiles};
use bip39::{Language, Mnemonic, MnemonicType};
use ogmios_client::jsonrpsee::{client_for_url, OgmiosClients};
use ogmios_client::query_ledger_state::QueryLedgerState;
use ogmios_client::transactions::{SubmitTransactionResponse, Transactions};
use ogmios_client::types::OgmiosUtxo;
use ogmios_client::OgmiosClientError;
use std::time::Duration;
use tokio::time::sleep;
use whisky::csl::{Address, Credential, EnterpriseAddress, NetworkInfo};
use whisky::data::constr0;
use whisky::{
	Asset, Budget, LanguageVersion, Network, OfflineTxEvaluator, TxBuilder, WData, WRedeemer,
	Wallet,
};

pub struct CardanoClient {
	pub ogmios_clients: OgmiosClients,
	pub test_files: TestFiles,
	pub wallet: Wallet,
	pub address: Address,
	pub network: Network,
	pub network_info: NetworkInfo,
}

impl CardanoClient {
	pub async fn new(ogmios_settings: OgmiosClientSettings, test_files: TestFiles) -> Self {
		let ogmios_clients = client_for_url(
			&ogmios_settings.base_url,
			Duration::from_secs(ogmios_settings.timeout_seconds),
		)
		.await
		.expect("Failed to initialize client");

		let wallet = CardanoClient::create_wallet();
		let network_info = CardanoClient::network_info(&ogmios_settings.network);
		let address = CardanoClient::address(&wallet, network_info.network_id());

		Self {
			ogmios_clients,
			test_files,
			wallet,
			address,
			network: ogmios_settings.network,
			network_info,
		}
	}

	fn network_info(network: &Network) -> NetworkInfo {
		match network {
			Network::Mainnet => NetworkInfo::mainnet(),
			Network::Preprod => NetworkInfo::testnet_preprod(),
			Network::Preview => NetworkInfo::testnet_preview(),
			Network::Custom(_) => panic!("Custom networks are not supported"), //TODO: ask Rado about it
		}
	}

	fn create_wallet() -> Wallet {
		let mnemonic = Mnemonic::new(MnemonicType::Words24, Language::English);
		let phrase = mnemonic.phrase().to_string();
		Wallet::new_mnemonic(&phrase).expect("Failed to create a wallet")
	}

	fn address(wallet: &Wallet, network_id: u8) -> Address {
		let pub_key_hash = wallet.account.public_key.hash();
		let payment_cred = Credential::from_keyhash(&pub_key_hash);
		let private_key_hash = wallet.account.private_key.to_hex();
		println!("Private key hash: {}", private_key_hash);
		EnterpriseAddress::new(network_id, &payment_cred).to_address()
	}

	pub async fn make_collateral(&self) -> Option<OgmiosUtxo> {
		let assets = vec![Asset::new_from_str("lovelace", "5000000")];
		self.fund_wallet(assets).await
	}

	pub async fn fund_wallet(&self, assets: Vec<Asset>) -> Option<OgmiosUtxo> {
		let tx_id_hex = match self.send(assets).await {
			Ok(response) => hex::encode(response.transaction.id),
			Err(e) => panic!("Failed to send assets: {:?}", e),
		};
		println!("Funded wallet with transaction id: {}", tx_id_hex);
		self.find_utxo_by_tx_id(&self.address_as_bech32(), tx_id_hex).await
	}

	pub fn address_as_bech32(&self) -> String {
		self.address.to_bech32(None).unwrap()
	}

	pub async fn register(
		&self,
		midnight_address_hex: &str,
		tx_in: &OgmiosUtxo,
		collateral_utxo: &OgmiosUtxo,
	) -> Result<SubmitTransactionResponse, OgmiosClientError> {
		let validator_address = self
			.test_files
			.policies
			.mapping_validator_address(self.network_info.network_id());
		let cardano_address_hex = self.address.to_hex();
		let datum = serde_json::to_string(&serde_json::json!({"constructor": 0,"fields": [{ "bytes": cardano_address_hex }, { "bytes": midnight_address_hex }]})).unwrap();
		let payment_addr = self.address_as_bech32();
		let auth_token_policy_id = self.test_files.policies.auth_token_policy_id();
		let send_assets = vec![
			Asset::new_from_str("lovelace", "150000000"),
			Asset::new_from_str(&auth_token_policy_id, "1"),
		];
		let minting_script = self.test_files.policies.auth_token_policy();
		let network = Network::Custom(local_env_cost_models());

		let mut tx_builder = TxBuilder::new_core();
		tx_builder
			.network(network)
			.set_evaluator(Box::new(OfflineTxEvaluator::new()))
			.tx_in(
				&hex::encode(tx_in.transaction.id),
				tx_in.index.into(),
				&CardanoClient::build_asset_vector(&tx_in),
				&payment_addr,
			)
			.tx_in_collateral(
				&hex::encode(collateral_utxo.transaction.id),
				collateral_utxo.index.into(),
				&CardanoClient::build_asset_vector(&collateral_utxo),
				&payment_addr,
			)
			.tx_out(&validator_address, &send_assets)
			.tx_out_inline_datum_value(&WData::JSON(datum))
			.mint_plutus_script_v2()
			.mint(1, &auth_token_policy_id, "")
			.minting_script(&minting_script)
			.mint_redeemer_value(&WRedeemer {
				data: WData::JSON(constr0(serde_json::json!([])).to_string()),
				ex_units: Budget { mem: 376570, steps: 94156294 },
			})
			.change_address(&payment_addr)
			.complete_sync(None)
			.expect("could not complete sync");

		let signed_tx = self.wallet.sign_tx(&tx_builder.tx_hex());
		let tx_bytes = hex::decode(signed_tx.expect("signing didn't work"))
			.expect("Failed to decode hex string");
		self.ogmios_clients.submit_transaction(&tx_bytes).await
	}

	pub async fn deregister(
		&self,
		tx_in: &OgmiosUtxo,
		register_tx: &OgmiosUtxo,
		collateral_utxo: &OgmiosUtxo,
	) -> Result<SubmitTransactionResponse, OgmiosClientError> {
		let validator_address = self
			.test_files
			.policies
			.mapping_validator_address(self.network_info.network_id());
		let datum =
			serde_json::to_string(&serde_json::json!({"constructor": 0,"fields": []})).unwrap();
		let payment_addr = self.address_as_bech32();
		let auth_token_policy_id = self.test_files.policies.auth_token_policy_id();
		let send_assets = vec![Asset::new_from_str("lovelace", "20000000")];

		let minting_script = self.test_files.policies.auth_token_policy();
		let network = Network::Custom(local_env_cost_models());
		let mapping_validator_cbor = self.test_files.policies.mapping_validator_cbor();
		let register_asset_tx_vector = CardanoClient::build_asset_vector(&register_tx);
		println!("Register tx assets: {:?}", register_asset_tx_vector);

		let script_hash = whisky::get_script_hash(&mapping_validator_cbor, LanguageVersion::V2);
		println!("Mapping validator script hash: {:?}", script_hash);

		let mut tx_builder = whisky::TxBuilder::new_core();
		tx_builder
			.network(network.clone())
			.set_evaluator(Box::new(OfflineTxEvaluator::new()))
			.tx_in(
				&hex::encode(tx_in.transaction.id),
				tx_in.index.into(),
				&CardanoClient::build_asset_vector(&tx_in),
				&payment_addr,
			)
			.spending_plutus_script_v2()
			.tx_in(
				&hex::encode(register_tx.transaction.id),
				register_tx.index.into(),
				&CardanoClient::build_asset_vector(&register_tx),
				&validator_address,
			)
			.tx_in_inline_datum_present()
			.tx_in_script(&mapping_validator_cbor)
			.tx_in_redeemer_value(&WRedeemer {
				data: WData::JSON(datum),
				ex_units: Budget { mem: 3765700, steps: 941562940 },
			})
			.tx_in_collateral(
				&hex::encode(collateral_utxo.transaction.id),
				collateral_utxo.index.into(),
				&CardanoClient::build_asset_vector(&collateral_utxo),
				&payment_addr,
			)
			.tx_out(&payment_addr, &send_assets)
			.mint_plutus_script_v2()
			.mint(-1, &auth_token_policy_id, "")
			.minting_script(&minting_script)
			.mint_redeemer_value(&WRedeemer {
				data: WData::JSON(constr0(serde_json::json!([])).to_string()),
				ex_units: Budget { mem: 3765700, steps: 941562940 },
			})
			.change_address(&payment_addr)
			.complete_sync(None)
			.unwrap();

		let signed_tx = self.wallet.sign_tx(&tx_builder.tx_hex());
		let tx_bytes = hex::decode(signed_tx.unwrap()).expect("Failed to decode hex string");
		self.ogmios_clients.submit_transaction(&tx_bytes).await
	}

	pub async fn mint_tokens(
		&self,
		amount: i32,
	) -> Result<SubmitTransactionResponse, OgmiosClientError> {
		let policy_id = self.test_files.policies.cnight_token_policy_id();
		let minting_script = self.test_files.policies.cnight_token_policy();
		let network = Network::Custom(local_env_cost_models());
		let payment_addr = self.address_as_bech32();
		let collateral_utxo = match self.make_collateral().await {
			Some(utxo) => utxo,
			None => panic!("UTXO not found after funding"),
		};

		let utxos = self.ogmios_clients.query_utxos(&[payment_addr.clone()]).await.unwrap();
		assert!(!utxos.is_empty(), "No UTXOs found for payment address {}", payment_addr);

		let utxo = utxos
			.iter()
			.max_by_key(|u| u.value.lovelace)
			.expect("No UTXO with lovelace found");
		let input_tx_hash = hex::encode(utxo.transaction.id);
		let input_index = utxo.index;
		let input_assets = CardanoClient::build_asset_vector(&utxo);

		let assets = vec![
			Asset::new_from_str("lovelace", "1500000"),
			Asset::new_from_str(policy_id.as_str(), &amount.to_string()),
		];

		let mut tx_builder = whisky::TxBuilder::new_core();
		tx_builder
			.network(network.clone())
			.set_evaluator(Box::new(OfflineTxEvaluator::new()))
			.tx_in(&input_tx_hash, input_index.into(), &input_assets, &payment_addr)
			.tx_in_collateral(
				&hex::encode(collateral_utxo.transaction.id),
				collateral_utxo.index.into(),
				&CardanoClient::build_asset_vector(&collateral_utxo),
				&payment_addr,
			)
			.tx_out(&payment_addr, &assets)
			.mint_plutus_script_v2()
			.mint(amount.into(), &policy_id, "")
			.minting_script(&minting_script)
			.mint_redeemer_value(&WRedeemer {
				data: WData::JSON(constr0(serde_json::json!([])).to_string()),
				ex_units: Budget { mem: 376570, steps: 94156294 },
			})
			.change_address(&payment_addr)
			.complete_sync(None)
			.unwrap();

		let signed_tx = self.wallet.sign_tx(&tx_builder.tx_hex());
		let tx_bytes = hex::decode(signed_tx.unwrap()).expect("Failed to decode hex string");
		self.ogmios_clients.submit_transaction(&tx_bytes).await
	}

	pub async fn send(
		&self,
		assets: Vec<Asset>,
	) -> Result<SubmitTransactionResponse, OgmiosClientError> {
		let payment_addr = self.test_files.payments.addr().expect("Failed to get payment address");
		let utxos = self.ogmios_clients.query_utxos(&[payment_addr.clone()]).await?;
		assert!(!utxos.is_empty());

		let utxo = utxos
			.iter()
			.max_by_key(|u| u.value.lovelace)
			.expect("No UTXO with lovelace found");
		let skey_json = self.test_files.payments.skey().expect("Failed to get payment skey");
		let skey_value: serde_json::Value =
			serde_json::from_str(&skey_json).expect("Invalid skey JSON");
		let cbor_hex = skey_value["cborHex"].as_str().expect("No cborHex in skey JSON");

		let address_as_bech32 = self.address_as_bech32();
		let tx_hex = TxBuilder::new_core()
			.tx_in(
				&hex::encode(utxo.transaction.id),
				utxo.index.into(),
				&CardanoClient::build_asset_vector(utxo),
				address_as_bech32.as_str(),
			)
			.tx_out(address_as_bech32.as_str(), &assets)
			.change_address(&payment_addr)
			.signing_key(&cbor_hex)
			.complete_sync(None)
			.unwrap()
			.complete_signing()
			.unwrap();
		let tx_bytes = hex::decode(tx_hex).expect("Failed to decode hex string");
		self.ogmios_clients.submit_transaction(&tx_bytes).await
	}

	pub async fn find_utxo_by_tx_id(&self, address: &str, tx_id_hex: String) -> Option<OgmiosUtxo> {
		const MAX_ATTEMPTS: u32 = 10;
		const PAUSE: Duration = Duration::from_secs(1);
		let tx_id_bytes = hex::decode(tx_id_hex).expect("invalid hex tx_id");

		for _ in 0..MAX_ATTEMPTS {
			let utxos = self
				.ogmios_clients
				.query_utxos(&[address.into()])
				.await
				.expect("Failed to query Ogmios UTXO");

			if let Some(found) = utxos
				.into_iter()
				.find(|utxo| utxo.transaction.id.as_ref() == tx_id_bytes.as_slice())
			{
				return Some(found);
			}
			sleep(PAUSE).await;
		}
		None
	}

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

	pub async fn is_utxo_unspent_for_3_blocks(&self, address: &str, tx_id: &str) -> bool {
		// Get the current block number (slot) as the starting point
		const SLOTS_NUMBER: u64 = 3;
		const LIMIT: i32 = 5;
		let start_slot = self.ogmios_clients.get_tip().await.unwrap().slot;
		println!(
			"Current slot is {}. Waiting for {} more slots (limit {} checks)...",
			start_slot, SLOTS_NUMBER, LIMIT
		);

		let target = start_slot
			.checked_add(SLOTS_NUMBER)
			.expect("start_slot + SLOTS_NUMBER overflowed");

		let mut last_slot = start_slot;
		for iteration in 0..=LIMIT {
			let tip = self.ogmios_clients.get_tip().await.unwrap();

			if tip.slot > last_slot {
				println!("Slot advanced: {} -> {}", last_slot, tip.slot);
				last_slot = tip.slot;

				if last_slot >= target {
					break;
				}
			}
			sleep(Duration::from_secs(1)).await;
			if iteration == LIMIT {
				panic!("Limit reached and nr: {} as target was not reached", target);
			}
		}

		// After 3 slots, check if the UTXO is still present
		let utxos = self.ogmios_clients.query_utxos(&[address.into()]).await.unwrap();
		let still_unspent = utxos.iter().any(|u| hex::encode(u.transaction.id) == tx_id);
		if still_unspent {
			println!("UTXO {} is still unspent after 3 slots.", tx_id);
		} else {
			println!("UTXO {} was spent within 3 slots.", tx_id);
		}
		still_unspent
	}
}
