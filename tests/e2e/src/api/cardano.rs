use crate::config::{self, Constants, OgmiosClientSettings};
use aiken_contracts_lib::{
    FederatedOpsCandidate, GovernanceMember, build_federated_ops_datum,
    build_federated_ops_redeemer, build_governance_redeemer, build_versioned_multisig_datum,
    convert_cost_models,
};
use bip39::{Language, Mnemonic, MnemonicType};
use ogmios_client::jsonrpsee::client_for_url;
use ogmios_client::query_ledger_state::{OgmiosTip, QueryLedgerState};
use ogmios_client::query_network::{QueryNetwork, ShelleyGenesisConfigurationResponse};
use ogmios_client::transactions::{SubmitTransactionResponse, Transactions};
use ogmios_client::types::OgmiosUtxo;
use ogmios_client::{OgmiosClient, OgmiosClientError, OgmiosParams};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use whisky::csl::{
    Address, Bip32PrivateKey, Credential, EnterpriseAddress, NetworkInfo, PrivateKey, RewardAddress,
};
use whisky::data::{constr0, constr1};
use whisky::{
    Asset, Budget, LanguageVersion, Network, OfflineTxEvaluator, Protocol, TxBuilder,
    TxBuilderParam, UTxO, UtxoInput, UtxoOutput, WData, WError, WRedeemer, Wallet, WalletType,
};

const OGMIOS_MAX_ATTEMPTS: u32 = 5;

/// Classify an ogmios error as retriable. Returns `Some((delay, label))` if we should retry,
/// `None` for terminal errors. The label is for logging.
///
/// Two retriable classes today:
/// - WS transport task died (jsonrpsee "background task closed") — retry quickly (2s).
/// - Cardano-node mempool refused admission because tx validation was too slow under load
///   ("MempoolTxTooSlow", server error 3997) — tx is valid, node is just busy. Back off
///   longer (5s) so the mempool can clear concurrent submissions.
fn retry_delay_for(e: &OgmiosClientError) -> Option<(Duration, &'static str)> {
    let OgmiosClientError::RequestError(s) = e else {
        return None;
    };
    if s.contains("background task closed") {
        Some((Duration::from_secs(2), "WS transient"))
    } else if s.contains("MempoolTxTooSlow") {
        Some((Duration::from_secs(5), "node mempool busy"))
    } else {
        None
    }
}

/// Detects the "ogmios response was missing a required field" parse failure — typically the
/// local devnet's `protocolParameters` omitting `plutus:v2`. Distinct from transport flakes:
/// we won't get a different answer by retrying.
fn is_partial_params_error(e: &OgmiosClientError) -> bool {
    matches!(e, OgmiosClientError::ResponseError(s) if s.contains("missing field"))
}

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
    QueryShelleyGenesisConfig,
}

#[derive(Debug)]
pub enum OgmiosResponse {
    QueryTip(OgmiosTip),
    QueryUtxo(Vec<OgmiosUtxo>),
    SubmitTx(SubmitTransactionResponse),
    QueryShelleyGenesisConfig(ShelleyGenesisConfigurationResponse),
}

pub struct CardanoClient {
    pub ogmios_settings: OgmiosClientSettings,
    pub constants: Constants,
    pub wallet: Wallet,
    pub network: Network,
    pub network_info: NetworkInfo,
    pub protocol: Protocol,
}

impl CardanoClient {
    pub async fn new(ogmios_settings: OgmiosClientSettings, constants: Constants) -> Self {
        let wallet = Self::create_wallet();
        Self::print_addresses(&wallet, &Self::network_info(&ogmios_settings.network));
        Self::from_wallet(ogmios_settings, constants, wallet).await
    }

    pub async fn new_from_funded(
        ogmios_settings: OgmiosClientSettings,
        constants: Constants,
    ) -> Self {
        let wallet = Self::wallet_for_funded(constants.payments.funded_address_skey_cbor.as_str());
        Self::from_wallet(ogmios_settings, constants, wallet).await
    }

    async fn from_wallet(
        ogmios_settings: OgmiosClientSettings,
        constants: Constants,
        wallet: Wallet,
    ) -> Self {
        let network_info = Self::network_info(&ogmios_settings.network);
        let (network, protocol) = Self::fetch_chain_params(&ogmios_settings, &constants).await;

        Self {
            ogmios_settings: ogmios_settings.clone(),
            constants,
            wallet,
            network,
            network_info,
            protocol,
        }
    }

    async fn fetch_chain_params(
        config: &OgmiosClientSettings,
        constants: &Constants,
    ) -> (Network, Protocol) {
        let mut last_err = None;
        for attempt in 1..=OGMIOS_MAX_ATTEMPTS {
            match Self::fetch_chain_params_once(config).await {
                Ok(v) => return v,
                Err(e) => match retry_delay_for(&e) {
                    Some((delay, label)) => {
                        tracing::info!(
                            "ogmios protocol-params query: {} on attempt {}/{}; retry in {:?}",
                            label,
                            attempt,
                            OGMIOS_MAX_ATTEMPTS,
                            delay,
                        );
                        last_err = Some(e);
                        sleep(delay).await;
                    }
                    None if is_partial_params_error(&e) => {
                        return Self::fallback_chain_params(constants, &e);
                    }
                    None => panic!("Failed to query protocol parameters: {:?}", e),
                },
            }
        }
        panic!(
            "Failed to query protocol parameters after {} attempts: {:?}",
            OGMIOS_MAX_ATTEMPTS,
            last_err.unwrap(),
        );
    }

