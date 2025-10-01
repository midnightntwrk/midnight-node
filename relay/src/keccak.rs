use std::fmt::Debug;

use crate::{beefy::BeefySignedCommitment, error::Error, types::RootHash};

use binary_merkle_tree::verify_proof;
use parity_scale_codec::{Decode, Encode};
use sp_consensus_beefy::{BeefySignatureHasher, ValidatorSet, ecdsa_crypto::Public};
use sp_core::{H256, KeccakHasher};

pub type LeafIndex = u32;
pub type ProofItems = Vec<(LeafIndex, Vec<H256>)>;
pub type Signers = Vec<Public>;

/// Contains the merkle root hash of all authorities,
/// And the proof for a few chosen authorities

#[derive(Debug, Clone, Encode, Decode)]
pub struct AuthoritiesProof {
	pub root: RootHash,

	/// the total number of validators
	pub total_leaves: u32,

	/// list of (<index of a signer in the commitment file>, <its proof in the validator set>)
	pub items: ProofItems,
}

impl AuthoritiesProof {
	pub fn verify_proof(&self, validators: &[Public]) -> Result<(), Error> {
		if (self.total_leaves as usize) != validators.len() {
			return Err(Error::MismatchTotalAuthorities {
				proof_size: self.total_leaves,
				validators_size: validators.len(),
			});
		};

		for (leaf_index, proof) in &self.items {
			// access the validator
			let leaf = &validators[*leaf_index as usize];

			// verify
			if !verify_proof::<KeccakHasher, _, _>(
				&self.root,
				proof.clone(),
				self.total_leaves,
				*leaf_index,
				leaf,
			) {
				return Err(Error::MismatchAuthority {
					root: self.root,
					leaf_index: *leaf_index,
					validator: leaf.clone(),
				});
			}
		}

		Ok(())
	}
}

/// Returns a tuple of
/// (
/// 	AuthoritiesProof, using Keccak256 hashing
/// 	Signers in the commitment
/// )
///
/// # Arguments
/// * `beefy_signed_commitment` - the commitment file signed by majority of the authorities in beefy
/// * `validator_set` - the current active validators
pub fn generate_authorities_proof(
	beefy_signed_commitment: &BeefySignedCommitment,
	validator_set: &ValidatorSet<Public>,
) -> Result<(AuthoritiesProof, Signers), Error> {
	// checking of the block number is not important, when creating this proof
	let block_number = beefy_signed_commitment.commitment.block_number;

	// verify the signatures in the commitment are from the validator set
	beefy_signed_commitment
		.verify_signatures::<_, BeefySignatureHasher>(block_number, validator_set)
		.map_err(|_| Error::NoMatchingSignature(block_number))?;

	// collect all the indices (similar index position in the validator set) with signatures
	let sig_indices: Vec<usize> = beefy_signed_commitment
		.signatures
		.iter()
		.enumerate()
		// skip the indices with no signatures
		.filter_map(|(index, sig)| sig.clone().map(|_| index))
		.collect();

	let validators = validator_set.validators();
	// calculate the root hash, which is the same as the "keyset_commitment" of the BeefyAuthoritySet
	let root = binary_merkle_tree::merkle_root::<KeccakHasher, _>(validators);

	let mut items = vec![];
	let mut signers = vec![];

	for sig in sig_indices {
		// create a proof for EACH signer in the commitment, using its index
		let proof =
			binary_merkle_tree::merkle_proof::<KeccakHasher, _, _>(validators, sig as LeafIndex);

		items.push((proof.leaf_index, proof.proof));
		signers.push(validators[sig].clone());
	}

	Ok((AuthoritiesProof { root, total_leaves: validators.len() as u32, items }, signers))
}
