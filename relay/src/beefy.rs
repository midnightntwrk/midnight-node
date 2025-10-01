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
	EncodableOpaqueLeaf, LeafIndex, LeafProof,
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
	pub consensus_proof: LeavesProof<H256>,
	//todo
	pub authorities_proof: AuthoritiesProof,
	pub signed_commitment: BeefySignedCommitment,
	pub validators: Vec<Public>,
}

impl BeefyRelayChainProof {
	pub fn create(
		consensus_proof: LeavesProof<H256>,
		signed_commitment: BeefySignedCommitment,
		validator_set: ValidatorSet<Public>,
		next_authorities: BeefyAuthoritySet<H256>,
	) -> Result<Self, Error> {
		verify_next_authority_set(next_authorities, &consensus_proof)?;

		let authorities_proof = generate_authorities_proof(&signed_commitment, &validator_set)?;

		Ok(BeefyRelayChainProof {
			consensus_proof,
			authorities_proof,
			signed_commitment,
			validators: validator_set.validators().to_vec(),
		})
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

	/// Returns a list of peaks per leaf index, taken from the LeafProof
	pub fn peak_nodes(&self) -> Vec<PeakNode> {
		let leaf_proof = self.leaf_proof();
		leaf_proof
			.leaf_indices
			.iter()
			.map(|leaf_index| {
				let mmr_size = leaf_index_to_mmr_size(*leaf_index);
				let peaks = get_peaks(mmr_size);

				let utils = NodesUtils::new(*leaf_index);
				let num_of_peaks: u64 = utils.number_of_peaks();
				// println!(
				// 	"\nNumber of peaks {num_of_peaks}: of leaf index({leaf_index}) with mmr size({mmr_size})"
				// );

				PeakNode { leaf_index: *leaf_index, peaks, num_of_peaks, mmr_size }
			})
			.collect()
	}

	/// Returns the decoded leaves of `LeavesProof`
	pub fn mmr_leaves(&self) -> Result<Vec<MmrLeaf>, Error> {
		get_mmr_leaves(&self.consensus_proof)
	}

	/// Returns all the node hashes of the peaks
	pub fn node_hashes(&self) -> Vec<H256> {
		let leaf_proof = self.leaf_proof();
		let result = leaf_proof.items;

		result
	}

	/// Returns the decoded `LeafProof`, from `LeavesProof`
	pub fn leaf_proof(&self) -> LeafProof<H256> {
		let leaf_proof_as_bytes = &self.consensus_proof.proof;

		Decode::decode(&mut &leaf_proof_as_bytes.0[..]).expect("Failed to decode to LeafProof")
	}
}

#[derive(Debug)]
pub struct PeakNode {
	pub leaf_index: LeafIndex,
	pub peaks: Vec<u64>,
	pub num_of_peaks: u64,
	pub mmr_size: u64,
}

fn verify_next_authority_set(
	next_auth_set: BeefyAuthoritySet<H256>,
	consensus_proof: &LeavesProof<H256>,
) -> Result<(), Error> {
	let mmr_leaves = get_mmr_leaves(consensus_proof)?;

	for leaf in mmr_leaves {
		if leaf.beefy_next_authority_set != next_auth_set {
			return Err(Error::InvalidNextAuthoritySet {
				expected: leaf.beefy_next_authority_set,
				actual: next_auth_set,
			});
		}
	}

	Ok(())
}

fn get_mmr_leaves(consensus_proof: &LeavesProof<H256>) -> Result<Vec<MmrLeaf>, Error> {
	let mut mmr_leaves = vec![];

	let leaves = &consensus_proof.leaves.0;
	let leaves: Vec<EncodableOpaqueLeaf> = Decode::decode(&mut &leaves[..])?;

	for leaf in leaves {
		let leaf_as_bytes = leaf.into_opaque_leaf().0;

		let mmr_leaf: MmrLeaf = Decode::decode(&mut &leaf_as_bytes[..])?;

		mmr_leaves.push(mmr_leaf);
	}

	Ok(mmr_leaves)
}
