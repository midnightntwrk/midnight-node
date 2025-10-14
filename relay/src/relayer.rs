use crate::{
	beefy::BeefyRelayChainProof,
	error::Error,
	helpers::MnMetaConversion,
	mmr::{LeavesProofExt, PeakNodes},
	mn_meta,
	types::{Block, BlockHash, RootHash},
};

use mmr_rpc::LeavesProof;
use parity_scale_codec::Encode;
use sp_consensus_beefy::{
	ValidatorSet as BeefyValidatorSet, VersionedFinalityProof,
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
	runtime_api::Payload,
};

type SubxtBlock = subxt::blocks::Block<PolkadotConfig, OnlineClient<PolkadotConfig>>;
type BeefySignedCommitment = sp_consensus_beefy::SignedCommitment<Block, ECDSASig>;

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
					self.check_proof_items(&proof.mmr_proof).await?;

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

		// Identifies whether using from best block, or the commitment's block hash
		let (best_block, at_block_hash) = self.choose_params(&beef_signed_commitment).await?;

		// The commitment block number to create proof from
		let block_to_query = beef_signed_commitment.commitment.block_number;

		// Generate the mmr proof
		let mmr_proof = self.get_mmr_proof(block_to_query, best_block, at_block_hash).await?;

		let validator_set = self.get_beefy_validator_set(at_block_hash).await?;
		let next_authorities = self.get_next_beefy_authority_set(at_block_hash).await?;

		let relay_proof = BeefyRelayChainProof::create(
			mmr_proof,
			beef_signed_commitment,
			validator_set,
			next_authorities,
		)?;

		//

		Ok(relay_proof)
	}

	/// Returns a tuple of  2 options; whether we query with the latest (best block), or by the block hash from the commitment
	async fn choose_params(
		&self,
		beefy_signed_commitment: &BeefySignedCommitment,
	) -> Result<(Option<Block>, Option<BlockHash>), Error> {
		let commitment_block = beefy_signed_commitment.commitment.block_number;

		let best_block = self.get_best_block_number().await;

		let at_block_hash = match &best_block {
			None => {
				let commitment_block_hash = self.get_block_hash(commitment_block).await?;
				println!(
					"Cannot retrieve best block; using Commitment block hash: {commitment_block_hash}"
				);

				Some(commitment_block_hash)
			},
			Some(block_number) => {
				println!("Querying from the best block number: {block_number}");
				None
			},
		};

		Ok((best_block, at_block_hash))
	}
}

// For Proof creation
impl Relayer {
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
	pub async fn check_proof_items(&self, proof: &LeavesProof<H256>) -> Result<(), Error> {
		let at_block_hash = Some(proof.block_hash);

		let PeakNodes { peaks, num_of_peaks, .. } = proof.peak_nodes();
		let LeafProof { items, .. } = proof.leaf_proof();

		// loop through each peak, and ascertain if it exists on chain
		for peak in &peaks[0..(num_of_peaks as usize)] {
			let mmr_nodes_query = mn_meta::storage().mmr().nodes(*peak);

			let node_hash = self.storage_fetcher(at_block_hash, &mmr_nodes_query).await?.ok_or(
				Error::PeakNotFound { node_index: *peak, at_block_hash: proof.block_hash },
			)?;

			if !items.contains(&node_hash) {
				return Err(Error::InvalidPeak {
					node_index: *peak,
					at_block_hash: proof.block_hash,
				});
			}
		}

		Ok(())
	}

	pub async fn verify_authorities_proof(
		&self,
		proof: &BeefyRelayChainProof,
	) -> Result<(), Error> {
		let (_, at_block_hash) = self.choose_params(&proof.signed_commitment).await?;

		let validator_set = self.get_beefy_validator_set(at_block_hash).await?;

		let validators = validator_set.validators();

		Ok(())
	}

	async fn get_mmr_proof(
		&self,
		block_to_query: Block,
		best_block_number: Option<Block>,
		at_block_hash: Option<BlockHash>,
	) -> Result<LeavesProof<H256>, Error> {
		let mut params = RpcParams::new();
		params.push(vec![block_to_query])?;
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
		at_block_hash: Option<BlockHash>,
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
		at_block_hash: Option<BlockHash>,
	) -> Result<BeefyValidatorSet<BeefyPublic>, Error> {
		let validator_set_call = mn_meta::apis().beefy_api().validator_set();

		let validator_set = self.runtime_api(at_block_hash, validator_set_call).await?;

		validator_set
			.map(|v_set| v_set.into_non_metadata())
			.ok_or(Error::EmptyValidatorSet)
	}

	// Below are for authority set proofs
	async fn get_beefy_authority_set(
		&self,
		at_block_hash: Option<BlockHash>,
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
		at_block_hash: Option<BlockHash>,
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

	async fn get_best_block_number(&self) -> Option<Block> {
		self.api.blocks().at_latest().await.map(|block| block.number()).ok()
	}

	async fn storage_fetcher<'address, Addr>(
		&self,
		at_block_hash: Option<BlockHash>,
		address: &'address Addr,
	) -> Result<Option<Addr::Target>, Error>
	where
		Addr: subxt::storage::Address<IsFetchable = subxt::utils::Yes> + 'address,
	{
		let storage_fetcher = match at_block_hash {
			Some(at_block_hash) => self.api.storage().at(at_block_hash),
			None => self.api.storage().at_latest().await?,
		};

		storage_fetcher.fetch(address).await.map_err(|e| Error::ClientError(e))
	}

	async fn runtime_api<T: Payload>(
		&self,
		at_block_hash: Option<BlockHash>,
		payload: T,
	) -> Result<T::ReturnType, Error> {
		match at_block_hash {
			Some(at_block_hash) => self.api.runtime_api().at(at_block_hash).call(payload).await,
			None => {
				let result = self.api.runtime_api().at_latest().await?;
				result.call(payload).await
			},
		}
		.map_err(|e| Error::ClientError(e))
	}
}
