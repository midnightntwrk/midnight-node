use sp_core::H256;

use mmr_rpc::LeavesProof;
use subxt::ext::codec::{Decode, Encode};

use sp_consensus_beefy::{
	ValidatorSet,
	ecdsa_crypto::{Public, Signature},
	known_payloads::MMR_ROOT_ID,
	mmr::BeefyAuthoritySet,
};

use crate::{
	authorities::{AuthoritiesProof, generate_authorities_proof},
	cardano_encoding::SignedCommitment,
	error::Error,
	helpers::HexBeefyRelayChainProof,
	mmr::LeavesProofExt,
	types::{Block, ExtraData},
};

pub type BeefySignedCommitment = sp_consensus_beefy::SignedCommitment<Block, Signature>;

pub struct BeefyRelayChainProof {
	pub mmr_proof: LeavesProof<H256>,
	pub authorities_proof: AuthoritiesProof,
	pub signed_commitment: BeefySignedCommitment,
	/// list of signers from the commitment file
	pub validator_set: Vec<Public>,
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
		mmr_proof.verify_next_authority_set(expected_next_authorities)?;

		// generate proofs for each signer in the commitment, with the validator set as basis
		let authorities_proof =
			generate_authorities_proof(&beefy_signed_commitment, &validator_set)?;

		let validators = validator_set.validators().to_vec();
		Ok(BeefyRelayChainProof {
			mmr_proof,
			authorities_proof,
			signed_commitment: beefy_signed_commitment,
			validator_set: validators,
		})
	}

	/// outputs the entire proof
	pub fn print_as_hex(&self) {
		let result = HexBeefyRelayChainProof::from(self);
		println!("{result:#?}");
	}
}

pub trait CommitmentExt {
	fn block_number(&self) -> Block;

	fn mmr_root_hash(&self) -> Option<H256>;

	fn hex_scale_encoded(&self) -> String;

	fn as_cardano(&self, validators: &[Public]) -> SignedCommitment;
}

impl CommitmentExt for BeefySignedCommitment {
	fn block_number(&self) -> Block {
		self.commitment.block_number
	}

	fn mmr_root_hash(&self) -> Option<H256> {
		self.commitment.payload.get_decoded::<H256>(&MMR_ROOT_ID)
	}

	fn hex_scale_encoded(&self) -> String {
		let scale_encoded = self.encode();
		hex::encode(&scale_encoded)
	}

	fn as_cardano(&self, validators: &[Public]) -> SignedCommitment {
		SignedCommitment::from_signed_commitment_and_validators(self.clone(), validators)
	}
}
