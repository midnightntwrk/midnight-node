use crate::cardano_encoding::{Commitment as DCommitment, Payload, RelayChainProof, SignedCommitment, ToPlutusData, Vote};
use midnight_node_res::subxt_metadata::api::{self as mn_meta, runtime_types::sp_consensus_beefy::mmr::BeefyAuthoritySet, system::storage::types::block_hash};

use mn_meta::runtime_types::sp_consensus_beefy::ecdsa_crypto::Public as BeefyRuntimePublic;

use futures::StreamExt;
use sp_consensus_beefy::{ecdsa_crypto, known_payloads::{self, MMR_ROOT_ID}, BeefyAuthorityId, Commitment, VersionedFinalityProof};
use sc_consensus_beefy::justification::BeefyVersionedFinalityProof;
use subxt::{
    backend::rpc::RpcClient,
    rpc_params, OnlineClient, PolkadotConfig
};
use parity_scale_codec::{Encode, Decode};
use thiserror::Error;

use mmr_rpc::LeavesProof;
use subxt::{
    backend::rpc::RpcParams,
    utils::H256
};

use serde::Serialize;

#[cfg(feature = "std")]
use midnight_node_runtime::Block;

#[derive(Serialize)]
struct JustificationOutput {
    commitment_payload: String,
    block_number: u32,
    validator_set: Vec<String>,
    signatures: Vec<Option<String>>,
}


