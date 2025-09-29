use rs_merkle::{MerkleProof, MerkleTree};
use serde::{Deserialize, Serialize};
use sp_core::{H256};
use std::{fmt::Display, fs::File, io::BufReader, path::Path};

use mmr_rpc::LeavesProof;
use subxt::{
	PolkadotConfig,
	backend::rpc::RpcClient,
	ext::{
		codec::{Decode, Encode},
		subxt_rpcs::{LegacyRpcMethods, rpc_params},
	},
};

use sp_consensus_beefy::{ecdsa_crypto::Public, known_payloads::MMR_ROOT_ID, mmr::BeefyAuthoritySet, BeefySignatureHasher, ValidatorSet};
use sp_mmr_primitives::{
	EncodableOpaqueLeaf, LeafProof,
	mmr_lib::{helper::get_peaks, leaf_index_to_mmr_size},
	utils::NodesUtils,
};

use crate::{
	cardano_encoding::SignedCommitment, helpers::{HexBeefyRelayChainProof, ToHex}, keccak::{authorities_merkle_tree, get_authorities_proof, KeccakHasher}, Block
};

pub type BeefyKeys = Vec<BeefyKeyInfo>;

pub type BeefySignedCommitment =
	sp_consensus_beefy::SignedCommitment<Block, sp_consensus_beefy::ecdsa_crypto::Signature>;

pub type MmrLeaf = sp_consensus_beefy::mmr::MmrLeaf<Block, H256, H256, Vec<u8>>;
pub type LeafIndex = u64;
pub type PeakNodes = Vec<(LeafIndex, Vec<u64>)>;

/// Used for inserting keys to the keystore
#[derive(Serialize, Deserialize)]
pub struct BeefyKeyInfo {
	/// Secret seed, for inserting beefy key
	suri: String,

	/// The public key of the secret seed (in ECDSA)
	pub_key: String,
}

impl BeefyKeyInfo {
	pub async fn insert_key(self, rpc: &RpcClient) {
		let params = rpc_params!["beef".to_string(), self.suri, self.pub_key.clone()];

		if let Err(e) = rpc.request::<()>("author_insertKey", params).await {
			println!("Warning: failed to insert key({}): {e:?}", self.pub_key);
			return;
		}

		println!("Added beefy key: {}", self.pub_key);
	}
}

pub fn keys_from_file<T: AsRef<Path> + Display>(key_file: T) -> BeefyKeys {
	let file_open_err = format!("failed to read from key_file {key_file}");
	let file_read_err = format!("cannot read beefy keys in key_file {key_file}");

	let key_file = File::open(key_file).expect(&file_open_err);
	let reader = BufReader::new(key_file);

	// Read the JSON contents of the file as an instance of `User`.
	serde_json::from_reader(reader).expect(&file_read_err)
}

pub struct BeefyRelayChainProof {
	pub consensus_proof: LeavesProof<H256>,
	//todo
	pub merkle_authorities: MerkleTree<KeccakHasher>,
	pub signed_commitment: BeefySignedCommitment,
	pub validators: Vec<Public>
}

impl BeefyRelayChainProof {
	pub fn create(
		consensus_proof: LeavesProof<H256>, 
		signed_commitment: BeefySignedCommitment, 
		validator_set: ValidatorSet<Public>,
		next_authorities: BeefyAuthoritySet<H256>
	) -> Option<Self> {
		if !verify_next_authority_set(next_authorities, &consensus_proof) {
			return None;
		}

		let merkle_authorities = authorities_merkle_tree(&signed_commitment, &validator_set)?;

		Some(BeefyRelayChainProof { consensus_proof, merkle_authorities, signed_commitment, validators: validator_set.validators().to_vec() })
	}

	pub fn print_as_hex(&self) {
		let result = HexBeefyRelayChainProof::from(self);
		println!("{result:#?}");
	}

	/// Mmr root hash taken from the commitment
	pub fn mmr_root_hash(&self) -> Option<H256> {
		self.signed_commitment.commitment.payload.get_decoded::<H256>(&MMR_ROOT_ID)
	}

	/// Block number taken from the commitment
	pub fn block_number(&self) -> Block {
		self.signed_commitment.commitment.block_number
	}

	/// Block hash taken from the commitment
	pub fn block_hash(&self) -> H256 {
		self.consensus_proof.block_hash
	}

	/// A String representation of the scale encoded signed commitment
	pub fn hex_scale_encoded_signed_commitment(&self) -> String {
		let scale_encoded = self.signed_commitment.encode();
		hex::encode(&scale_encoded)
	}

	/// The Beefy signed commitment, converted to Cardano-friendly signed commitment
	pub fn signed_commitment_as_cardano(&self) -> SignedCommitment {
		SignedCommitment::from_signed_commitment_and_validators(
			self.signed_commitment.clone(),
			&self.validators,
		)
	}

	pub fn authorities_proof(&self) -> MerkleProof<KeccakHasher> {
		get_authorities_proof(&self.merkle_authorities, &self.signed_commitment)
	}

	/// Returns a list of peaks per leaf index, taken from the LeafProof
	pub fn peak_nodes(&self) -> PeakNodes {
		let leaf_proof = self.leaf_proof();

		leaf_proof
			.leaf_indices
			.iter()
			.map(|leaf_index| {
				let mmr_size = leaf_index_to_mmr_size(*leaf_index);
				let peaks = get_peaks(mmr_size);

				let utils = NodesUtils::new(*leaf_index);
				let peak_len: u64 = utils.number_of_peaks();
				println!(
					"\nNumber of peaks {peak_len}: of leaf index({leaf_index}) with mmr size({mmr_size})"
				);

				(*leaf_index, peaks)
			})
			.collect()
	}

	/// Returns the decoded leaves of `LeavesProof`
	pub fn mmr_leaves(&self) -> Vec<MmrLeaf> {
		get_mmr_leaves(&self.consensus_proof)
	}

	/// Returns all the node hashes of the peaks
	pub fn node_hashes(&self) -> Vec<H256> {
		let leaf_proof = self.leaf_proof();
		leaf_proof.items
	}

	/// Returns the decoded `LeafProof`, from `LeavesProof`
	pub fn leaf_proof(&self) -> LeafProof<H256> {
		let leaf_proof_as_bytes = &self.consensus_proof.proof;

		Decode::decode(&mut &leaf_proof_as_bytes.0[..]).expect("Failed to decode to LeafProof")
	}
}

fn verify_next_authority_set(next_auth_set:BeefyAuthoritySet<H256>,  consensus_proof:&LeavesProof<H256>) -> bool {
	let mmr_leaves = get_mmr_leaves(consensus_proof);

	for leaf in mmr_leaves {
		if leaf.beefy_next_authority_set != next_auth_set {
			println!("WARNING: next authority sets are invalid: from proof: {:?}, from storage: {:?}",leaf.beefy_next_authority_set, next_auth_set);
			return false
		}
	}

	true
}

fn get_mmr_leaves(consensus_proof:&LeavesProof<H256>) -> Vec<MmrLeaf>  {
	let mut mmr_leaves = vec![];

		let leaves = &consensus_proof.leaves.0;
		let leaves: Vec<EncodableOpaqueLeaf> =
			Decode::decode(&mut &leaves[..]).expect("failed to convert to mmrleaf");

		for leaf in leaves {
			let leaf_as_bytes = leaf.into_opaque_leaf().0;

			let mmr_leaf: MmrLeaf =
				Decode::decode(&mut &leaf_as_bytes[..]).expect("failed to decode to mmrleaf");

			mmr_leaves.push(mmr_leaf);
		}

		mmr_leaves
}