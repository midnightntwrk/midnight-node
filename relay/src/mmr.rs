use parity_scale_codec::Decode;
use sp_core::H256;
use sp_mmr_primitives::EncodableOpaqueLeaf;

use crate::{BlockNumber, Error, LeafExtra, MmrProof};

pub type ExtraData = Vec<u8>;
pub type MmrLeaf = sp_consensus_beefy::mmr::MmrLeaf<BlockNumber, H256, H256, ExtraData>;

pub fn extract_leaf_extra_from_mmr_proof(mmr_proof: &MmrProof) -> Result<LeafExtra, Error> {
	let leaf = extract_leaf(mmr_proof)?;
	extract_leaf_extra_from_mmr_leaf(leaf)
}

/// Get the leaves from the mmr_proof. The generated proof is mostly for 1 block;
/// the proof will always return just 1 leaf.
fn extract_leaf(mmr_proof: &MmrProof) -> Result<MmrLeaf, Error> {
	let leaves = &mmr_proof.leaves.0;
	let leaves: Vec<EncodableOpaqueLeaf> = Decode::decode(&mut &leaves[..])?;

	// only decode the first leaf found
	let Some(leaf) = leaves.into_iter().next() else {
		return Err(Error::NoLeafFound);
	};

	let leaf_as_bytes = leaf.into_opaque_leaf().0;

	Decode::decode(&mut &leaf_as_bytes[..]).map_err(Error::Codec)
}

fn extract_leaf_extra_from_mmr_leaf(mmr_leaf: MmrLeaf) -> Result<LeafExtra, Error> {
	let leaf_extra = mmr_leaf.leaf_extra;
	Decode::decode(&mut &leaf_extra[..]).map_err(Error::Codec)
}

#[cfg(test)]
mod test {
	use parity_scale_codec::Decode;

	use crate::{
		BeefyId,
		mmr::{MmrLeaf, extract_leaf_extra_from_mmr_leaf},
	};

	const HEX_SCALE_ENCODED_LEAVES: &str = "006000000039a01a91078fef3e0aa8c92c6dee64b9255cb7c666fc340b5e66db5ec00fd5a6010000000000000004000000a33c1baaa379963ee43c3a7983a3157080c32a462a9774f1fe6d2f0480428e5ca804020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a10100000000000000";
	const ALICE_BEEFY_ID: &str =
		"020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a1";

	fn decode_data<T: Decode>(hex_data: &str) -> T {
		let hex_to_bytes = hex::decode(hex_data).expect("failed to convert hex to bytes");
		Decode::decode(&mut &hex_to_bytes[..]).expect("failed to convert to leaf")
	}

	#[test]
	fn test_decode_leaf_extra() {
		let mmr_leaf = decode_data::<MmrLeaf>(HEX_SCALE_ENCODED_LEAVES);

		let leaves_extra =
			extract_leaf_extra_from_mmr_leaf(mmr_leaf).expect("failed to extract leaf extra");

		assert_eq!(leaves_extra.len(), 1);

		let (auth_id, stake) = leaves_extra.into_iter().next().unwrap();
		assert_eq!(stake, 1);

		assert_eq!(auth_id, decode_data::<BeefyId>(ALICE_BEEFY_ID));
	}
}
