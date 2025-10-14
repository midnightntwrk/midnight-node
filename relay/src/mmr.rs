use mmr_rpc::LeavesProof;

use parity_scale_codec::Decode;
use sp_consensus_beefy::mmr::BeefyAuthoritySet;
use sp_core::H256;
use sp_mmr_primitives::{
	EncodableOpaqueLeaf, LeafProof, NodeIndex,
	mmr_lib::{helper::get_peaks, leaf_index_to_mmr_size},
	utils::NodesUtils,
};

use crate::{
	error::Error,
	types::{Block, ExtraData},
};

pub type MmrLeaf = sp_consensus_beefy::mmr::MmrLeaf<Block, H256, H256, ExtraData>;

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

pub trait LeavesProofExt {
	/// Decodes and access the proof data
	fn leaf_proof(&self) -> LeafProof<H256>;

	/// Identifies the peaks of this mmr proof
	fn peak_nodes(&self) -> PeakNodes;

	/// Returns only 1
	fn mmr_leaves(&self) -> Result<Vec<MmrLeaf>, Error>;

	/// Verifies the proof's next authority set is the same as the provided one
	fn verify_next_authority_set(
		&self,
		expected_next_auth_set: BeefyAuthoritySet<H256>,
	) -> Result<(), Error>;
}

impl LeavesProofExt for LeavesProof<H256> {
	fn leaf_proof(&self) -> LeafProof<H256> {
		let leaf_proof_as_bytes = &self.proof;

		Decode::decode(&mut &leaf_proof_as_bytes.0[..]).expect("Failed to decode to LeafProof")
	}

	fn peak_nodes(&self) -> PeakNodes {
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

	fn mmr_leaves(&self) -> Result<Vec<MmrLeaf>, Error> {
		let mut mmr_leaves = vec![];

		let leaves = &self.leaves.0;
		let leaves: Vec<EncodableOpaqueLeaf> = Decode::decode(&mut &leaves[..])?;

		for leaf in leaves {
			let leaf_as_bytes = leaf.into_opaque_leaf().0;

			let mmr_leaf: MmrLeaf = Decode::decode(&mut &leaf_as_bytes[..])?;

			mmr_leaves.push(mmr_leaf);
		}

		Ok(mmr_leaves)
	}

	fn verify_next_authority_set(
		&self,
		expected_next_auth_set: BeefyAuthoritySet<H256>,
	) -> Result<(), Error> {
		let mmr_leaves = self.mmr_leaves()?;

		for leaf in mmr_leaves {
			if leaf.beefy_next_authority_set != expected_next_auth_set {
				return Err(Error::InvalidNextAuthoritySet {
					expected: expected_next_auth_set,
					actual: leaf.beefy_next_authority_set,
				});
			}
		}

		Ok(())
	}
}
