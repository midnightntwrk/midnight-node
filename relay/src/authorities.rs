#![allow(dead_code)]

use midnight_primitives_beefy::{BEEFY_LOG_TARGET, BeefyStake, BeefyStakes};
use rs_merkle::proof_tree::ProofNode;
use sp_consensus_beefy::{BeefySignatureHasher, ValidatorSet, ecdsa_crypto::Public as EcdsaPublic};
use sp_core::keccak_256;
use subxt::utils::to_hex;

use crate::{BeefyId, BeefySignedCommitment, Error, justification::BeefyStakesInfo};

pub type Hash = [u8; 32];
pub type RootHash = sp_core::H256;

/// Contains the merkle root hash of all authorities,
/// And the proof for a few chosen authorities
#[derive(Debug, Clone)]
pub struct AuthoritiesProof {
	pub root: RootHash,

	/// the total number of validators
	pub total_leaves: u32,

	/// a proof tree containing
	pub proof: ProofNode<Hash>,
}

impl AuthoritiesProof {
	/// Returns AuthoritiesProof, using Keccak256 hashing
	///
	/// # Arguments
	/// * `beefy_signed_commitment` - the commitment file signed by majority of the authorities in beefy
	/// * `validator_set` - the current active validators
	pub fn try_new(
		beefy_signed_commitment: &BeefySignedCommitment,
		validator_set: &ValidatorSet<EcdsaPublic>,
	) -> Result<Self, Error> {
		// collect signatures
		let sig_indices = collect_signature_indices(beefy_signed_commitment, validator_set)?;

		let payload = &beefy_signed_commitment.commitment.payload;
		let stakes_info = BeefyStakesInfo::try_from(payload)?;
		log::debug!(target: BEEFY_LOG_TARGET, "🥩 Beefy Stakes Info: {stakes_info:#?}");

		// convert current stakes into keccak hashes
		let keccak_hashes = prep_merkle_leaves(stakes_info.current_stakes);

		// create the merkle tree
		let tree = rs_merkle::MerkleTree::<KeccakHasher>::from_leaves(&keccak_hashes);

		// calculate the root hash, which is the same as the "keyset_commitment" of the BeefyAuthoritySet
		let root_slice = tree.root().ok_or(Error::InvalidAuthoritiesProofCreation)?;
		let root = RootHash::from_slice(&root_slice);

		let proof = tree.ordered_proof_tree(&sig_indices);

		Ok(AuthoritiesProof { root, total_leaves: validator_set.validators().len() as u32, proof })
	}
}

#[derive(Clone)]
pub struct KeccakHasher;

impl rs_merkle::Hasher for KeccakHasher {
	type Hash = Hash;
	fn hash(data: &[u8]) -> Self::Hash {
		keccak_256(data)
	}
}

/// Prepare the leaves to create the merkle tree
///
/// # Arguments
///
/// * `beefy_stakes` - contains the beefy ids/validators with their stakes
fn prep_merkle_leaves(beefy_stakes: BeefyStakes<BeefyId>) -> Vec<Hash> {
	// pair up the validators with its stakes
	beefy_stakes
		.into_iter()
		.enumerate()
		.map(|(idx, beefy_stake)| {
			let v = beefy_stake.0.clone();
			let keccak = prep_leaf_hash(beefy_stake);
			log::trace!(target: BEEFY_LOG_TARGET, "🥩 V({idx}): ecdsa: {v:?} keccak: {}", to_hex(keccak.as_slice()));

			keccak
		})
		.collect()
}

/// Create Leaf hash using keccak, based on the tuple of validator and its stake.
///
/// # Arguments
///
/// * `beefy_stake` - contains the beefy id/validator with its stake
fn prep_leaf_hash(beefy_stake: BeefyStake<BeefyId>) -> Hash {
	// convert public key to bytes
	let mut data = beefy_stake.0.into_inner().0.to_vec();

	// convert stake to bytes
	let stake_bytes = beefy_stake.1.to_le_bytes();

	// append the validator bytes with the stake bytes
	data.extend_from_slice(&stake_bytes);

	keccak_256(&data)
}

/// Verify and collect all the indices (similar index position in the validator set) with signatures
///
/// # Arguments
///
/// * `beefy_signed_commitment` - commitment file from the Beefy Justification
/// * `validator_set` - the current validator set
fn collect_signature_indices(
	beefy_signed_commitment: &BeefySignedCommitment,
	validator_set: &ValidatorSet<EcdsaPublic>,
) -> Result<Vec<usize>, Error> {
	// checking of the block number is not important, when creating this proof
	let block_number = beefy_signed_commitment.commitment.block_number;

	// verify the signatures in the commitment are from the validator set
	beefy_signed_commitment
		.verify_signatures::<_, BeefySignatureHasher>(block_number, validator_set)
		.map_err(|e| Error::NoMatchingSignature(block_number, e))?;

	Ok(beefy_signed_commitment
		.signatures
		.iter()
		.enumerate()
		// skip the indices with no signatures
		.filter_map(|(index, sig)| sig.clone().map(|_| index))
		.collect())
}

#[cfg(test)]
mod test {
	use super::Hash;
	use midnight_primitives_beefy::BeefyStakes;
	use sp_consensus_beefy::ValidatorSetId;
	use sp_core::bytes::from_hex;
	use subxt::utils::to_hex;

