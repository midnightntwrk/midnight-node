use sp_consensus_beefy::{VersionedFinalityProof, ecdsa_crypto::Signature as EcdsaSignature};
use sp_core::Bytes;
use subxt::{
	OnlineClient, PolkadotConfig,
	backend::rpc::RpcClient,
	ext::subxt_rpcs::{
		client::{RpcParams, RpcSubscription},
		rpc_params,
	},
	runtime_api::Payload,
};

use crate::{
	BeefySignedCommitment, BeefyValidatorSet, BlockNumber, Error, MmrProof,
	authorities::AuthoritiesProof, helper::MnMetaConversion, mmr::get_beefy_ids_with_stakes,
	mn_meta,
};

pub type BlockHash = sp_core::H256;

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
			self.handle_justification_stream_data(justification.0).await?;
		}

		Ok(())
	}

	async fn handle_justification_stream_data(&self, justification: Vec<u8>) -> Result<(), Error> {
		// decode the justifcation
		let VersionedFinalityProof::<BlockNumber, EcdsaSignature>::V1(beef_signed_commitment) =
			parity_scale_codec::Decode::decode(&mut &justification[..])?;

		// Identifies whether using from best block, or the commitment's block hash
		let (best_block, at_block_hash) = self.choose_params(&beef_signed_commitment).await?;

		// The commitment block number to create proof from
		let block_to_query = beef_signed_commitment.commitment.block_number;

		// Generate the mmr proof
		let mmr_proof = self.get_mmr_proof(block_to_query, best_block, at_block_hash).await?;
		println!("Get MMR Proof: {mmr_proof:?}");

		// retrieve necessary data in creating the AuthoritiesProof
		let validator_set = self.get_beefy_validator_set(at_block_hash).await?;

		// Only need the leaf extra
		let mut leaf_extra = get_beefy_ids_with_stakes(&mmr_proof)?;

		let _authorities_proof =
			AuthoritiesProof::try_new(&beef_signed_commitment, &validator_set, &mut leaf_extra)?;

		Ok(())
	}

	async fn get_mmr_proof(
		&self,
		block_to_query: BlockNumber,
		best_block_number: Option<BlockNumber>,
		at_block_hash: Option<BlockHash>,
	) -> Result<MmrProof, Error> {
		let mut params = RpcParams::new();
		params.push(vec![block_to_query])?;
		params.push(best_block_number)?;
		params.push(at_block_hash)?;

		let raw_proof_data = self.rpc.request_raw("mmr_generateProof", params.build()).await?;

		let raw_proof_data = raw_proof_data.get();
		serde_json::from_str(raw_proof_data)
			.map_err(|_| Error::JsonDecodeError(raw_proof_data.to_string()))
	}

	/// Returns a tuple of  2 options; whether we query with the latest (best block), or by the block hash from the commitment
	async fn choose_params(
		&self,
		beefy_signed_commitment: &BeefySignedCommitment,
	) -> Result<(Option<BlockNumber>, Option<BlockHash>), Error> {
		let commitment_block = beefy_signed_commitment.commitment.block_number;

		let best_block = self.get_best_block_number().await;

		let at_block_hash = match &best_block {
			None => {
				println!("Cannot retrieve best block; try using Commitment block hash...");
				self.get_block_hash(commitment_block).await
			},
			Some(block_number) => {
				println!("\nQuerying from the best block number: {block_number}");
				None
			},
		};

		Ok((best_block, at_block_hash))
	}

	/// Returns the validator set with beefy ids, based on the provided block hash
	async fn get_beefy_validator_set(
		&self,
		at_block_hash: Option<BlockHash>,
	) -> Result<BeefyValidatorSet, Error> {
		let validator_set_call = mn_meta::apis().beefy_api().validator_set();

		let validator_set = self.runtime_api(at_block_hash, validator_set_call).await?;

		validator_set
			.map(|v_set| v_set.into_non_metadata())
			.ok_or(Error::EmptyValidatorSet)
	}

	/// Returns the Best Block Number, or None if querying fails.
	/// No need to throw an error
	async fn get_best_block_number(&self) -> Option<BlockNumber> {
		match self.api.blocks().at_latest().await.map(|block| block.number()) {
			Ok(block) => Some(block),
			Err(e) => {
				println!("WARNING: Failed to get best block number: {e:?}");
				None
			},
		}
	}

	/// Returns the Block Hash of the provided block number, or None if querying fails.
	/// No need to throw an error
	async fn get_block_hash(&self, block: BlockNumber) -> Option<BlockHash> {
		let params = rpc_params![block];

		match self.rpc.request("chain_getBlockHash", params).await {
			Ok(result) => result,
			Err(e) => {
				println!("WARNING: Failed to get block hash for block({block}: {e:?})");
				None
			},
		}
	}

	/// Helper function for querying via the runtime api
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
		.map_err(Error::Subxt)
	}
}
