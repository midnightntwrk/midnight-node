//! Extension of Custom Implementations related to Beefy and Mmr

use crate::{CrossChainPublic, Runtime};
use core::marker::PhantomData;

use authority_selection_inherents::CommitteeMember;

use pallet_beefy::Config as BeefyConfig;
use pallet_beefy_mmr::Config as BeefyMmrConfig;
use pallet_mmr::Config as MmrConfig;

use pallet_session_validator_management::{
	CommitteeInfo, Config as SessionValidatorMngConfig, Pallet as SessionValidatorMngPallet,
};
use sp_consensus_beefy::{
	OnNewValidatorSet, ValidatorSet, ValidatorSetId,
	ecdsa_crypto::AuthorityId,
	mmr::{BeefyAuthoritySet, BeefyDataProvider},
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

pub struct AuthoritiesProvider<T> {
	_phantom: PhantomData<T>,
}

/// An api to be used and accessed by the Node
sp_api::decl_runtime_apis! {
	pub trait BeefStakesApi {
		/// Returns a tuple of the (Current BeefStakes, Next BeefStakes)
		fn beef_stakes() -> DoubleBeefStakes<Runtime>;
	}
}

/// Collects the BeefStakes for both the current validator set and the next validator set
/// Returns a tuple of the (Current BeefStakes, Next BeefStakes)
pub fn collect_beef_stakes() -> DoubleBeefStakes<Runtime> {
	// Similar set of validators of pallet beefy fn validator_set();
	// the benefit of this is being an unwrapped value of Vec<Public>
	let current_validators = pallet_beefy::pallet::Authorities::<Runtime>::get().to_vec();

	let current_committee = SessionValidatorMngPallet::<Runtime>::current_committee_storage();

	let beef_stakes = compute_beef_stakes(current_validators, current_committee);

	let next_validators = pallet_beefy::pallet::NextAuthorities::<Runtime>::get().to_vec();
	let next_beefy_stakes = match SessionValidatorMngPallet::<Runtime>::next_committee_storage() {
		Some(next_committee) => compute_beef_stakes(next_validators, next_committee),
		None => {
			log::info!("XXXXXXXXXXXXXXXXXXXXXXXXXXXX No Next Committee");
			Vec::new()
		},
	};

	(beef_stakes, next_beefy_stakes)
}

impl OnNewValidatorSet<BeefyIdOf<Runtime>> for AuthoritiesProvider<Runtime> {
	fn on_new_validator_set(
		validator_set: &ValidatorSet<BeefyIdOf<Runtime>>,
		next_validator_set: &ValidatorSet<BeefyIdOf<Runtime>>,
	) {
		log::info!("XXXXXXXXXXXXXXXXXXXXXXXXXXXX Updating Beefy MMR New Authorities....");

		let current_committee = SessionValidatorMngPallet::<Runtime>::current_committee_storage();

		let current_beefy_stakes =
			compute_beef_stakes(validator_set.validators().to_vec(), current_committee);
		let current_authority_set =
			compute_authority_set(validator_set.id(), current_beefy_stakes.clone());

		if let Some(next_committee) = SessionValidatorMngPallet::<Runtime>::next_committee_storage()
		{
			let next_beefy_stakes =
				compute_beef_stakes(next_validator_set.validators().to_vec(), next_committee);
			let next_authority_set =
				compute_authority_set(next_validator_set.id(), next_beefy_stakes);

			log::info!(
				"XXXXXXXXXXXXXXXXXXXXXXXXXXXX Updating Beefy MMR NEXT Authorities: {next_authority_set:?}"
			);
			pallet_beefy_mmr::pallet::BeefyNextAuthorities::<Runtime>::put(&next_authority_set);
		}

		log::info!(
			"XXXXXXXXXXXXXXXXXXXXXXXXXXXX Updating Beefy MMR CURRENT Authorities: {current_authority_set:?}"
		);
		pallet_beefy_mmr::pallet::BeefyAuthorities::<Runtime>::put(&current_authority_set);
	}
}

pub fn compute_beef_stakes(
	validators: Vec<BeefyIdOf<Runtime>>,
	committee: CommitteeInfoOf<Runtime>,
) -> BeefStakes<Runtime> {
	let mut committee_members = committee.committee;

	let mut beefy_with_stakes = Vec::new();

	for validator in validators {
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
		binary_merkle_tree::merkle_root::<<Runtime as MmrConfig>::Hashing, _>(beef_stakes_as_bytes)
			.into();

	log::info!("XXXXXXXXXXXXXXXXXXXXXXXXXXXX NEW KEYSET_COMMITMENT: {keyset_commitment}");

	BeefyAuthoritySet { id, len: len as u32, keyset_commitment }
}

fn xchain_public_to_beefy(xchain_pub_key: CrossChainPublic) -> AuthorityId {
	let xchain_pub_key = xchain_pub_key.into_inner();
	AuthorityId::from(xchain_pub_key)
}
