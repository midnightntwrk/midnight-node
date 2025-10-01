use std::fmt::Debug;

use sp_consensus_beefy::{ValidatorSet, ecdsa_crypto::Public, mmr::BeefyAuthoritySet};
use sp_core::{ByteArray, Bytes, H256, crypto::Ss58Codec};
use sp_mmr_primitives::LeafProof;
use subxt::ext::codec::Encode;

use mn_meta::runtime_types::sp_consensus_beefy::{
	ValidatorSet as MidnBeefyValidatorSet, ecdsa_crypto::Public as MidnBeefyPublic,
	mmr::BeefyAuthoritySet as MidnBeefyAuthSet,
};

use crate::{
	beefy::{BeefyRelayChainProof, MmrLeaf, PeakNodes},
	keccak::AuthoritiesProof,
	mn_meta,
	types::Block,
};

pub trait ToHex {
	fn as_hex(&self) -> String;
}

impl ToHex for Bytes {
	fn as_hex(&self) -> String {
		hex::encode(&mut &self[..])
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
	peak_nodes: PeakNodes,

	scale_encoded_beefy_commitment: String,
	beefy_commitment_root_hash: String,
	beefy_commitment_block_hash: String,
	beefy_commitment_block_number: Block,
	beefy_commitment_signatures: Vec<String>,

	scale_encoded_authorities_proof: String,
	authorities_proof: AuthoritiesProof,

	signers: Vec<String>,
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

		let signers = value.signers.iter().into_iter().map(|v: &Public| format!("{v:?}")).collect();

		let beefy_commitment_root_hash = value
			.mmr_root_hash()
			.map(|root_hash| format!("{:#?}", root_hash))
			.unwrap_or("None".to_string());

		let leaves = value
			.mmr_leaves()
			.expect("should return leaves")
			.iter()
			.map(|leaf| {
				let encode = leaf.encode();
				let scale_encoded_leaf_hash = hex::encode(&encode);

				HexMmrLeaf { scale_encoded_leaf_hash, leaf: leaf.clone() }
			})
			.collect();

		let peak_nodes = value.peak_nodes();
		let authorities_proof = value.authorities_proof.clone();
		let scale_encoded_authorities_proof = authorities_proof.encode();
		let scale_encoded_authorities_proof = hex::encode(scale_encoded_authorities_proof);

		HexBeefyRelayChainProof {
			leaves_proof_block_hash: format!("{:#?}", value.mmr_proof.block_hash),
			scale_encoded_leaves_hash: value.mmr_proof.leaves.as_hex(),
			leaves,
			scale_encoded_leaf_proof_hash: value.mmr_proof.proof.as_hex(),
			leaf_proof: value.leaf_proof(),
			peak_nodes,
			scale_encoded_beefy_commitment: value.hex_scale_encoded_signed_commitment(),
			beefy_commitment_root_hash,
			beefy_commitment_block_hash: format!("{:#?}", value.block_hash()),
			beefy_commitment_block_number: value.block_number(),
			beefy_commitment_signatures,

			scale_encoded_authorities_proof,
			authorities_proof,

			signers,
		}
	}
}

// ------ Converting types from metadata, to the sp-consensus libraries
// todo: check `substitute_type` of subxt

pub trait MnMetaConversion<T> {
	fn into_non_metadata(self) -> T;
}

impl MnMetaConversion<ValidatorSet<Public>> for MidnBeefyValidatorSet<MidnBeefyPublic> {
	fn into_non_metadata(self) -> ValidatorSet<Public> {
		let mut validators = vec![];

		for validator in self.validators {
			validators.push(validator.into_non_metadata());
		}

		ValidatorSet::new(validators, self.id).expect("cannot create from empty validators")
	}
}

impl MnMetaConversion<Public> for MidnBeefyPublic {
	fn into_non_metadata(self) -> Public {
		Public::from_slice(self.0.as_slice()).expect("failed to convert to Beefy Public")
	}
}

impl<T> MnMetaConversion<BeefyAuthoritySet<T>> for MidnBeefyAuthSet<T> {
	fn into_non_metadata(self) -> BeefyAuthoritySet<T> {
		BeefyAuthoritySet { id: self.id, len: self.len, keyset_commitment: self.keyset_commitment }
	}
}