	use crate::{
		BeefyId, BeefySignedCommitment, BeefyValidatorSet,
		authorities::{collect_signature_indices, prep_leaf_hash, prep_merkle_leaves},
		helper::test::{ECDSA_ALICE, ECDSA_BOB, ECDSA_CHARLIE, ECDSA_DAVE, decode, get_ecdsa},
	};

	const ENCODED_BEEFY_COMMITMENT: &str = "0x146362b00000000000000000040000007f0c9b27381104febfb4a6be51e8fc0f08ba70060531fc5fcf60dcbed1f4e5f96373950210020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a100000000000000000390084fdbf27d2b79d26a4f13f0ccd982cb755a661969143c37cbc49ef5b91f2701000000000000000389411795514af1627765eceffcbd002719f031604fadd7d188e2dc585b4e1afb010000000000000003bc9d0ca094bd5b8b3225d7651eac5d18c1c04bf8ae8f8b263eebca4e1410ed0c00000000000000006d68805d6013253f0020cdae55a436208887cbd691f9cf93278497fc5e10aae814c4d06e62b0010000000000000004000000a5d8a7ba3b85661890415507aed407f1b3e7f86c0133b195ac43612171f5daca6e73950210020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a101000000000000000390084fdbf27d2b79d26a4f13f0ccd982cb755a661969143c37cbc49ef5b91f2700000000000000000389411795514af1627765eceffcbd002719f031604fadd7d188e2dc585b4e1afb010000000000000003bc9d0ca094bd5b8b3225d7651eac5d18c1c04bf8ae8f8b263eebca4e1410ed0c000000000000000081040000000000000000000004d0040000000cb128f92056bf1af3f4762e80071a6f42c55dee9f9c5044fb45173a86325ebd8c53d2478e29685cb3dfe929f0f887129d36865a116573c66c4edfd83384d3a3bd01691f180f0ff53d3fde30c992ff4fb3cad2a01089c1885e11d72a507a0c08ce9d2a412c772ba4877775a6521bb4cdca1a8809a9b8df7116c9abe6c0d67df7dd40014b768e1b85bcd09e1d562c59a24b12cafc8d4bab0b11c92873631c0552bbe379005eafcf82f25ba800a57e3debea0069e106d4eb85b73d207631756d84c8d1fe01";

	const EXPECTED_CHARLIE_3_STAKE: &str =
		"0x2d297d196ec83b90e18828db774fbee18d984cfee0fde6038fe1bb4d4d4ac96a";
	const EXPECTED_ALICE_1_STAKE: &str =
		"0x3b6cb1c06474ec7cf8cd27fa86009e7321d97a61257b8f5216e4c79161f928ca";

	fn sample_beef_stakes() -> BeefyStakes<BeefyId> {
		vec![
			(get_ecdsa(ECDSA_ALICE), 1),
			(get_ecdsa(ECDSA_BOB), 2),
			(get_ecdsa(ECDSA_CHARLIE), 3),
			(get_ecdsa(ECDSA_DAVE), 4),
		]
	}

	fn sample_validator_set(validator_set_id: ValidatorSetId) -> BeefyValidatorSet {
		let validators = vec![
			get_ecdsa(ECDSA_ALICE),
			get_ecdsa(ECDSA_BOB),
			get_ecdsa(ECDSA_CHARLIE),
			get_ecdsa(ECDSA_DAVE),
		];

		BeefyValidatorSet::new(validators, validator_set_id)
			.expect("should be able to create a validator set")
	}

	#[test]
	fn test_prep_leaf_hash() {
		let result = prep_leaf_hash((get_ecdsa(ECDSA_CHARLIE), 3));
		let hex_encoded_result = to_hex(result);
		assert_eq!(hex_encoded_result, EXPECTED_CHARLIE_3_STAKE);

		let result = prep_leaf_hash((get_ecdsa(ECDSA_ALICE), 1));
		let hex_encoded_result = to_hex(result);
		assert_eq!(hex_encoded_result, EXPECTED_ALICE_1_STAKE);
	}

	#[test]
	fn test_collect_signature_indices() {
		let beefy_commitment: BeefySignedCommitment = decode(ENCODED_BEEFY_COMMITMENT);

		let v_set = sample_validator_set(beefy_commitment.commitment.validator_set_id);

		let result = collect_signature_indices(&beefy_commitment, &v_set)
			.expect("failed to collect signatures");

		assert_eq!(result.len(), 3);
		assert!(result.contains(&0));
		assert!(result.contains(&1));
		assert!(!result.contains(&2));
		assert!(result.contains(&3));
	}

	#[test]
	fn test_prep_merkle_leaves() {
		let beef_stakes = sample_beef_stakes();

		let keccak_hashes = prep_merkle_leaves(beef_stakes);
		assert_eq!(keccak_hashes.len(), 4);

		let alice_stake_decoded =
			from_hex(EXPECTED_ALICE_1_STAKE).expect("failed to conver to bytes");
		let alice_stake_decoded: Hash =
			alice_stake_decoded.try_into().expect("failed to convert to sized array");
		assert!(keccak_hashes.contains(&alice_stake_decoded));

		let charlie_stake_decoded =
			from_hex(EXPECTED_CHARLIE_3_STAKE).expect("failed to conver to bytes");
		let charlie_stake_decoded: Hash =
			charlie_stake_decoded.try_into().expect("failed to convert to sized array");
		assert!(keccak_hashes.contains(&charlie_stake_decoded));
	}
}
