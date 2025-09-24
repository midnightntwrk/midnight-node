use crate::{
	Block,
	beefy::{self, BeefyRelayChainProof, PeakNodes},
	mn_meta,
};

use mmr_rpc::LeavesProof;
use mn_meta::runtime_types::sp_consensus_beefy::{
	ecdsa_crypto::Public as MidnBeefyPublic, mmr::BeefyAuthoritySet,
};
use sp_consensus_beefy::{
	SignedCommitment as BeefySignedCommitment, VersionedFinalityProof,
	ecdsa_crypto::Signature as ECDSASig,
};
use sp_core::{Bytes, H256};
use subxt::{
	OnlineClient, PolkadotConfig,
	backend::rpc::RpcClient,
	ext::{
		codec::Decode,
		subxt_rpcs::{
			client::{RpcParams, RpcSubscription},
			rpc_params,
		},
	},
};

pub struct Relayer {
	// Shared RPC client interface for the relayer
	rpc: RpcClient,
	// Shared subxt api client for the relayer
	api: OnlineClient<PolkadotConfig>,
}

impl Relayer {
	pub async fn new(node_url: &str) -> Self {
		println!("Connecting to {node_url}");

		// TODO: Handle
		let api = OnlineClient::<PolkadotConfig>::from_insecure_url(node_url)
			.await
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
		let mut sub: RpcSubscription<Bytes> = self
			.rpc
			.subscribe(
				"beefy_subscribeJustifications",
				rpc_params![],
				"beefy_unsubscribeJustifications",
			)
			.await
			.expect("beefy subsciption failed: ");

		while let Some(result) = sub.next().await {
			let justification = result.expect("failed to get justification");

			let proof = self.handle_justification_stream_data(justification).await;
			println!("------------- THE PROOF: {proof:?}");
			let commitment_hash = proof.hex_scale_encoded_signed_commitment();
			println!("Commitment Hash: {commitment_hash}");
			let _ = proof.mmr_root_hash();
			let _ = proof.mmr_leaves();
			let _ = proof.peak_nodes();
		}
	}

	async fn handle_justification_stream_data(&self, justification: Bytes) -> BeefyRelayChainProof {
		let VersionedFinalityProof::<Block, ECDSASig>::V1(beef_signed_commitment) =
			Decode::decode(&mut &justification[..])
				.expect("failed to parse to VersionedFinalityProof");

		let mmr_proof = self.get_consensus_proof(&beef_signed_commitment).await;
		let at_block_hash = Some(mmr_proof.block_hash);

		let validator_set = self.get_beefy_validator_set(at_block_hash).await;

		// TODO: get encodings for authority set, next authority set, and construct
		let authority_set = self.get_beefy_authority_set(at_block_hash).await;
		let next_authority_set = self.get_next_beefy_authority_set(at_block_hash).await;

		BeefyRelayChainProof {
			consensus_proof: mmr_proof,
			//todo
			authority_proof: (),
			signed_commitment: beef_signed_commitment,
			validator_set,
		}
	}

	pub async fn get_consensus_proof(
		&self,
		beefy_signed_commitment: &BeefySignedCommitment<Block, ECDSASig>,
	) -> LeavesProof<H256> {
		let block = beefy_signed_commitment.commitment.block_number;
		println!("Block Number: {block}");

		let at_block_hash = self.get_block_hash(block).await;

		self.get_mmr_proof(block, at_block_hash).await
	}

	pub async fn verify_proof(
		&self,
		root_hash: H256,
		leaves_proof: LeavesProof<H256>,
	) -> Option<bool> {
		let root_hash_as_hex = hex::encode(root_hash);

		let mut rpc_params = RpcParams::new();
		rpc_params.push(root_hash_as_hex).expect("could not insert root hash in params");
		rpc_params.push(leaves_proof).expect("could not insert leaves proof in params");

		self.rpc
			.request::<Option<bool>>("mmr_verifyProofStateless", rpc_params)
			.await
			.expect("failed to get result of verifying proof")
	}

	pub async fn check_proof_items(
		&self,
		at_block_hash: H256,
		proof_items: &Vec<H256>,
		peak_nodes: PeakNodes,
	) {
		for (leaf_index, peaks) in peak_nodes {
			for peak in peaks {
				let mmr_nodes_query = mn_meta::storage().mmr().nodes(peak);
				let storage_fetcher = self.api.storage().at(at_block_hash);

				match storage_fetcher
					.fetch(&mmr_nodes_query)
					.await
					.expect("failed to get mmr nodes")
				{
					Some(node_hash) => {
						let result = proof_items.contains(&node_hash);
						let hash_as_hex = hex::encode(node_hash);

						println!(
							"LeafIndex({leaf_index}): Node Peak({peak})({hash_as_hex}) in proof: {result}"
						);
					},
					None => println!("LeafIndex({leaf_index}): Node({peak}) not found"),
				}
			}
		}
	}

