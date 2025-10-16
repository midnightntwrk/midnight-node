//! Encodings for representing BEEFY commitments and related
//! data structures as Cardano PlutusData.
use mmr_rpc::LeavesProof;
use pallas::ledger::primitives::BigInt;
use pallas::{
	codec::utils::MaybeIndefArray,
	ledger::primitives::{BoundedBytes, Constr, PlutusData},
};

use sp_consensus_beefy::known_payloads::MMR_ROOT_ID;
use sp_consensus_beefy::mmr::BeefyAuthoritySet;
use sp_core::H256;

use std::fmt::Debug;

use crate::authorities::AuthoritiesProof;
use crate::helpers::ToHex;
use crate::mmr::LeavesProofExt;
use crate::types::{BeefySignedCommitment, BeefyValidatorSet, Block, Hashes};

// Known encoding tag
pub const TAG: u64 = 121;

pub trait ToPlutusData {
	fn to_plutus_data(&self) -> PlutusData;
}

pub struct RelayChainProof {
	pub signed_commitment: SignedCommitment,
	pub latest_mmr_leaf: BeefyMmrLeaf,
	pub mmr_proof: Hashes,
	pub proof: AuthoritiesProof,
}

impl ToPlutusData for RelayChainProof {
	fn to_plutus_data(&self) -> PlutusData {
		let mmr_proof_plutus_data = PlutusData::Array(MaybeIndefArray::Indef(
			self.mmr_proof
				.iter()
				.map(|hash| PlutusData::BoundedBytes(BoundedBytes::from(hash.to_vec())))
				.collect(),
		));

		PlutusData::Constr(Constr {
			tag: TAG,
			any_constructor: None,
			fields: MaybeIndefArray::Indef(vec![
				self.signed_commitment.to_plutus_data(),
				self.latest_mmr_leaf.to_plutus_data(),
				mmr_proof_plutus_data,
				self.proof.to_plutus_data(),
			]),
		})
	}
}

impl Debug for RelayChainProof {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let mmr_proof_hex: Vec<String> = self.mmr_proof.iter().map(hex::encode).collect();

		f.debug_struct("RelayChainProof")
			.field("signed_commitment", &self.signed_commitment)
			.field("latest_mmr_leaf", &self.latest_mmr_leaf)
			.field("mmr_proof", &mmr_proof_hex)
			.field("proof", &self.proof)
			.finish()
	}
}

impl RelayChainProof {
	pub fn generate(
		beefy_signed_commitment: BeefySignedCommitment,
		leaves_proof: LeavesProof<H256>,
		validator_set: &BeefyValidatorSet,
		expected_next_authorities: &BeefyAuthoritySet<H256>,
	) -> Result<Self, crate::error::Error> {
		// verify that the next authorities is the same in the provided proof
		leaves_proof.verify_next_authority_set(expected_next_authorities)?;

		// generate proofs for each signer in the commitment, with the validator set as basis
		let proof = AuthoritiesProof::try_new(&beefy_signed_commitment, validator_set)?;

		let mmr_proof = leaves_proof.proof_items();

		Ok(RelayChainProof {
			signed_commitment: SignedCommitment::from_signed_commitment_and_validators(
				beefy_signed_commitment,
				validator_set.validators(),
			),
			latest_mmr_leaf: leaves_proof.into(),
			mmr_proof,
			proof,
		})
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
		validators: &[sp_consensus_beefy::ecdsa_crypto::Public],
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

#[derive(Clone, PartialEq, Eq)]
pub struct AuthoritySetCommitment {
	pub id: usize,
	pub len: usize,
	pub root: Vec<u8>,
}

impl ToPlutusData for AuthoritySetCommitment {
	fn to_plutus_data(&self) -> PlutusData {
		PlutusData::Constr(Constr {
			tag: TAG,
			any_constructor: None,
			fields: MaybeIndefArray::Indef(vec![
				PlutusData::BigInt(BigInt::Int((self.id as i64).into())),
				PlutusData::BigInt(BigInt::Int((self.len as i64).into())),
				PlutusData::BoundedBytes(self.root.clone().into()),
			]),
		})
	}
}

impl From<sp_consensus_beefy::mmr::BeefyAuthoritySet<H256>> for AuthoritySetCommitment {
	fn from(value: sp_consensus_beefy::mmr::BeefyAuthoritySet<H256>) -> Self {
		AuthoritySetCommitment {
			id: value.id as usize,
			len: value.len as usize,
			root: value.keyset_commitment.0.to_vec(),
		}
	}
}

impl Debug for AuthoritySetCommitment {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let hex_root = self.root.as_hex();

		f.debug_struct("AuthoritySetCommitment")
			.field("id", &self.id)
			.field("len", &self.len)
			.field("root", &hex_root)
			.finish()
	}
}

#[derive(Clone, PartialEq, Eq)]
pub struct BeefyMmrLeaf {
	pub version: u8,
	pub parent_number: Block,
	pub parent_hash: Vec<u8>,
	pub next_authority_set: AuthoritySetCommitment,
	pub extra: Vec<u8>,
	pub k_index: usize,
	pub leaf_index: u64,
}

impl ToPlutusData for BeefyMmrLeaf {
	fn to_plutus_data(&self) -> PlutusData {
		PlutusData::Constr(Constr {
			tag: TAG,
			any_constructor: None,
			fields: MaybeIndefArray::Indef(vec![
				PlutusData::BigInt(BigInt::Int((self.version as i64).into())),
				PlutusData::BigInt(BigInt::Int((self.parent_number as i64).into())),
				PlutusData::BoundedBytes(self.parent_hash.clone().into()),
				self.next_authority_set.to_plutus_data(),
				PlutusData::BoundedBytes(self.extra.clone().into()),
				PlutusData::BigInt(BigInt::Int((self.k_index as i64).into())),
				PlutusData::BigInt(BigInt::Int((self.leaf_index as i64).into())),
			]),
		})
	}
}

impl std::fmt::Debug for BeefyMmrLeaf {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let parent_hash_hex = self.parent_hash.as_hex();

		f.debug_struct("BeefyMmrLeaf")
			.field("version", &self.version)
			.field("parent_number", &self.parent_number)
			.field("parent_hash", &parent_hash_hex)
			.field("next_authority_set", &self.next_authority_set)
			.field("extra", &self.extra)
			.field("k_index", &self.k_index)
			.field("leaf_index", &self.leaf_index)
			.finish()
	}
}
