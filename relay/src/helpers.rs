use std::fmt::Debug;

use sp_core::{Bytes, H256};
use sp_mmr_primitives::LeafProof;
use subxt::ext::codec::Encode;

use crate::{
	Block,
	beefy::{BeefyRelayChainProof, MmrLeaf},
};

pub trait ToHex {
	fn as_hex(&self) -> String;
}

impl ToHex for H256 {
	fn as_hex(&self) -> String {
		hex::encode(&self.0)
	}
}

impl ToHex for Bytes {
	fn as_hex(&self) -> String {
		hex::encode(&mut &self[..])
	}
}

impl ToHex for crate::mn_meta::runtime_types::sp_consensus_beefy::ecdsa_crypto::Public {
	fn as_hex(&self) -> String {
		hex::encode(self.0)
	}
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct HexMmrLeaf {
	scale_encoded_leaf_hash: String,
	leaf: MmrLeaf,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct HexBeefyRelayChainProof {
	leaves_proof_block_hash: String,
	scale_encoded_leaves_hash: String,
	leaves: Vec<HexMmrLeaf>,

	scale_encoded_leaf_proof_hash: String,
	leaf_proof: LeafProof<H256>,

	scale_encoded_beefy_commitment: String,
	beefy_commitment_root_hash: String,
	beefy_commitment_block_hash: String,
	beefy_commitment_block_number: Block,
	beefy_commitment_signatures: Vec<String>,

	validator_set: Vec<String>,
}

impl From<&BeefyRelayChainProof> for HexBeefyRelayChainProof {
	fn from(value: &BeefyRelayChainProof) -> Self {
		let beefy_commitment_signatures: Vec<String> = value
			.signed_commitment
			.signatures
			.iter()
			.map(|opt_sig| match opt_sig {
				Some(sig) => hex::encode(sig),
				None => "none".to_string(),
			})
			.collect();

		let validator_set = value.validator_set.iter().into_iter().map(|v| v.as_hex()).collect();

		let beefy_commitment_root_hash = value
			.mmr_root_hash()
			.map(|root_hash| root_hash.as_hex())
			.unwrap_or("None".to_string());

		let leaves = value
			.mmr_leaves()
			.iter()
			.map(|leaf| {
				let encode = leaf.encode();
				let scale_encoded_leaf_hash = hex::encode(&encode);

				HexMmrLeaf { scale_encoded_leaf_hash, leaf: leaf.clone() }
			})
			.collect();

		HexBeefyRelayChainProof {
			leaves_proof_block_hash: value.consensus_proof.block_hash.as_hex(),
			scale_encoded_leaves_hash: value.consensus_proof.leaves.as_hex(),
			leaves,
			scale_encoded_leaf_proof_hash: value.consensus_proof.proof.as_hex(),
			leaf_proof: value.leaf_proof(),
			scale_encoded_beefy_commitment: value.hex_scale_encoded_signed_commitment(),
			beefy_commitment_root_hash,
			beefy_commitment_block_hash: value.block_hash().as_hex(),
			beefy_commitment_block_number: value.block_number(),
			beefy_commitment_signatures,
			validator_set,
		}
	}
}