	async fn get_mmr_proof(
		&self,
		// The block to query for
		block: u32,
		at_block_hash: Option<H256>,
	) -> LeavesProof<H256> {
		let params = rpc_params![
			[block],
			// TODO: do we need specificity to these? The storage might change
			None::<u64>,
			at_block_hash
		];

		// TODO: handle
		let raw_proof_data = self
			.rpc
			.request_raw("mmr_generateProof", params.build())
			.await
			.expect("failed to get raw proof data");

		serde_json::from_str(raw_proof_data.get()).expect("failed to parse raw proof")
	}

	// getting
	async fn current_mmr_root(&self, block_hash: H256) {
		let hex_block_hash = hex::encode(block_hash);
		let params = rpc_params![hex_block_hash.clone()];

		match self.rpc.request::<String>("mmr_root", params).await {
			Ok(root) => println!("Root Hash: {root}"),
			Err(e) => {
				println!("Warning: failed to get mmr proof of block hash({hex_block_hash}): {e:?}")
			},
		}
	}

	async fn get_beefy_validator_set(&self, at_block_hash: Option<H256>) -> Vec<MidnBeefyPublic> {
		let beefy_validator_set_query = mn_meta::storage().beefy().authorities();

		let storage_fetcher = match at_block_hash {
			Some(block_hash) => self.api.storage().at(block_hash),
			None => self.api.storage().at_latest().await.expect("failed to get latest storage"),
		};

		let Some(validator_set) = storage_fetcher
			.fetch(&beefy_validator_set_query)
			.await
			.expect("failed to get validator set")
		else {
			println!("WARN: no validator set found");
			return vec![];
		};

		let validators_as_hex: Vec<String> =
			validator_set.0.iter().map(|validator| hex::encode(validator.0)).collect();
		println!("All Validators: {validators_as_hex:#?}");

		validator_set.0
	}

	// Below are for authority set proofs
	async fn get_beefy_authority_set(
		&self,
		at_block_hash: Option<H256>,
	) -> BeefyAuthoritySet<H256> {
		let get_authority_set_query = mn_meta::storage().beefy_mmr_leaf().beefy_authorities();

		let storage_fetcher = match at_block_hash {
			Some(block_hash) => self.api.storage().at(block_hash),
			None => self.api.storage().at_latest().await.expect("failed to get latest storage"),
		};

		storage_fetcher
			.fetch(&get_authority_set_query)
			.await
			.expect("failed to get authority set")
			.expect("No BeefyAuthoritySet found")
	}

	async fn get_next_beefy_authority_set(
		&self,
		at_block_hash: Option<H256>,
	) -> BeefyAuthoritySet<H256> {
		let get_next_authority_set_query =
			mn_meta::storage().beefy_mmr_leaf().beefy_next_authorities();

		let storage_fetcher = match at_block_hash {
			Some(block_hash) => self.api.storage().at(block_hash),
			None => self.api.storage().at_latest().await.expect("failed to get latest storage"),
		};

		storage_fetcher
			.fetch(&get_next_authority_set_query)
			.await
			.expect("failed to get next authority set")
			.expect("No Next BeefyAuthoritySet found")
	}

	async fn get_block_hash(&self, block: Block) -> Option<H256> {
		let params = rpc_params![block];
		let block_hash: Option<H256> = self
			.rpc
			.request("chain_getBlockHash", params)
			.await
			.expect("chain_getBlockHash failed");

		block_hash
	}

	// failed
	// async fn get_ancestry_proof(&self, previous_block:Block, best_known_block_number:Option<Block>) {
	//    let gen_ancestry_proof =  mn_meta::runtime_apis::beefy_api::BeefyApi.generate_ancestry_proof(previous_block, best_known_block_number);

	//    let runtime_api = self.api.runtime_api().at_latest().await
	//         .expect("failed to get runtime api");

	//     let opaq_ancestry_proof = runtime_api.call(gen_ancestry_proof).await
	//         .expect("failed to query ancestry proof").expect("No ancestry proof found");

	//     let ancestry_proof: AncestryProof<H256> = Decode::decode(&mut &opaq_ancestry_proof.0[..]).expect("failed to decode to AncestryProof");

	//    println!("\nANCESTRY PROOF: {ancestry_proof:?}");

	// }
}
