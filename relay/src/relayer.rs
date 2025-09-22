use crate::{beefy, cardano_encoding::SignedCommitment, mn_meta};

use mn_meta::runtime_types::sp_consensus_beefy::{ecdsa_crypto::Public as MidnBeefyPublic, mmr::BeefyAuthoritySet};
use mmr_rpc::{LeavesProof};
use sp_core::{ Bytes, H256};
use sp_consensus_beefy::{ mmr::MmrLeaf as BeefyMmrLeaf, ecdsa_crypto::Signature as ECDSASig, VersionedFinalityProof};

use sp_mmr_primitives::{
    AncestryProof, EncodableOpaqueLeaf, LeafProof,
    mmr_lib::{helper::get_peaks, leaf_index_to_mmr_size}, utils::NodesUtils
};

use subxt::{backend::rpc::RpcClient, ext::{ codec::Decode, subxt_rpcs::{client::{RpcParams, RpcSubscription}, rpc_params}}, OnlineClient, PolkadotConfig};

pub type Block = u32;
pub type MmrLeaf = BeefyMmrLeaf<Block, H256,H256,Vec<u8>>;
pub type LeafIndex = u64;
pub type PeakNodes = Vec<(LeafIndex,Vec<u64>)>;

pub struct Relayer {
    // Shared RPC client interface for the relayer
    rpc: RpcClient,
    // Shared subxt api client for the relayer
    api: OnlineClient<PolkadotConfig>
}

impl Relayer {
    pub async fn new(node_url: &str) -> Self {
        println!("Connecting to {node_url}");

        // TODO: Handle
        let api = OnlineClient::<PolkadotConfig>::from_insecure_url(node_url).await
        .expect("Online Client failed to connect");
        let rpc = RpcClient::from_url(node_url).await.expect("RPC Client failed to connect");
        Relayer { rpc, api }
    }

    pub async fn new_with_keys_file(node_url: &str, key_file_path: String) -> Self {
		let relayer = Self::new(node_url).await;

		// reading keys from file
		let beefy_key_infos = beefy::keys_from_file(&key_file_path);

		// insert each key to keystore
		for key_info in beefy_key_infos {
			key_info.insert_key(&relayer.rpc).await;
		}

		relayer
	}

    pub async fn run_relay(&self) {
        let mut sub: RpcSubscription<Bytes>  = self.rpc.subscribe(
            "beefy_subscribeJustifications", rpc_params![], "beefy_unsubscribeJustifications")
            .await.expect("beefy subsciption failed: ");

        while let Some(result) = sub.next().await {
            let justification =  result.expect("failed to get justification");
        
            self.handle_justification_stream_data(justification).await;
        }
    }

    async fn handle_justification_stream_data(&self, justification: Bytes) {
        let VersionedFinalityProof::<Block,ECDSASig>::V1(beef_signed_commitment) = Decode::decode (&mut &justification[..]).expect("failed to parse to VersionedFinalityProof");

        let block = beef_signed_commitment.commitment.block_number;
        println!("Block Number: {block}");

        // TODO: decide whether you want to specify a block hash as of a certain time. Otherwise, `None` indicates latest (finalized?) block hash
        let at_latest_block_hash = None;

        let validator_set = self.get_beefy_validator_set(at_latest_block_hash).await;

        let validators_as_hex : Vec<String> = validator_set.iter().map(|validator| {
            hex::encode(validator.0)
        }).collect();

        println!("Beefy Validator set: {:#?}", validators_as_hex);

        let signed_commitment = SignedCommitment::from_signed_commitment_and_validators(
            beef_signed_commitment, validator_set);

        // TODO: get encodings for MMR proof, then do something with it
        let mmr_proof = self.get_mmr_proof(block, at_latest_block_hash).await; 

        let leaf_proof = get_decoded_proof(&mmr_proof);
        let peak_nodes = get_peak_nodes(&leaf_proof);

        let block_hash = mmr_proof.block_hash;
        let hex_block_hash = hex::encode(block_hash);
        println!("block hash: {hex_block_hash}");
       
        get_leaves(&mmr_proof);

        self.check_proof(block_hash, &leaf_proof.items, peak_nodes).await;

        // TODO: get encodings for authority set, next authority set, and construct       
        let authority_set = self.get_beefy_authority_set(at_latest_block_hash).await;
        let next_authority_set = self.get_next_beefy_authority_set(at_latest_block_hash).await;
   
        // TODO: turn into Cardano-friendly version
        // let relay_chain_proof = RelayChainProof {
        //     mmr_proof,
        //     signed_commitment
        // };
   }

    async fn get_mmr_proof(&self,
        // The block to query for
        block: u32,
        // The block hash representing the chain as of the time you care about. Note this is different from the above.
        // If unclear, use the same block hash that you use for other queries on the same data
        at_block_hash: Option<H256>
    ) -> LeavesProof<H256> {
            let mut params = RpcParams::new();

            let params = rpc_params![
                [block],
                 // TODO: do we need specificity to these? The storage might change 
                 None::<u64>,
                 at_block_hash
            ];

            // TODO: handle
            let raw_proof_data = self.rpc.request_raw(
                "mmr_generateProof", params.build())
                .await.expect("failed to ger raw proof data");

          serde_json::from_str(raw_proof_data.get())
            .expect("failed to parse raw proof")
    }