    /// Some environments (e.g. the local devnet's ogmios) return a `queryLedgerState/
    /// protocolParameters` response without all three Plutus cost-model versions, which the
    /// strongly-typed `ogmios-client` deserializer rejects whole. When that happens we fall
    /// back to the cost models baked into `Constants` and whisky's default fee/size params —
    /// fine for local dev where the node runs with mainnet-default protocol params anyway.
    fn fallback_chain_params(
        constants: &Constants,
        err: &OgmiosClientError,
    ) -> (Network, Protocol) {
        tracing::info!(
            "ogmios returned incomplete protocol params ({:?}); falling back to \
             Constants.cost_model + whisky defaults (intended for local dev)",
            err,
        );
        (
            Network::Custom(constants.cost_model.clone()),
            Protocol::default(),
        )
    }

    async fn fetch_chain_params_once(
        config: &OgmiosClientSettings,
    ) -> Result<(Network, Protocol), OgmiosClientError> {
        let client = client_for_url(
            &config.base_url,
            Duration::from_secs(config.timeout_seconds),
        )
        .await
        .map_err(|e| OgmiosClientError::RequestError(format!("connect: {}", e)))?;
        let params = client.query_protocol_parameters().await?;

        let network = Network::Custom(convert_cost_models(&params.plutus_cost_models));

        let price_mem = *params.script_execution_prices.memory.numer() as f64
            / *params.script_execution_prices.memory.denom() as f64;
        let price_step = *params.script_execution_prices.cpu.numer() as f64
            / *params.script_execution_prices.cpu.denom() as f64;
        let protocol = Protocol {
            min_fee_a: params.min_fee_coefficient as u64,
            min_fee_b: params.min_fee_constant.lovelace,
            max_tx_size: params.max_transaction_size.bytes,
            max_val_size: params.max_value_size.bytes,
            coins_per_utxo_size: params.min_utxo_deposit_coefficient,
            key_deposit: params.stake_credential_deposit.lovelace,
            pool_deposit: params.stake_pool_deposit.lovelace,
            max_collateral_inputs: params.max_collateral_inputs as i32,
            collateral_percent: params.collateral_percentage as f64,
            min_fee_ref_script_cost_per_byte: params.min_fee_reference_scripts.base as u64,
            price_mem,
            price_step,
            ..Default::default()
        };

        Ok((network, protocol))
    }

    /// Build a fresh whisky `TxBuilder` pre-loaded with the protocol params we fetched from
    /// ogmios. Use this instead of `TxBuilder::new_core()` so fee / size / utxo-min computation
    /// matches the connected network (preview, preprod, mainnet, etc.) rather than whisky's
    /// hard-coded mainnet defaults.
    fn new_tx_builder(&self) -> TxBuilder {
        TxBuilder::new(TxBuilderParam {
            evaluator: None,
            fetcher: None,
            submitter: None,
            params: Some(self.protocol.clone()),
        })
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
        tracing::info!("Generated mnemonic phrase: {}", phrase);
        Wallet::new_mnemonic(&phrase).expect("Failed to create a wallet")
    }

