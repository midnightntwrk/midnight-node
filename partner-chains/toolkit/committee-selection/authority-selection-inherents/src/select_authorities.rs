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

type RawSessionKey = (KeyTypeId, Vec<u8>);

/// Selects authorities using the Ariadne selection algorithm and data sourced from Partner Chains smart contracts on Cardano.
/// Seed is constructed from the MC epoch nonce and the sidechain epoch.
///
/// `to_owner_id` and `key_owner` required by [`remove_keys_owned_by_other_accounts`]
pub fn select_authorities<
	TAccountId: Clone + Ord + From<ecdsa::Public>,
	TAccountKeys: Clone + Ord + MaybeFromCandidateKeys + sp_runtime::traits::OpaqueKeys,
	MaxAuthorities: Get<u32>,
	TOwnerId: PartialEq,
>(
	genesis_utxo: UtxoId,
	input: AuthoritySelectionInputs,
	sidechain_epoch: ScEpochNumber,
	to_owner_id: impl Fn(&TAccountId) -> TOwnerId,
	key_owner: impl Fn(KeyTypeId, &[u8]) -> Option<TOwnerId>,
) -> Option<BoundedVec<CommitteeMember<TAccountId, TAccountKeys>, MaxAuthorities>> {
	Some(BoundedVec::truncate_from(select_candidates::<TAccountId, TAccountKeys, TOwnerId>(
		genesis_utxo,
		input,
		sidechain_epoch,
		to_owner_id,
		key_owner,
	)?))
}

fn select_candidates<
	TAccountId: Clone + Ord + From<ecdsa::Public>,
	TAccountKeys: Clone + Ord + MaybeFromCandidateKeys + sp_runtime::traits::OpaqueKeys,
	TOwnerId: PartialEq,
