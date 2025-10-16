use std::{fmt::Debug, ops::Deref};

use crate::{
	cardano_encoding::{TAG, ToPlutusData},
	error::Error,
	types::{BeefySignedCommitment, Hash, RootHash},
};

use pallas::{
	codec::utils::MaybeIndefArray,
	ledger::primitives::{BigInt, BoundedBytes, Constr, PlutusData},
};
use rs_merkle::proof_tree::ProofNode;
use sp_consensus_beefy::{BeefySignatureHasher, ValidatorSet, ecdsa_crypto::Public};
use sp_core::{H256, keccak_256};

#[derive(Clone)]
pub struct KeccakHasher;

impl rs_merkle::Hasher for KeccakHasher {
	type Hash = Hash;
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

impl AuthoritiesProof {
	/// Returns AuthoritiesProof, using Keccak256 hashing
	///
	/// # Arguments
	/// * `beefy_signed_commitment` - the commitment file signed by majority of the authorities in beefy
	/// * `validator_set` - the current active validators
	pub fn try_new(
		beefy_signed_commitment: &BeefySignedCommitment,
		validator_set: &ValidatorSet<Public>,
	) -> Result<Self, Error> {
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

		let v: Vec<Hash> =
			validators.iter().map(|v| keccak_256(&v.clone().into_inner().0)).collect();

		let tree = rs_merkle::MerkleTree::<KeccakHasher>::from_leaves(&v);

		// calculate the root hash, which is the same as the "keyset_commitment" of the BeefyAuthoritySet
		let root = tree.root().ok_or(Error::InvalidAuthoritiesProofCreation)?;
		let root = H256::from_slice(&root);

		let proof = tree.ordered_proof_tree(&sig_indices);

		Ok(AuthoritiesProof { root, total_leaves: validators.len() as u32, proof })
	}
}

// convert the ProofNode to plutusdata
fn proof_node_to_plutus_data<T: Clone + Into<Vec<u8>>>(proof: &ProofNode<T>) -> PlutusData {
	match proof {
		ProofNode::Leaf(hash) => PlutusData::BoundedBytes(BoundedBytes::from(hash.clone().into())),
		ProofNode::Node(nodes) => PlutusData::Array(MaybeIndefArray::Indef(
			nodes.iter().map(|node| proof_node_to_plutus_data(node.deref())).collect(),
		)),
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		authorities::{KeccakHasher, proof_node_to_plutus_data},
		helpers::ToHex,
	};
	use rs_merkle::MerkleTree;
	use sp_core::keccak_256;

	// ECDSA Keys
	const ALICE: &str = "0x020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a1";
	const BOB: &str = "0x0390084fdbf27d2b79d26a4f13f0ccd982cb755a661969143c37cbc49ef5b91f27";
	const CHARLIE: &str = "0x0389411795514af1627765eceffcbd002719f031604fadd7d188e2dc585b4e1afb";
	const DAVE: &str = "0x03bc9d0ca094bd5b8b3225d7651eac5d18c1c04bf8ae8f8b263eebca4e1410ed0c";
	const EVE: &str = "0x031d10105e323c4afce225208f71a6441ee327a65b9e646e772500c74d31f669aa";
	const FERDIE: &str = "0x0291f1217d5a04cb83312ee3d88a6e6b33284e053e6ccfc3a90339a0299d12967c";
	const GEORGE: &str = "0x032fd22c2a15d1d45395db478f8a21c6a386a1370a9a4f9007ceb7c518ab8ed3b5";
	const HELEN: &str = "0x0218651a8672108c0ddc7f89861155362ec9644c17a0349d1963df2fa8cac99db4";
	const IAN: &str = "0x0366ba055f22be271e382cecb8c040d3a8d28dd93139ef71d35886290b3f5f853d";
	const JASON: &str = "0x03bafd6bdaa7de4e0866d124f36a45d8dff1f2729a0cbdceeb545887b8ec9f8bf1";
	const KATE: &str = "0x0329a36b4032e289b139dcce82d6a26ee390502eea596b71eac53449491b1072b2";
	const LEE: &str = "0x031b0e3acea7c21c161fe56c4430c051544701897a4a5e4f675db200d36f80d339";
	const MARY: &str = "0x02fde7f65c7297a525b436f086c456775c75e5d694f6cb0247609f72cd5b05b0c5";
	const NEIL: &str = "0x0342a70472b9a5ebae8a62297426bdef31efb38115976152b19df206749d38a254";
	const OSCAR: &str = "0x02f7b57a536759f4103a689e3c70063badb6e343b3f17f312599c750835cde7d2d";
	const PAUL: &str = "0x03cb3d1122b035a4f7a024ba14717206db1af7f8e93c9ca50604f2ab0abfb71b4a";
	const QUEEN: &str = "0x0209493bc9f358eb9550b0caf835de24a18c8b61dea93e782dc101f5b236711387";
	const RAY: &str = "0x03b3455feb7dfa426ec764c91c90200fa47a5a99dac4050a16c3c9507e37c0a28e";
	const SALLY: &str = "0x03f39efa906e1a1aa881e69fbe9f8b00c89c9514d2863c77bc1b03da9f4e7af458";
	const TRAVIS: &str = "0x03b2b93c48ad101362e7a0870838dcbf1bd623b62c992c20ca65251a6161f3595c";
	const UNA: &str = "0x034e33866019a1969056e561f2c59167a817f5bf17aa5b40f5849830d50580dec0";
	const VIVIAN: &str = "0x02bed57f739247d0c94b9b3a9b88fe090aa4dd5c31e9407cbadd11758cc53f4e22";
	const WILL: &str = "0x036f1977a04a395f2d2db23373e6da456a8a865bf15bda3a539f890ebb5b6f9015";
	const XAVIER: &str = "0x0329e43c00d6c79174a1f74e2c9271e4ad664fe005cb6f77f2e73a9d4e1d05bcc5";
	const YURI: &str = "0x0248cba650b828994cd0629b4502d045a01b2e99a40a7c6c163efae8020bc72ee1";
	const ZELDA: &str = "0x02896a2ead73860a453e60aa0d3b59ce06a4f180d8ef937f3b3ef96ea6a10c7073";

	fn validators() -> Vec<[u8; 32]> {
		let data = [
			ALICE, BOB, CHARLIE, DAVE, EVE, FERDIE, GEORGE, HELEN, IAN, JASON, KATE, LEE, MARY,
			NEIL, OSCAR, PAUL, QUEEN, RAY, SALLY, TRAVIS, UNA, VIVIAN, WILL, XAVIER, YURI, ZELDA,
		];

		data.iter()
			.enumerate()
			.map(|(idx, d)| {
				let bytes = d.as_bytes();

				let keccak = keccak_256(bytes);

				println!("V{idx}: {}", hex::encode(keccak));

				keccak
			})
			.collect()
	}

	#[test]
	fn test_ordered_proof_tree() {
		let leaves = validators()[..10].to_vec();

		let chosen = vec![0, 7, 9];

		let tree = MerkleTree::<KeccakHasher>::from_leaves(&leaves);

		let proof = tree.ordered_proof_tree(&chosen);
		println!("proof_node: {proof:#?}");
		let proof_to_plutus = proof_node_to_plutus_data(&proof);

		let actual_plutus_hex = proof_to_plutus.as_hex();
		println!("\nplutux hex: {actual_plutus_hex}");

		let expected_plutus_hex = "9f9f9f9f58202b61e2fc26f9440e8ab4dac2a9257510a5c11d56db1fa56f5f7e24cfa227e46458203d5e37ef0229d121e6e644c4a0d8680cc3af892eec11b865c0d83e9d9cb0b087ff5820d2e4d978b047dbd39f53bbcc249f07bc3ca4ed154499cf055df8124bb81db9c1ff9f5820d7d428326173db529e5207b6d907b69aa9288fcd37b792c7458706ea8603c86c9f58209b862519cc6f905c7baf6741bb7593ed7d63655e4e4a5d38d68204aa9b3c43d658200ca08abbdcbe73bd5350e1b83b5b4db36913936173ba62587f7a8c6985f9aa20ffffff9f582071d1061a002931d5f7b622705a864c507166d2694a581006bda0aa545fc5145c5820819a9ac0e4050a01b8f38fd987a294c8032924f6cf14428a2e89621766ad49e8ffff";
		assert_eq!(actual_plutus_hex, expected_plutus_hex.to_string());

		let root = tree.root().unwrap();
		let root_hex = hex::encode(root);
		println!("\nRoot: {root_hex}");
	}
}
