// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! What `pallet-candidate-registrations` needs from this runtime.
//!
//! The session key half lives here rather than in the pallet because this is
//! the only place that knows what [`SessionKeys`] is made of. Verifying a
//! proof of possession and filling a `SessionKeys` field are the same
//! expression, so a registration that survives verification always assembles,
//! and adding a key type to `SessionKeys` without handling it here is a
//! missing-field compile error.

use midnight_primitives_candidate_registrations::{
	CandidateKeys, CrossChainAccount, CrossChainPublicKeyBytes, KeyCollision, KeyTypeIdBytes,
	RegistrationInvalidReason, SessionKeyAuthority,
};
use pallet_candidate_registrations::validation::{
	find_key, verify_ecdsa, verify_ed25519, verify_sr25519,
};
use pallet_session::SessionInterface;
use sp_core::{
	crypto::{
		KeyTypeId,
		key_types::{AURA, BABE, BEEFY, GRANDPA},
	},
	ecdsa,
};

use crate::{AccountId, CrossChainPublic, Runtime, opaque::SessionKeys};

/// Derives a registered candidate's account from their cross-chain key.
///
/// Goes through `CrossChainPublic`'s own conversion rather than repeating it,
/// because that conversion is the one the committee hand-off applies:
/// `committee_account_ids` maps each member with `authority_id().into()`. A
/// second derivation that drifted from it would bind a candidate's session
/// keys under an account the hand-off never asks about, and `set_keys` would
/// then refuse those keys as `DuplicatedKey` — aborting the rotation for the
/// whole committee, which is the failure registering keys here exists to
/// prevent.
pub struct CrossChainAccountId;

impl CrossChainAccount<AccountId> for CrossChainAccountId {
	fn account_id(cross_chain_pub_key: &CrossChainPublicKeyBytes) -> AccountId {
		CrossChainPublic::from(ecdsa::Public::from_raw(cross_chain_pub_key.0)).into()
	}
}

/// Verifies and binds a registration's session keys.
pub struct RuntimeSessionKeyAuthority;

impl SessionKeyAuthority<AccountId> for RuntimeSessionKeyAuthority {
	type Keys = SessionKeys;

	fn verify_and_assemble(
		keys: &CandidateKeys,
		message: &[u8],
	) -> Result<Self::Keys, RegistrationInvalidReason> {
		// One expression per field: the proof that is checked is the key that
		// is stored. Adding a field to `SessionKeys` fails to compile here.
		Ok(SessionKeys {
			aura: verify_sr25519(find_key(keys, AURA)?, message)?.into(),
			grandpa: verify_ed25519(find_key(keys, GRANDPA)?, message)?.into(),
			babe: verify_sr25519(find_key(keys, BABE)?, message)?.into(),
			beefy: verify_ecdsa(find_key(keys, BEEFY)?, message)?.into(),
		})
	}

	fn bind(account: &AccountId, keys: Self::Keys) -> Result<(), KeyCollision> {
		// `set_keys` is the privileged path: it does not ask for an ownership
		// proof, because the proofs in the registration are that proof. It
		// does need the account to exist, and a candidate's account need not
		// have been touched before.
		if !frame_system::Pallet::<Runtime>::account_exists(account) {
			frame_system::Pallet::<Runtime>::inc_providers(account);
		}

		<pallet_session::Pallet<Runtime> as SessionInterface>::set_keys(account, keys)
			.map_err(|_| KeyCollision)
	}

	fn key_owner(id: KeyTypeIdBytes, bytes: &[u8]) -> Option<AccountId> {
		// `ValidatorId` is `AccountId` here (see `pallet_session::Config`), so
		// the owner the session pallet indexes is the account directly.
		pallet_session::Pallet::<Runtime>::key_owner(KeyTypeId(id), bytes)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::{Pair, crypto::AccountId32};
	use sp_io::hashing::blake2_256;

	/// The derivation is consensus-visible: changing it moves every candidate
	/// that has already registered to a different account, while their session
	/// keys stay bound to the old one. Pinned against the formula rather than
	/// against a second call to the code under test.
	#[test]
	fn a_candidate_account_is_blake2_256_of_the_compressed_cross_chain_key() {
		for seed in 1u8..9 {
			let raw = ecdsa::Pair::from_seed(&[seed; 32]).public().0;

			let derived = CrossChainAccountId::account_id(&CrossChainPublicKeyBytes(raw));

			assert_eq!(derived, AccountId32::new(blake2_256(&raw)), "seed {seed}");
		}
	}

	/// The hand-off maps committee members with `authority_id().into()`
	/// (`committee_account_ids`). A candidate must land on that same account,
	/// or `set_keys` refuses their keys as `DuplicatedKey` at rotation.
	#[test]
	fn a_candidate_account_is_the_account_the_committee_is_applied_under() {
		for seed in 1u8..9 {
			let raw = ecdsa::Pair::from_seed(&[seed; 32]).public().0;

			let at_registration = CrossChainAccountId::account_id(&CrossChainPublicKeyBytes(raw));
			let at_hand_off: AccountId =
				CrossChainPublic::from(ecdsa::Public::from_raw(raw)).into();

			assert_eq!(at_registration, at_hand_off, "seed {seed}");
		}
	}
}
