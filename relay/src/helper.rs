#![allow(dead_code)]

use crate::{BeefyValidatorSet, mn_meta};
use sp_consensus_beefy::ecdsa_crypto::Public;
use sp_core::ByteArray;

use mn_meta::runtime_types::sp_consensus_beefy::{
	ValidatorSet as MidnBeefyValidatorSet, ecdsa_crypto::Public as MidnBeefyPublic,
};
use pallas::codec::minicbor::to_vec;
use subxt::utils::to_hex;

pub trait HexExt {
	fn as_hex(&self) -> String;
}

impl HexExt for sp_core::Bytes {
	fn as_hex(&self) -> String {
		to_hex(&self[..])
	}
}

impl HexExt for pallas::ledger::primitives::PlutusData {
	fn as_hex(&self) -> String {
		let plutus_to_vec = to_vec(self).expect("should be able to convert to Vec<u8>");

		to_hex(&plutus_to_vec)
	}
}

impl HexExt for Vec<u8> {
	fn as_hex(&self) -> String {
		to_hex(self)
	}
}

impl HexExt for [u8; 32] {
	fn as_hex(&self) -> String {
		to_hex(self)
	}
}

// ------ Converting types from metadata, to the sp-consensus libraries ------
// todo: check `substitute_type` of subxt

pub trait MnMetaConversion<T> {
	fn into_non_metadata(self) -> T;
}

impl MnMetaConversion<BeefyValidatorSet> for MidnBeefyValidatorSet<MidnBeefyPublic> {
	fn into_non_metadata(self) -> BeefyValidatorSet {
		let mut validators = vec![];

		for validator in self.validators {
			validators.push(validator.into_non_metadata());
		}

		BeefyValidatorSet::new(validators, self.id).expect("cannot create from empty validators")
	}
}

impl MnMetaConversion<Public> for MidnBeefyPublic {
	fn into_non_metadata(self) -> Public {
		Public::from_slice(self.0.as_slice()).expect("failed to convert to Beefy Public")
	}
}

/// helper module for tests
pub mod test {
	use parity_scale_codec::Decode;
	use sp_core::{bytes::from_hex, crypto::Ss58Codec};

	use crate::BeefyId;

	pub const ECDSA_ALICE: &str = "KW39r9CJjAVzmkf9zQ4YDb2hqfAVGdRqn53eRqyruqpxAP5YL";
	pub const ECDSA_BOB: &str = "KWByAN7WfZABWS5AoWqxriRmF5f2jnDqy3rB5pfHLGkY93ibN";
	pub const ECDSA_CHARLIE: &str = "KWBpGtyJLBkJERdZT1a1uu19c2uPpZm9nFd8SGtCfRUAT3Y4w";
	pub const ECDSA_DAVE: &str = "KWCycezxoy7MWTTqA5JDKxJbqVMiNfqThKFhb5dTfsbNaGbrW";

	pub fn decode<T: Decode>(hex: &str) -> T {
		let hex_bytes = from_hex(hex).expect("invalid bytes");
		Decode::decode(&mut &hex_bytes[..]).expect("conversion failed")
	}

	pub fn get_ecdsa(hex_key: &str) -> BeefyId {
		BeefyId::from_ss58check(hex_key).expect("should be able to convert to beefyid")
	}
}
