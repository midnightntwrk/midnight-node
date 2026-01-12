use crate::config::NodeClientSettings;
use blake2::digest::{Update, VariableOutput};
use blake2::Blake2bVar;
use hex::ToHex;
use midnight_node_ledger_helpers::{DefaultDB, DustWallet, WalletSeed, serialize_untagged};
use midnight_node_metadata::midnight_metadata_latest::c_night_observation::storage::types::utxo_owners::UtxoOwners;
use midnight_node_metadata::midnight_metadata_latest::federated_authority_observation::events::{CouncilMembersReset, TechnicalCommitteeMembersReset};
use midnight_node_metadata::midnight_metadata_latest::runtime_types::midnight_primitives_cnight_observation::ObservedUtxo;
use midnight_node_metadata::midnight_metadata_latest::{
	self as mn_meta,
	c_night_observation::{self}
	,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use subxt::backend::rpc::RpcClient;
use subxt::blocks::ExtrinsicEvents;
use subxt::ext::subxt_rpcs::rpc_params;
use subxt::tx::TxProgress;
use subxt::utils::H256;
use subxt::{OnlineClient, SubstrateConfig};
use tokio::time::{sleep, timeout, Instant};

/// D-Parameter response from RPC
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DParameterResponse {
    /// Number of permissioned candidates
    pub num_permissioned_candidates: u16,
    /// Number of registered candidates
    pub num_registered_candidates: u16,
}

/// Ariadne parameters response from systemParameters_getAriadneParameters RPC
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AriadneParametersResponse {
    /// The D-parameter (from pallet-system-parameters)
    pub d_parameter: DParameterResponse,
    /// List of permissioned candidates from Cardano Aiken contracts
    pub permissioned_candidates: Option<Vec<serde_json::Value>>,
    /// Map of candidate registrations
    pub candidate_registrations: serde_json::Value,
}

pub struct MidnightClient {
    pub online_client: OnlineClient<SubstrateConfig>,
    rpc_client: RpcClient,
}

impl MidnightClient {
    pub async fn new(node_settings: NodeClientSettings) -> Self {
        let online_client =
            OnlineClient::<SubstrateConfig>::from_insecure_url(&node_settings.base_url)
                .await
                .expect("Failed to initialize online client");
        let rpc_client = RpcClient::from_insecure_url(&node_settings.base_url)
            .await
            .expect("Failed to initialize RPC client");
        Self {
            online_client,
            rpc_client,
        }
    }

    pub fn new_seed() -> WalletSeed {
        let seed_bytes: [u8; 32] = rand::random();
        println!("Midnight seed: {}", hex::encode(seed_bytes));
        WalletSeed::from(seed_bytes)
    }

    pub fn new_dust_hex(wallet_seed: WalletSeed) -> String {
        let dust_wallet = DustWallet::<DefaultDB>::default(wallet_seed, None);
        let dust_public = dust_wallet.public_key;
        let mut dust_bytes = serialize_untagged(&dust_public).unwrap();
        if dust_bytes.len() == 32 {
            dust_bytes.push(0);
        }
        let dust_public_hex = dust_bytes.encode_hex::<String>();
        println!("Dust public key hex: {}", dust_public_hex);
        dust_public_hex
    }

    pub async fn subscribe_to_cnight_observation_events(
        &self,
        tx_id: &[u8],
    ) -> Result<ExtrinsicEvents<SubstrateConfig>, Box<dyn std::error::Error>> {
        println!(
            "Subscribing for cNIGHT observation extrinsic with tx_id: 0x{}",
            hex::encode(tx_id)
        );
        let mut blocks_sub = self.online_client.blocks().subscribe_finalized().await?;

        let inner = async {
            while let Some(block_result) = blocks_sub.next().await {
                let block = block_result?;

                let block_number = block.header().number;
                println!("Finalized block #{}", block_number);

                let extrinsic = block.extrinsics().await?;

                for ext in extrinsic.iter() {
                    let Ok(decoded) = ext.as_root_extrinsic::<mn_meta::Call>() else {
                        continue;
                    };

                    let Some(utxos) = MidnightClient::extract_process_tokens_utxos(&decoded) else {
                        continue;
                    };

                    println!(
                        "  NativeTokenObservation::process_tokens called with {} UTXOs",
                        utxos.len()
                    );

                    if utxos.is_empty() {
                        continue;
                    }

                    if utxos.iter().any(|u| u.header.tx_hash.0 == tx_id) {
                        println!(
                            "*** Found UTXO with matching registration tx hash: 0x{} ***",
                            hex::encode(tx_id)
                        );
                        let events = ext.events().await?;
                        return Ok(events);
                    } else {
                        for u in utxos {
                            let seen = u.header.tx_hash.0;
                            println!(
                                "Tx hash 0x{} does not match expected registration tx hash 0x{}",
                                hex::encode(seen),
                                hex::encode(tx_id)
                            );
                        }
                    }
                }
            }
            Err("Did not find registration event".into())
        };

        timeout(Duration::from_secs(60), inner)
            .await
            .unwrap_or_else(|_| Err("Timeout waiting for registration event".into()))
    }

    pub fn calculate_nonce(prefix: &[u8], tx_hash: [u8; 32], tx_index: u16) -> String {
        let mut hasher = Blake2bVar::new(32).expect("valid output size");

        hasher.update(prefix);
        hasher.update(&tx_hash);
        hasher.update(&tx_index.to_be_bytes());

        let mut out = [0u8; 32];
        hasher
            .finalize_variable(&mut out)
            .expect("finalize succeeds");
        hex::encode(out)
    }

    fn extract_process_tokens_utxos(call: &mn_meta::Call) -> Option<&Vec<ObservedUtxo>> {
        match call {
            mn_meta::Call::CNightObservation(c_night_observation::Call::process_tokens {
                utxos,
                ..
            }) => Some(utxos),
            _ => None,
        }
    }

    pub async fn query_night_utxo_owners(
        &self,
        utxo: String,
    ) -> Result<Option<UtxoOwners>, Box<dyn std::error::Error>> {
        let nonce = hex::decode(&utxo).unwrap();
        let storage_address = mn_meta::storage()
            .c_night_observation()
            .utxo_owners(H256(nonce.try_into().unwrap()));

        let owners = self
            .online_client
            .storage()
            .at_latest()
            .await?
            .fetch(&storage_address)
            .await?;

        Ok(owners)
    }

    pub async fn poll_utxo_owners_until_change(
        &self,
        utxo: String,
        initial_value: Option<UtxoOwners>,
        timeout_secs: u64,
        poll_interval_ms: u64,
    ) -> Result<Option<UtxoOwners>, Box<dyn std::error::Error>> {
        let start = Instant::now();
        loop {
            let current_value = self.query_night_utxo_owners(utxo.clone()).await?;
            if current_value.as_ref().map(|v| v.0.0.clone())
                != initial_value.as_ref().map(|v| v.0.0.clone())
            {
                println!("UtxoOwners storage changed: {:?}", current_value);
                return Ok(current_value);
            }
            if start.elapsed() > Duration::from_secs(timeout_secs) {
                println!("Timeout reached without change");
                return Ok(current_value);
            }
            sleep(Duration::from_millis(poll_interval_ms)).await;
        }
    }

    pub async fn subscribe_to_federated_authority_events(
        &self,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("Subscribing to federated authority observation events");

        let mut blocks_sub = self.online_client.blocks().subscribe_finalized().await?;

        let result = timeout(Duration::from_secs(120), async {
            while let Some(block) = blocks_sub.next().await {
                let block = block?;
                let block_number = block.header().number;
                println!("Checking block #{block_number} for federated authority events");

                let events = block.events().await?;

                // Check for CouncilMembersReset event
                let council_reset = events.find::<CouncilMembersReset>().flatten().next();

                // Check for TechnicalCommitteeMembersReset event
                let tech_committee_reset = events
                    .find::<TechnicalCommitteeMembersReset>()
                    .flatten()
                    .next();

                if let Some(event) = &council_reset {
                    println!(
                        "✓ Found CouncilMembersReset event with {} members",
                        event.members.len()
                    );
                }
                if let Some(event) = &tech_committee_reset {
                    println!(
                        "✓ Found TechnicalCommitteeMembersReset event with {} members",
                        event.members.len()
                    );
                }

                if council_reset.is_some() && tech_committee_reset.is_some() {
                    return Ok(());
                }
            }
            Err("Did not find all federated authority events".into())
        })
        .await;

        result.unwrap_or_else(|_| Err("Timeout waiting for federated authority events".into()))
    }

    /// Get the current D-Parameter via RPC.
    ///
    /// Returns the number of permissioned and registered candidates.
    pub async fn get_d_parameter(&self) -> Result<DParameterResponse, Box<dyn std::error::Error>> {
        let response: DParameterResponse = self
            .rpc_client
            .request("systemParameters_getDParameter", rpc_params![])
            .await?;

        Ok(response)
    }

    /// Get the D-Parameter at a specific block hash.
    pub async fn get_d_parameter_at(
        &self,
        block_hash: H256,
    ) -> Result<DParameterResponse, Box<dyn std::error::Error>> {
        let response: DParameterResponse = self
            .rpc_client
            .request("systemParameters_getDParameter", rpc_params![block_hash])
            .await?;

        Ok(response)
    }

    /// Get the current best block hash from the node.
    pub async fn get_best_block_hash(&self) -> Result<H256, Box<dyn std::error::Error>> {
        let block = self.online_client.blocks().at_latest().await?;
        Ok(block.hash())
    }

    /// Get block hash at a specific block height/number.
    pub async fn get_block_hash_at_height(
        &self,
        block_number: u32,
    ) -> Result<H256, Box<dyn std::error::Error>> {
        let block_hash: Option<H256> = self
            .rpc_client
            .request("chain_getBlockHash", rpc_params![block_number])
            .await?;

        block_hash.ok_or_else(|| format!("No block found at height {}", block_number).into())
    }

    /// Wait for a new finalized block and return its hash.
    pub async fn wait_for_next_finalized_block(&self) -> Result<H256, Box<dyn std::error::Error>> {
        let mut blocks_sub = self.online_client.blocks().subscribe_finalized().await?;

        let result = timeout(Duration::from_secs(30), async {
            if let Some(block_result) = blocks_sub.next().await {
                let block = block_result?;
                println!("New finalized block #{}", block.header().number);
                return Ok(block.hash());
            }
            Err("No block received".into())
        })
        .await;

        result.unwrap_or_else(|_| Err("Timeout waiting for finalized block".into()))
    }

    /// Get Ariadne parameters including permissioned candidates and D-Parameter.
    ///
    /// The D-Parameter is sourced from pallet-system-parameters (on-chain),
    /// while permissioned candidates come from Cardano Aiken contracts.
    pub async fn get_ariadne_parameters(
        &self,
        epoch_number: u64,
        d_parameter_at: Option<H256>,
    ) -> Result<AriadneParametersResponse, Box<dyn std::error::Error>> {
        let response: AriadneParametersResponse = match d_parameter_at {
            Some(hash) => {
                self.rpc_client
                    .request(
                        "systemParameters_getAriadneParameters",
                        rpc_params![epoch_number, hash],
                    )
                    .await?
            }
            None => {
                self.rpc_client
                    .request(
                        "systemParameters_getAriadneParameters",
                        rpc_params![epoch_number],
                    )
                    .await?
            }
        };

        Ok(response)
    }

    // ========== Midnight Transaction Submission Methods ==========
    // Used for DDoS mitigation E2E tests (TC-0003-06)

    /// Submit a raw Midnight transaction and watch for result.
    /// Returns the transaction progress if submission succeeds.
    pub async fn submit_midnight_tx(
        &self,
        tx_bytes: Vec<u8>,
    ) -> Result<TxProgress<SubstrateConfig, OnlineClient<SubstrateConfig>>, subxt::Error> {
        let mn_tx = mn_meta::tx().midnight().send_mn_transaction(tx_bytes);
        let unsigned_extrinsic = self.online_client.tx().create_unsigned(&mn_tx)?;
        unsigned_extrinsic.submit_and_watch().await
    }

    /// Submit a Midnight transaction expecting it to be rejected at pre_dispatch.
    /// Returns Ok(error_message) if rejected as expected.
    /// Returns Err if the transaction was unexpectedly accepted.
    pub async fn submit_expecting_rejection(
        &self,
        tx_bytes: Vec<u8>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        println!("Submitting transaction expecting rejection...");
        match self.submit_midnight_tx(tx_bytes).await {
            Err(e) => {
                println!("Transaction rejected as expected: {}", e);
                Ok(e.to_string())
            }
            Ok(_) => Err(
                "Transaction was unexpectedly accepted - should have been rejected at pre_dispatch"
                    .into(),
            ),
        }
    }

    /// Submit a Midnight transaction expecting it to succeed.
    /// Waits for the transaction to be included in a block.
    pub async fn submit_expecting_success(
        &self,
        tx_bytes: Vec<u8>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("Submitting transaction expecting success...");
        let mut progress = self.submit_midnight_tx(tx_bytes).await?;

        // Wait for inclusion in block
        while let Some(status) = progress.next().await {
            match status? {
                subxt::tx::TxStatus::InBestBlock(block_info) => {
                    println!(
                        "Transaction included in best block: {:?}",
                        block_info.block_hash()
                    );
                    return Ok(());
                }
                subxt::tx::TxStatus::InFinalizedBlock(block_info) => {
                    println!(
                        "Transaction finalized in block: {:?}",
                        block_info.block_hash()
                    );
                    return Ok(());
                }
                subxt::tx::TxStatus::Error { message } => {
                    return Err(format!("Transaction error: {}", message).into());
                }
                subxt::tx::TxStatus::Invalid { message } => {
                    return Err(format!("Transaction invalid: {}", message).into());
                }
                subxt::tx::TxStatus::Dropped { message } => {
                    return Err(format!("Transaction dropped: {}", message).into());
                }
                _ => {
                    // Continue waiting for other statuses
                }
            }
        }
        Err("Transaction progress ended without confirmation".into())
    }
}
