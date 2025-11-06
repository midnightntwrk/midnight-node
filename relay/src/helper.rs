use crate::{BeefyValidatorSet, mn_meta};
use sp_consensus_beefy::ecdsa_crypto::Public;
use sp_core::ByteArray;

use mn_meta::runtime_types::sp_consensus_beefy::{
	ValidatorSet as MidnBeefyValidatorSet, ecdsa_crypto::Public as MidnBeefyPublic,
};

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
