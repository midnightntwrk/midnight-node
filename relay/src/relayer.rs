use crate::{
	beefy::{self, BeefyRelayChainProof, PeakNodes}, helpers::{FromMnMeta, ToHex}, mn_meta, Block, BlockHash
};

use mmr_rpc::LeavesProof;
use sp_consensus_beefy::{
	ecdsa_crypto::{Public as BeefyPublic, Signature as ECDSASig}, mmr::BeefyAuthoritySet, SignedCommitment as BeefySignedCommitment, ValidatorSet as BeefyValidatorSet, VersionedFinalityProof
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

	pub async fn run_relay_by_subscription(&self) {
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

			let Some(proof) = self.handle_justification_stream_data(justification.0).await else {
				println!("Proof failed");
				continue
			};

			let block_hash = proof.block_hash();
			

			proof.print_as_hex();
		}
	}

	async fn handle_justification_stream_data(
		&self,
		justification: Vec<u8>,
	) -> Option<BeefyRelayChainProof> {
		let VersionedFinalityProof::<Block, ECDSASig>::V1(beef_signed_commitment) =
			Decode::decode(&mut &justification[..])
				.expect("failed to parse to VersionedFinalityProof");

		let mmr_proof = self.get_consensus_proof(&beef_signed_commitment).await;
		let at_block_hash = mmr_proof.block_hash;

		// TODO: get encodings for authority set, next authority set, and construct
		let validator_set = self.get_beefy_validator_set(at_block_hash).await?;
		let next_authorities = self.get_next_beefy_authority_set(at_block_hash).await;

		BeefyRelayChainProof::create(mmr_proof, beef_signed_commitment, validator_set, next_authorities)
	}

	pub async fn get_consensus_proof(
		&self,
		beefy_signed_commitment: &BeefySignedCommitment<Block, ECDSASig>,
	) -> LeavesProof<H256> {
		let commitment_block = beefy_signed_commitment.commitment.block_number;

		let best_block = self.api.blocks().at_latest().await.expect("get the best block");
		println!("\nBest Block Number: {}", best_block.number());

		println!("Creating proof of block({commitment_block})....");

		let at_block_hash = self.get_block_hash(commitment_block).await;
		if let Some(block_hash) = &at_block_hash {
			println!("Block Hash: {}", block_hash.as_hex());

			self.current_mmr_root(*block_hash).await;
		};

		self.get_mmr_proof(vec![commitment_block], None, at_block_hash).await
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
		blocks: Vec<Block>,
		best_block_number: Option<Block>,
		at_block_hash: Option<BlockHash>,
	) -> LeavesProof<H256> {
		let mut params = RpcParams::new();
		params.push(blocks).expect("should be able to push multiple blocks");
		params.push(best_block_number).expect("failed to add best_block_number");
		params.push(at_block_hash).expect("failed to add block hash");

		println!("Generating proof with params: {params:?}");

		// TODO: handle
		let raw_proof_data = self
			.rpc
			.request_raw("mmr_generateProof", params.build())
			.await
			.expect("failed to get raw proof data");

		serde_json::from_str(raw_proof_data.get()).expect("failed to parse raw proof")
	}

	// getting mmr root based on the block hash
	async fn current_mmr_root(&self, block_hash: H256) {
		let params = rpc_params![block_hash.as_hex()];

		match self.rpc.request::<String>("mmr_root", params).await {
			Ok(root) => println!("Root Hash: {root}"),
			Err(e) => {
				println!(
					"Warning: failed to get mmr proof of block hash({}): {e:?}",
					block_hash.as_hex()
				)
			},
		}
	}

	async fn get_beefy_validators(&self, at_block_hash: BlockHash) -> Vec<BeefyPublic> {
		let beefy_validator_set_query = mn_meta::storage().beefy().authorities();
		let storage_fetcher = self.api.storage().at(at_block_hash);

		let Some(validators) = storage_fetcher
			.fetch(&beefy_validator_set_query)
			.await
			.expect("failed to get validator set")
		else {
			println!("WARN: no validator set found");
			return vec![];
		};

		validators.0.into_iter().map(|v| BeefyPublic::into_non_metadata(v)).collect()
	}

	async fn get_beefy_validator_set(&self, at_block_hash: BlockHash) -> Option<BeefyValidatorSet<BeefyPublic>> {
		let validator_set_call = mn_meta::apis().beefy_api().validator_set();
		
		let validator_set = self.api.runtime_api().at(at_block_hash).call(validator_set_call).await.expect("failed to query validator set");

		validator_set.map(|v_set| BeefyValidatorSet::into_non_metadata(v_set))
	}


	// Below are for authority set proofs
	async fn get_beefy_authority_set(&self, at_block_hash: BlockHash) -> BeefyAuthoritySet<H256> {
		let get_authority_set_query = mn_meta::storage().beefy_mmr_leaf().beefy_authorities();

		let storage_fetcher = self.api.storage().at(at_block_hash);

		let result = storage_fetcher
			.fetch(&get_authority_set_query)
			.await
			.expect("failed to get authority set")
			.expect("No BeefyAuthoritySet found");

		BeefyAuthoritySet::into_non_metadata(result)
	}

	async fn get_beefy_authorities_proof(&self, at_block_hash: BlockHash) -> BeefyAuthoritySet<H256> {
		let authorities_proof_call = mn_meta::apis().beefy_mmr_api().authority_set_proof();

		let result = self.api.runtime_api().at(at_block_hash).call(authorities_proof_call).await.expect("failed to query authorities proof");

		BeefyAuthoritySet::into_non_metadata(result)

	}

	async fn get_next_beefy_authority_set(
		&self,
		at_block_hash: BlockHash,
	) ->  BeefyAuthoritySet<H256> {
		let get_next_authority_set_query =
			mn_meta::storage().beefy_mmr_leaf().beefy_next_authorities();

		let storage_fetcher = self.api.storage().at(at_block_hash);

		let result = storage_fetcher
			.fetch(&get_next_authority_set_query)
			.await
			.expect("failed to get next authority set")
			.expect("No Next BeefyAuthoritySet found");

		BeefyAuthoritySet::into_non_metadata(result)
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
}