    fn print_addresses(wallet: &Wallet, network_info: &NetworkInfo) {
        let delegated_payment_address = wallet
            .get_change_address(whisky::AddressType::Payment)
            .expect("Failed to get change address");
        tracing::info!("Payment address: {}", delegated_payment_address);

        let payment_public_key_hash = wallet.account.as_ref().unwrap().public_key.hash().to_hex();
        tracing::info!("Payment public key hash: {}", payment_public_key_hash);

        let stake_cred = wallet.addresses.base_address.as_ref().unwrap().stake_cred();

        let reward_address = RewardAddress::new(network_info.network_id(), &stake_cred)
            .to_address()
            .to_bech32(None)
            .unwrap();

        tracing::info!("Reward (stake) address: {}", reward_address);
        tracing::info!(
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
        tx_ins: &[OgmiosUtxo],
        tx_out_addr: &str,
        assets: Vec<Asset>,
    ) -> Option<OgmiosUtxo> {
        let tx_id_hex = match self.send(tx_ins, tx_out_addr, assets).await {
            Ok(response) => hex::encode(response.transaction.id),
            Err(e) => panic!("Failed to send assets: {:?}", e),
        };
        tracing::info!("Funded wallet with transaction id: {}", tx_id_hex);
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
                tracing::info!("Derived enterprise address: {}", address_bech32);
                address_bech32
            }
        }
    }

    async fn ogmios_request(
        config: &OgmiosClientSettings,
        req: OgmiosRequest,
    ) -> Result<OgmiosResponse, OgmiosClientError> {
        let mut last_err = None;
        for attempt in 1..=OGMIOS_MAX_ATTEMPTS {
            match Self::ogmios_request_once(config, req.clone()).await {
                Ok(v) => return Ok(v),
                Err(e) => match retry_delay_for(&e) {
                    Some((delay, label)) => {
                        tracing::info!(
                            "ogmios request: {} on attempt {}/{}; retry in {:?}",
                            label,
                            attempt,
                            OGMIOS_MAX_ATTEMPTS,
                            delay,
                        );
                        last_err = Some(e);
                        sleep(delay).await;
                    }
                    None => return Err(e),
                },
            }
        }
        Err(last_err.unwrap())
    }

    async fn ogmios_request_once(
        config: &OgmiosClientSettings,
        req: OgmiosRequest,
    ) -> Result<OgmiosResponse, OgmiosClientError> {
        let client = client_for_url(
            &config.base_url,
            Duration::from_secs(config.timeout_seconds),
        )
        .await
        .map_err(|e| OgmiosClientError::RequestError(format!("connect: {}", e)))?;
        match req {
            OgmiosRequest::QueryTip => {
                let tip = client.get_tip().await?;
                Ok(OgmiosResponse::QueryTip(tip))
            }
            OgmiosRequest::QueryUtxo { address } => {
                let utxos = client.query_utxos(&[address]).await?;
                Ok(OgmiosResponse::QueryUtxo(utxos))
            }
            OgmiosRequest::SubmitTx { tx_bytes } => {
                let response = client.submit_transaction(&tx_bytes).await?;
                Ok(OgmiosResponse::SubmitTx(response))
            }
            OgmiosRequest::QueryShelleyGenesisConfig => {
                let config = client.shelley_genesis_configuration().await?;
                Ok(OgmiosResponse::QueryShelleyGenesisConfig(config))
            }
        }
    }

    /// Query Ogmios for the current Cardano tip block height
    /// (`queryNetwork/blockHeight`). This returns the *block* number, not the
    /// slot number — the security parameter `k` is measured in blocks, so
    /// block height is the correct unit for stability comparisons.
    async fn query_block_height(client: &impl OgmiosClient) -> Result<u64, OgmiosClientError> {
        client
            .request("queryNetwork/blockHeight", OgmiosParams::empty_by_name())
            .await
    }

    /// Fetch the Cardano security parameter (k) from the Shelley genesis
    /// configuration. The Midnight mainchain follower only processes Cardano
    /// blocks that are >= k blocks behind the tip; the stability barrier in
    /// `tests/lib.rs` uses this to know how long to wait before observation
    /// assertions can succeed.
    pub async fn cardano_security_parameter(
        ogmios_settings: &OgmiosClientSettings,
    ) -> Result<u32, OgmiosClientError> {
        let response =
            Self::ogmios_request(ogmios_settings, OgmiosRequest::QueryShelleyGenesisConfig).await?;
        match response {
            OgmiosResponse::QueryShelleyGenesisConfig(config) => Ok(config.security_parameter),
            _ => Err(OgmiosClientError::RequestError(
                "Unexpected response type for QueryShelleyGenesisConfig".into(),
            )),
        }
    }

    /// Snapshot the Cardano tip, but only after it has advanced past the
    /// initial reading by `REQUIRED_ADVANCE` blocks. Use this when you've
    /// just submitted a transaction and need a tip that's guaranteed to
    /// be past the tx's landing block (`current_block_height` alone is
    /// racy: a tx submitted milliseconds before this call is still in
    /// the mempool, so the immediate tip doesn't include it).
    ///
    /// We wait for 2 blocks (not 1) because Cardano's mempool ordering
    /// is non-deterministic — a freshly submitted tx may slip into
    /// either `baseline + 1` or `baseline + 2` depending on whether
    /// the block producer's mempool snapshot beat ours. Two blocks of
    /// advance gives us a safe upper bound on the landing block.
    ///
    /// Polls every 2s until either (a) tip advances by ≥2 blocks, or
    /// (b) the 5-minute wait budget is exhausted (in which case the
    /// latest known tip is returned — downstream waits will catch a
    /// genuinely slow chain).
    pub async fn snapshot_tip_after_advance(ogmios_settings: &OgmiosClientSettings) -> Option<u64> {
        const POLL_INTERVAL: Duration = Duration::from_secs(2);
        const WAIT_BUDGET: Duration = Duration::from_secs(300);
        const REQUIRED_ADVANCE: u64 = 2;
        let baseline = Self::current_block_height(ogmios_settings).await?;
        let start = Instant::now();
        let mut latest = baseline;
        while start.elapsed() < WAIT_BUDGET {
            sleep(POLL_INTERVAL).await;
            if let Some(now) = Self::current_block_height(ogmios_settings).await {
                latest = now;
                if now >= baseline + REQUIRED_ADVANCE {
                    return Some(now);
                }
            }
        }
        Some(latest)
    }

    /// Returns the current Cardano tip block height. Retries on transient
    /// errors (connection refused, broken pipe, etc.) — Ogmios occasionally
    /// flakes during high-concurrency test runs, and a single hiccup
    /// shouldn't abort a multi-hour await target snapshot. Returns `None`
    /// only after exhausting the retry budget.
    pub async fn current_block_height(ogmios_settings: &OgmiosClientSettings) -> Option<u64> {
        const MAX_ATTEMPTS: u32 = 5;
        const RETRY_DELAY: Duration = Duration::from_secs(2);
        for attempt in 1..=MAX_ATTEMPTS {
            let result = async {
                let client = client_for_url(
                    &ogmios_settings.base_url,
                    Duration::from_secs(ogmios_settings.timeout_seconds),
                )
                .await
                .map_err(|e| format!("connect: {e}"))?;
                Self::query_block_height(&client)
                    .await
                    .map_err(|e| format!("query: {e}"))
            }
            .await;
            match result {
                Ok(h) => return Some(h),
                Err(e) => {
                    if attempt < MAX_ATTEMPTS {
                        tracing::warn!(
                            "current_block_height: attempt {attempt}/{MAX_ATTEMPTS} failed: {e}; \
                             retrying in {RETRY_DELAY:?}"
                        );
                        sleep(RETRY_DELAY).await;
                    } else {
                        tracing::warn!(
                            "current_block_height: exhausted {MAX_ATTEMPTS} attempts; \
                             last error: {e}"
                        );
                    }
                }
            }
        }
        None
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
        let validator_address = config::mapping_validator_address();
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
        let mapping_validator_policy_id = config::mapping_validator_policy_id();
        let send_assets = vec![
            Asset::new_from_str("lovelace", "2000000"),
            Asset::new_from_str(&mapping_validator_policy_id, "1"),
        ];
        let minting_script = config::mapping_validator_cbor_double_encoding();
        let mut tx_builder = self.new_tx_builder();
        tx_builder
            .network(self.network.clone())
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
        let validator_address = config::mapping_validator_address();
        let datum =
            serde_json::to_string(&serde_json::json!({"constructor": 0,"fields": []})).unwrap();
        let payment_addr = self.address_as_bech32();
        let mapping_validator_policy_id = config::mapping_validator_policy_id();
        let send_assets = vec![Asset::new_from_str("lovelace", "2000000")];
        let minting_script = config::mapping_validator_cbor_double_encoding();
        let mapping_validator_cbor = config::mapping_validator_cbor_double_encoding();
        let register_asset_tx_vector = Self::build_asset_vector(register_tx);
        tracing::info!(
            "Register tx assets: [{}]",
            register_asset_tx_vector
                .iter()
                .map(|a| format!("{}={}", a.unit(), a.quantity()))
                .collect::<Vec<_>>()
                .join(", "),
        );
        let script_hash = whisky::get_script_hash(&mapping_validator_cbor, LanguageVersion::V2);
        match &script_hash {
            Ok(h) => tracing::info!("Mapping validator script hash: 0x{h}"),
            Err(e) => tracing::warn!("Mapping validator script hash unavailable: {e:?}"),
        }

        let mut tx_builder = self.new_tx_builder();
        tx_builder
            .network(self.network.clone())
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
        amount: u64,
        collateral_utxo: &OgmiosUtxo,
    ) -> Result<SubmitTransactionResponse, OgmiosClientError> {
        let policy_id = config::cnight_token_policy_id();
        let minting_script = config::cnight_token_cbor_double_encoding();
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

        let mut tx_builder = self.new_tx_builder();
        tx_builder
            .network(self.network.clone())
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

        // Diagnostic: log the mint's spend input so a downstream "All
        // inputs are spent" failure can be correlated against parallel
        // register/mint submissions that may be claiming the same UTXO.
        tracing::info!(
            "  mint_tokens input: {input_tx_hash}#{input_index} (collateral: {}#{})",
            hex::encode(collateral_utxo.transaction.id),
            collateral_utxo.index,
        );
        let tx_hex = tx_builder.tx_hex();
        let tx_fingerprint: String = tx_hex.chars().take(32).collect();
        tracing::info!(
            "  mint_tokens tx_hex prefix: {tx_fingerprint}... (len={})",
            tx_hex.len()
        );

        let signed_tx = self.wallet.sign_tx(&tx_hex);
        let tx_bytes = hex::decode(signed_tx.unwrap()).expect("Failed to decode hex string");
        let request = OgmiosRequest::SubmitTx { tx_bytes };
        let response = match Self::ogmios_request(&self.ogmios_settings, request).await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(
                    "Ogmios mint_tokens SubmitTx failed: {e}\n  input: {input_tx_hash}#{input_index}\n  tx_hex prefix: {tx_fingerprint}... (len={})",
                    tx_hex.len()
                );
                panic!("Ogmios mint_tokens SubmitTx failed: {e:?}");
            }
        };
        match response {
            OgmiosResponse::SubmitTx(res) => {
                tracing::info!(
                    "  mint_tokens submitted: tx_id=0x{}",
                    hex::encode(res.transaction.id)
                );
                Ok(res)
            }
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
        let mut tx_builder = self.new_tx_builder();
        tx_builder
            .network(self.network.clone())
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
        tx_ins: &[OgmiosUtxo],
        tx_out_addr: &str,
        assets: Vec<Asset>,
    ) -> Result<SubmitTransactionResponse, OgmiosClientError> {
        let payment_addr = self.address_as_bech32();
        tracing::info!(
            "Sending assets from {} ({} input UTXOs) to address: {}",
            payment_addr,
            tx_ins.len(),
            tx_out_addr,
        );
        // Diagnostic: log each input UTXO ref so a downstream "All inputs
        // are spent / already included" failure can be correlated against
        // (a) earlier submissions in this run, (b) another parallel test
        // claiming the same faucet worker, or (c) stale UTXOs left in the
        // faucet wallet from a prior run.
        let inputs_fmt: Vec<String> = tx_ins
            .iter()
            .map(|u| format!("{}#{}", hex::encode(u.transaction.id), u.index))
            .collect();
        for r in &inputs_fmt {
            tracing::info!("  send input: {r}");
        }

        let mut tx_builder = self.new_tx_builder();
        for tx_in in tx_ins {
            tx_builder.tx_in(
                &hex::encode(tx_in.transaction.id),
                tx_in.index.into(),
                &Self::build_asset_vector(tx_in),
                payment_addr.as_str(),
            );
        }
        tx_builder
            .tx_out(tx_out_addr, &assets)
            .change_address(&payment_addr)
            .complete_sync(None)
            .unwrap();
        // Diagnostic fingerprint of the constructed (unsigned) tx body.
        // The full body is too long for normal logs but the prefix is
        // sufficient to spot identical txs being built twice (retry path
        // or two tests racing on the same inputs).
        let tx_hex = tx_builder.tx_hex();
        let tx_fingerprint: String = tx_hex.chars().take(32).collect();
        tracing::info!(
            "  send tx_hex prefix: {tx_fingerprint}... (len={})",
            tx_hex.len()
        );

        let signed_tx = self.wallet.sign_tx(&tx_hex).expect("Failed to sign tx");
        let tx_bytes = hex::decode(signed_tx).expect("Failed to decode hex string");
        let request = OgmiosRequest::SubmitTx { tx_bytes };
        let response = match Self::ogmios_request(&self.ogmios_settings, request).await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(
                    "Ogmios SubmitTx failed: {e}\n  inputs ({}): [{}]\n  tx_hex prefix: {tx_fingerprint}... (len={})",
                    inputs_fmt.len(),
                    inputs_fmt.join(", "),
                    tx_hex.len(),
                );
                panic!("Ogmios SubmitTx failed: {e:?}");
            }
        };
        match response {
            OgmiosResponse::SubmitTx(res) => {
                tracing::info!(
                    "  send submitted: tx_id=0x{}",
                    hex::encode(res.transaction.id)
                );
                Ok(res)
            }
            _ => Err(OgmiosClientError::RequestError(
                "Unexpected response type".into(),
            )),
        }
    }

    pub async fn consolidate_utxos(
        &self,
        batch_size: usize,
    ) -> Result<Vec<String>, OgmiosClientError> {
        let payment_addr = self.address_as_bech32();
        let utxos = self.utxos().await;
        tracing::info!(
            "Consolidating {} UTXOs at {} (batch size {})",
            utxos.len(),
            payment_addr,
            batch_size
        );

        let mut tx_ids = Vec::new();
        for (i, chunk) in utxos.chunks(batch_size).enumerate() {
            if chunk.len() < 2 {
                tracing::info!("Batch {}: skipping (only {} UTXO)", i, chunk.len());
                continue;
            }

            let mut tx_builder = self.new_tx_builder();
            for utxo in chunk {
                tx_builder.tx_in(
                    &hex::encode(utxo.transaction.id),
                    utxo.index.into(),
                    &Self::build_asset_vector(utxo),
                    payment_addr.as_str(),
                );
            }
            tx_builder
                .change_address(&payment_addr)
                .complete_sync(None)
                .unwrap();

            let signed_tx = self
                .wallet
                .sign_tx(&tx_builder.tx_hex())
                .expect("Failed to sign tx");
            let tx_bytes = hex::decode(signed_tx).expect("Failed to decode hex string");
            let request = OgmiosRequest::SubmitTx { tx_bytes };
            let response = Self::ogmios_request(&self.ogmios_settings, request).await?;
            match response {
                OgmiosResponse::SubmitTx(res) => {
                    let tx_id_hex = hex::encode(res.transaction.id);
                    tracing::info!(
                        "Batch {}: consolidated {} UTXOs in tx {}",
                        i,
                        chunk.len(),
                        tx_id_hex
                    );
                    tx_ids.push(tx_id_hex);
                }
                _ => {
                    return Err(OgmiosClientError::RequestError(
                        "Unexpected response type".into(),
                    ));
                }
            }
        }

        if let Some(last) = tx_ids.last() {
            self.find_utxo_by_tx_id(&payment_addr, last.clone()).await;
            let after = self.utxos().await;
            tracing::info!(
                "Consolidation complete: {} UTXOs -> {} UTXOs ({} txs submitted)",
                utxos.len(),
                after.len(),
                tx_ids.len()
            );
        }

        Ok(tx_ids)
    }

    /// Poll Ogmios until `tx_id` has produced a UTXO at `address`,
    /// confirming the tx has been included in a Cardano block (not just
    /// accepted into the mempool). This is a **cheap** wait — typically
    /// 20–60s on Preview, one Cardano block — NOT a full stability wait.
    ///
    /// Use this between dependent submission phases to avoid mempool
    /// collisions: e.g. between `register(...)` and `mint_tokens(...)`
    /// when whisky's tx-funding would otherwise pick the same wallet
    /// UTXO that register is consuming in the mempool, and Cardano
    /// rejects the second tx with "All inputs are spent".
    ///
    /// Returns `Err` on timeout with the tx_id + address in the message
    /// so CI failures point directly at the missing inclusion.
    pub async fn wait_for_tx_inclusion(
        &self,
        tx_id: &[u8; 32],
        address: &str,
        max_wait: Duration,
    ) -> Result<(), OgmiosClientError> {
        let tx_id_hex = hex::encode(tx_id);
        let start = std::time::Instant::now();
        let poll_interval = Duration::from_secs(5);
        tracing::info!(
            "wait_for_tx_inclusion: polling for tx 0x{tx_id_hex} at {address} (timeout {max_wait:?})"
        );
        loop {
            if self
                .find_utxo_by_tx_id(address, tx_id_hex.clone())
                .await
                .is_some()
            {
                tracing::info!(
                    "wait_for_tx_inclusion: tx 0x{tx_id_hex} included after {:?}",
                    start.elapsed()
                );
                return Ok(());
            }
            if start.elapsed() > max_wait {
                return Err(OgmiosClientError::RequestError(format!(
                    "wait_for_tx_inclusion timed out: tx 0x{tx_id_hex} not found at {address} within {max_wait:?}"
                )));
            }
            tokio::time::sleep(poll_interval).await;
        }
    }

    pub async fn find_utxo_by_tx_id(&self, address: &str, tx_id_hex: String) -> Option<OgmiosUtxo> {
        const MAX_ATTEMPTS: u32 = 120;
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

    pub async fn find_utxos_by_tx_id(&self, address: &str, tx_id_hex: String) -> Vec<OgmiosUtxo> {
        const MAX_ATTEMPTS: u32 = 120;
        const PAUSE: Duration = Duration::from_secs(2);
        let tx_id_bytes = hex::decode(&tx_id_hex).expect("invalid hex tx_id");
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
            let matches: Vec<_> = utxos
                .into_iter()
                .filter(|u| u.transaction.id.as_ref() == tx_id_bytes.as_slice())
                .collect();
            if !matches.is_empty() {
                return matches;
            }
            sleep(PAUSE).await;
        }
        vec![]
    }

    pub async fn split_to_self(
        &self,
        candidate_utxos: &[OgmiosUtxo],
        n_outputs: usize,
        lovelace_per_output: u64,
    ) -> Result<[u8; 32], OgmiosClientError> {
        const SELECTION_THRESHOLD_LOVELACE: u64 = 5_000_000;
        let payment_addr = self.address_as_bech32();
        let candidates: Vec<UTxO> = candidate_utxos.iter().map(Self::to_whisky_utxo).collect();

        let mut tx_builder = self.new_tx_builder();
        let output_assets = vec![Asset::new_from_str(
            "lovelace",
            &lovelace_per_output.to_string(),
        )];
        for _ in 0..n_outputs {
            tx_builder.tx_out(&payment_addr, &output_assets);
        }
        // whisky's auto fee calc under-pays severely for multi-input txs (observed ~4x on
        // Preview). Override with a worst-case fee derived from the protocol params we fetched
        // from ogmios: enough to cover anything up to max_tx_size plus a small cushion. The
        // change UTXO absorbs the leftover.
        let prime_fee = self.protocol.min_fee_a * self.protocol.max_tx_size as u64
            + self.protocol.min_fee_b
            + 100_000;
        tx_builder
            .change_address(&payment_addr)
            .select_utxos_from(&candidates, SELECTION_THRESHOLD_LOVELACE)
            .set_fee(&prime_fee.to_string())
            .complete_sync(None)
            .unwrap();

        let signed_tx = self
            .wallet
            .sign_tx(&tx_builder.tx_hex())
            .expect("Failed to sign split tx");
        let tx_bytes = hex::decode(signed_tx).expect("Failed to decode hex string");
        let request = OgmiosRequest::SubmitTx { tx_bytes };
        let response = Self::ogmios_request(&self.ogmios_settings, request).await?;
        match response {
            OgmiosResponse::SubmitTx(res) => Ok(res.transaction.id),
            _ => Err(OgmiosClientError::RequestError(
                "Unexpected response type".into(),
            )),
        }
    }

    fn to_whisky_utxo(u: &OgmiosUtxo) -> UTxO {
        UTxO {
            input: UtxoInput {
                output_index: u.index as u32,
                tx_hash: hex::encode(u.transaction.id),
            },
            output: UtxoOutput {
                address: u.address.clone(),
                amount: Self::build_asset_vector(u),
                data_hash: None,
                plutus_data: None,
                script_ref: None,
                script_hash: None,
            },
        }
    }

    /// Query all UTxOs at a given address
    pub async fn query_utxos(&self, address: &str) -> Vec<OgmiosUtxo> {
        let request = OgmiosRequest::QueryUtxo {
            address: address.to_string(),
        };

        let response = Self::ogmios_request(&self.ogmios_settings, request)
            .await
            .unwrap();

        match response {
            OgmiosResponse::QueryUtxo(utxos) => utxos,
            _ => vec![],
        }
    }

    pub fn build_asset_vector(utxo: &OgmiosUtxo) -> Vec<Asset> {
        let mut assets: Vec<Asset> = utxo
            .value
            .native_tokens
            .iter()
            .flat_map(|(policy_id, tokens)| {
                let policy_hex = hex::encode(policy_id);
                tokens.iter().map(move |token| {
                    // Cardano "unit" = policy_id || asset_name (both hex,
                    // concatenated). Passing only `policy_hex` produced an
                    // empty asset name on the output side, which caused
                    // ledger to reject split_to_self with "in and out value
                    // not conserved" whenever the faucet's UTXOs held a
                    // named native token (e.g. leftover "Reward token" from
                    // a prior cNIGHT mint cycle): input asset name was
                    // "Reward token", change output asset name was "".
                    let unit = format!("{}{}", policy_hex, hex::encode(&token.name));
                    Asset::new_from_str(&unit, &token.amount.to_string())
                })
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
        const LIMIT: i32 = 50;
        let response = Self::ogmios_request(&self.ogmios_settings, OgmiosRequest::QueryTip)
            .await
            .unwrap();
        let tip = match response {
            OgmiosResponse::QueryTip(tip) => tip,
            _ => panic!("Unexpected response type"),
        };
        let start_slot = tip.slot;
        tracing::info!(
            "Current slot is {}. Waiting for {} more slots (limit {} checks)...",
            start_slot,
            SLOTS_NUMBER,
            LIMIT
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
                tracing::info!("Slot advanced: {} -> {}", last_slot, tip.slot);
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
            tracing::info!("UTXO {} is still unspent after 3 slots.", tx_id);
        } else {
            tracing::info!("UTXO {} was spent within 3 slots.", tx_id);
        }
        still_unspent
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
        _total_signers: u64,
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

        // Build the VersionedMultisig datum and redeemer using shared library
        let members: Vec<GovernanceMember> = sr25519_pubkeys
            .iter()
            .map(|(cardano_hash, sr25519_key)| GovernanceMember {
                cardano_hash: cardano_hash.clone(),
                sr25519_key: sr25519_key.clone(),
            })
            .collect();

        let datum = build_versioned_multisig_datum(&members);
        let redeemer = build_governance_redeemer(&members);

        // Validation: Verify script hash matches policy ID
        let calculated_hash = whisky::get_script_hash(script_cbor, LanguageVersion::V3);
        if let Ok(hash) = calculated_hash {
            if hash != policy_id {
                tracing::info!("WARNING: Script hash mismatch!");
                tracing::info!("  Expected (policy_id): {}", policy_id);
                tracing::info!("  Calculated from script: {}", hash);
                tracing::info!("  This transaction may fail validation!");
            }
        }

        tracing::info!("Deploying governance contract");
        tracing::info!("  Script address: {}", script_address);
        tracing::info!("  Policy ID: {}", policy_id);
        tracing::info!("  Total signers: {}", members.len());
        tracing::info!(
            "  One-shot UTXO: {}#{}",
            hex::encode(one_shot_utxo.transaction.id),
            one_shot_utxo.index
        );
        tracing::info!("  Datum: {}", serde_json::to_string_pretty(&datum).unwrap());
        tracing::info!(
            "  Redeemer: {}",
            serde_json::to_string_pretty(&redeemer).unwrap()
        );

        let send_assets = vec![
            Asset::new_from_str("lovelace", "2000000"), // 2 ADA
            Asset::new_from_str(policy_id, "1"),        // The governance NFT
        ];

        let mut tx_builder = self.new_tx_builder();
        tx_builder
            .network(self.network.clone())
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

        tracing::info!("✓ Transaction Built Successfully");

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

    /// Deploy a federated operators contract with the FederatedOps datum format
    ///
    /// The FederatedOps datum format is:
    /// [data: Data, appendix: List<PermissionedCandidateDatumV1>, logic_round: Int]
    /// where PermissionedCandidateDatumV1 = [partner_chains_key: ByteArray, keys: List<CandidateKey>]
    /// and CandidateKey = [id: ByteArray, bytes: ByteArray]
    ///
    /// # Arguments
    /// * `candidates` - List of (ecdsa_key, aura_key, grandpa_key) tuples where:
    ///   - ecdsa_key: The cross-chain (crch) ECDSA public key (33 bytes, hex string)
    ///   - aura_key: The SR25519 public key for AURA consensus (32 bytes, hex string)
    ///   - grandpa_key: The ED25519 public key for GRANDPA finality (32 bytes, hex string)
    #[allow(clippy::too_many_arguments)]
    pub async fn deploy_federated_ops_contract(
        &self,
        tx_in: &OgmiosUtxo,
        collateral_utxo: &OgmiosUtxo,
        one_shot_utxo: &OgmiosUtxo,
        script_cbor: &str,
        script_address: &str,
        policy_id: &str,
        candidates: Vec<(String, String, String)>, // (ecdsa_key, aura_key, grandpa_key) tuples
    ) -> Result<SubmitTransactionResponse, OgmiosClientError> {
        let payments = self.constants.payments.clone();
        let funded_addr = payments.funded_address;
        let funded_skey_cbor = payments.funded_address_skey_cbor;

        let funded_addr_parsed =
            Address::from_bech32(&funded_addr).expect("Invalid funded address");
        let payment_keyhash = funded_addr_parsed
            .payment_cred()
            .expect("No payment credential in address")
            .to_keyhash()
            .expect("Payment credential is not a keyhash");
        let payment_keyhash_hex = hex::encode(payment_keyhash.to_bytes());

        // Build the FederatedOps datum and redeemer using shared library
        let fedops_candidates: Vec<FederatedOpsCandidate> = candidates
            .iter()
            .map(|(ecdsa_key, aura_key, grandpa_key)| FederatedOpsCandidate {
                ecdsa_key: ecdsa_key.clone(),
                aura_key: aura_key.clone(),
                grandpa_key: grandpa_key.clone(),
            })
            .collect();

        let datum = build_federated_ops_datum(&fedops_candidates);
        let redeemer = build_federated_ops_redeemer(&fedops_candidates);

        tracing::info!("Deploying federated operators contract");
        tracing::info!("  Script address: {}", script_address);
        tracing::info!("  Policy ID: {}", policy_id);
        tracing::info!("  Candidates: {}", fedops_candidates.len());
        tracing::info!(
            "  One-shot UTXO: {}#{}",
            hex::encode(one_shot_utxo.transaction.id),
            one_shot_utxo.index
        );
        tracing::info!("  Datum: {}", serde_json::to_string_pretty(&datum).unwrap());

        let send_assets = vec![
            Asset::new_from_str("lovelace", "2000000"),
            Asset::new_from_str(policy_id, "1"),
        ];

        let mut tx_builder = self.new_tx_builder();
        tx_builder
            .network(self.network.clone())
            .set_evaluator(Box::new(OfflineTxEvaluator::new()))
            .tx_in(
                &hex::encode(tx_in.transaction.id),
                tx_in.index.into(),
                &Self::build_asset_vector(tx_in),
                &funded_addr,
            )
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
            .tx_out(script_address, &send_assets)
            .tx_out_inline_datum_value(&WData::JSON(datum.to_string()))
            .mint_plutus_script_v3()
            .mint(1, policy_id, "")
            .minting_script(script_cbor)
            .mint_redeemer_value(&WRedeemer {
                data: WData::JSON(redeemer.to_string()),
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

        tracing::info!("✓ Transaction Built Successfully");

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

        let mut tx_builder = self.new_tx_builder();
        tx_builder
            .network(self.network.clone())
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
    /// Build and sign — but do not submit — a Cardano bridge transfer that
    /// sends cNight to the ICS validator address with metadata identifying the
    /// target Midnight address. Pair with `submit_tx` to publish.
    pub async fn make_bridge_transfer(
        &self,
        cnight_utxo: &OgmiosUtxo,
        payment_utxo: &OgmiosUtxo,
        ics_address: &str,
        amount: u64,
        recipient: BridgeTransferRecipient,
    ) -> Result<SignedBridgeTransaction, OgmiosClientError> {
        const BRIDGE_METADATUM_LABEL: u64 = 6_500_973;
        let policy_id = config::cnight_token_policy_id();
        let network = Network::Custom(self.constants.cost_model.clone());
        let payment_addr = self.address_as_bech32();

        let estimated_fee_and_required_change = 250_000 + 1_500_000;
        let send_assets = vec![
            Asset::new_from_str(
                "lovelace",
                &(cnight_utxo.value.lovelace + payment_utxo.value.lovelace
                    - estimated_fee_and_required_change)
                    .to_string(),
            ),
            Asset::new_from_str(&policy_id, &amount.to_string()),
        ];

        let mut unsigned_tx_hex = TxBuilder::new_core()
            .network(network)
            .set_evaluator(Box::new(OfflineTxEvaluator::new()))
            .tx_in(
                &hex::encode(cnight_utxo.transaction.id),
                cnight_utxo.index.into(),
                &Self::build_asset_vector(cnight_utxo),
                &payment_addr,
            )
            .tx_in(
                &hex::encode(payment_utxo.transaction.id),
                payment_utxo.index.into(),
                &Self::build_asset_vector(payment_utxo),
                &payment_addr,
            )
            .tx_in_collateral(
                &hex::encode(payment_utxo.transaction.id),
                payment_utxo.index.into(),
                &Self::build_asset_vector(payment_utxo),
                &payment_addr,
            )
            .tx_out(ics_address, &send_assets)
            .tx_out_inline_datum_value(&WData::JSON(
                serde_json::json!({"constructor": 0, "fields": []}).to_string(),
            ))
            .metadata_value(
                &BRIDGE_METADATUM_LABEL.to_string(),
                BRIDGE_ADDRESS_METADATUM_PLACEHOLDER_JSON,
            )
            .change_address(&payment_addr)
            .complete_sync(None)
            .unwrap()
            .tx_hex();

        if let BridgeTransferRecipient::Address(address) = &recipient {
            unsigned_tx_hex = replace_with_bytes_metadatum(
                &unsigned_tx_hex,
                BRIDGE_METADATUM_LABEL,
                address.as_slice(),
            )
            .expect("Failed to swap placeholder for bytes metadatum on bridge transfer tx");
        }

        let signed_tx_hex = self
            .wallet
            .sign_tx(&unsigned_tx_hex)
            .expect("Failed to sign bridge transfer tx");
        // Compute the Cardano tx id from the signed body. Using `FixedTransaction`
        // preserves the original CBOR byte ordering, so the hash matches what
        // the node will report when the tx lands.
        let fixed_tx = whisky::csl::FixedTransaction::from_hex(&signed_tx_hex)
            .expect("Failed to parse signed bridge transfer as FixedTransaction");
        let tx_id: [u8; 32] = fixed_tx
            .transaction_hash()
            .to_bytes()
            .try_into()
            .expect("Cardano tx hash must be 32 bytes");
        let signed_tx_bytes = hex::decode(&signed_tx_hex).expect("Failed to decode signed tx hex");
        Ok(SignedBridgeTransaction {
            tx_id,
            signed_tx_bytes,
        })
    }

    /// Submit an already built and signed Cardano transaction (CBOR bytes) via Ogmios.
    pub async fn submit_tx(
        &self,
        signed_tx_bytes: Vec<u8>,
    ) -> Result<SubmitTransactionResponse, OgmiosClientError> {
        let request = OgmiosRequest::SubmitTx {
            tx_bytes: signed_tx_bytes,
        };
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

/// A Cardano transaction that has been built and signed but not yet submitted
/// to the network. Carries both the wire bytes (for `submit_tx`) and the tx hash.
pub struct SignedBridgeTransaction {
    /// 32-byte Cardano tx id (blake2b-256 of the transaction body).
    pub tx_id: [u8; 32],
    /// CBOR-encoded signed transaction, ready to hand to Ogmios.
    pub signed_tx_bytes: Vec<u8>,
}

pub enum BridgeTransferRecipient {
    Address([u8; 32]),
    /// For testing handling of transfers with invalid recipient
    Invalid,
}

/// Whiskey can't encode bytes metadatum. This value is put as metadatum item,
/// to allow proper fee calculation. Later CSL code replaces this placeholder.
const BRIDGE_ADDRESS_METADATUM_PLACEHOLDER_JSON: &str = "\"placeholderplaceholderplaceholde\"";

/// Replace metadatum item with bytes metadatum using pure CSL.
fn replace_with_bytes_metadatum(
    unsigned_tx_hex: &str,
    label: u64,
    bytes: &[u8],
) -> Result<String, whisky::csl::JsError> {
    use whisky::csl;

    let bytes_metadatum = csl::TransactionMetadatum::new_bytes(bytes.to_vec())?;
    let mut list = csl::MetadataList::new();
    list.add(&bytes_metadatum);
    let list_metadatum = csl::TransactionMetadatum::new_list(&list);

    let mut metadata = csl::GeneralTransactionMetadata::new();
    metadata.insert(&csl::BigNum::from(label), &list_metadatum);

    let mut aux_data = csl::AuxiliaryData::new();
    aux_data.set_metadata(&metadata);
    let aux_hash = csl::hash_auxiliary_data(&aux_data);

    let tx = csl::Transaction::from_hex(unsigned_tx_hex)?;
    let mut body = tx.body();
    body.set_auxiliary_data_hash(&aux_hash);

    let new_tx = csl::Transaction::new(&body, &tx.witness_set(), Some(aux_data));
    Ok(new_tx.to_hex())
}
