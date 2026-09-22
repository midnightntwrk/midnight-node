//! Functionality related to selecting the validators from the valid candidates

use alloc::collections::{BTreeMap, BTreeSet};

use crate::authority_selection_inputs::AuthoritySelectionInputs;
use crate::filter_invalid_candidates::{
	Candidate, filter_invalid_permissioned_candidates, filter_trustless_candidates_registrations,
};
use crate::{CommitteeMember, MaybeFromCandidateKeys};
use log::{info, warn};
use plutus::*;
use sidechain_domain::{EpochNonce, ScEpochNumber, UtxoId};
use sp_core::{Get, U256, ecdsa};
use sp_runtime::{BoundedVec, KeyTypeId};

/// Selects authorities using the Ariadne selection algorithm and data sourced from Partner Chains smart contracts on Cardano.
/// Seed is constructed from the MC epoch nonce and the sidechain epoch.
pub fn select_authorities<
	TAccountId: Clone + Ord + From<ecdsa::Public>,
	TAccountKeys: Clone + Ord + MaybeFromCandidateKeys + sp_runtime::traits::OpaqueKeys,
	MaxAuthorities: Get<u32>,
>(
	genesis_utxo: UtxoId,
	input: AuthoritySelectionInputs,
	sidechain_epoch: ScEpochNumber,
) -> Option<BoundedVec<CommitteeMember<TAccountId, TAccountKeys>, MaxAuthorities>> {
	Some(BoundedVec::truncate_from(select_candidates::<TAccountId, TAccountKeys>(
		genesis_utxo,
		input,
		sidechain_epoch,
	)?))
}

fn select_candidates<
	TAccountId: Clone + Ord + From<ecdsa::Public>,
	TAccountKeys: Clone + Ord + MaybeFromCandidateKeys + sp_runtime::traits::OpaqueKeys,
>(
	genesis_utxo: UtxoId,
	input: AuthoritySelectionInputs,
	sidechain_epoch: ScEpochNumber,
) -> Option<Vec<CommitteeMember<TAccountId, TAccountKeys>>> {
	let valid_registered_candidates = filter_trustless_candidates_registrations::<
		TAccountId,
		TAccountKeys,
	>(input.registered_candidates, genesis_utxo);
	let valid_permissioned_candidates = filter_invalid_permissioned_candidates::<
		TAccountId,
		TAccountKeys,
	>(input.permissioned_candidates);

	let (valid_registered_candidates, valid_permissioned_candidates) =
		remove_duplicated_keys(valid_registered_candidates, valid_permissioned_candidates);

	let valid_permissioned_count = valid_permissioned_candidates.len();
	let valid_registered_count = valid_registered_candidates.len();

	let random_seed = seed_from_nonce_and_sc_epoch(&input.epoch_nonce, &sidechain_epoch);

	if let Some(validators) = selection::ariadne_v2::select_authorities(
		input.d_parameter.num_registered_candidates,
		input.d_parameter.num_permissioned_candidates,
		valid_registered_candidates,
		valid_permissioned_candidates,
		random_seed,
	) {
		info!(
			"💼 Selected committee of {} seats for epoch {} from {valid_permissioned_count} permissioned and {valid_registered_count} registered candidates",
			validators.len(),
			sidechain_epoch
		);
		Some(validators.into_iter().map(|member| member.into()).collect())
	} else {
		warn!("🚫 Failed to select validators for epoch {}", sidechain_epoch);
		None
	}
}

/// Removes duplicates from candidates, so downstream there are no conflicts of the keys.
///
/// Removes any permissioned candidate from the list that has a key already used by candidate present earlier in the list.
/// This is an unlikely human error: the list should be thoroughly verified.
///
/// Removes any registered candidate that has a key used by other candidate in permissioned or registered list regardless of their position in the list.
/// Reasoning is following:
/// 	* remove registered candidates pairwise because one has lost his key(s) and the other stole them; position on list do not indicate who is who in this case
///     * candidate could register and was not yet removed from permissoned list - permissioned list takes priority
///
/// Note: currently registrations data does not contain session keys signatures, so the ownership is not proven.
/// Before Midnight opens for registered candidates this lack of proof of keys ownership will be fixed.
/// If the fix is not present then this deduplication allows attack on registered candidate (eleminate them cheaply),
/// but still protects downstream from consequences of two accounts owning the same key.
/// Issue: https://github.com/midnightntwrk/midnight-node/issues/1761
fn remove_duplicated_keys<TAccountId: Clone + Ord, TAccountKeys: sp_runtime::traits::OpaqueKeys>(
	registered: Vec<(Candidate<TAccountId, TAccountKeys>, selection::Weight)>,
	permissioned: Vec<Candidate<TAccountId, TAccountKeys>>,
) -> (
	Vec<(Candidate<TAccountId, TAccountKeys>, selection::Weight)>,
	Vec<Candidate<TAccountId, TAccountKeys>>,
) {
	// Drop permissioned candidates that have any key that is duplicate of any earlier candidate key.
	let mut warn = false;
	let mut permissioned_keys = BTreeSet::new();
	let permissioned: Vec<_> = permissioned
		.into_iter()
		.filter(|candidate| {
			let cks = candidate.account_keys();
			let keys: Vec<_> = TAccountKeys::key_ids()
				.iter()
				.map(|kt| (*kt, cks.get_raw(*kt).to_vec()))
				.collect();
			let has_collision = keys.iter().any(|key| permissioned_keys.contains(key));
			if !has_collision {
				permissioned_keys.extend(keys);
			} else {
				warn = true;
			}
			!has_collision
		})
		.collect();
	if warn {
		log::warn!(
			"Permissioned candidates list contains a conflict and some candidates were filtered out! Check and fix the list on Cardano!"
		);
	}
	// Count how many times each key appears among the registered candidates, so that
	// every candidate sharing a duplicated key can be dropped, regardless of position.
	let mut registered_key_counts: BTreeMap<(KeyTypeId, Vec<u8>), usize> = BTreeMap::new();
	for (candidate, _) in &registered {
		for kt in TAccountKeys::key_ids() {
			let key_bytes = candidate.account_keys().get_raw(*kt).to_vec();
			let key = (*kt, key_bytes);
			*registered_key_counts.entry(key).or_insert(0) += 1;
		}
	}

	let registered: Vec<_> = registered
		.into_iter()
		.filter(|(candidate, _)| {
			TAccountKeys::key_ids().iter().all(|kt| {
				let key_bytes = candidate.account_keys().get_raw(*kt).to_vec();
				let key = (*kt, key_bytes);
				!permissioned_keys.contains(&key) && registered_key_counts[&key] <= 1
			})
		})
		.collect();

	(registered, permissioned)
}

/// Generate 32 byte seed from epoch nonce and Partner Chain epoch number
pub fn seed_from_nonce_and_sc_epoch(
	epoch_nonce: &EpochNonce,
	partner_chain_epoch_number: &ScEpochNumber,
) -> [u8; 32] {
	U256::from_big_endian(&epoch_nonce.as_array())
		.overflowing_add(U256::from(partner_chain_epoch_number.0))
		.0
		.to_big_endian()
}

#[cfg(test)]
mod tests {
	use super::*;
	use sidechain_domain::{EpochNonce, ScEpochNumber};
	use sp_core::U256;

	#[test]
	fn should_create_correct_seed() {
		let nonce_vec = Vec::from(U256::from(10).to_big_endian());
		assert_eq!(
			seed_from_nonce_and_sc_epoch(&EpochNonce(nonce_vec), &ScEpochNumber(2)),
			U256::from(12).to_big_endian()
		);
	}
}
