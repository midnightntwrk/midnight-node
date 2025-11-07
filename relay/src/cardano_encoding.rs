//! Encodings for representing BEEFY commitments and related
//! data structures as Cardano PlutusData.
//!
use std::{fmt::Debug, ops::Deref as _};

use crate::{
	BeefyId, BeefySignedCommitment, BeefyValidatorSet, MmrProof, authorities::AuthoritiesProof,
	helper::HexExt, mmr::get_beefy_ids_with_stakes,
};

use pallas::{
	codec::utils::MaybeIndefArray,
	ledger::primitives::{BigInt, BoundedBytes, Constr, PlutusData},
};

use rs_merkle::proof_tree::ProofNode;
use sp_consensus_beefy::known_payloads::MMR_ROOT_ID;

// Known encoding tag
pub const TAG: u64 = 121;

pub trait ToPlutusData {
	fn to_plutus_data(&self) -> PlutusData;
}

pub struct RelayChainProof {
	pub signed_commitment: SignedCommitment,
	pub proof: AuthoritiesProof,
}

impl RelayChainProof {
	pub fn generate(
		beefy_signed_commitment: BeefySignedCommitment,
		mmr_proof: MmrProof,
		validator_set: BeefyValidatorSet,
	) -> Result<Self, crate::error::Error> {
		let mut beefy_ids_and_stakes = get_beefy_ids_with_stakes(&mmr_proof)?;

		// generate proofs for each signer in the commitment, with the validator set as basis
		let proof = AuthoritiesProof::try_new(
			&beefy_signed_commitment,
			&validator_set,
			&mut beefy_ids_and_stakes,
		)?;

		Ok(RelayChainProof {
			signed_commitment: SignedCommitment::from_signed_commitment_and_validators(
				beefy_signed_commitment,
				validator_set.validators(),
			),
			proof,
		})
	}
}

impl ToPlutusData for RelayChainProof {
	fn to_plutus_data(&self) -> PlutusData {
		PlutusData::Constr(Constr {
			tag: TAG,
			any_constructor: None,
			fields: MaybeIndefArray::Indef(vec![
				self.signed_commitment.to_plutus_data(),
				self.proof.to_plutus_data(),
			]),
		})
	}
}

impl Debug for RelayChainProof {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("RelayChainProof")
			.field("signed_commitment", &self.signed_commitment)
			.field("proof", &self.proof)
			.finish()
	}
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

impl Debug for Vote {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let sig_hex = self.signature.as_hex();
		let pub_key_hex = self.public_key.as_hex();

		f.debug_struct("Vote")
			.field("signature", &sig_hex)
			.field("authority_index", &self.authority_index)
			.field("public_key", &pub_key_hex)
			.finish()
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

impl Debug for Payload {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let id_hex = self.id.as_hex();
		let data_hex = self.data.as_hex();
		f.debug_struct("Payload").field("id", &id_hex).field("data", &data_hex).finish()
	}
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
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
		signed_commitment: BeefySignedCommitment,
		validators: &[BeefyId],
	) -> Self {
		let commitment = Commitment {
			payloads: signed_commitment
				.commitment
				.payload
				.get_all_raw(&MMR_ROOT_ID)
				.map(|i| Payload { id: MMR_ROOT_ID.to_vec(), data: i.clone() })
				.collect(),
			block_number: signed_commitment.commitment.block_number as usize,
			validator_set_id: signed_commitment.commitment.validator_set_id as usize,
		};

		let votes = signed_commitment
			.signatures
			.iter()
			.enumerate()
			.zip(validators.iter())
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

// convert the ProofNode to plutusdata
fn proof_node_to_plutus_data<T: Clone + Into<Vec<u8>>>(proof: &ProofNode<T>) -> PlutusData {
	match proof {
		ProofNode::Leaf(hash) => PlutusData::BoundedBytes(BoundedBytes::from(hash.clone().into())),
		ProofNode::Node(nodes) => PlutusData::Array(MaybeIndefArray::Indef(
			nodes.iter().map(|node| proof_node_to_plutus_data(node.deref())).collect(),
		)),
	}
}
