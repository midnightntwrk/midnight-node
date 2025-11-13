//! Extension of Custom Implementations related to Beefy and Mmr

use crate::{CrossChainPublic, Runtime};
use core::marker::PhantomData;

use authority_selection_inherents::CommitteeMember;

use pallet_beefy::Config as BeefyConfig;
use pallet_beefy_mmr::{Config as BeefyMmrConfig, Pallet as BeefyMmrPallet};
use pallet_mmr::Config as MmrConfig;

use pallet_session_validator_management::{
	CommitteeInfo, Config as SessionValidatorMngConfig, Pallet as SessionValidatorMngPallet,
};
use sp_consensus_beefy::{
	ValidatorSetId,
	ecdsa_crypto::AuthorityId,
	mmr::{BeefyAuthoritySet, BeefyNextAuthoritySet},
};

use sp_runtime::traits::Convert;
use sp_std::vec::Vec;

type MerkleRootOf<T> = <<T as MmrConfig>::Hashing as sp_runtime::traits::Hash>::Output;

type BeefyIdOf<T> = <T as BeefyConfig>::BeefyId;

type CommitteeInfoOf<T> = CommitteeInfo<
	<T as SessionValidatorMngConfig>::ScEpochNumber,
	<T as SessionValidatorMngConfig>::CommitteeMember,
	<T as SessionValidatorMngConfig>::MaxValidators,
>;

/// The StakeDelegation
pub type Stake = u64;
pub type BeefyAuthoritySetOf<T> = BeefyAuthoritySet<MerkleRootOf<T>>;

/// A List of tuple (Beefy Ids, stake)
pub type BeefStakes<T> = Vec<(BeefyIdOf<T>, Stake)>;

/// A tuple of the (Current Beef Stakes, Next Beef Stakes)
pub type DoubleBeefStakes<T> = (BeefStakes<T>, BeefStakes<T>);

/// A tuple of the (Current Beef Stakes, Next Beef Stakes)
pub type IdAndBeefStake<T> = (BeefStakes<T>, BeefStakes<T>);

pub struct AuthoritiesProvider<T> {
	_phantom: PhantomData<T>,
}

// An api to be used and accessed by the Node
sp_api::decl_runtime_apis! {
	pub trait BeefStakesApi<H>
	where
		BeefyAuthoritySet<H>: parity_scale_codec::Decode,
	{
		/// Gets the current beef stakes
		fn current_beef_stakes() -> BeefStakes<Runtime>;

		/// Gets the next beef stakes
		fn next_beef_stakes() -> BeefStakes<Runtime>;

		/// Returns the authority set based on the current beef stakes
		fn compute_current_authority_set(
			beef_stakes: BeefStakes<Runtime>,
		) ->  BeefyAuthoritySet<H>;

		/// Returns the authority set based on the next beef stakes
		fn compute_next_authority_set(
			beef_stakes: BeefStakes<Runtime>,
		) -> BeefyNextAuthoritySet<H>;
	}
}

pub fn current_beef_stakes() -> BeefStakes<Runtime> {
	// Similar set of validators of pallet beefy fn validator_set();
	// the benefit of this is being an unwrapped value of Vec<Public>
	let current_validators = pallet_beefy::pallet::Authorities::<Runtime>::get().to_vec();

	let current_committee = SessionValidatorMngPallet::<Runtime>::current_committee_storage();

	compute_beef_stakes(current_validators, current_committee)
}

pub fn next_beef_stakes() -> BeefStakes<Runtime> {
	let next_validators = pallet_beefy::pallet::NextAuthorities::<Runtime>::get().to_vec();

	match SessionValidatorMngPallet::<Runtime>::next_committee_storage() {
		Some(next_committee) => {
			log::info!("XXXXXXXXXXXXXXXXXXXXXXXXXXXX NEXT VALIDATORS: {next_validators:?}");

			compute_beef_stakes(next_validators, next_committee)
		},
		None => {
			log::info!("XXXXXXXXXXXXXXXXXXXXXXXXXXXX No Next Committee, setting stakes to 0");
			next_validators.into_iter().map(|v| (v, 0)).collect()
		},
	}
}

pub fn compute_current_authority_set(
	beef_stakes: BeefStakes<Runtime>,
) -> BeefyAuthoritySetOf<Runtime> {
	// get the validator set id
	let authority_proof = BeefyMmrPallet::<Runtime>::authority_set_proof();
	let id = authority_proof.id;

	compute_authority_set(id, beef_stakes)
}

pub fn compute_next_authority_set(
	beef_stakes: BeefStakes<Runtime>,
) -> BeefyAuthoritySetOf<Runtime> {
	let authority_proof = BeefyMmrPallet::<Runtime>::next_authority_set_proof();
	let id = authority_proof.id;

	compute_authority_set(id, beef_stakes)
}

fn compute_beef_stakes(
	validators: Vec<BeefyIdOf<Runtime>>,
	committee: CommitteeInfoOf<Runtime>,
) -> BeefStakes<Runtime> {
	let mut committee_members = committee.committee;

	let mut beefy_with_stakes = Vec::new();

	for validator in validators {
		log::info!("XXXXXXXXXXXXXXXXXXXXXXXXXXXX Check if Validator({validator}): in committee:");
		let position = committee_members.iter().position(|elem| match elem {
			CommitteeMember::Permissioned { id, .. } => {
				// convert to beefy
				let committee_id = xchain_public_to_beefy(id.clone());
				if committee_id == validator {
					beefy_with_stakes.push((
						committee_id,
						// default stake
						1,
					));
					// remove from the list
					true
				} else {
					false
				}
			},
			CommitteeMember::Registered { .. } => false,
		});

		if let Some(pos) = position {
			let _ = committee_members.remove(pos);
		} else {
			log::info!(
				"XXXXXXXXXXXXXXXXXXXXXXXXXXXX No match found for Validator({validator}), set stake to 0"
			);
			beefy_with_stakes.push((validator, 0));
		}
	}

	beefy_with_stakes
}

fn compute_authority_set(
	id: ValidatorSetId,
	beef_stakes: BeefStakes<Runtime>,
) -> BeefyAuthoritySetOf<Runtime> {
	let len = beef_stakes.len();

	let beef_stakes_as_bytes = beef_stakes
		.into_iter()
		.map(|(id, stake)| {
			let mut data_bytes =
				<Runtime as BeefyMmrConfig>::BeefyAuthorityToMerkleLeaf::convert(id);

			// convert stake to bytes
			let stake_bytes = stake.to_le_bytes();

			data_bytes.extend_from_slice(&stake_bytes);

			data_bytes
		})
		.collect::<Vec<_>>();

	let keyset_commitment =
		binary_merkle_tree::merkle_root::<<Runtime as MmrConfig>::Hashing, _>(beef_stakes_as_bytes);

	log::info!("XXXXXXXXXXXXXXXXXXXXXXXXXXXX KEYSET_COMMITMENT: {keyset_commitment}");

	BeefyAuthoritySet { id, len: len as u32, keyset_commitment }
}

fn xchain_public_to_beefy(xchain_pub_key: CrossChainPublic) -> AuthorityId {
	let xchain_pub_key = xchain_pub_key.into_inner();
	AuthorityId::from(xchain_pub_key)
}
