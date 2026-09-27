#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{collections::BTreeMap, vec::Vec};
use parity_scale_codec::{Codec, Decode};
use sp_consensus_beefy::{
	ValidatorSet,
	ecdsa_crypto::AuthorityId,
	mmr::{BeefyAuthoritySet, BeefyNextAuthoritySet},
};
use sp_core::H256;
use sp_runtime::{RuntimeAppPublic, traits::Keccak256};

/// The key type for inserting Beefy keys into the keystore
pub const BEEFY_KEY_TYPE: &str = "beef";

pub const BEEFY_LOG_TARGET: &str = "midnight-beefy";

/// The StakeDelegation
pub type Stake = u64;
pub type BeefyAuthoritySetOf<Hash> = BeefyAuthoritySet<Hash>;

pub type BeefyStake<AuthorityId> = (AuthorityId, Stake);

/// A List of tuple (Beefy Ids, stake)
pub type BeefyStakes<AuthorityId> = Vec<BeefyStake<AuthorityId>>;

/// Each distinct compressed key of `validators` with its seat count, ascending by key.
pub fn seats(validators: &[AuthorityId]) -> BTreeMap<[u8; 33], u32> {
	validators.iter().fold(BTreeMap::new(), |mut seats, validator| {
		*seats.entry(validator.clone().into_inner().0).or_insert(0) += 1;
		seats
	})
}

/// The committee leaf `key ‖ seats (u32 LE)`.
pub fn seat_leaf(key: &[u8; 33], seats: u32) -> [u8; 37] {
	let mut leaf = [0u8; 37];
	leaf[..33].copy_from_slice(key);
	leaf[33..].copy_from_slice(&seats.to_le_bytes());
	leaf
}

/// The committee commitment of `set`: the Keccak binary Merkle root of its seat leaves, with the total seat count as `len`.
pub fn authority_set_commitment(set: &ValidatorSet<AuthorityId>) -> BeefyAuthoritySet<H256> {
	let leaves = seats(set.validators()).into_iter().map(|(key, seats)| seat_leaf(&key, seats));
	BeefyAuthoritySet {
		id: set.id(),
		len: set.len() as u32,
		keyset_commitment: binary_merkle_tree::merkle_root::<Keccak256, _>(leaves),
	}
}

/// Ids to identify Beefy stakes
pub mod known_payloads {
	use sp_consensus_beefy::BeefyPayloadId;

	pub const CURRENT_BEEFY_STAKES_ID: BeefyPayloadId = *b"cs";
	pub const CURRENT_BEEFY_AUTHORITY_SET: BeefyPayloadId = *b"cb";
	pub const NEXT_BEEFY_STAKES_ID: BeefyPayloadId = *b"ns";
	pub const NEXT_BEEFY_AUTHORITY_SET: BeefyPayloadId = *b"nb";
}

// An api to be used and accessed by the Node
sp_api::decl_runtime_apis! {
	pub trait BeefyStakesApi<Hash, AuthorityId>
	where
		BeefyAuthoritySet<Hash>: Decode,
		AuthorityId: Codec + RuntimeAppPublic
	{
		/// Gets the current beefy stakes
		fn current_beefy_stakes() -> BeefyStakes<AuthorityId>;

		/// Gets the next beefy stakes
		fn next_beefy_stakes() -> Option<BeefyStakes<AuthorityId>>;

		/// Returns the authority set based on the current beef stakes
		fn compute_current_authority_set(
			beefy_stakes: BeefyStakes<AuthorityId>,
		) ->  BeefyAuthoritySet<Hash>;

		/// Returns the authority set based on the next beef stakes
		fn compute_next_authority_set(
			beefy_stakes: BeefyStakes<AuthorityId>,
		) -> BeefyNextAuthoritySet<Hash> ;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use hex_literal::hex;
	use sp_core::ecdsa;

	fn key(bytes: [u8; 33]) -> AuthorityId {
		ecdsa::Public::from_raw(bytes).into()
	}

	/// `tests/vectors/bridge/commitment.json` (permutation) in midnight-reserve-contracts.
	#[test]
	fn commitment_matches_the_contract_vector() {
		let k1 = key(hex!("0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"));
		let k2 = key(hex!("02c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5"));
		let k3 = key(hex!("02f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9"));
		let set = ValidatorSet::new(vec![k2.clone(), k1, k2, k3], 4).unwrap();

		let commitment = authority_set_commitment(&set);

		assert_eq!(commitment.id, 4);
		assert_eq!(commitment.len, 4);
		assert_eq!(
			commitment.keyset_commitment,
			H256(hex!("052f9919c3c309e60ca131820fde7ce63f1d1d7d3746f066f5263b285c787096"))
		);
	}
}
