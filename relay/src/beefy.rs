use sp_core::H256;

use mmr_rpc::LeavesProof;
use subxt::ext::codec::{Decode, Encode};

use sp_consensus_beefy::{
	ValidatorSet,
	ecdsa_crypto::{Public, Signature},
	known_payloads::MMR_ROOT_ID,
	mmr::BeefyAuthoritySet,
};
use sp_mmr_primitives::{
	EncodableOpaqueLeaf, LeafIndex, LeafProof, NodeIndex,
	mmr_lib::{helper::get_peaks, leaf_index_to_mmr_size},
	utils::NodesUtils,
};

use crate::{
	cardano_encoding::SignedCommitment,
	error::Error,
	helpers::HexBeefyRelayChainProof,
	keccak::{AuthoritiesProof, generate_authorities_proof},
	types::{Block, ExtraData},
};

pub type BeefySignedCommitment = sp_consensus_beefy::SignedCommitment<Block, Signature>;

pub type MmrLeaf = sp_consensus_beefy::mmr::MmrLeaf<Block, H256, H256, ExtraData>;

pub struct BeefyRelayChainProof {
	pub mmr_proof: LeavesProof<H256>,
	pub authorities_proof: AuthoritiesProof,
	pub signed_commitment: BeefySignedCommitment,
	/// list of signers from the commitment file
	pub signers: Vec<Public>,
}

impl BeefyRelayChainProof {
	/// Returns the mmr proof and authorities proof after verifying gainst the current active validators
	///
	/// # Arguments
	/// * `mmr_proof` - contains the latest leaf of the mmr and its proof in the latest mmr root hash
	/// * `beefy_signed_commitment` - the commitment file signed by majority of the authorities in beefy
	/// * `validator_set` - the current active validators
	/// * `expected_next_authorities` - the next authorities, that should be similar in the data of the latest leaf
	pub fn create(
		mmr_proof: LeavesProof<H256>,
		beefy_signed_commitment: BeefySignedCommitment,
		validator_set: ValidatorSet<Public>,
		expected_next_authorities: BeefyAuthoritySet<H256>,
	) -> Result<Self, Error> {
		// verify that the next authorities is the same in the provided proof
		verify_next_authority_set(expected_next_authorities, &mmr_proof)?;

		// generate proofs for each signer in the commitment, with the validator set as basis
		let (authorities_proof, signers) =
			generate_authorities_proof(&beefy_signed_commitment, &validator_set)?;

		Ok(BeefyRelayChainProof {
			mmr_proof,
			authorities_proof,
			signed_commitment: beefy_signed_commitment,
			signers,
		})
	}

	/// outputs the entire proof
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
		self.mmr_proof.block_hash
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
			&self.signers,
		)
	}

	/// Returns a list of peaks per leaf index, taken from the `mmr_proof``
	pub fn peak_nodes(&self) -> PeakNodes {
		let leaf_proof = self.leaf_proof();

		// use the biggest leaf index in the list of indices, to get the peaks.
		let max_leaf_index =
			leaf_proof.leaf_indices.iter().max().expect("should return the max index");

		// determines the number of nodes, give the
		let mmr_size = leaf_index_to_mmr_size(*max_leaf_index);
		let peaks = get_peaks(mmr_size);

		let utils = NodesUtils::new(*max_leaf_index);
		let num_of_peaks: u64 = utils.number_of_peaks();

		PeakNodes { peaks, num_of_peaks, mmr_size }
	}

	/// Returns the decoded leaves of the `mmr_proof``
	pub fn mmr_leaves(&self) -> Result<Vec<MmrLeaf>, Error> {
		get_mmr_leaves(&self.mmr_proof)
	}

	/// Returns all the node hashes of the peaks
	pub fn node_hashes(&self) -> Vec<H256> {
		let leaf_proof = self.leaf_proof();
		let result = leaf_proof.items;

		result
	}

	/// Returns the decoded proof of the last leaf, taken from the `mmr_proof``
	pub fn leaf_proof(&self) -> LeafProof<H256> {
		let leaf_proof_as_bytes = &self.mmr_proof.proof;

		Decode::decode(&mut &leaf_proof_as_bytes.0[..]).expect("Failed to decode to LeafProof")
	}
}

///  The peaks in the `mmr_proof` with additional data
#[derive(Debug)]
pub struct PeakNodes {
	/// the peaks (with its node index) of the mmr
	pub peaks: Vec<NodeIndex>,

	/// the total number of peaks in the `mmr_proof`
	pub num_of_peaks: u64,

	/// the number of nodes in this mmr
	pub mmr_size: u64,
}

/// Verifies the next authority set is similar in the provided proof
fn verify_next_authority_set(
	next_auth_set: BeefyAuthoritySet<H256>,
	mmr_proof: &LeavesProof<H256>,
) -> Result<(), Error> {
	let mmr_leaves = get_mmr_leaves(mmr_proof)?;

	for leaf in mmr_leaves {
		if leaf.beefy_next_authority_set != next_auth_set {
			return Err(Error::InvalidNextAuthoritySet {
				expected: next_auth_set,
				actual: leaf.beefy_next_authority_set,
			});
		}
	}

	Ok(())
}

fn get_mmr_leaves(mmr_proof: &LeavesProof<H256>) -> Result<Vec<MmrLeaf>, Error> {
	let mut mmr_leaves = vec![];

	let leaves = &mmr_proof.leaves.0;
	let leaves: Vec<EncodableOpaqueLeaf> = Decode::decode(&mut &leaves[..])?;

	for leaf in leaves {
		let leaf_as_bytes = leaf.into_opaque_leaf().0;

		let mmr_leaf: MmrLeaf = Decode::decode(&mut &leaf_as_bytes[..])?;

		mmr_leaves.push(mmr_leaf);
	}

	Ok(mmr_leaves)
}