>(
	genesis_utxo: UtxoId,
	input: AuthoritySelectionInputs,
	sidechain_epoch: ScEpochNumber,
	to_owner_id: impl Fn(&TAccountId) -> TOwnerId,
	key_owner: impl Fn(KeyTypeId, &[u8]) -> Option<TOwnerId>,
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

	let (valid_registered_candidates, valid_permissioned_candidates) =
		remove_keys_owned_by_other_accounts(
			valid_registered_candidates,
			valid_permissioned_candidates,
			to_owner_id,
			key_owner,
		);

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
	let mut permissioned_keys: BTreeSet<RawSessionKey> = BTreeSet::new();
	let permissioned: Vec<_> = permissioned
		.into_iter()
		.filter(|candidate| {
			let cks = candidate.account_keys();
			let keys: Vec<RawSessionKey> = TAccountKeys::key_ids()
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
	let mut registered_key_counts: BTreeMap<RawSessionKey, usize> = BTreeMap::new();
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

/// Removes any candidate (permissioned or registered) that has a key already recorded on-chain
/// in `pallet_session`'s `KeyOwner` storage as belonging to a *different* account.
///
/// This is to prevent committee rotation stall when `set_keys` of `pallet_session` fail.
/// One stolen key, if selected to the committee, could stall the rotation.
///
/// Deliberately runs late in filtering pipeline, because it accesses storage.
/// The trade-off is known: earlier phase could remove the original owner of the lost key.
///
/// `to_owner_id` and `key_owner` are injected rather than called directly against
/// `pallet_session` so this crate doesn't need a dependency on that pallet or its concrete
/// `AccountId` type:
/// - `to_owner_id` mirrors converting a candidate's sidechain public key into
///   `pallet_session`'s account/`ValidatorId` type — a hash of the public key (e.g.
///   `blake2_256` for an ecdsa-derived key), not a lossless conversion.
/// - `key_owner` mirrors `pallet_session::Pallet::<T>::key_owner`.
fn remove_keys_owned_by_other_accounts<TAccountId, TAccountKeys, TOwnerId: PartialEq>(
	registered: Vec<(Candidate<TAccountId, TAccountKeys>, selection::Weight)>,
	permissioned: Vec<Candidate<TAccountId, TAccountKeys>>,
	to_owner_id: impl Fn(&TAccountId) -> TOwnerId,
	key_owner: impl Fn(KeyTypeId, &[u8]) -> Option<TOwnerId>,
) -> (
	Vec<(Candidate<TAccountId, TAccountKeys>, selection::Weight)>,
	Vec<Candidate<TAccountId, TAccountKeys>>,
)
where
	TAccountKeys: sp_runtime::traits::OpaqueKeys,
{
	let owned_by_other_account = |candidate: &Candidate<TAccountId, TAccountKeys>| {
		let owner_id = to_owner_id(candidate.account_id());
		TAccountKeys::key_ids().iter().any(|kt| {
			let key_bytes = candidate.account_keys().get_raw(*kt);
			key_owner(*kt, key_bytes).is_some_and(|owner| owner != owner_id)
		})
	};

	let permissioned: Vec<_> = permissioned
		.into_iter()
		.filter(|candidate| !owned_by_other_account(candidate))
		.collect();
	let registered: Vec<_> = registered
		.into_iter()
		.filter(|(candidate, _)| !owned_by_other_account(candidate))
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
	use crate::filter_invalid_candidates::{CandidateWithStake, PermissionedCandidate};
	use sidechain_domain::{EpochNonce, ScEpochNumber, StakeDelegation, StakePoolPublicKey};
	use sp_core::U256;
	use sp_runtime::impl_opaque_keys;
	use sp_runtime::testing::UintAuthorityId;
	use sp_runtime::traits::OpaqueKeys;

	#[test]
	fn should_create_correct_seed() {
		let nonce_vec = Vec::from(U256::from(10).to_big_endian());
		assert_eq!(
			seed_from_nonce_and_sc_epoch(&EpochNonce(nonce_vec), &ScEpochNumber(2)),
			U256::from(12).to_big_endian()
		);
	}

	impl_opaque_keys! {
		pub struct TestKeys {
			pub foo: UintAuthorityId,
		}
	}

	fn keys(id: u64) -> TestKeys {
		TestKeys { foo: UintAuthorityId::from(id) }
	}

	fn raw_key(id: u64) -> Vec<u8> {
		keys(id).get_raw(TestKeys::key_ids()[0]).to_vec()
	}

	fn permissioned_candidate(account_id: u64, key_id: u64) -> Candidate<u64, TestKeys> {
		Candidate::Permissioned(PermissionedCandidate { account_id, account_keys: keys(key_id) })
	}

	fn registered_candidate(
		account_id: u64,
		key_id: u64,
	) -> (Candidate<u64, TestKeys>, selection::Weight) {
		let candidate = Candidate::Registered(CandidateWithStake {
			stake_pool_pub_key: StakePoolPublicKey([0u8; 32]),
			stake_delegation: StakeDelegation(1),
			account_id,
			account_keys: keys(key_id),
		});
		(candidate, 1u128)
	}

	#[test]
	fn remove_keys_owned_by_other_accounts_drops_conflicting_candidates() {
		// Permissioned:
		// - account 1, key 1: key 1 is unclaimed on-chain -> kept
		// - account 2, key 2: key 2 is owned on-chain by a *different* account (999) -> removed
		let permissioned = vec![permissioned_candidate(1, 1), permissioned_candidate(2, 2)];

		// Registered:
		// - account 3, key 3: key 3 is unclaimed on-chain -> kept
		// - account 4, key 4: key 4 is owned on-chain by a *different* account (999) -> removed
		let registered = vec![registered_candidate(3, 3), registered_candidate(4, 4)];

		let conflicting_keys = [raw_key(2), raw_key(4)];
		let to_owner_id = |id: &u64| *id;
		let key_owner = |_kt: KeyTypeId, key_bytes: &[u8]| -> Option<u64> {
			conflicting_keys.contains(&key_bytes.to_vec()).then_some(999)
		};

		let (registered_out, permissioned_out) =
			remove_keys_owned_by_other_accounts(registered, permissioned, to_owner_id, key_owner);

		assert_eq!(
			permissioned_out.iter().map(|c| *c.account_id()).collect::<Vec<_>>(),
			vec![1],
			"the permissioned candidate whose key is owned by a different account must be removed"
		);
		assert_eq!(
			registered_out.iter().map(|(c, _)| *c.account_id()).collect::<Vec<_>>(),
			vec![3],
			"the registered candidate whose key is owned by a different account must be removed"
		);
	}

	#[test]
	fn remove_keys_owned_by_other_accounts_keeps_candidates_renewing_their_own_key() {
		// Account 1 already owns key 1 on-chain (e.g. renewing it for a new epoch): not a
		// conflict, since `set_keys` only rejects a key already owned by a *different* account.
		let permissioned = vec![permissioned_candidate(1, 1)];
		let registered = vec![registered_candidate(2, 2)];

		let to_owner_id = |id: &u64| *id;
		let key_owner = |_kt: KeyTypeId, key_bytes: &[u8]| -> Option<u64> {
			if key_bytes == raw_key(1) {
				Some(1)
			} else if key_bytes == raw_key(2) {
				Some(2)
			} else {
				None
			}
		};

		let (registered_out, permissioned_out) =
			remove_keys_owned_by_other_accounts(registered, permissioned, to_owner_id, key_owner);

		assert_eq!(permissioned_out.iter().map(|c| *c.account_id()).collect::<Vec<_>>(), vec![1]);
		assert_eq!(
			registered_out.iter().map(|(c, _)| *c.account_id()).collect::<Vec<_>>(),
			vec![2]
		);
	}
}
