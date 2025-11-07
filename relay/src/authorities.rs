use std::fmt::Debug;

use rs_merkle::proof_tree::ProofNode;
use sp_consensus_beefy::{BeefySignatureHasher, ValidatorSet, ecdsa_crypto::Public as EcdsaPublic};
use sp_core::keccak_256;

use crate::{BeefyIdWithStake, BeefyIdsWithStakes, BeefySignedCommitment, Error, helper::HexExt};

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
	/// * `extra_data` - contains all the beefy validators and their corresponding stakes
	pub fn try_new(
		beefy_signed_commitment: &BeefySignedCommitment,
		validator_set: &ValidatorSet<EcdsaPublic>,
		beefy_ids_and_stakes: &mut BeefyIdsWithStakes,
	) -> Result<Self, Error> {
		// collect signatures
		let sig_indices = collect_signature_indices(beefy_signed_commitment, validator_set)?;

		// extract only the validators
		let ecdsa_validators = validator_set.validators();

		// merge ecdsa validators and their respective stakes, then convert into keccak hashes
		let keccak_hashes = prep_merkle_leaves(ecdsa_validators, beefy_ids_and_stakes);

		// create the merkle tree
		let tree = rs_merkle::MerkleTree::<KeccakHasher>::from_leaves(&keccak_hashes);

		// calculate the root hash, which is the same as the "keyset_commitment" of the BeefyAuthoritySet
		let root_slice = tree.root().ok_or(Error::InvalidAuthoritiesProofCreation)?;
		let root = RootHash::from_slice(&root_slice);

		let proof = tree.ordered_proof_tree(&sig_indices);

		Ok(AuthoritiesProof { root, total_leaves: ecdsa_validators.len() as u32, proof })
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
/// This function will also update `beefy_ids_and_stakes`, to shorten the search on every call.
///
/// # Arguments
///
/// * `validator` - the beefy id value
/// * `beefy_ids_and_stakes` - contains the beefy id/validator with its stake
fn prep_merkle_leaves(
	ecdsa_validators: &[EcdsaPublic],
	beefy_ids_and_stakes: &mut BeefyIdsWithStakes,
) -> Vec<Hash> {
	// pair up the validators with its stakes
	ecdsa_validators
		.iter()
		.enumerate()
		.map(|(idx, v)| {
			let keccak = prep_leaf_hash(v, beefy_ids_and_stakes);
			println!("V({idx}): ecdsa: {:?} keccak: {}", v, keccak.as_hex());

			keccak
		})
		.collect()
}

/// Create Leaf hash using keccak, based on the tuple of validator and its stake.
/// If there is no stake found for the validator, the default stake value will be 0
///
/// This function will also update `beefy_ids_and_stakes`, to shorten the search on every call.
///
/// # Arguments
///
/// * `validator` - the beefy id value
/// * `beefy_ids_and_stakes` - contains the beefy id/validator with its stake
fn prep_leaf_hash(validator: &EcdsaPublic, beefy_ids_and_stakes: &mut BeefyIdsWithStakes) -> Hash {
	// if no stake found, give it a 0 by default
	let leaf_data =
		get_leaf_data(validator, beefy_ids_and_stakes).unwrap_or((validator.clone(), 0));

	// convert validator to bytes
	let mut data = leaf_data.0.into_inner().0.to_vec();

	// convert stake to bytes
	let stake_bytes = leaf_data.1.to_be_bytes();

	// append the validator bytes with the stake bytes
	data.extend_from_slice(&stake_bytes);

	keccak_256(&data)
}

/// Find a match of the validator in the list of beefy ids, and return the stakes.
/// This function will also update `beefy_ids_with_stakes`, to shorten the search on every call.
///
/// # Arguments
///
/// * `validator` - the beefy id value
/// * `beefy_ids_and_stakes` - contains the beefy id/validator with its stake
fn get_leaf_data(
	validator: &EcdsaPublic,
	beefy_ids_and_stakes: &mut BeefyIdsWithStakes,
) -> Option<BeefyIdWithStake> {
	let mut chosen_idx = None;

	for (index, xtra_data) in beefy_ids_and_stakes.iter().enumerate() {
		// if found, remove from the list
		if validator == &xtra_data.0 {
			chosen_idx = Some(index);
			break;
		}
	}

	Some(
		// remove from the list, as it has already been found
		beefy_ids_and_stakes.remove(chosen_idx?),
	)
}

/// Verify and collect all the indices (similar index position in the validator set) with signatures
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
	use parity_scale_codec::Decode;
	use sp_consensus_beefy::ValidatorSetId;
	use sp_core::crypto::Ss58Codec;

	use crate::{
		BeefyId, BeefyIdsWithStakes, BeefySignedCommitment, BeefyValidatorSet,
		authorities::{
			collect_signature_indices, get_leaf_data, prep_leaf_hash, prep_merkle_leaves,
		},
		helper::HexExt,
	};

	const ECDSA_ALICE: &str = "KW39r9CJjAVzmkf9zQ4YDb2hqfAVGdRqn53eRqyruqpxAP5YL";
	const ECDSA_BOB: &str = "KWByAN7WfZABWS5AoWqxriRmF5f2jnDqy3rB5pfHLGkY93ibN";
	const ECDSA_CHARLIE: &str = "KWBpGtyJLBkJERdZT1a1uu19c2uPpZm9nFd8SGtCfRUAT3Y4w";
	const ECDSA_DAVE: &str = "KWCycezxoy7MWTTqA5JDKxJbqVMiNfqThKFhb5dTfsbNaGbrW";
	const ECDSA_EVE: &str = "KW9NRAHXUXhBnu3j1AGzUXs2AuiEPCSjYe8oGan44nwvH5qKp";

	const ENCODED_BEEFY_COMMITMENT: &str = "046d6880d2cdb932c91082bb0586eabba6aa4dd441b380e7d47a1270999280fd2247ddfd21000000000000000000000004d0040000000ce9f6db6b33ffcd1054b3f92d7fcdc7b80545a4f52be08466bbf36d75997cfc1b193a4de4c901f6b2a4446c447d228354336b466bb6ec4ddcf7b0db85c150677c016c0e79d739cdcbb0d0e78d6a0f44ad9a2333c9a61deb1083c501f3ce07ccbc5a0cc42b78f5a8665c3b714160c103af6a7f3d7502e7aa2c232f3906dc57ed96c40173124cca66ca103b705b3af70a3945d54535de9983794a7bb93356373e71da8a12342ddd9000cb49895d237111300d32790ac610fc327cccd0b8cffb6b8ae42e01";

	const EXPECTED_CHARLIE_3_STAKE: &str =
		"6d925a8985217bc65f79b39af205ca61e3aae0381772a281a9a71c108d23a6a3";
	const EXPECTED_ALICE_1_STAKE: &str =
		"62ee9a00c1179f75310749f4aadd8b5cc6096e1b4092bc07b7014cacc9df007c";

	fn get_ecdsa(hex_key: &str) -> BeefyId {
		BeefyId::from_ss58check(hex_key).expect("should be able to convert to beefyid")
	}

	fn sample_extra_data() -> BeefyIdsWithStakes {
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

	fn sample_beefy_commitment() -> BeefySignedCommitment {
		let hex_encoded = hex::decode(ENCODED_BEEFY_COMMITMENT).expect("failed to convert");

		Decode::decode(&mut &hex_encoded[..]).expect("failed to decode BeefySignedCommitment")
	}

	#[test]
	fn test_get_leaf_data() {
		let mut extra_data = sample_extra_data();

		let result =
			get_leaf_data(&get_ecdsa(ECDSA_DAVE), &mut extra_data).expect("failed to find Dave");
		assert_eq!(result.1, 4);

		let result =
			get_leaf_data(&get_ecdsa(ECDSA_BOB), &mut extra_data).expect("failed to find Bob");
		assert_eq!(result.1, 2);

		let result = get_leaf_data(&get_ecdsa(ECDSA_CHARLIE), &mut extra_data)
			.expect("failed to find Charlie");
		assert_eq!(result.1, 3);

		// unknown data
		let result = get_leaf_data(&get_ecdsa(ECDSA_EVE), &mut extra_data);
		assert_eq!(result, None);
	}

	#[test]
	fn test_prep_leaf_hash() {
		let mut extra_data = sample_extra_data();

		let result = prep_leaf_hash(&get_ecdsa(ECDSA_CHARLIE), &mut extra_data);
		assert_eq!(result.as_hex(), EXPECTED_CHARLIE_3_STAKE);

		let result = prep_leaf_hash(&get_ecdsa(ECDSA_ALICE), &mut extra_data);

		assert_eq!(result.as_hex(), EXPECTED_ALICE_1_STAKE);
	}

	#[test]
	fn test_collect_signature_indices() {
		let beefy_commitment = sample_beefy_commitment();

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
		let validator_set = sample_validator_set(0);
		let validators = validator_set.validators();

		let mut extra_data = sample_extra_data();

		let keccak_hashes = prep_merkle_leaves(validators, &mut extra_data);
		assert_eq!(keccak_hashes.len(), 4);

		let alice_stake_decoded =
			hex::decode(EXPECTED_ALICE_1_STAKE).expect("failed to conver to bytes");
		let alice_stake_decoded: Hash =
			alice_stake_decoded.try_into().expect("failed to convert to sized array");
		assert!(keccak_hashes.contains(&alice_stake_decoded));

		let charlie_stake_decoded =
			hex::decode(EXPECTED_CHARLIE_3_STAKE).expect("failed to conver to bytes");
		let charlie_stake_decoded: Hash =
			charlie_stake_decoded.try_into().expect("failed to convert to sized array");
		assert!(keccak_hashes.contains(&charlie_stake_decoded));
	}
}
