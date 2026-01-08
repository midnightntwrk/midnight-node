use crate::config::{Constants, OgmiosClientSettings};
use bip39::{Language, Mnemonic, MnemonicType};
use ogmios_client::OgmiosClientError;
use ogmios_client::jsonrpsee::client_for_url;
use ogmios_client::query_ledger_state::{OgmiosTip, QueryLedgerState};
use ogmios_client::transactions::{SubmitTransactionResponse, Transactions};
use ogmios_client::types::OgmiosUtxo;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::sleep;
use whisky::csl::{
    Address, Bip32PrivateKey, Credential, EnterpriseAddress, NetworkInfo, PrivateKey, RewardAddress,
};
use whisky::data::{constr0, constr1};
use whisky::{
    Asset, Budget, LanguageVersion, Network, OfflineTxEvaluator, TxBuilder, WData, WError,
    WRedeemer, Wallet, WalletType,
};

#[derive(Debug)]
pub enum GetUtxoError {
    Io(std::io::Error),
    InvalidFormat,
    MissingFile,
    NotFoundOnChain,
}

impl From<std::io::Error> for GetUtxoError {
    fn from(e: std::io::Error) -> Self {
        GetUtxoError::Io(e)
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum OgmiosRequest {
    QueryTip,
    QueryUtxo { address: String },
    SubmitTx { tx_bytes: Vec<u8> },
}

#[derive(Debug)]
pub enum OgmiosResponse {
    QueryTip(OgmiosTip),
    QueryUtxo(Vec<OgmiosUtxo>),
    SubmitTx(SubmitTransactionResponse),
}

pub struct CardanoClient {
    pub ogmios_settings: OgmiosClientSettings,
    pub constants: Constants,
    pub wallet: Wallet,
    pub network: Network,
    pub network_info: NetworkInfo,
}

impl CardanoClient {
    pub async fn new(ogmios_settings: OgmiosClientSettings, constants: Constants) -> Self {
        let wallet = Self::create_wallet();
        Self::print_addresses(&wallet, &Self::network_info(&ogmios_settings.network));
        Self::from_wallet(ogmios_settings, constants, wallet)
    }

    pub async fn new_from_funded(
        ogmios_settings: OgmiosClientSettings,
        constants: Constants,
    ) -> Self {
        let wallet = Self::wallet_for_funded(constants.payments.funded_address_skey_cbor.as_str());
        Self::from_wallet(ogmios_settings, constants, wallet)
    }

    fn from_wallet(
        ogmios_settings: OgmiosClientSettings,
        constants: Constants,
        wallet: Wallet,
    ) -> Self {
        let network_info = Self::network_info(&ogmios_settings.network);

        Self {
            ogmios_settings: ogmios_settings.clone(),
            constants,
            wallet,
            network: ogmios_settings.network,
            network_info,
        }
    }

    fn network_info(network: &Network) -> NetworkInfo {
        match network {
            Network::Mainnet => NetworkInfo::mainnet(),
            Network::Preprod => NetworkInfo::testnet_preprod(),
            Network::Preview => NetworkInfo::testnet_preview(),
            Network::Custom(_) => panic!("Custom networks are not supported"),
        }
    }

    fn create_wallet() -> Wallet {
        let mnemonic = Mnemonic::new(MnemonicType::Words24, Language::English);
        let phrase = mnemonic.phrase().to_string();
        println!("Generated mnemonic phrase: {}", phrase);
        Wallet::new_mnemonic(&phrase).expect("Failed to create a wallet")
    }

    fn print_addresses(wallet: &Wallet, network_info: &NetworkInfo) {
        let delegated_payment_address = wallet
            .get_change_address(whisky::AddressType::Payment)
            .expect("Failed to get change address");
        println!("Payment address: {}", delegated_payment_address);

        let payment_public_key_hash = wallet.account.as_ref().unwrap().public_key.hash().to_hex();
        println!("Payment public key hash: {}", payment_public_key_hash);

        let stake_cred = wallet.addresses.base_address.as_ref().unwrap().stake_cred();

        let reward_address = RewardAddress::new(network_info.network_id(), &stake_cred)
            .to_address()
            .to_bech32(None)
            .unwrap();

        println!("Reward (stake) address: {}", reward_address);
        println!(
            "Stake public key hash: {}",
            stake_cred.to_keyhash().unwrap().to_hex()
        );
    }

    fn wallet_for_funded(cli_skey: &str) -> Wallet {
        let cli_hex = cli_skey
            .strip_prefix("5820")
            .unwrap_or(cli_skey)
            .to_string();
        Wallet::new_cli(cli_hex.as_str()).expect("Failed to create a funded wallet")
    }

    fn derive_stake_signing_key_from_mnemonic(wallet: &Wallet) -> Result<PrivateKey, WError> {
        let phrase = match &wallet.wallet_type {
            WalletType::MnemonicWallet(mw) => &mw.mnemonic_phrase,
            _ => {
                return Err(WError::new(
                    "derive_stake_signing_key_from_mnemonic",
                    "wallet does not contain mnemonic",
                ));
            }
        };
        let mnemonic = Mnemonic::from_phrase(phrase, Language::English).unwrap();
        let entropy = mnemonic.entropy();

        let mut root = Bip32PrivateKey::from_bip39_entropy(entropy, &[]);

        // m / 1852' / 1815' / 0'
        root = root
            .derive(1852 | 0x8000_0000)
            .derive(1815 | 0x8000_0000)
            .derive(0x8000_0000);

        // stake: /2/0
        let stake_xprv = root.derive(2).derive(0);

        Ok(PrivateKey::from_extended_bytes(&stake_xprv.to_raw_key().as_bytes()).unwrap())
    }

    pub async fn fund_wallet(
        &self,
        tx_in: &OgmiosUtxo,
        tx_out_addr: &str,
        assets: Vec<Asset>,
    ) -> Option<OgmiosUtxo> {
        let tx_id_hex = match self.send(tx_in, tx_out_addr, assets).await {
            Ok(response) => hex::encode(response.transaction.id),
            Err(e) => panic!("Failed to send assets: {:?}", e),
        };
        println!("Funded wallet with transaction id: {}", tx_id_hex);
        self.find_utxo_by_tx_id(tx_out_addr, tx_id_hex).await
    }

    pub fn address_as_bech32(&self) -> String {
        match self.wallet.get_change_address(whisky::AddressType::Payment) {
            Ok(addr) => addr,
            Err(_) => {
                let pub_key_hash = self.wallet.account.as_ref().unwrap().public_key.hash();
                let cred = Credential::from_keyhash(&pub_key_hash);
                let address_bech32 = EnterpriseAddress::new(self.network_info.network_id(), &cred)
                    .to_address()
                    .to_bech32(None)
                    .unwrap();
                println!("Derived enterprise address: {}", address_bech32);
                address_bech32
            }
        }
    }

    async fn ogmios_request(
        config: &OgmiosClientSettings,
        req: OgmiosRequest,
    ) -> Result<OgmiosResponse, OgmiosClientError> {
        let client = client_for_url(
            &config.base_url,
            Duration::from_secs(config.timeout_seconds),
        )
        .await
        .expect("Failed to connecto to ogmios");
        match req {
            OgmiosRequest::QueryTip => {
                let ledger_state = client.get_tip().await.expect("Failed to get chain tip");
                Ok(OgmiosResponse::QueryTip(ledger_state))
            }
            OgmiosRequest::QueryUtxo { address } => {
                let utxos = client
                    .query_utxos(&[address])
                    .await
                    .expect("Failed to get utxos");
                Ok(OgmiosResponse::QueryUtxo(utxos))
            }
            OgmiosRequest::SubmitTx { tx_bytes } => {
                let response = client.submit_transaction(&tx_bytes).await;
                match response {
                    Err(e) => Err(e),
                    Ok(res) => Ok(OgmiosResponse::SubmitTx(res)),
                }
            }
        }
    }

    pub async fn utxos(&self) -> Vec<OgmiosUtxo> {
        let request = OgmiosRequest::QueryUtxo {
            address: self.address_as_bech32(),
        };
        let response = Self::ogmios_request(&self.ogmios_settings, request).await;
        match response {
            Ok(OgmiosResponse::QueryUtxo(utxos)) => utxos,
            _ => vec![],
        }
    }

    pub async fn utxo_with_max_lovelace(&self) -> Option<OgmiosUtxo> {
        let utxos = self.utxos().await;
        utxos.iter().max_by_key(|u| u.value.lovelace).cloned()
    }

    pub async fn register(
        &self,
        midnight_address_hex: &str,
        tx_in: &OgmiosUtxo,
        collateral_utxo: &OgmiosUtxo,
    ) -> Result<SubmitTransactionResponse, OgmiosClientError> {
        let policies = self.constants.policies.clone();
        let validator_address = policies.mapping_validator_address();
        let datum = serde_json::to_string(&serde_json::json!(
            {
                "constructor": 0,
                "fields": [
                    {
                        "constructor": 0,
                        "fields": [
                            {
                                "bytes": &self.wallet.addresses.base_address.as_ref().unwrap().stake_cred().to_keyhash().unwrap().to_hex()
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
        let payment_addr = self.address_as_bech32();
        let mapping_validator_policy_id = policies.mapping_validator_policy_id();
        let send_assets = vec![
            Asset::new_from_str("lovelace", "2000000"),
            Asset::new_from_str(&mapping_validator_policy_id, "1"),
        ];
        let minting_script = policies.mapping_validator_cbor_double_encoding();
        let network = Network::Custom(self.constants.cost_model.clone());

        let mut tx_builder = TxBuilder::new_core();
        tx_builder
            .network(network.clone())
            .set_evaluator(Box::new(OfflineTxEvaluator::new()))
            .tx_in(
                &hex::encode(tx_in.transaction.id),
                tx_in.index.into(),
                &Self::build_asset_vector(tx_in),
                &payment_addr,
            )
            .tx_in_collateral(
                &hex::encode(collateral_utxo.transaction.id),
                collateral_utxo.index.into(),
                &Self::build_asset_vector(collateral_utxo),
                &payment_addr,
            )
            .tx_out(&validator_address, &send_assets)
            .tx_out_inline_datum_value(&WData::JSON(datum))
            .mint_plutus_script_v3()
            .mint(1, &mapping_validator_policy_id, "")
            .minting_script(&minting_script)
            .mint_redeemer_value(&WRedeemer {
                data: WData::JSON(constr0(serde_json::json!([])).to_string()),
                ex_units: Budget {
                    mem: 14000000,
                    steps: 10000000000,
                },
            })
            .change_address(&payment_addr)
            .required_signer_hash(
                &self
                    .wallet
                    .addresses
                    .base_address
                    .as_ref()
                    .unwrap()
                    .stake_cred()
                    .to_keyhash()
                    .unwrap()
                    .to_hex(),
            )
            .complete_sync(None)
            .unwrap();

        let signed_tx = self.wallet.sign_tx(&tx_builder.tx_hex());

        // sign with stake key
        let stake_signing_key = Self::derive_stake_signing_key_from_mnemonic(&self.wallet).unwrap();
        let stake_wallet = Wallet::new_cli(&stake_signing_key.to_hex()).unwrap();
        let signed_by_stake_tx = stake_wallet.sign_tx(&signed_tx.unwrap());

        let tx_bytes =
            hex::decode(signed_by_stake_tx.unwrap()).expect("Failed to decode hex string");
        let request = OgmiosRequest::SubmitTx { tx_bytes };
        let response = Self::ogmios_request(&self.ogmios_settings, request)
            .await
            .unwrap();
        match response {
            OgmiosResponse::SubmitTx(res) => Ok(res),
            _ => Err(OgmiosClientError::RequestError(
                "Unexpected response type".into(),
            )),
        }
    }

    pub async fn deregister(
        &self,
        tx_in: &OgmiosUtxo,
        register_tx: &OgmiosUtxo,
        collateral_utxo: &OgmiosUtxo,
    ) -> Result<SubmitTransactionResponse, OgmiosClientError> {
        let policies = self.constants.policies.clone();
        let validator_address = policies.mapping_validator_address();
        let datum =
            serde_json::to_string(&serde_json::json!({"constructor": 0,"fields": []})).unwrap();
        let payment_addr = self.address_as_bech32();
        let mapping_validator_policy_id = policies.mapping_validator_policy_id();
        let send_assets = vec![Asset::new_from_str("lovelace", "2000000")];
        let minting_script = policies.mapping_validator_cbor_double_encoding();
        let network = Network::Custom(self.constants.cost_model.clone());
        let mapping_validator_cbor = policies.mapping_validator_cbor_double_encoding();
        let register_asset_tx_vector = Self::build_asset_vector(register_tx);
        println!("Register tx assets: {:?}", register_asset_tx_vector);
        let script_hash = whisky::get_script_hash(&mapping_validator_cbor, LanguageVersion::V2);
        println!("Mapping validator script hash: {:?}", script_hash);

        let mut tx_builder = TxBuilder::new_core();
        tx_builder
            .network(network.clone())
            .set_evaluator(Box::new(OfflineTxEvaluator::new()))
            .tx_in(
                &hex::encode(tx_in.transaction.id),
                tx_in.index.into(),
                &Self::build_asset_vector(tx_in),
                &payment_addr,
            )
            .spending_plutus_script_v3()
            .tx_in(
                &hex::encode(register_tx.transaction.id),
                register_tx.index.into(),
                &Self::build_asset_vector(register_tx),
                &validator_address,
            )
            .tx_in_inline_datum_present()
            .tx_in_script(&mapping_validator_cbor)
            .tx_in_redeemer_value(&WRedeemer {
                data: WData::JSON(datum),
                ex_units: Budget {
                    mem: 3765700,
                    steps: 941562940,
                },
            })
            .tx_in_collateral(
                &hex::encode(collateral_utxo.transaction.id),
                collateral_utxo.index.into(),
                &Self::build_asset_vector(collateral_utxo),
                &payment_addr,
            )
            .tx_out(&payment_addr, &send_assets)
            .mint_plutus_script_v3()
            .mint(-1, &mapping_validator_policy_id, "")
            .minting_script(&minting_script)
            .mint_redeemer_value(&WRedeemer {
                data: WData::JSON(constr1(serde_json::json!([])).to_string()),
                ex_units: Budget {
                    mem: 3765700,
                    steps: 941562940,
                },
            })
            .change_address(&payment_addr)
            .required_signer_hash(
                &self
                    .wallet
                    .addresses
                    .base_address
                    .as_ref()
                    .unwrap()
                    .stake_cred()
                    .to_keyhash()
                    .unwrap()
                    .to_hex(),
            )
            .complete_sync(None)
            .unwrap();

        let signed_tx = self
            .wallet
            .sign_tx(&tx_builder.tx_hex())
            .expect("Failed to sign tx");

        // sign with stake key
        let stake_signing_key = Self::derive_stake_signing_key_from_mnemonic(&self.wallet).unwrap();
        let stake_wallet = Wallet::new_cli(&stake_signing_key.to_hex()).unwrap();
        let signed_by_stake_tx = stake_wallet.sign_tx(&signed_tx);

        let tx_bytes =
            hex::decode(signed_by_stake_tx.unwrap()).expect("Failed to decode hex string");
        let request = OgmiosRequest::SubmitTx { tx_bytes };
        let response = Self::ogmios_request(&self.ogmios_settings, request).await;
        match response {
            Ok(OgmiosResponse::SubmitTx(res)) => Ok(res),
            _ => Err(OgmiosClientError::RequestError(
                "Unexpected response type".into(),
            )),
        }
    }

    pub async fn mint_tokens(
        &self,
        amount: i32,
        collateral_utxo: &OgmiosUtxo,
    ) -> Result<SubmitTransactionResponse, OgmiosClientError> {
        let policies = self.constants.policies.clone();

        let policy_id = policies.cnight_token_policy_id();
        let minting_script = policies.cnight_token_cbor_double_encoding();
        let network = Network::Custom(self.constants.cost_model.clone());

        let payment_addr = self.address_as_bech32();

        let request = OgmiosRequest::QueryUtxo {
            address: payment_addr.clone(),
        };
        let response = Self::ogmios_request(&self.ogmios_settings, request)
            .await
            .unwrap();
        let utxos = match response {
            OgmiosResponse::QueryUtxo(utxos) => utxos,
            _ => vec![],
        };

        assert!(
            !utxos.is_empty(),
            "No UTXOs found for payment address {}",
            payment_addr
        );
        let utxo = utxos
            .iter()
            .max_by_key(|u| u.value.lovelace)
            .expect("No UTXO with lovelace found");
        let input_tx_hash = hex::encode(utxo.transaction.id);
        let input_index = utxo.index;
        let input_assets = Self::build_asset_vector(utxo);

        let assets = vec![
            Asset::new_from_str("lovelace", "1500000"),
            Asset::new_from_str(&policy_id, amount.to_string().as_str()),
        ];

        let mut tx_builder = whisky::TxBuilder::new_core();
        tx_builder
            .network(network.clone())
            .set_evaluator(Box::new(OfflineTxEvaluator::new()))
            .tx_in(
                &input_tx_hash,
                input_index.into(),
                &input_assets,
                &payment_addr,
            )
            .tx_in_collateral(
                &hex::encode(collateral_utxo.transaction.id),
                collateral_utxo.index.into(),
                &Self::build_asset_vector(collateral_utxo),
                &payment_addr,
            )
            .tx_out(&payment_addr, &assets)
            .mint_plutus_script_v3()
            .mint(amount.into(), &policy_id, "")
            .minting_script(&minting_script)
            .mint_redeemer_value(&WRedeemer {
                data: WData::JSON(constr0(serde_json::json!([])).to_string()),
                ex_units: Budget {
                    mem: 14000000,
                    steps: 10000000000,
                },
            })
            .change_address(&payment_addr)
            .complete_sync(None)
            .unwrap();

        let signed_tx = self.wallet.sign_tx(&tx_builder.tx_hex());
        let tx_bytes = hex::decode(signed_tx.unwrap()).expect("Failed to decode hex string");
        let request = OgmiosRequest::SubmitTx { tx_bytes };
        let response = Self::ogmios_request(&self.ogmios_settings, request)
            .await
            .unwrap();
        match response {
            OgmiosResponse::SubmitTx(res) => Ok(res),
            _ => Err(OgmiosClientError::RequestError(
                "Unexpected response type".into(),
            )),
        }
    }

    pub async fn rotate_cnight(
        &self,
        utxo: &OgmiosUtxo,
    ) -> Result<SubmitTransactionResponse, OgmiosClientError> {
        let payment_addr = self.address_as_bech32();
        let input_tx_hash = hex::encode(utxo.transaction.id);
        let input_index = utxo.index;
        let input_assets = &Self::build_asset_vector(utxo);
        let network: Network = Network::Custom(self.constants.cost_model.clone());
        let mut tx_builder = whisky::TxBuilder::new_core();
        tx_builder
            .network(network.clone())
            .set_evaluator(Box::new(OfflineTxEvaluator::new()))
            .tx_in(
                &input_tx_hash,
                input_index.into(),
                input_assets,
                &payment_addr,
            )
            .change_address(&payment_addr)
            .complete_sync(None)
            .unwrap();

        let signed_tx = self.wallet.sign_tx(&tx_builder.tx_hex());
        let tx_bytes = hex::decode(signed_tx.unwrap()).expect("Failed to decode hex string");
        let request = OgmiosRequest::SubmitTx { tx_bytes };
        let response = Self::ogmios_request(&self.ogmios_settings, request)
            .await
            .unwrap();
        match response {
            OgmiosResponse::SubmitTx(res) => Ok(res),
            _ => Err(OgmiosClientError::RequestError(
                "Unexpected response type".into(),
            )),
        }
    }

    pub async fn send(
        &self,
        tx_in: &OgmiosUtxo,
        tx_out_addr: &str,
        assets: Vec<Asset>,
    ) -> Result<SubmitTransactionResponse, OgmiosClientError> {
        let payment_addr = self.address_as_bech32();
        println!(
            "Sending assets from {} to address: {}",
            payment_addr, tx_out_addr
        );

        let input_tx_hash = hex::encode(tx_in.transaction.id);

        let address_as_bech32 = tx_out_addr.to_string();
        let mut tx_builder = TxBuilder::new_core();
        tx_builder
            .tx_in(
                &input_tx_hash,
                tx_in.index.into(),
                &Self::build_asset_vector(tx_in),
                address_as_bech32.as_str(),
            )
            .tx_out(address_as_bech32.as_str(), &assets)
            .change_address(&payment_addr)
            .complete_sync(None)
            .unwrap();

        let signed_tx = self
            .wallet
            .sign_tx(&tx_builder.tx_hex())
            .expect("Failed to sign tx");
        let tx_bytes = hex::decode(signed_tx).expect("Failed to decode hex string");
        let request = OgmiosRequest::SubmitTx { tx_bytes };
        let response = Self::ogmios_request(&self.ogmios_settings, request)
            .await
            .unwrap();
        match response {
            OgmiosResponse::SubmitTx(res) => Ok(res),
            _ => Err(OgmiosClientError::RequestError(
                "Unexpected response type".into(),
            )),
        }
    }

    pub async fn find_utxo_by_tx_id(&self, address: &str, tx_id_hex: String) -> Option<OgmiosUtxo> {
        const MAX_ATTEMPTS: u32 = 10;
        const PAUSE: Duration = Duration::from_secs(2);
        let tx_id_bytes = hex::decode(tx_id_hex.clone()).expect("invalid hex tx_id");
        let request = OgmiosRequest::QueryUtxo {
            address: address.to_string(),
        };

        for _ in 0..MAX_ATTEMPTS {
            let response = Self::ogmios_request(&self.ogmios_settings, request.clone())
                .await
                .unwrap();
            let utxos = match response {
                OgmiosResponse::QueryUtxo(utxos) => utxos,
                _ => vec![],
            };

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

        assets.insert(
            0,
            Asset::new_from_str("lovelace", &utxo.value.lovelace.to_string()),
        );
        assets
    }

    pub async fn is_utxo_unspent_for_3_blocks(&self, address: &str, tx_id: &str) -> bool {
        // Get the current block number (slot) as the starting point
        const SLOTS_NUMBER: u64 = 3;
        const LIMIT: i32 = 5;
        let response = Self::ogmios_request(&self.ogmios_settings, OgmiosRequest::QueryTip)
            .await
            .unwrap();
        let tip = match response {
            OgmiosResponse::QueryTip(tip) => tip,
            _ => panic!("Unexpected response type"),
        };
        let start_slot = tip.slot;
        println!(
            "Current slot is {}. Waiting for {} more slots (limit {} checks)...",
            start_slot, SLOTS_NUMBER, LIMIT
        );

        let target = start_slot
            .checked_add(SLOTS_NUMBER)
            .expect("start_slot + SLOTS_NUMBER overflowed");

        let mut last_slot = start_slot;
        for iteration in 0..=LIMIT {
            let response = Self::ogmios_request(&self.ogmios_settings, OgmiosRequest::QueryTip)
                .await
                .unwrap();
            let tip = match response {
                OgmiosResponse::QueryTip(tip) => tip,
                _ => panic!("Unexpected response type"),
            };

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
        let request = OgmiosRequest::QueryUtxo {
            address: address.to_string(),
        };
        let response = Self::ogmios_request(&self.ogmios_settings, request)
            .await
            .unwrap();
        let utxos = match response {
            OgmiosResponse::QueryUtxo(utxos) => utxos,
            _ => vec![],
        };
        let still_unspent = utxos.iter().any(|u| hex::encode(u.transaction.id) == tx_id);
        if still_unspent {
            println!("UTXO {} is still unspent after 3 slots.", tx_id);
        } else {
            println!("UTXO {} was spent within 3 slots.", tx_id);
        }
        still_unspent
    }

    /// Retrieve the pre-created one-shot UTxO from the local environment
    ///
    /// The local-environment creates these UTxOs during Cardano setup in entrypoint.sh
    /// The UTxO references are saved to files that we read here
    ///
    /// # Arguments
    /// * `governance_type` - Either "council" or "techauth"
    pub async fn one_shot_utxo(&self, governance_type: &str) -> Result<OgmiosUtxo, GetUtxoError> {
        let current_dir = std::env::current_dir().map_err(GetUtxoError::Io)?;

        let file_path = self
            .find_runtime_values_file(&current_dir, governance_type)
            .ok_or(GetUtxoError::MissingFile)?;

        let utxo_ref = fs::read_to_string(&file_path)?.trim().to_string();

        let (tx_hash, _index) = utxo_ref
            .split_once('#')
            .ok_or(GetUtxoError::InvalidFormat)?;

        let request = OgmiosRequest::QueryUtxo {
            address: self.constants.payments.funded_address.to_string(),
        };
        let response = Self::ogmios_request(&self.ogmios_settings, request)
            .await
            .unwrap();
        let utxos = match response {
            OgmiosResponse::QueryUtxo(utxos) => utxos,
            _ => vec![],
        };

        utxos
            .into_iter()
            .find(|u| hex::encode(u.transaction.id) == tx_hash)
            .ok_or(GetUtxoError::NotFoundOnChain)
    }

    fn find_runtime_values_file(&self, start: &Path, governance_type: &str) -> Option<PathBuf> {
        let filename = format!("{governance_type}.oneshot.utxo");
        let rel_dir = Path::new(&self.constants.runtime_values_location);

        for dir in start.ancestors() {
            let candidate = dir.join(rel_dir).join(&filename);
            if candidate.exists() {
                return Some(candidate);
            }
        }

        None
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
        &self,
        tx_in: &OgmiosUtxo,
        collateral_utxo: &OgmiosUtxo,
        one_shot_utxo: &OgmiosUtxo,
        script_cbor: &str,
        script_address: &str,
        policy_id: &str,
        sr25519_pubkeys: Vec<(String, String)>, // (cardano_pubkey_hash, sr25519_pubkey)
        total_signers: u64,
    ) -> Result<SubmitTransactionResponse, OgmiosClientError> {
        // Load the funded_address credentials (owner of all inputs)
        let payments = self.constants.payments.clone();
        let funded_addr = payments.funded_address;
        let funded_skey_cbor = payments.funded_address_skey_cbor;

        // Extract the verification key hash from the funded address for required signatories
        // The address format is: payment credential hash (28 bytes)
        // For enterprise addresses: addr_test + network_tag + payment_keyhash
        let funded_addr_parsed =
            Address::from_bech32(&funded_addr).expect("Invalid funded address");
        let payment_keyhash = funded_addr_parsed
            .payment_cred()
            .expect("No payment credential in address")
            .to_keyhash()
            .expect("Payment credential is not a keyhash");
        let payment_keyhash_hex = hex::encode(payment_keyhash.to_bytes());

        // Build the VersionedMultisig datum
        // Format: Constr(0, [data: [total_signers, members_map], round: Int])
        let multisig_data = serde_json::json!({
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
        let datum = serde_json::json!({
            "constructor": 0,
            "fields": [
                multisig_data,
                {"int": 0}  // round starts at 0
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
        println!(
            "  Redeemer: {}",
            serde_json::to_string_pretty(&redeemer).unwrap()
        );

        let send_assets = vec![
            Asset::new_from_str("lovelace", "2000000"), // 2 ADA
            Asset::new_from_str(policy_id, "1"),        // The governance NFT
        ];

        let network = Network::Custom(self.constants.cost_model.clone());

        let mut tx_builder = TxBuilder::new_core();
        tx_builder
            .network(network.clone())
            .set_evaluator(Box::new(OfflineTxEvaluator::new()))
            // Add regular input for fees
            .tx_in(
                &hex::encode(tx_in.transaction.id),
                tx_in.index.into(),
                &Self::build_asset_vector(tx_in),
                &funded_addr,
            )
            // Add one-shot input (consumed by minting policy)
            .tx_in(
                &hex::encode(one_shot_utxo.transaction.id),
                one_shot_utxo.index.into(),
                &Self::build_asset_vector(one_shot_utxo),
                &funded_addr,
            )
            .tx_in_collateral(
                &hex::encode(collateral_utxo.transaction.id),
                collateral_utxo.index.into(),
                &Self::build_asset_vector(collateral_utxo),
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
                ex_units: Budget {
                    mem: 14000000,
                    steps: 10000000000,
                },
            })
            .change_address(&funded_addr)
            .required_signer_hash(&payment_keyhash_hex)
            .signing_key(&funded_skey_cbor)
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

        let request = OgmiosRequest::SubmitTx { tx_bytes };
        let response = Self::ogmios_request(&self.ogmios_settings, request)
            .await
            .unwrap();
        match response {
            OgmiosResponse::SubmitTx(res) => Ok(res),
            _ => Err(OgmiosClientError::RequestError(
                "Unexpected response type".into(),
            )),
        }
    }

    pub fn reward_address_bytes(&self) -> [u8; 29] {
        let cred = self
            .wallet
            .addresses
            .base_address
            .as_ref()
            .unwrap()
            .stake_cred();
        RewardAddress::new(self.network_info.network_id(), &cred)
            .to_address()
            .to_bytes()
            .try_into()
            .unwrap()
    }

    pub async fn spend_cnight(
        &self,
        utxo: &OgmiosUtxo,
        recipient_address: &str,
    ) -> Result<SubmitTransactionResponse, OgmiosClientError> {
        let payment_addr = self.address_as_bech32();
        let input_tx_hash = hex::encode(utxo.transaction.id);
        let input_index = utxo.index;
        let input_assets = &Self::build_asset_vector(utxo);

        let network: Network = Network::Custom(self.constants.cost_model.clone());
        let mut tx_builder = whisky::TxBuilder::new_core();
        tx_builder
            .network(network.clone())
            .set_evaluator(Box::new(OfflineTxEvaluator::new()))
            .tx_in(
                &input_tx_hash,
                input_index.into(),
                input_assets,
                &payment_addr,
            )
            .change_address(recipient_address)
            .complete_sync(None)
            .unwrap();

        let signed_tx = self.wallet.sign_tx(&tx_builder.tx_hex());
        let tx_bytes = hex::decode(signed_tx.unwrap()).expect("Failed to decode hex string");
        let request = OgmiosRequest::SubmitTx { tx_bytes };
        let response = Self::ogmios_request(&self.ogmios_settings, request)
            .await
            .unwrap();
        match response {
            OgmiosResponse::SubmitTx(res) => Ok(res),
            _ => Err(OgmiosClientError::RequestError(
                "Unexpected response type".into(),
            )),
        }
    }
}