#[derive(Debug, Error)]
enum RelayerError {
    #[error("stream error: {0}")]
    Stream(#[from] subxt::Error),
    #[error("decode error: {0}")]
    Decode(#[from] parity_scale_codec::Error),
    #[error("Unsupported BEEFY version")]
    UnsupportedBeefyVersion,
}

pub struct Relayer {
    // Shared RPC client interface for the relayer
    rpc: RpcClient,
    // Shared subxt api client for the relayer
    api: OnlineClient<PolkadotConfig>
}

impl Relayer {
    pub async fn new(node_url: &str) -> Self {
        // TODO: Handle
        let api = OnlineClient::<PolkadotConfig>::from_insecure_url(node_url).await.unwrap();
        let rpc = RpcClient::from_url(node_url).await.unwrap();
        Relayer { rpc, api }
    }

    pub async fn run_relay(&self) {
        // TODO: Handle
        let mut sub: subxt::backend::rpc::RpcSubscription<sp_core::Bytes> = self.rpc.subscribe(
            "beefy_subscribeJustifications",
            // No params are needed for this rpc method
            rpc_params![],
            "beefy_unsubscribeJustifications",
        ).await.unwrap();
    
        // Main loop: subscribe to justifications stream so we can act as soon as we get any justification. These are available on each epoch progression, 
        // as well as several times over the span of an epoch
        while let Some(justification) = sub.next().await {
            let justification: Result<sp_core::Bytes, RelayerError> =
            justification.map_err(RelayerError::from); 
            self.handle_justification_stream_data(justification).await;
        }
    }

    async fn handle_justification_stream_data(&self, justification_data: Result<sp_core::Bytes, RelayerError>) -> Result<(), RelayerError>  {
        let justification_data = justification_data?;
        let finality_proof: BeefyVersionedFinalityProof<Block, ecdsa_crypto::AuthorityId> = Decode::decode(&mut &justification_data[..])?;

        let signed_commitment = if let VersionedFinalityProof::V1(signed_commitment) = finality_proof {
            signed_commitment
        } else {
            return Err(RelayerError::UnsupportedBeefyVersion)
        };

        let block = signed_commitment.clone().commitment.block_number;

        // TODO: decide whether you want to specify a block hash as of a certain time. Otherwise, `None` indicates latest (finalized?) block hash
        let at_latest_block_hash = None;

        let validator_set = self.get_beefy_validator_set(at_latest_block_hash).await;

        let hex_keys: Vec<String> = validator_set.iter().map(|v| hex::encode(v.0)).collect();
        println!("Beefy validator set: {:?}", hex_keys);

        let signed_commitment = SignedCommitment::from_signed_commitment_and_validators(
            signed_commitment,
            validator_set
        );

        // TODO: get encodings for MMR proof, then do something with it
        let mmr_proof = self.get_mmr_proof(block, at_latest_block_hash).await;

        // TODO: get encodings for authority set, next authority set, and construct 
        let authority_set = self.get_beefy_authority_set(at_latest_block_hash).await.unwrap();
        let next_authority_set = self.get_next_beefy_authority_set(at_latest_block_hash).await.unwrap();

        // TODO: turn into Cardano-friendly version
        // let relay_chain_proof = RelayChainProof {
        //     mmr_proof,
        //     signed_commitment
        // };

        Ok(())
    }

    // Get the BEEFY validator set
    pub async fn get_beefy_validator_set(&self, at_block_hash: Option<H256>) -> Vec<BeefyRuntimePublic> {
        let beefy_validator_set_query = mn_meta::storage().beefy().authorities();

        let storage_fetcher = if let Some(block_hash) = at_block_hash {
            self.api.storage().at(block_hash)
        } else {
            self.api.storage().at_latest().await.unwrap()
        };

        // TODO: handle
        let validator_set =
            storage_fetcher.fetch(&beefy_validator_set_query).await.unwrap().unwrap();
        validator_set.0
    }
    
    // Get the MMR root hash
    pub async fn get_mmr_root(&self, at_block_hash: Option<H256>) -> H256 {
        let mmr_root_query = mn_meta::storage().mmr().root_hash();

        let storage_fetcher = if let Some(block_hash) = at_block_hash {
            self.api.storage().at(block_hash)
        } else {
            self.api.storage().at_latest().await.unwrap()
        };

        // TODO: handle
        let mmr_root = storage_fetcher.fetch(&mmr_root_query).await.unwrap().unwrap();
        mmr_root
    }

    pub async fn get_mmr_proof(
        &self,
        // The block to query for
        block: u32,
        // The block hash representing the chain as of the time you care about. Note this is different from the above.
        // If unclear, use the same block hash that you use for other queries on the same data
        at_block_hash: Option<H256>
    ) -> LeavesProof<sp_core::H256> {
        let mut params = RpcParams::new();
    
        let params = rpc_params![
            [block],
            // TODO: do we need specificity to these? The storage might change 
            None::<u64>,
            at_block_hash
        ];

        // TODO: handle
        let raw_proof_data = self.rpc.request_raw(
            "mmr_generateProof",
            params.build()
        ).await.unwrap();

        let proof: LeavesProof<sp_core::H256> = serde_json::from_str(raw_proof_data.get()).unwrap();
        proof
    }

    async fn get_num_leaves(&self, at_block_hash: Option<H256>) -> Option<u64> {
        let num_leaves_query = mn_meta::storage().mmr().number_of_leaves();

        let storage_fetcher = if let Some(block_hash) = at_block_hash {
            self.api.storage().at(block_hash)
        } else {
            self.api.storage().at_latest().await.unwrap()
        };

        let num_leaves = storage_fetcher.fetch(&num_leaves_query).await.unwrap();
        num_leaves
    }

    // Below are for authority set proofs
    async fn get_beefy_authority_set(&self, at_block_hash: Option<H256>) -> Option<BeefyAuthoritySet<H256>> {
        let get_authority_set_query = mn_meta::storage().beefy_mmr_leaf().beefy_authorities();

        let storage_fetcher = if let Some(block_hash) = at_block_hash {
            self.api.storage().at(block_hash)
        } else {
            self.api.storage().at_latest().await.unwrap()
        };

        let authority_set = storage_fetcher.fetch(&get_authority_set_query).await.unwrap();
        authority_set
    }
    
    async fn get_next_beefy_authority_set(&self, at_block_hash: Option<H256>) -> Option<BeefyAuthoritySet<H256>> {
        let get_next_authority_set_query = mn_meta::storage().beefy_mmr_leaf().beefy_next_authorities();

        let storage_fetcher = if let Some(block_hash) = at_block_hash {
            self.api.storage().at(block_hash)
        } else {
            self.api.storage().at_latest().await.unwrap()
        };

        let authority_set = storage_fetcher.fetch(&get_next_authority_set_query).await.unwrap();
        authority_set
    }
}
