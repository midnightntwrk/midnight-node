//! Extension of Custom Implementations related to Beefy and Mmr

use crate::{CrossChainPublic, Runtime};
use core::marker::PhantomData;

use authority_selection_inherents::CommitteeMember;

use midnight_primitives_beefy::BeefyStakes;
use pallet_beefy_mmr::{Config as BeefyMmrConfig, Pallet as BeefyMmrPallet};
use pallet_mmr::Config as MmrConfig;

use pallet_session_validator_management::{
	CommitteeInfo, Config as SessionValidatorMngConfig, Pallet as SessionValidatorMngPallet,
};
use sp_consensus_beefy::{
	OnNewValidatorSet, ValidatorSetId, ecdsa_crypto::AuthorityId as BeefyId, mmr::BeefyAuthoritySet,
};

use sp_core::H256;
use sp_runtime::traits::Convert;
use sp_std::vec::Vec;

type CommitteeInfoOf<T> = CommitteeInfo<
	<T as SessionValidatorMngConfig>::ScEpochNumber,
	<T as SessionValidatorMngConfig>::CommitteeMember,
	<T as SessionValidatorMngConfig>::MaxValidators,
>;

pub const BEEFY_LOG_TARGET: &str = "midnight-beefy";

pub fn current_beefy_stakes(validators: Option<Vec<BeefyId>>) -> BeefyStakes<BeefyId> {
	let current_validators = validators.unwrap_or(
		// Similar set of validators of pallet beefy fn validator_set();
		// the benefit of this is being an unwrapped value of Vec<Public>
		pallet_beefy::pallet::Authorities::<Runtime>::get().to_vec(),
	);

	let current_committee = SessionValidatorMngPallet::<Runtime>::current_committee_storage();

	compute_beefy_stakes(current_validators, current_committee)
}

pub fn next_beefy_stakes(next_validators: Option<Vec<BeefyId>>) -> Option<BeefyStakes<BeefyId>> {
	let next_validators =
		next_validators.unwrap_or(pallet_beefy::pallet::NextAuthorities::<Runtime>::get().to_vec());

	SessionValidatorMngPallet::<Runtime>::next_committee_storage().map(|committee| {
		let beefy_stakes = compute_beefy_stakes(next_validators, committee);

		let result = pallet_beefy_mmr::pallet::BeefyNextAuthorities::<Runtime>::get();

		// This is mostly during first run of the chain, where BeefyNextAuthorities was not set.
		if result.keyset_commitment.0 == [0u8; 32] {
			let current_validator_set_id = pallet_beefy::pallet::ValidatorSetId::<Runtime>::get();

			// increment by 1
			let next_set_id = current_validator_set_id + 1;

			let next_authority_set = compute_authority_set(next_set_id, beefy_stakes.clone());

			pallet_beefy_mmr::pallet::BeefyNextAuthorities::<Runtime>::put(&next_authority_set);
			log::debug!(
				"🥩 Out-of-session update on the \"Next\" authority set: {next_authority_set:?}"
			);
		}

		beefy_stakes
	})
}

pub fn compute_current_authority_set(
	beefy_stakes: BeefyStakes<BeefyId>,
) -> BeefyAuthoritySet<H256> {
	// get the validator set id
	let authority_proof = BeefyMmrPallet::<Runtime>::authority_set_proof();
	let id = authority_proof.id;

	compute_authority_set(id, beefy_stakes)
}

pub fn compute_next_authority_set(beefy_stakes: BeefyStakes<BeefyId>) -> BeefyAuthoritySet<H256> {
	let authority_proof = BeefyMmrPallet::<Runtime>::next_authority_set_proof();
	let id = authority_proof.id;

	compute_authority_set(id, beefy_stakes)
}

pub struct AuthoritiesProvider<T> {
	_phantom: PhantomData<T>,
}

impl OnNewValidatorSet<BeefyId> for AuthoritiesProvider<Runtime> {
	fn on_new_validator_set(
		validator_set: &sp_consensus_beefy::ValidatorSet<BeefyId>,
		next_validator_set: &sp_consensus_beefy::ValidatorSet<BeefyId>,
	) {
		log::info!(target: BEEFY_LOG_TARGET, "🥩 Updating Beefy MMR Authorities....");

		let curr_validators = validator_set.validators().to_vec();
		let beefy_stakes = current_beefy_stakes(Some(curr_validators));
		let curr_authority_set = compute_authority_set(validator_set.id(), beefy_stakes);

		log::info!( target: BEEFY_LOG_TARGET, "🥩 New \"Current\" authority set: {curr_authority_set:?}");

		let next_validators = next_validator_set.validators().to_vec();
		if let Some(next_beefy_stakes) = next_beefy_stakes(Some(next_validators)) {
			let next_authority_set =
				compute_authority_set(next_validator_set.id(), next_beefy_stakes);
			log::info!(target: BEEFY_LOG_TARGET, "🥩 New \"Next\" authority set: {next_authority_set:?}");

			pallet_beefy_mmr::pallet::BeefyNextAuthorities::<Runtime>::put(&next_authority_set);
		} else {
			log::info!(target: BEEFY_LOG_TARGET, "🥩 No \"Next\" committee found. No update on `BeefyNextAuthorities`");
		}

		pallet_beefy_mmr::pallet::BeefyAuthorities::<Runtime>::put(&curr_authority_set);
	}
}

fn compute_beefy_stakes(
	validators: Vec<BeefyId>,
	committee: CommitteeInfoOf<Runtime>,
) -> BeefyStakes<BeefyId> {
	let mut committee_members = committee.committee;

	let mut beefy_with_stakes = Vec::new();

	for validator in validators {
		let position = committee_members.iter().position(|member| match member {
			CommitteeMember::Permissioned { id, .. } => {
				are_ids_equal(id.clone(), validator.clone())
			},
			CommitteeMember::Registered { id, .. } => are_ids_equal(id.clone(), validator.clone()),
		});

		// if a position found, remove from the committee list; it will shorten the search in the next iteration
		if let Some(pos) = position {
			let _ = committee_members.remove(pos);
			beefy_with_stakes.push((
				validator, // default stake
				1,
			));
		} else {
			log::warn!("🥩 No match found for {validator}, setting stake to 0");
			beefy_with_stakes.push((validator, 0));
		}
	}

	beefy_with_stakes
}

fn compute_authority_set(
	id: ValidatorSetId,
	beefy_stakes: BeefyStakes<BeefyId>,
) -> BeefyAuthoritySet<H256> {
	let len = beefy_stakes.len();

	let beefy_stakes_as_bytes = beefy_stakes
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

	let keyset_commitment = binary_merkle_tree::merkle_root::<<Runtime as MmrConfig>::Hashing, _>(
		beefy_stakes_as_bytes,
	);

	BeefyAuthoritySet { id, len: len as u32, keyset_commitment }
}

fn are_ids_equal(committee_id: CrossChainPublic, validator: BeefyId) -> bool {
	// convert to a datatype similar to the validator
	let committee_beefy_key = xchain_public_to_beefy(committee_id);

	committee_beefy_key == validator
}

fn xchain_public_to_beefy(xchain_pub_key: CrossChainPublic) -> BeefyId {
	let xchain_pub_key = xchain_pub_key.into_inner();
	BeefyId::from(xchain_pub_key)
}
