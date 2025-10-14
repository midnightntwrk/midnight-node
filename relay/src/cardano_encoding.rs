//! Encodings for representing BEEFY commitments and related
//! data structures as Cardano PlutusData.

use pallas::ledger::primitives::BigInt;
use pallas::{
	codec::utils::MaybeIndefArray,
	ledger::primitives::{BoundedBytes, Constr, PlutusData},
};

use sp_consensus_beefy::ecdsa_crypto::Public;
use sp_consensus_beefy::{SignedCommitment as BeefySignedCommitment, known_payloads::MMR_ROOT_ID};

use crate::authorities::AuthoritiesProof;

// Known encoding tag
pub const TAG: u64 = 121;

pub trait ToPlutusData {
	fn to_plutus_data(&self) -> PlutusData;
}

#[derive(Clone)]
pub struct Vote {
	pub signature: Vec<u8>,
	pub authority_index: usize,
	pub public_key: Vec<u8>,
}

impl ToPlutusData for Vote {
	fn to_plutus_data(&self) -> PlutusData {
		PlutusData::Constr(Constr {
			tag: TAG,
			any_constructor: None,
			// Order matters here
			fields: MaybeIndefArray::Indef(vec![
				PlutusData::BoundedBytes(self.signature.clone().into()),
				PlutusData::BigInt(BigInt::Int((self.authority_index as i64).into())),
				PlutusData::BoundedBytes(self.public_key.clone().into()),
			]),
		})
	}
}

#[derive(Clone)]
pub struct Payload {
	pub id: Vec<u8>,
	pub data: Vec<u8>,
}

impl ToPlutusData for Payload {
	fn to_plutus_data(&self) -> PlutusData {
		PlutusData::Constr(Constr {
			tag: TAG,
			any_constructor: None,
			// List order must match field order of struct
			fields: MaybeIndefArray::Indef(vec![
				PlutusData::BoundedBytes(self.id.clone().into()), // Hash
				PlutusData::BoundedBytes(self.data.clone().into()),
			]),
		})
	}
}

#[derive(Clone)]
pub struct Commitment {
	pub payloads: Vec<Payload>,
	pub block_number: usize,
	pub validator_set_id: usize,
}

impl ToPlutusData for Commitment {
	fn to_plutus_data(&self) -> PlutusData {
		PlutusData::Constr(Constr {
			tag: TAG,
			any_constructor: None,
			// List order must match field order of struct
			fields: MaybeIndefArray::Indef(vec![
				PlutusData::Array(MaybeIndefArray::Indef(
					// Node
					self.payloads.iter().map(|i| i.to_plutus_data()).collect(),
				)),
				PlutusData::BigInt(BigInt::Int((self.block_number as i64).into())),
				PlutusData::BigInt(BigInt::Int((self.validator_set_id as i64).into())),
			]),
		})
	}
}

#[derive(Clone)]
pub struct SignedCommitment {
	pub commitment: Commitment,
	pub votes: Vec<Vote>,
}

impl ToPlutusData for SignedCommitment {
	fn to_plutus_data(&self) -> PlutusData {
		PlutusData::Constr(Constr {
			tag: TAG,
			any_constructor: None,
			// List order must match field order of struct
			fields: MaybeIndefArray::Indef(vec![
				self.commitment.to_plutus_data(),
				PlutusData::Array(MaybeIndefArray::Indef(
					self.votes.iter().map(|i| i.to_plutus_data()).collect(),
				)),
			]),
		})
	}
}

impl SignedCommitment {
	pub fn from_signed_commitment_and_validators(
		signed_commitment: BeefySignedCommitment<u32, sp_consensus_beefy::ecdsa_crypto::Signature>,
		validator_set: &[Public],
	) -> Self {
		let commitment = Commitment {
			payloads: signed_commitment
				.commitment
				.payload
				.get_all_raw(&MMR_ROOT_ID)
				.into_iter()
				.map(|i| Payload { id: MMR_ROOT_ID.to_vec(), data: i.clone() })
				.collect(),
			block_number: signed_commitment.commitment.block_number as usize,
			validator_set_id: signed_commitment.commitment.validator_set_id as usize,
		};

		let votes = signed_commitment
			.signatures
			.iter()
			.enumerate()
			.zip(validator_set.iter())
			.filter_map(|((i, sig), pk)| {
				match sig {
					Some(sig) => {
						let mut signature = sig.clone().to_vec();
						// Substrate adds an extra byte to these signatures. We'll remove this manually for compatibility
						signature.pop();

						Some(Vote {
							signature,
							authority_index: i,
							public_key: pk.clone().into_inner().to_vec(),
						})
					},
					None => None,
				}
			})
			.collect();

		SignedCommitment { commitment, votes }
	}
}

pub struct RelayChainProof {
	pub signed_commitment: SignedCommitment,
	// latest_mmr_leaf: BeefyMmrLeaf,
	pub mmr_proof: Vec<u8>,
	pub authorities_proof: AuthoritiesProof,
}

impl ToPlutusData for RelayChainProof {
	fn to_plutus_data(&self) -> PlutusData {
		PlutusData::Constr(Constr {
			tag: TAG,
			any_constructor: None,
			fields: MaybeIndefArray::Indef(vec![self.signed_commitment.to_plutus_data()]),
		})
	}
}
