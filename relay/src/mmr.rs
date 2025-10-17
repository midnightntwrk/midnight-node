use crate::{
	error::Error,
	types::{Block, ExtraData, Hash},
};

use mmr_rpc::LeavesProof;
use parity_scale_codec::Decode;
use sp_consensus_beefy::mmr::BeefyAuthoritySet;
use sp_core::H256;
use sp_mmr_primitives::{
	EncodableOpaqueLeaf, LeafProof, NodeIndex,
	mmr_lib::{helper::get_peaks, leaf_index_to_mmr_size},
	utils::NodesUtils,
};

pub type MmrLeaf = sp_consensus_beefy::mmr::MmrLeaf<Block, H256, H256, ExtraData>;

///  The peaks in the `mmr_proof` with additional data
#[derive(Debug)]
pub struct PeakNodes {
	/// the peaks (with its node index) of the mmr
	pub peaks: Vec<NodeIndex>,

	/// the total number of peaks in the `mmr_proof`
	pub num_of_peaks: u64,

	#[allow(dead_code)]
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

	fn proof_items(&self) -> Vec<Hash>;

	/// Verifies the proof's next authority set is the same as the provided one
	fn verify_next_authority_set(
		&self,
		expected_next_auth_set: &BeefyAuthoritySet<H256>,
	) -> Result<(), Error>;
}

impl LeavesProofExt for LeavesProof<H256> {
	fn leaf_proof(&self) -> LeafProof<H256> {
		let leaf_proof_as_bytes = &self.proof;

		Decode::decode(&mut &leaf_proof_as_bytes.0[..]).expect("Failed to decode to LeafProof")
	}

	fn proof_items(&self) -> Vec<Hash> {
		let leaf_proof = self.leaf_proof();

		leaf_proof.items.iter().map(|item| item.0).collect()
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
		expected_next_auth_set: &BeefyAuthoritySet<H256>,
	) -> Result<(), Error> {
		let mmr_leaves = self.mmr_leaves()?;

		for leaf in mmr_leaves {
			if &leaf.beefy_next_authority_set != expected_next_auth_set {
				return Err(Error::InvalidNextAuthoritySet {
					expected: expected_next_auth_set.clone(),
					actual: leaf.beefy_next_authority_set,
				});
			}
		}

		Ok(())
	}
}

#[allow(clippy::from_over_into)]
impl Into<crate::cardano_encoding::BeefyMmrLeaf> for LeavesProof<H256> {
	fn into(self) -> crate::cardano_encoding::BeefyMmrLeaf {
		let proof = self.leaf_proof();

		// We are only proving one leaf at a time.
		let mut mmr_leaf = self.mmr_leaves().expect("LeavesProof missing leaves");
		// size of the mmr_leaves is always 1.
		let mmr_leaf = mmr_leaf.pop().unwrap();

		// combine the major and minor version
		let (major, minor) = mmr_leaf.version.split();
		let version = (major << 5) + minor;

		crate::cardano_encoding::BeefyMmrLeaf {
			version,
			parent_number: mmr_leaf.parent_number_and_hash.0,
			parent_hash: mmr_leaf.parent_number_and_hash.1.0.to_vec(),
			next_authority_set: mmr_leaf.beefy_next_authority_set.into(),
			extra: mmr_leaf.leaf_extra,
			k_index: 0,
			leaf_index: proof.leaf_indices[0],
		}
	}
}
