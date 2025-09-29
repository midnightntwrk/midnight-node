use crate::{beefy::BeefySignedCommitment, mn_meta};

use rs_merkle::{MerkleProof, MerkleTree};

use sp_consensus_beefy::{ecdsa_crypto::{Public, Signature}, ValidatorSet, BeefySignatureHasher};
use sp_core::keccak_256;
use sp_mmr_primitives::mmr_lib::MerkleProof as PolkadotMerkleProof;



#[derive(Clone)]
pub struct KeccakHasher; 
impl rs_merkle::Hasher for KeccakHasher {
	type Hash = [u8;32];

	fn hash(data: &[u8]) -> Self::Hash {
		keccak_256(data)
	}
} 

pub fn authorities_merkle_tree(beefy_signed_commitment: &BeefySignedCommitment, validator_set: &ValidatorSet<Public>) -> Option<MerkleTree<KeccakHasher>> {
    let block_number = beefy_signed_commitment.commitment.block_number;
   
    beefy_signed_commitment.verify_signatures::<_,BeefySignatureHasher>(block_number, validator_set)
		.expect("validator set is not the same as in the commitment");


		// Vec<proof_hashes>
	let keccak_validators:Vec<[u8;32]> = validator_set.validators().iter().map(|validator| {
		// ecdsa public is [u8;33] ; Hashing immediately to keccak
		// [u8:33]
		keccak_256(validator.as_ref())
	}
	).collect();

	let tree = MerkleTree::<KeccakHasher>::from_leaves(&keccak_validators);

	let leaves = tree.leaves();
	println!("QWERTYQWERT QWERTY.      leaves: {leaves:?}");

    
	Some(tree)
}

pub fn get_authorities_proof(authorities_tree:&MerkleTree<KeccakHasher>, commitment:&BeefySignedCommitment) -> MerkleProof<KeccakHasher> {

    // number of validators should equate to the number of signatures in the SignedCommitment
	let sig_indices: Vec<usize> = commitment.signatures.iter().enumerate().map(|(index,_)| index).collect();

	let result = authorities_tree.proof(&sig_indices);

	let proof_hashes = result.proof_hashes();

	println!("--------------------- {proof_hashes:?}");
	

    result
}



// pub fn authorities_proof(beefy_signed_commitment: &BeefySignedCommitment, validator_set: &ValidatorSet<Public>) {
//     let mmr_size  = beefy_signed_commitment.signature_count();

//     let validators_hashes = validator_set.


//     PolkadotMerkleProof::new(mmr_size, proof)
// }