    async fn get_beefy_validator_set(&self, at_block_hash: Option<H256>) -> Vec<MidnBeefyPublic> {
        let beefy_validator_set_query = mn_meta::storage().beefy().authorities();

        let storage_fetcher = match at_block_hash {
            Some(block_hash) => self.api.storage().at(block_hash),
            None => self.api.storage().at_latest().await.expect("failed to get latest storage")
        };

        let Some(validator_set) = storage_fetcher.fetch(&beefy_validator_set_query)
        .await.expect("failed to get validator set") else {
            println!("WARN: no validator set found");
            return vec![];
        };

        validator_set.0
    }

    async fn check_proof(&self, at_block_hash:H256, proof_items:&Vec<H256>, peak_nodes:PeakNodes) {
        for (leaf_index, peaks) in peak_nodes {
            for peak in peaks {
                let mmr_nodes_query = mn_meta::storage().mmr().nodes(peak);

                 let storage_fetcher = self.api.storage().at(at_block_hash);

                 match storage_fetcher.fetch(&mmr_nodes_query).await.expect("failed to get mmr nodes") {
                    Some(node_hash) => {
                        let result = proof_items.contains(&node_hash);
                        let hash_as_hex = hex::encode(node_hash);

                        println!("LeafIndex({leaf_index}): Node Peak({peak})({hash_as_hex}) in proof: {result}");
                    },
                    None => println!("LeafIndex({leaf_index}): Node({peak}) not found")
                 }
            };    
        }
    }

    // Below are for authority set proofs
    async fn get_beefy_authority_set(&self, at_block_hash: Option<H256>) -> BeefyAuthoritySet<H256> {
        let get_authority_set_query = mn_meta::storage().beefy_mmr_leaf().beefy_authorities();

          let storage_fetcher = match at_block_hash {
            Some(block_hash) => self.api.storage().at(block_hash),
            None => self.api.storage().at_latest().await.expect("failed to get latest storage")
        };

        storage_fetcher.fetch(&get_authority_set_query)
            .await.expect("failed to get authority set").expect("No BeefyAuthoritySet found")
    }

    async fn get_next_beefy_authority_set(&self, at_block_hash: Option<H256>) -> BeefyAuthoritySet<H256> {
        let get_next_authority_set_query = mn_meta::storage().beefy_mmr_leaf().beefy_next_authorities();

        let storage_fetcher = match at_block_hash {
            Some(block_hash) => self.api.storage().at(block_hash),
            None => self.api.storage().at_latest().await.expect("failed to get latest storage")
        };

        storage_fetcher.fetch(&get_next_authority_set_query)
            .await.expect("failed to get next authority set").expect("No Next BeefyAuthoritySet found")
    }

    // failed
    async fn get_ancestry_proof(&self, previous_block:Block, best_known_block_number:Option<Block>) {
       let gen_ancestry_proof =  mn_meta::runtime_apis::beefy_api::BeefyApi.generate_ancestry_proof(previous_block, best_known_block_number);

       let runtime_api = self.api.runtime_api().at_latest().await
            .expect("failed to get runtime api");

        let opaq_ancestry_proof = runtime_api.call(gen_ancestry_proof).await
            .expect("failed to query ancestry proof").expect("No ancestry proof found");

        let ancestry_proof: AncestryProof<H256> = Decode::decode(&mut &opaq_ancestry_proof.0[..]).expect("failed to decode to AncestryProof");

       println!("\nANCESTRY PROOF: {ancestry_proof:?}");

    }
}


fn get_decoded_proof(leaves_proof:&LeavesProof<H256>) -> LeafProof<H256> {
    let encoded_mmr_proof = &leaves_proof.proof;

    let hex_mmr_proof = hex::encode(&encoded_mmr_proof.0);
    println!("hex mmr proof: {hex_mmr_proof}");

    Decode::decode(&mut &encoded_mmr_proof.0[..]).expect("Failed to decode to LeafProof")
}

// List of (Leaf Indices, and the peaks)
fn get_peak_nodes(leaf_proof:&LeafProof<H256>) -> PeakNodes {
    println!("decoded proof: {leaf_proof:#?}");

    leaf_proof.leaf_indices.iter().map(|leaf_index| {
        let mmr_size = leaf_index_to_mmr_size(*leaf_index);
        let peaks = get_peaks(mmr_size);

        let utils = NodesUtils::new(*leaf_index) ;

        let peak_len = utils.number_of_peaks();
        println!("\nNumber of peaks {peak_len}: of leaf index({leaf_index}) with mmr size({mmr_size})");

        (*leaf_index, peaks)

    }).collect()
}

fn get_leaves(leaves_proof:&LeavesProof<H256>) {
    let leaves:Vec<EncodableOpaqueLeaf> = Decode::decode(&mut &leaves_proof.leaves.0[..])
            .expect("failed to convert to mmrleaf");

    for leaf in leaves {
        let leaf_as_bytes = leaf.into_opaque_leaf().0;
        let mmr_leaf: MmrLeaf = Decode::decode(&mut &leaf_as_bytes[..])
        .expect("failed to decode to mmrleaf");

        println!("The MMR Leaf: {mmr_leaf:#?}");
    }
}