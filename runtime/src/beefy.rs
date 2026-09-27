//! The committee commitments that `pallet_beefy_mmr` puts in the MMR leaf.

use crate::Runtime;
use midnight_primitives_beefy::authority_set_commitment;
use sp_consensus_beefy::{OnNewValidatorSet, ValidatorSet, ecdsa_crypto::AuthorityId as BeefyId};

/// Caches the committee commitments of the BEEFY sets that `pallet_beefy_mmr` puts in the MMR leaf.
pub struct SeatCommitments;

impl OnNewValidatorSet<BeefyId> for SeatCommitments {
	fn on_new_validator_set(current: &ValidatorSet<BeefyId>, next: &ValidatorSet<BeefyId>) {
		pallet_beefy_mmr::BeefyAuthorities::<Runtime>::put(authority_set_commitment(current));
		pallet_beefy_mmr::BeefyNextAuthorities::<Runtime>::put(authority_set_commitment(next));
	}
}
