use std::fmt::Debug;

use crate::{
	beefy::BeefySignedCommitment,
	cardano_encoding::{TAG, ToPlutusData},
	error::Error,
	types::RootHash,
};

use pallas::{
	codec::utils::MaybeIndefArray,
	ledger::primitives::{BigInt, BoundedBytes, Constr, PlutusData},
};
use rs_merkle::proof_tree::ProofNode;
use sp_consensus_beefy::{BeefySignatureHasher, ValidatorSet, ecdsa_crypto::Public};
use sp_core::{H256, keccak_256};

pub type LeafIndex = u32;
pub type ProofItems = Vec<(LeafIndex, H256)>;
pub type Signers = Vec<Public>;

#[derive(Clone)]
pub struct KeccakHasher;

impl rs_merkle::Hasher for KeccakHasher {
	type Hash = [u8; 32];
	fn hash(data: &[u8]) -> Self::Hash {
		keccak_256(data)
	}
}

/// Contains the merkle root hash of all authorities,
/// And the proof for a few chosen authorities
#[derive(Debug, Clone)]
pub struct AuthoritiesProof {
	pub root: RootHash,

	/// the total number of validators
	pub total_leaves: u32,

	/// a proof tree containing
	pub proof: ProofNode<[u8; 32]>,
}

impl ToPlutusData for AuthoritiesProof {
	fn to_plutus_data(&self) -> PlutusData {
		PlutusData::Constr(Constr {
			tag: TAG,
			any_constructor: None,
			fields: MaybeIndefArray::Indef(vec![
				PlutusData::BoundedBytes(self.root.as_bytes().to_vec().into()),
				PlutusData::BigInt(BigInt::Int((self.total_leaves as i64).into())),
				proof_node_to_plutus_data(&self.proof),
			]),
		})
	}
}
/// Returns AuthoritiesProof, using Keccak256 hashing
///
/// # Arguments
/// * `beefy_signed_commitment` - the commitment file signed by majority of the authorities in beefy
/// * `validator_set` - the current active validators
pub fn generate_authorities_proof(
	beefy_signed_commitment: &BeefySignedCommitment,
	validator_set: &ValidatorSet<Public>,
) -> Result<AuthoritiesProof, Error> {
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

	let v: Vec<[u8; 32]> =
		validators.iter().map(|v| keccak_256(&v.clone().into_inner().0)).collect();

	let tree = rs_merkle::MerkleTree::<KeccakHasher>::from_leaves(&v);

	// calculate the root hash, which is the same as the "keyset_commitment" of the BeefyAuthoritySet
	let root = tree.root().ok_or(Error::InvalidAuthoritiesProofCreation)?;
	let root = H256::from_slice(&root);

	let proof = tree.ordered_proof_tree(&sig_indices);

	Ok(AuthoritiesProof { root, total_leaves: validators.len() as u32, proof })
}

// convert the ProofNode to plutusdata
fn proof_node_to_plutus_data<T: Clone + Into<Vec<u8>>>(proof: &ProofNode<T>) -> PlutusData {
	match proof {
		ProofNode::Leaf(hash) => PlutusData::BoundedBytes(BoundedBytes::from(hash.clone().into())),
		ProofNode::Node(nodes) => PlutusData::Array(MaybeIndefArray::Indef(
			// Node
			nodes
				.iter()
				.map(|node: &Box<ProofNode<T>>| proof_node_to_plutus_data(node))
				.collect(),
		)),
	}
}

#[cfg(test)]
mod tests {
	use crate::authorities::proof_node_to_plutus_data;
	use pallas::codec::minicbor::to_vec;
	use rs_merkle::{Hasher, MerkleTree, algorithms::Sha256};

