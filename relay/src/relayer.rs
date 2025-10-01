use crate::{
	beefy::{self, BeefyRelayChainProof, PeakNodes},
	error::Error,
	helpers::MnMetaConversion,
	mn_meta,
	types::{Block, BlockHash, RootHash},
};

use mmr_rpc::LeavesProof;
use parity_scale_codec::Encode;
use sp_consensus_beefy::{
	SignedCommitment as BeefySignedCommitment, ValidatorSet as BeefyValidatorSet,
	VersionedFinalityProof,
	ecdsa_crypto::{Public as BeefyPublic, Signature as ECDSASig},
	mmr::BeefyAuthoritySet,
};
use sp_core::{Bytes, H256};
use sp_mmr_primitives::LeafProof;
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
	pub async fn new(node_url: &str) -> Result<Self, Error> {
		println!("Connecting to {node_url}");

		let api = OnlineClient::<PolkadotConfig>::from_insecure_url(node_url).await?;

		let rpc = RpcClient::from_url(node_url).await?;

		Ok(Relayer { rpc, api })
	}

	/// Listens and subscribes to the beefy justifications, printing out proofs per justification
	pub async fn run_relay_by_subscription(&self) -> Result<(), Error> {
		let mut sub: RpcSubscription<Bytes> = self
			.rpc
			.subscribe(
				"beefy_subscribeJustifications",
				rpc_params![],
				"beefy_unsubscribeJustifications",
			)
			.await?;

		while let Some(result) = sub.next().await {
			let justification = result?;

			match self.handle_justification_stream_data(justification.0).await {
				Ok(proof) => {
					self.check_proof_items(&proof).await?;

					proof.print_as_hex();
				},
				Err(e) => {
					println!("Handling Justification failed: {e:?}");
				},
			};
		}

		Err(Error::SubscriptionEnd)
	}

	async fn handle_justification_stream_data(
		&self,
		justification: Vec<u8>,
	) -> Result<BeefyRelayChainProof, Error> {
		let VersionedFinalityProof::<Block, ECDSASig>::V1(beef_signed_commitment) =
			Decode::decode(&mut &justification[..])?;

		let mmr_proof = self.get_mmr_proof(&beef_signed_commitment).await?;
		let at_block_hash = mmr_proof.block_hash;

		// TODO: get encodings for authority set, next authority set, and construct
		let validator_set = self.get_beefy_validator_set(at_block_hash).await?;
		let next_authorities = self.get_next_beefy_authority_set(at_block_hash).await?;

		BeefyRelayChainProof::create(
			mmr_proof,
			beef_signed_commitment,
			validator_set,
			next_authorities,
		)
	}
}

// For Proof creation
impl Relayer {
	pub async fn get_mmr_proof(
		&self,
		beefy_signed_commitment: &BeefySignedCommitment<Block, ECDSASig>,
	) -> Result<LeavesProof<H256>, Error> {
		let commitment_block = beefy_signed_commitment.commitment.block_number;

		let best_block = self.api.blocks().at_latest().await?;
		println!("\nBest Block Number: {}", best_block.number());

		println!("Creating proof of block({commitment_block})....");

		let at_block_hash = self.get_block_hash(commitment_block).await?;
		println!("AT BLOCKHASH({:#?})", at_block_hash);

		let root_hash = self.current_mmr_root(at_block_hash).await?;
		println!("WITH ROOTHASH({root_hash})");

		self._get_mmr_proof(vec![commitment_block], None, Some(at_block_hash)).await
	}

	/// Direct checking of the mmr proof
	pub async fn verify_mmr_proof(
		&self,
		root_hash: H256,
		leaves_proof: LeavesProof<H256>,
	) -> Result<bool, Error> {
		let mut rpc_params = RpcParams::new();
		rpc_params.push(root_hash)?;
		rpc_params.push(leaves_proof)?;

		let result =
			self.rpc.request::<Option<bool>>("mmr_verifyProofStateless", rpc_params).await?;

		result.ok_or(Error::ProofVerificationFailed)
	}

	/// Checks the items of the proof, whether these node hashes exists at a certain block on the chain
	pub async fn check_proof_items(&self, proof: &BeefyRelayChainProof) -> Result<(), Error> {
		let at_block_hash = proof.block_hash();
		let PeakNodes { peaks, num_of_peaks, .. } = proof.peak_nodes();
		let LeafProof { items, .. } = proof.leaf_proof();

		// loop through each peak, and ascertain if it exists on chain
		for peak in &peaks[0..(num_of_peaks as usize)] {
			let mmr_nodes_query = mn_meta::storage().mmr().nodes(*peak);

			let Some(node_hash) = self.storage_fetcher(at_block_hash, &mmr_nodes_query).await?
			else {
				return Err(Error::PeakNotFound { node_index: *peak, at_block_hash });
			};

			if !items.contains(&node_hash) {
				return Err(Error::InvalidPeak { node_index: *peak, at_block_hash });
			}
		}

		Ok(())
	}

