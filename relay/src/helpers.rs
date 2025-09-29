use std::{fmt::Debug};

use rs_merkle::{MerkleProof, MerkleTree};
use sp_consensus_beefy::{ecdsa_crypto::Public, mmr::BeefyAuthoritySet, ValidatorSet};
use sp_core::{crypto::Ss58Codec, ByteArray, Bytes, H256};
use sp_mmr_primitives::LeafProof;
use subxt::ext::codec::Encode;

use mn_meta::runtime_types::sp_consensus_beefy::{
	ecdsa_crypto::Public as MidnBeefyPublic,
	mmr::BeefyAuthoritySet as MidnBeefyAuthSet,
	ValidatorSet as MidnBeefyValidatorSet, 
};

use crate::{
	beefy::{BeefyRelayChainProof, MmrLeaf}, keccak::KeccakHasher, mn_meta, Block
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

impl ToHex for MerkleProof<KeccakHasher> {
	fn as_hex(&self) -> String {
		let bytes = self.to_bytes();
		hex::encode(&bytes)
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

	authorities_root_hash: String,
	authorities_proof_hash: String,
	authorities_hashes: Vec<String>,

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

		let validator_set = value.validators.iter().into_iter().map(|v: &Public| v.to_ss58check()).collect();

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

		let authorities_proof = value.authorities_proof();

		let authorities_proof_hash = authorities_proof.as_hex();
		let authorities_hashes = authorities_proof.proof_hashes_hex();
	
		let authorities_root_hash = value.merkle_authorities.root_hex().expect("should print the root");


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

			authorities_root_hash,
			authorities_proof_hash,
			authorities_hashes,

			validator_set,
		}
	}
}


// ------ Converting types from metadata, to the sp-consensus libraries
// todo: check `substitute_type` of subxt

pub trait FromMnMeta<T> {
	fn into_non_metadata(value:T) -> Self;
}

impl FromMnMeta<MidnBeefyValidatorSet<MidnBeefyPublic>> for ValidatorSet<Public> {
	fn into_non_metadata(value:MidnBeefyValidatorSet<MidnBeefyPublic>) -> Self {
		let validators: Vec<Public> = value.validators.into_iter().map(|validator| {
			Public::into_non_metadata(validator)
		}).collect();

		ValidatorSet::new(validators, value.id).expect("Validators list should not be empty")
	}
}

impl FromMnMeta<MidnBeefyPublic> for Public {
	fn into_non_metadata(value:MidnBeefyPublic) -> Self {
		Public::from_slice(value.0.as_slice()).expect("conversion to Beefy Public should work")
	}
}

impl <T> FromMnMeta<MidnBeefyAuthSet<T>> for BeefyAuthoritySet<T> {
	fn into_non_metadata(value:MidnBeefyAuthSet<T>) -> Self {
		BeefyAuthoritySet { id: value.id, len: value.len, keyset_commitment: value.keyset_commitment }
	 
	}
}