	// aura keys
	const ALICE: &str = "0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d";
	const BOB: &str = "0x8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48";
	const CHARLIE: &str = "0x90b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe22";
	const DAVE: &str = "0x306721211d5404bd9da88e0204360a1a9ab8b87c66c1bc2fcdd37f3c2222cc20";
	const EVE: &str = "0xe659a7a1628cdd93febc04a4e0646ea20e9f5f0ce097d9a05290d4a9e054df4e";
	const FERDIE: &str = "0x1cbd2d43530a44705ad088af313e18f80b53ef16b36177cd4b77b846f2a5f07c";
	const GEORGE: &str = "0x4603307f855321776922daeea21ee31720388d097cdaac66f05a6f8462b31757";
	const HELEN: &str = "0x6a59f7fc9be2dbb25f4657c5595066847f49ef28ea5b1608de85f2a8889f161b";
	const IAN: &str = "0x1206960f920a23f7f4c43cc9081ec2ed0721f31a9bef2c10fd7602e16e08a32c";
	const JASON: &str = "0xea7fd19a248dcb4df01ddb3eb8b5182ac4e746c30eaff6ac8037eff5f5f62b79";
	const KATE: &str = "0x065c1bec3ac7d3ea775d2f67ad7fc1af31eeb7a597c5e69586329077f7b3c91b";
	const LEE: &str = "0xf8df1d2141c540964438e216c68f983d4ab7e6c404dc0a4129f34b2c7e8e1e78";
	const MARY: &str = "0x449d48b26d27c3749d18f0c222118a8a56f89082f4c274c7efe6a3c5796ea116";
	const NEIL: &str = "0x465a0ea4af3a0602b316adb99135d709d61197f773214441142cd7f345800324";
	const OLLY: &str = "0x62438644119f708a0bdbdf023408f510d06f12b0071d24e24cf6e5b0eebb955f";
	const PAUL: &str = "0x88c0baf245f96a3e413e1d0caf26a2385545498411974111d3b83bdc92d3f37f";
	const QUEEN: &str = "0xe6f1747cf558b97a3c304241e989967e2eb7e3b22aa40da0c7b06d42e7e0c710";
	const RAY: &str = "0xd42831d4464ee5274a6dc766702f4dd23900e785021fe70c2705a45c6e70204f";
	const SALLY: &str = "0xb82a2b215c1822e62d58deac13fc923cacaf3f1cc6c87d22d9f30a3f5897e504";
	const TRAVIS: &str = "0x6420e16c5bc3447093208106ac06eeee732205231ecaf6d76ae4a20718553367";
	const UNA: &str = "0xa612a23c6d125798859b53de6da9f94da9959677baea6af2393d77f34fb3851e";
	const VIVIAN: &str = "0x98caac7fc712d1f6671eda36ca6bf495c9014200d50e4e4ec89a4e6936e08f3f";
	const WALTER: &str = "0x2e0884eda15ebe70e6f65fecb952b4c80f0dd45ad255abf5d97aced8c0822410";
	const XAVIER: &str = "0xa413ba4813feeedec4c761f0244f2fb1e5f08e00469df33450a800f707a34b74";
	const YURI: &str = "0x2c1d0a5982e73053480ab86462baf6d415858eca5ed27f3f1973db5f1b21ce20";
	const ZELDA: &str = "0x9a5867e72be0dc48bec83f58d1d31cff36e617dc90a36974b22dbe81ecec4911";

	fn validators() -> Vec<[u8; 32]> {
		let data = [
			ALICE, BOB, CHARLIE, DAVE, EVE, FERDIE, GEORGE, HELEN, IAN, JASON, KATE, LEE, MARY,
			NEIL, OLLY, PAUL, QUEEN, RAY, SALLY, TRAVIS, UNA, VIVIAN, WALTER, XAVIER, YURI, ZELDA,
		];

		data.iter()
			.enumerate()
			.map(|(idx, d)| {
				let bytes = d.as_bytes();

				let hash_bytes = Sha256::hash(bytes);
				let bytes_hex = hex::encode(&hash_bytes);
				println!("V({idx}): {bytes_hex}");

				hash_bytes
			})
			.collect()
	}

	#[test]
	fn test_ordered_proof_tree() {
		let leaves = validators()[..10].to_vec();

		let chosen = vec![0, 7, 9];

		let tree = MerkleTree::<Sha256>::from_leaves(&leaves);

		let proof = tree.ordered_proof_tree(&chosen);
		let proof_to_plutus = proof_node_to_plutus_data(&proof);
		let plutus_to_vec = to_vec(proof_to_plutus).expect("should return a vec");
		let actual_plutus_hex = hex::encode(&plutus_to_vec);

		let expected_plutus_hex = "9f9f9f9f5820cadd8a0f816db29c89dda607a3665b3fd26416c60c9580386a681e8215cbc1765820fbf1f275d3f00db5c92788433e89084528314599aefc29572a92acff507fddcaff5820d59baecc4c589f7f51d3f6f99fc683afe506970f76d6860a29900a97b604bf30ff9f5820c66a47df434a6cfe8f1624365f768596210dae7e7941012e06b659cb8f9d99979f582058e86be9514e9fc51737ff3def8204ea6586c170d9219820c2bcbfe8c6dcc3685820d07bb8191891e5c6d110dfd2f09bdf9059260d1c2624081d266ef6c87e41c971ffffff9f5820644ea9de200aacff70588f72790f7c214c3b22c5b158dcf89c452dac1da383645820fb34e5d1e29695e563670c4b411d5d8019045416733e80d358cf082d23259fc2ffff";

		assert_eq!(actual_plutus_hex, expected_plutus_hex.to_string());
	}
}
