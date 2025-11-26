#![allow(dead_code)]

use midnight_primitives_beefy::{
	BeefyStakes,
	known_payloads::{
		CURRENT_BEEFY_AUTHORITY_SET, CURRENT_BEEFY_STAKES_ID, NEXT_BEEFY_AUTHORITY_SET,
		NEXT_BEEFY_STAKES_ID,
	},
};
use sp_consensus_beefy::{
	Payload,
	ecdsa_crypto::AuthorityId as BeefyId,
	mmr::{BeefyAuthoritySet, BeefyNextAuthoritySet},
};
use sp_core::H256;

use crate::Error;
#[derive(Debug)]
pub struct BeefyStakesInfo {
	current_stakes: BeefyStakes<BeefyId>,
	current_authority_set: BeefyAuthoritySet<H256>,
	next_stakes: BeefyStakes<BeefyId>,
	next_authority_set: BeefyNextAuthoritySet<H256>,
}

impl TryFrom<Payload> for BeefyStakesInfo {
	type Error = Error;

	fn try_from(value: Payload) -> Result<Self, Self::Error> {
		BeefyStakesInfo::try_from(&value)
	}
}

impl TryFrom<&Payload> for BeefyStakesInfo {
	type Error = Error;

	fn try_from(value: &Payload) -> Result<Self, Self::Error> {
		let current_stakes: BeefyStakes<BeefyId> = value
			.get_decoded(&CURRENT_BEEFY_STAKES_ID)
			.ok_or(Error::MissingCurrentBeefyStakes)?;
		let current_authority_set: BeefyAuthoritySet<H256> = value
			.get_decoded(&CURRENT_BEEFY_AUTHORITY_SET)
			.ok_or(Error::MissingCurrentAuthoritySet)?;

		let next_stakes: BeefyStakes<BeefyId> =
			value.get_decoded(&NEXT_BEEFY_STAKES_ID).ok_or(Error::MissingNextBeefyStakes)?;
		let next_authority_set: BeefyNextAuthoritySet<H256> = value
			.get_decoded(&NEXT_BEEFY_AUTHORITY_SET)
			.ok_or(Error::MissingNextAuthoritySet)?;

		Ok(BeefyStakesInfo {
			current_stakes,
			current_authority_set,
			next_stakes,
			next_authority_set,
		})
	}
}

#[cfg(test)]
mod test {
	use parity_scale_codec::Decode;
	use sp_consensus_beefy::{Payload, ecdsa_crypto::AuthorityId};
	use sp_core::{H256, bytes::from_hex, crypto::Ss58Codec};

	use crate::justification::BeefyStakesInfo;

	const ENCODED_PAYLOAD: &str = "0x146362b000000000000000000400000086fd5cd50b8bb99aa5c8befc197dd8273d17a4530b44e7aca182a4af271bd6a86373950210020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a101000000000000000390084fdbf27d2b79d26a4f13f0ccd982cb755a661969143c37cbc49ef5b91f2701000000000000000389411795514af1627765eceffcbd002719f031604fadd7d188e2dc585b4e1afb000000000000000003bc9d0ca094bd5b8b3225d7651eac5d18c1c04bf8ae8f8b263eebca4e1410ed0c00000000000000006d6880850783e47991669df4fa44075cd0fa5d8532d2a99fce644fcc33c7395522c8526e62b001000000000000000400000086fd5cd50b8bb99aa5c8befc197dd8273d17a4530b44e7aca182a4af271bd6a86e73950210020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a101000000000000000390084fdbf27d2b79d26a4f13f0ccd982cb755a661969143c37cbc49ef5b91f2701000000000000000389411795514af1627765eceffcbd002719f031604fadd7d188e2dc585b4e1afb000000000000000003bc9d0ca094bd5b8b3225d7651eac5d18c1c04bf8ae8f8b263eebca4e1410ed0c0000000000000000";
	const EXPECTED_ROOT: &str =
		"0x86fd5cd50b8bb99aa5c8befc197dd8273d17a4530b44e7aca182a4af271bd6a8";

	const ECDSA_ALICE: &str = "KW39r9CJjAVzmkf9zQ4YDb2hqfAVGdRqn53eRqyruqpxAP5YL";
	const ECDSA_BOB: &str = "KWByAN7WfZABWS5AoWqxriRmF5f2jnDqy3rB5pfHLGkY93ibN";
	const ECDSA_CHARLIE: &str = "KWBpGtyJLBkJERdZT1a1uu19c2uPpZm9nFd8SGtCfRUAT3Y4w";
	const ECDSA_DAVE: &str = "KWCycezxoy7MWTTqA5JDKxJbqVMiNfqThKFhb5dTfsbNaGbrW";

	fn decode<T: Decode>(hex: &str) -> T {
		let hex_bytes = from_hex(hex).expect("invalid bytes");

		Decode::decode(&mut &hex_bytes[..]).expect("conversion failed")
	}

	fn get_ecdsa(hex_key: &str) -> AuthorityId {
		AuthorityId::from_ss58check(hex_key).expect("should be able to convert to beefyid")
	}

	#[test]
	fn test_extract_beefy_stakes() {
		let payload = decode::<Payload>(ENCODED_PAYLOAD);

		let stakes_info =
			BeefyStakesInfo::try_from(&payload).expect("should return BeefyStakesInfo");

		let expected_root = decode::<H256>(EXPECTED_ROOT);
		let current_authority_set = stakes_info.current_authority_set;
		assert_eq!(current_authority_set.keyset_commitment, expected_root.clone());
		assert_eq!(current_authority_set.len, 4);

		assert!(stakes_info.current_stakes.contains(&(get_ecdsa(ECDSA_ALICE), 1)));
		assert!(stakes_info.current_stakes.contains(&(get_ecdsa(ECDSA_BOB), 1)));
		assert!(stakes_info.current_stakes.contains(&(get_ecdsa(ECDSA_CHARLIE), 0)));
		assert!(stakes_info.current_stakes.contains(&(get_ecdsa(ECDSA_DAVE), 0)));

		let next_authority_set = stakes_info.next_authority_set;
		assert_eq!(next_authority_set.keyset_commitment, expected_root);
		assert_eq!(next_authority_set.id, 1);

		assert_eq!(stakes_info.next_stakes.len(), 4);
	}
}
