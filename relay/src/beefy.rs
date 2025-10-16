use pallas::{
	codec::utils::MaybeIndefArray,
	ledger::primitives::{BigInt, Constr, PlutusData},
};
use sp_core::H256;

use mmr_rpc::LeavesProof;

use sp_consensus_beefy::mmr::BeefyAuthoritySet;

use crate::{
	cardano_encoding::{AuthoritySetCommitment, TAG, ToPlutusData},
	error::Error,
	mmr::LeavesProofExt,
	types::Block,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeefyConsensusState {
	pub latest_height: u64,
	pub activation_block: Block,
	pub current_authority_set: AuthoritySetCommitment,
	pub next_authority_set: AuthoritySetCommitment,
}

impl ToPlutusData for BeefyConsensusState {
	fn to_plutus_data(&self) -> PlutusData {
		PlutusData::Constr(Constr {
			tag: TAG,
			any_constructor: None,
			fields: MaybeIndefArray::Indef(vec![
				PlutusData::BigInt(BigInt::Int((self.latest_height as i64).into())),
				PlutusData::BigInt(BigInt::Int((self.activation_block as i64).into())),
				self.current_authority_set.to_plutus_data(),
				self.next_authority_set.to_plutus_data(),
			]),
		})
	}
}

impl BeefyConsensusState {
	/// # Arguments
	/// * `mmr_proof` - contains the latest leaf of the mmr and its proof in the latest mmr root hash
	/// * `current_authorities` - the current authorities in
	/// * `expected_next_authorities` - the next authorities, that should be similar in the data of the latest leaf
	pub fn try_new(
		mmr_proof: &LeavesProof<H256>,
		current_authorities: BeefyAuthoritySet<H256>,
		expected_next_authorities: &BeefyAuthoritySet<H256>,
	) -> Result<BeefyConsensusState, Error> {
		// verify the next authorities is the same in the provided proof
		mmr_proof.verify_next_authority_set(expected_next_authorities)?;

		let leaf_proof = mmr_proof.leaf_proof();
		Ok(BeefyConsensusState {
			latest_height: leaf_proof.leaf_count,
			activation_block: 0,
			current_authority_set: current_authorities.into(),
			next_authority_set: expected_next_authorities.clone().into(),
		})
	}
}
