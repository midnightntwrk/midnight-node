#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use midnight_primitives_beefy::{BEEFY_LOG_TARGET, compressed_key, seat_leaf, seats};
use rs_merkle::proof_tree::ProofNode;
use sp_consensus_beefy::{ValidatorSet, ecdsa_crypto::Public as EcdsaPublic};
use sp_crypto_hashing::keccak_256;

use crate::{BeefySignedCommitment, Error};

pub type Hash = [u8; 32];
pub type RootHash = sp_core::H256;

/// The committee root of a validator set, and the proof for the signers' leaves
#[derive(Debug, Clone)]
pub struct AuthoritiesProof {
	pub root: RootHash,

	/// the number of committee leaves (distinct keys)
	pub total_leaves: u32,

	/// a proof tree containing
	pub proof: ProofNode<Hash>,
}

impl AuthoritiesProof {
	/// The committee tree of `validator_set`, one `key ‖ seats` leaf per distinct key as the runtime commits it, and the proof of the signers' leaves.
	pub fn try_new(
		beefy_signed_commitment: &BeefySignedCommitment,
		validator_set: &ValidatorSet<EcdsaPublic>,
	) -> Result<Self, Error> {
		let signer_positions = collect_signature_indices(beefy_signed_commitment, validator_set)?;

		let seats = seats(validator_set.validators());
		let leaf_index: BTreeMap<[u8; 33], usize> =
			seats.keys().enumerate().map(|(index, key)| (*key, index)).collect();
		let leaves: Vec<Hash> =
			seats.iter().map(|(key, seats)| keccak_256(&seat_leaf(key, *seats))).collect();
		let signer_leaves: BTreeSet<usize> = signer_positions
			.iter()
			.map(|&position| leaf_index[&compressed_key(&validator_set.validators()[position])])
			.collect();

		let tree = rs_merkle::MerkleTree::<KeccakHasher>::from_leaves(&leaves);
		let root =
			RootHash::from_slice(&tree.root().ok_or(Error::InvalidAuthoritiesProofCreation)?);
		log::debug!(target: BEEFY_LOG_TARGET, "🥩 Committee root {root:?} over {} leaves", leaves.len());

		let proof = tree.ordered_proof_tree(&signer_leaves.into_iter().collect::<Vec<_>>());

		Ok(AuthoritiesProof { root, total_leaves: leaves.len() as u32, proof })
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
		.verify_signatures(block_number, validator_set)
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
	use parity_scale_codec::Encode;
	use sp_consensus_beefy::{
		Commitment, Payload, SignedCommitment, ValidatorSetId, known_payloads::MMR_ROOT_ID,
	};
	use sp_core::{H256, Pair, ecdsa};
	use sp_crypto_hashing::keccak_256;

	use crate::{
		BeefySignedCommitment, BeefyValidatorSet,
		authorities::{AuthoritiesProof, collect_signature_indices},
		helper::test::{ECDSA_ALICE, ECDSA_BOB, ECDSA_CHARLIE, ECDSA_DAVE, decode, get_ecdsa},
	};

	const ENCODED_BEEFY_COMMITMENT: &str = "0x146362b00000000000000000040000007f0c9b27381104febfb4a6be51e8fc0f08ba70060531fc5fcf60dcbed1f4e5f96373950210020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a100000000000000000390084fdbf27d2b79d26a4f13f0ccd982cb755a661969143c37cbc49ef5b91f2701000000000000000389411795514af1627765eceffcbd002719f031604fadd7d188e2dc585b4e1afb010000000000000003bc9d0ca094bd5b8b3225d7651eac5d18c1c04bf8ae8f8b263eebca4e1410ed0c00000000000000006d68805d6013253f0020cdae55a436208887cbd691f9cf93278497fc5e10aae814c4d06e62b0010000000000000004000000a5d8a7ba3b85661890415507aed407f1b3e7f86c0133b195ac43612171f5daca6e73950210020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a101000000000000000390084fdbf27d2b79d26a4f13f0ccd982cb755a661969143c37cbc49ef5b91f2700000000000000000389411795514af1627765eceffcbd002719f031604fadd7d188e2dc585b4e1afb010000000000000003bc9d0ca094bd5b8b3225d7651eac5d18c1c04bf8ae8f8b263eebca4e1410ed0c000000000000000081040000000000000000000004d0040000000cb128f92056bf1af3f4762e80071a6f42c55dee9f9c5044fb45173a86325ebd8c53d2478e29685cb3dfe929f0f887129d36865a116573c66c4edfd83384d3a3bd01691f180f0ff53d3fde30c992ff4fb3cad2a01089c1885e11d72a507a0c08ce9d2a412c772ba4877775a6521bb4cdca1a8809a9b8df7116c9abe6c0d67df7dd40014b768e1b85bcd09e1d562c59a24b12cafc8d4bab0b11c92873631c0552bbe379005eafcf82f25ba800a57e3debea0069e106d4eb85b73d207631756d84c8d1fe01";

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

	/// The secp256k1 key of secret scalar `n`, as in the contract's `keys.json`.
	fn scalar_key(n: u8) -> ecdsa::Pair {
		let mut secret = [0u8; 32];
		secret[31] = n;
		ecdsa::Pair::from_seed(&secret)
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

	/// `tests/vectors/bridge/commitment.json` (permutation) in midnight-reserve-contracts: seats (1, 2, 1).
	#[test]
	fn committee_root_matches_the_contract_vector() {
		let (k1, k2, k3) = (scalar_key(1), scalar_key(2), scalar_key(3));
		let validators = [&k2, &k1, &k2, &k3].map(|pair| pair.public().into()).to_vec();
		let validator_set = BeefyValidatorSet::new(validators, 4).unwrap();
		let commitment = Commitment {
			payload: Payload::from_single_entry(MMR_ROOT_ID, H256::repeat_byte(7).encode()),
			block_number: 7,
			validator_set_id: 4,
		};
		let signed_by = |pair: &ecdsa::Pair| {
			Some(pair.sign_prehashed(&keccak_256(&commitment.encode())).into())
		};
		let signed = SignedCommitment {
			signatures: vec![signed_by(&k2), signed_by(&k1), signed_by(&k2), None],
			commitment: commitment.clone(),
		};

		let proof = AuthoritiesProof::try_new(&signed, &validator_set).unwrap();

		assert_eq!(proof.total_leaves, 3);
		assert_eq!(
			proof.root,
			H256(hex_literal::hex!(
				"052f9919c3c309e60ca131820fde7ce63f1d1d7d3746f066f5263b285c787096"
			))
		);
	}
}
