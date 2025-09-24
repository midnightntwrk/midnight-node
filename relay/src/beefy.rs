use serde::{Deserialize, Serialize};
use sp_core::H256;
use std::{fmt::Display, fs::File, io::BufReader, path::Path};

use mmr_rpc::LeavesProof;
use subxt::{
	backend::rpc::RpcClient,
	ext::{
		codec::{Decode, Encode},
		subxt_rpcs::rpc_params,
	},
};

use sp_consensus_beefy::known_payloads::MMR_ROOT_ID;
use sp_mmr_primitives::{
	EncodableOpaqueLeaf, LeafProof,
	mmr_lib::{helper::get_peaks, leaf_index_to_mmr_size},
	utils::NodesUtils,
};

use crate::{Block, cardano_encoding::SignedCommitment};

pub type BeefyKeys = Vec<BeefyKeyInfo>;

pub type BeefySignedCommitment =
	sp_consensus_beefy::SignedCommitment<Block, sp_consensus_beefy::ecdsa_crypto::Signature>;
pub type BeefyValidatorSet =
	Vec<crate::mn_meta::runtime_types::sp_consensus_beefy::ecdsa_crypto::Public>;

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

#[derive(Debug)]
pub struct BeefyRelayChainProof {
	pub consensus_proof: LeavesProof<H256>,
	//todo
	pub authority_proof: (),
	pub signed_commitment: BeefySignedCommitment,
	pub validator_set: BeefyValidatorSet,
}

impl BeefyRelayChainProof {
	pub fn mmr_root_hash(&self) -> Option<H256> {
		let root_hash = self.signed_commitment.commitment.payload.get_decoded::<H256>(&MMR_ROOT_ID);

		if let Some(hash) = &root_hash {
			let hex_encoded = hex::encode(&hash.0);
			println!("Root Hash: {hex_encoded}");
		}

		root_hash
	}

	pub fn block_number(&self) -> Block {
		self.signed_commitment.commitment.block_number
	}

	pub fn block_hash(&self) -> H256 {
		self.consensus_proof.block_hash
	}

	pub fn hex_scale_encoded_signed_commitment(&self) -> String {
		let scale_encoded = self.signed_commitment.encode();
		hex::encode(&scale_encoded)
	}

	pub fn signed_commitment_as_cardano(&self) -> SignedCommitment {
		SignedCommitment::from_signed_commitment_and_validators(
			self.signed_commitment.clone(),
			&self.validator_set,
		)
	}

	// List of (Leaf Indices, and the peaks)
	pub fn peak_nodes(&self) -> PeakNodes {
		let leaf_proof = self.leaf_proof();

		leaf_proof
			.leaf_indices
			.iter()
			.map(|leaf_index| {
				let mmr_size = leaf_index_to_mmr_size(*leaf_index);
				let peaks = get_peaks(mmr_size);

				let utils = NodesUtils::new(*leaf_index);

				let peak_len = utils.number_of_peaks();
				println!(
					"\nNumber of peaks {peak_len}: of leaf index({leaf_index}) with mmr size({mmr_size})"
				);

				(*leaf_index, peaks)
			})
			.collect()
	}

	// todo, probably better to return the extra data ?
	pub fn mmr_leaves(&self) -> Vec<MmrLeaf> {
		let mut mmr_leaves = vec![];

		let leaves = &self.consensus_proof.leaves.0;
		let leaves: Vec<EncodableOpaqueLeaf> =
			Decode::decode(&mut &leaves[..]).expect("failed to convert to mmrleaf");

		for leaf in leaves {
			let leaf_as_bytes = leaf.into_opaque_leaf().0;
			let hex_opaque_leaf = hex::encode(&leaf_as_bytes);
			println!("\nEncodableOpaqueLeaf hex: {hex_opaque_leaf}");

			let mmr_leaf: MmrLeaf =
				Decode::decode(&mut &leaf_as_bytes[..]).expect("failed to decode to mmrleaf");
			let emcoded_mmr_leaf = mmr_leaf.encode();
			// let hex_mmr_leaf = hex::encode(&emcoded_mmr_leaf);
			// println!("MMR Leaf hex: {hex_mmr_leaf}");
			println!("The MMR Leaf: {mmr_leaf:#?}");

			mmr_leaves.push(mmr_leaf);
		}

		mmr_leaves
	}

	pub fn node_hashes(&self) -> Vec<H256> {
		let leaf_proof = self.leaf_proof();
		leaf_proof.items
	}

	fn leaf_proof(&self) -> LeafProof<H256> {
		let leaf_proof_as_bytes = &self.consensus_proof.proof;

		let hex_proof = hex::encode(&leaf_proof_as_bytes.0);
		println!("hex mmr proof: {hex_proof}");

		Decode::decode(&mut &leaf_proof_as_bytes.0[..]).expect("Failed to decode to LeafProof")
	}
}
