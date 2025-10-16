use pallas::codec::minicbor::to_vec;
use std::fmt::Debug;

use sp_consensus_beefy::{ValidatorSet, ecdsa_crypto::Public, mmr::BeefyAuthoritySet};
use sp_core::{ByteArray, Bytes};

use mn_meta::runtime_types::sp_consensus_beefy::{
	ValidatorSet as MidnBeefyValidatorSet, ecdsa_crypto::Public as MidnBeefyPublic,
	mmr::BeefyAuthoritySet as MidnBeefyAuthSet,
};

use crate::{mmr::MmrLeaf, mn_meta};

pub trait ToHex {
	fn as_hex(&self) -> String;
}

impl ToHex for Bytes {
	fn as_hex(&self) -> String {
		hex::encode(&self[..])
	}
}

impl ToHex for pallas::ledger::primitives::PlutusData {
	fn as_hex(&self) -> String {
		let plutus_to_vec = to_vec(self).expect("should be able to convert to Vec<u8>");

		hex::encode(&plutus_to_vec)
	}
}

impl ToHex for Vec<u8> {
	fn as_hex(&self) -> String {
		hex::encode(self)
	}
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct HexMmrLeaf {
	scale_encoded_leaf_hash: String,
	leaf: MmrLeaf,
}

// ------ Converting types from metadata, to the sp-consensus libraries ------
// todo: check `substitute_type` of subxt

pub trait MnMetaConversion<T> {
	fn into_non_metadata(self) -> T;
}

impl MnMetaConversion<ValidatorSet<Public>> for MidnBeefyValidatorSet<MidnBeefyPublic> {
	fn into_non_metadata(self) -> ValidatorSet<Public> {
		let mut validators = vec![];

		for validator in self.validators {
			validators.push(validator.into_non_metadata());
		}

		ValidatorSet::new(validators, self.id).expect("cannot create from empty validators")
	}
}

impl MnMetaConversion<Public> for MidnBeefyPublic {
	fn into_non_metadata(self) -> Public {
		Public::from_slice(self.0.as_slice()).expect("failed to convert to Beefy Public")
	}
}

impl<T> MnMetaConversion<BeefyAuthoritySet<T>> for MidnBeefyAuthSet<T> {
	fn into_non_metadata(self) -> BeefyAuthoritySet<T> {
		BeefyAuthoritySet { id: self.id, len: self.len, keyset_commitment: self.keyset_commitment }
	}
}
