use std::fmt::Debug;

use crate::{beefy::BeefySignedCommitment, error::Error, types::RootHash};

use binary_merkle_tree::verify_proof;

use sp_consensus_beefy::{BeefySignatureHasher, ValidatorSet, ecdsa_crypto::Public};
use sp_core::{H256, KeccakHasher};

pub type LeafIndex = u32;

#[derive(Clone)]
pub struct AuthorityProof {
	leaf_index: LeafIndex,
	leaf: Public,
	proof: Vec<H256>,
}
#[derive(Debug, Clone)]
pub struct AuthoritiesProof {
	pub root: RootHash,
	pub total_leaves: u32,
	pub proofs: Vec<AuthorityProof>,
}

pub fn generate_authorities_proof(
	beefy_signed_commitment: &BeefySignedCommitment,
	validator_set: &ValidatorSet<Public>,
) -> Result<AuthoritiesProof, Error> {
	// use sp_runtime::traits::Keccak256;
	let block_number = beefy_signed_commitment.commitment.block_number;

	// verify the signatures in the commitment are from the validator set
	beefy_signed_commitment
		.verify_signatures::<_, BeefySignatureHasher>(block_number, validator_set)
		.map_err(|_| Error::NoMatchingSignature(block_number))?;

	let sig_indices: Vec<usize> = beefy_signed_commitment
		.signatures
		.iter()
		.enumerate()
		.filter_map(|(index, sig)| sig.clone().map(|_| index))
		.collect();

	let validators = validator_set.validators();
	let root = binary_merkle_tree::merkle_root::<KeccakHasher, _>(validators);

	let mut proofs = vec![];
	let mut total_leaves = validators.len() as u32;
	for sig in sig_indices {
		let sig = sig as LeafIndex;
		let proof = binary_merkle_tree::merkle_proof::<KeccakHasher, _, _>(validators, sig);

		proofs.push(AuthorityProof {
			leaf: proof.leaf.clone(),
			leaf_index: proof.leaf_index,
			proof: proof.proof,
		});
	}

	Ok(AuthoritiesProof { root, total_leaves, proofs })
}

impl AuthoritiesProof {
	pub fn verify(&self) -> Vec<bool> {
		self.proofs
			.iter()
			.map(|proof| proof.verify(&self.root, self.total_leaves))
			.collect()
	}

	pub fn leaf_indices(&self) -> Vec<LeafIndex> {
		self.proofs.iter().map(|proof| proof.leaf_index).collect()
	}
}

impl AuthorityProof {
	fn verify(&self, root: &RootHash, number_of_leaves: u32) -> bool {
		verify_proof::<KeccakHasher, _, _>(
			root,
			self.proof.clone(),
			number_of_leaves,
			self.leaf_index,
			&self.leaf,
		)
	}
}

impl Debug for AuthorityProof {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("")
			.field("leaf", &self.leaf)
			.field("leaf_index", &self.leaf_index)
			.field("proof", &self.proof)
			.finish()
	}
}
