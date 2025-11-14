use crate::config::{Constants, OgmiosClientSettings};
use bip39::{Language, Mnemonic, MnemonicType};
use ogmios_client::OgmiosClientError;
use ogmios_client::jsonrpsee::{OgmiosClients, client_for_url};
use ogmios_client::query_ledger_state::QueryLedgerState;
use ogmios_client::transactions::{SubmitTransactionResponse, Transactions};
use ogmios_client::types::OgmiosUtxo;
use std::fs;
use std::path::{Path, PathBuf};
use std::slice::from_ref;
use std::time::Duration;
use tokio::time::sleep;
use whisky::csl::{Address, BaseAddress, Credential, NetworkInfo, RewardAddress};
use whisky::data::{constr0, constr1};
use whisky::{
    Asset, Budget, LanguageVersion, Network, OfflineTxEvaluator, TxBuilder, WData, WRedeemer,
    Wallet,
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

pub struct CardanoClient {
    pub ogmios_clients: OgmiosClients,
    pub constants: Constants,
    pub wallet: Wallet,
    pub address: Address,
    pub network: Network,
    pub network_info: NetworkInfo,
}

impl CardanoClient {
    pub async fn new(ogmios_settings: OgmiosClientSettings, constants: Constants) -> Self {
        let ogmios_clients = client_for_url(
            &ogmios_settings.base_url,
            Duration::from_secs(ogmios_settings.timeout_seconds),
        )
        .await
        .expect("Failed to initialize client");

        let wallet = Self::create_wallet();
        Self::from_wallet(ogmios_settings, constants, wallet, ogmios_clients)
    }

    pub async fn new_from_funded(
        ogmios_settings: OgmiosClientSettings,
        constants: Constants,
    ) -> Self {
        let ogmios_clients = client_for_url(
            &ogmios_settings.base_url,
            Duration::from_secs(ogmios_settings.timeout_seconds),
        )
        .await
        .expect("Failed to initialize client");

        let wallet = Self::wallet_for_funded(constants.payments.funded_address_skey_cbor.as_str());
        Self::from_wallet(ogmios_settings, constants, wallet, ogmios_clients)
    }

    fn from_wallet(
        ogmios_settings: OgmiosClientSettings,
        constants: Constants,
        wallet: Wallet,
        ogmios_clients: OgmiosClients,
    ) -> Self {
        let network_info = Self::network_info(&ogmios_settings.network);
        let address = Self::address(&wallet, network_info.network_id());

        Self {
            ogmios_clients,
            constants,
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
            Network::Custom(_) => panic!("Custom networks are not supported"),
        }
    }

    fn create_wallet() -> Wallet {
        let mnemonic = Mnemonic::new(MnemonicType::Words24, Language::English);
        let phrase = mnemonic.phrase().to_string();
        Wallet::new_mnemonic(&phrase).expect("Failed to create a wallet")
    }

    fn wallet_for_funded(cli_skey: &str) -> Wallet {
        let cli_hex = cli_skey
            .strip_prefix("5820")
            .unwrap_or(cli_skey)
            .to_string();
        Wallet::new_cli(cli_hex.as_str()).expect("Failed to create a funded wallet")
    }

    fn address(wallet: &Wallet, network_id: u8) -> Address {
        let pub_key_hash = wallet.account.public_key.hash();
        let cred = Credential::from_keyhash(&pub_key_hash);
        let private_key_hash = wallet.account.private_key.to_hex();
        println!("Private key hash: {}", private_key_hash);
        BaseAddress::new(network_id, &cred, &cred).to_address()
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
        self.find_utxo_by_tx_id(&self.address_as_bech32(), tx_id_hex)
            .await
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
        let policies = self.constants.policies.clone();
        let validator_address = policies.auth_token_address();
        let datum = serde_json::to_string(&serde_json::json!(
            {
                "constructor": 0,
                "fields": [
                    {
                        "constructor": 0,
                        "fields": [
                            {
                                "bytes": &self.wallet.account.public_key.hash().to_hex()
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
        let auth_token_policy_id = policies.auth_token_policy_id();
        let send_assets = vec![
            Asset::new_from_str("lovelace", "2000000"),
            Asset::new_from_str(&auth_token_policy_id, "1"),
        ];
        let minting_script = policies.auth_token_cbor;
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
            .mint(1, &auth_token_policy_id, "")
            .minting_script(&minting_script)
            .mint_redeemer_value(&WRedeemer {
                data: WData::JSON(constr0(serde_json::json!([])).to_string()),
                ex_units: Budget {
                    mem: 14000000,
                    steps: 10000000000,
                },
            })
            .change_address(&payment_addr)
            .required_signer_hash(&self.wallet.account.public_key.hash().to_hex())
            .complete_sync(None)
            .unwrap();

        let signed_tx = self.wallet.sign_tx(&tx_builder.tx_hex());
        let tx_bytes = hex::decode(signed_tx.unwrap()).expect("Failed to decode hex string");
        self.ogmios_clients.submit_transaction(&tx_bytes).await
    }

    pub async fn deregister(
        &self,
        tx_in: &OgmiosUtxo,
        register_tx: &OgmiosUtxo,
        collateral_utxo: &OgmiosUtxo,
    ) -> Result<SubmitTransactionResponse, OgmiosClientError> {
        let policies = self.constants.policies.clone();
        let validator_address = policies.auth_token_address();
        let datum =
            serde_json::to_string(&serde_json::json!({"constructor": 0,"fields": []})).unwrap();
        let payment_addr = self.address_as_bech32();
        let auth_token_policy_id = policies.auth_token_policy_id();
        let send_assets = vec![Asset::new_from_str("lovelace", "2000000")];
        let minting_script = policies.auth_token_cbor.clone();
        let network = Network::Custom(self.constants.cost_model.clone());
        let mapping_validator_cbor = policies.auth_token_cbor;
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
            .mint(-1, &auth_token_policy_id, "")
            .minting_script(&minting_script)
            .mint_redeemer_value(&WRedeemer {
                data: WData::JSON(constr1(serde_json::json!([])).to_string()),
                ex_units: Budget {
                    mem: 3765700,
                    steps: 941562940,
                },
            })
            .change_address(&payment_addr)
            .required_signer_hash(&self.wallet.account.public_key.hash().to_hex())
            .complete_sync(None)
            .unwrap();

        let signed_tx = self
            .wallet
            .sign_tx(&tx_builder.tx_hex())
            .expect("Failed to sign tx");
        let tx_bytes = hex::decode(signed_tx).expect("Failed to decode hex string");
        self.ogmios_clients.submit_transaction(&tx_bytes).await
    }

    pub async fn mint_tokens(
        &self,
        amount: i32,
    ) -> Result<SubmitTransactionResponse, OgmiosClientError> {
        let policies = self.constants.policies.clone();

        let policy_id = policies.cnight_token_policy_id();
        let minting_script = policies.cnight_token_cbor;
        let network = Network::Custom(self.constants.cost_model.clone());

        let payment_addr = self.address_as_bech32();
        let collateral_utxo = match self.make_collateral().await {
            Some(utxo) => utxo,
            None => panic!("UTXO not found after funding"),
        };

        let utxos = self
            .ogmios_clients
            .query_utxos(from_ref(&payment_addr))
            .await?;

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
                &Self::build_asset_vector(&collateral_utxo),
                &payment_addr,
            )
            .tx_out(&payment_addr, &assets)
            .mint_plutus_script_v2()
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
        self.ogmios_clients.submit_transaction(&tx_bytes).await
    }

    pub async fn send(
        &self,
        assets: Vec<Asset>,
    ) -> Result<SubmitTransactionResponse, OgmiosClientError> {
        let payments = self.constants.payments.clone();
        let payment_addr = payments.funded_address;
        let utxos = self
            .ogmios_clients
            .query_utxos(from_ref(&payment_addr))
            .await?;
        assert!(!utxos.is_empty());

        let utxo = utxos
            .iter()
            .max_by_key(|u| u.value.lovelace)
            .expect("No UTXO with lovelace found");
        let cbor_hex = payments.funded_address_skey_cbor;
        let input_tx_hash = hex::encode(utxo.transaction.id);

        let address_as_bech32 = self.address_as_bech32();
        let tx_hex = TxBuilder::new_core()
            .tx_in(
                &input_tx_hash,
                utxo.index.into(),
                &Self::build_asset_vector(utxo),
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
        let utxos = self
            .ogmios_clients
            .query_utxos(&[address.into()])
            .await
            .unwrap();
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

        let utxos = self
            .ogmios_clients
            .query_utxos(from_ref(&self.constants.payments.funded_address))
            .await
            .map_err(|_| GetUtxoError::NotFoundOnChain)?;

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

        self.ogmios_clients.submit_transaction(&tx_bytes).await
    }

    pub fn reward_address_bytes(&self) -> [u8; 29] {
        let pub_key_hash = self.wallet.account.public_key.hash();
        let cred = Credential::from_keyhash(&pub_key_hash);
        RewardAddress::new(self.network_info.network_id(), &cred)
            .to_address()
            .to_bytes()
            .try_into()
            .unwrap()
    }
}