	pub async fn verify_authorities_proof(
		&self,
		proof: &BeefyRelayChainProof,
	) -> Result<(), Error> {
		let at_block_hash = proof.block_hash();

		let validator_set = self.get_beefy_validator_set(at_block_hash).await?;

		let validators = validator_set.validators();

		Ok(())
	}

	async fn _get_mmr_proof(
		&self,
		// The block to query for
		blocks: Vec<Block>,
		best_block_number: Option<Block>,
		at_block_hash: Option<BlockHash>,
	) -> Result<LeavesProof<H256>, Error> {
		let mut params = RpcParams::new();
		params.push(blocks)?;
		params.push(best_block_number)?;
		params.push(at_block_hash)?;

		// TODO: handle
		let raw_proof_data = self.rpc.request_raw("mmr_generateProof", params.build()).await?;

		let raw_proof_data = raw_proof_data.get();
		serde_json::from_str(raw_proof_data)
			.map_err(|_| Error::SerdeDecode(raw_proof_data.to_string()))
	}
}

// Getting data from storage, or api
impl Relayer {
	// getting mmr root based on the block hash
	async fn current_mmr_root(&self, block_hash: BlockHash) -> Result<String, Error> {
		let params = rpc_params![block_hash];

		self.rpc.request::<String>("mmr_root", params).await.map_err(|e| Error::Rpc(e))
	}

	async fn get_beefy_validators(
		&self,
		at_block_hash: BlockHash,
	) -> Result<Vec<BeefyPublic>, Error> {
		let beefy_validator_set_query = mn_meta::storage().beefy().authorities();

		let validators = self
			.storage_fetcher(at_block_hash, &beefy_validator_set_query)
			.await?
			.map(|bounded_validators| bounded_validators.0)
			.ok_or(Error::EmptyValidatorSet);

		validators.map(|validators| {
			validators.into_iter().map(|validator| validator.into_non_metadata()).collect()
		})
	}

	async fn get_beefy_validator_set(
		&self,
		at_block_hash: BlockHash,
	) -> Result<BeefyValidatorSet<BeefyPublic>, Error> {
		let validator_set_call = mn_meta::apis().beefy_api().validator_set();

		let validator_set =
			self.api.runtime_api().at(at_block_hash).call(validator_set_call).await?;

		validator_set
			.map(|v_set| v_set.into_non_metadata())
			.ok_or(Error::EmptyValidatorSet)
	}

	// Below are for authority set proofs
	async fn get_beefy_authority_set(
		&self,
		at_block_hash: BlockHash,
	) -> Result<BeefyAuthoritySet<H256>, Error> {
		let get_authority_set_query = mn_meta::storage().beefy_mmr_leaf().beefy_authorities();

		let result = self
			.storage_fetcher(at_block_hash, &get_authority_set_query)
			.await?
			.map(|auth_set| auth_set.into_non_metadata());

		result.ok_or(Error::EmptyAuthoritySet)
	}

	async fn get_beefy_authorities_proof(
		&self,
		at_block_hash: BlockHash,
	) -> Result<BeefyAuthoritySet<H256>, Error> {
		let authorities_proof_call = mn_meta::apis().beefy_mmr_api().authority_set_proof();

		let result = self.api.runtime_api().at(at_block_hash).call(authorities_proof_call).await?;

		Ok(result.into_non_metadata())
	}

	async fn get_next_beefy_authority_set(
		&self,
		at_block_hash: BlockHash,
	) -> Result<BeefyAuthoritySet<H256>, Error> {
		let get_next_authority_set_query =
			mn_meta::storage().beefy_mmr_leaf().beefy_next_authorities();

		let result = self
			.storage_fetcher(at_block_hash, &get_next_authority_set_query)
			.await?
			.ok_or(Error::EmptyAuthoritySet)?;

		Ok(result.into_non_metadata())
	}

	async fn get_block_hash(&self, block: Block) -> Result<BlockHash, Error> {
		let params = rpc_params![block];
		let block_hash: Option<BlockHash> = self.rpc.request("chain_getBlockHash", params).await?;

		block_hash.ok_or(Error::NoBlockHash(block))
	}

	async fn storage_fetcher<'address, Addr>(
		&self,
		at_block_hash: BlockHash,
		address: &'address Addr,
	) -> Result<Option<Addr::Target>, Error>
	where
		Addr: subxt::storage::Address<IsFetchable = subxt::utils::Yes> + 'address,
	{
		let storage_fetcher = self.api.storage().at(at_block_hash);

		storage_fetcher.fetch(address).await.map_err(|e| Error::ClientError(e))
	}
}
