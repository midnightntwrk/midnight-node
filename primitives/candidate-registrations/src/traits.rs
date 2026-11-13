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

//! The two things the pallet needs from the runtime assembling it.

use core::fmt::Debug;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::traits::OpaqueKeys;

use crate::types::{
	CandidateKeys, CrossChainPublicKeyBytes, KeyTypeIdBytes, RegistrationInvalidReason,
};

/// Derives a candidate's `AccountId` from the cross-chain key in their
/// registration.
///
/// The derivation is the runtime's, not the pallet's: the same key has to map
/// to the same account here and wherever the committee is applied.
pub trait CrossChainAccount<AccountId> {
	/// The account this key controls.
	fn account_id(cross_chain_pub_key: &CrossChainPublicKeyBytes) -> AccountId;
}

/// A session key in this registration is already bound to a different account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyCollision;

/// Verifies a registration's session keys and binds them to its account.
///
/// Verification and assembly are one step on purpose. The runtime is the only
/// place that knows what `SessionKeys` is made of, so it is also the place
/// that checks each key's proof — the expression that verifies the Aura proof
/// is the expression that fills `SessionKeys.aura`. There is no second pass
/// that could fail for a reason the first did not already name, and adding a
/// key type to `SessionKeys` without handling it here is a missing-field
/// compile error rather than a runtime surprise.
///
/// Operators are not asked to submit `pallet_session::set_keys` themselves, so
/// the runtime does it for them. Doing it at registration — rather than only
/// once a candidate is selected — is what makes a key conflict a property of
/// the registration, decided and recorded at admission, instead of a committee
/// hand-off that fails at rotation.
pub trait SessionKeyAuthority<AccountId> {
	/// The runtime's `SessionKeys`.
	///
	/// Bounded for storage, not just for use: the keys a registration proved
	/// are kept on its record rather than rebuilt later from the datum's
	/// bytes. `impl_opaque_keys!` supplies everything here but `MaxEncodedLen`,
	/// which a runtime adds with `#[derive(MaxEncodedLen)]` on the macro
	/// invocation — this one already does, because
	/// `pallet_session_validator_management` stores `SessionKeys` too.
	type Keys: OpaqueKeys
		+ Clone
		+ Eq
		+ Debug
		+ Encode
		+ Decode
		+ DecodeWithMemTracking
		+ TypeInfo
		+ MaxEncodedLen;

	/// Checks every session key's proof of possession over `message` and
	/// assembles them.
	///
	/// Reports [`RegistrationInvalidReason::MissingKeyProof`] for a session
	/// key type `keys` does not carry and
	/// [`RegistrationInvalidReason::InvalidKeyProof`] for one whose proof does
	/// not verify. Keys of types that are not session keys are the pallet's
	/// business, not this function's; it may ignore them.
	fn verify_and_assemble(
		keys: &CandidateKeys,
		message: &[u8],
	) -> Result<Self::Keys, RegistrationInvalidReason>;

	/// Binds verified keys to `account`.
	///
	/// The implementation is responsible for making `account` exist first. It
	/// need not roll back on failure — the pallet calls this inside a storage
	/// layer it discards.
	fn bind(account: &AccountId, keys: Self::Keys) -> Result<(), KeyCollision>;

	/// The account a session key is currently bound to, if any.
	///
	/// [`bind`](Self::bind) can only say that *something* collided. This is how
	/// the pallet finds out what, and with whom — so an operator who collided
	/// with their own previous registration is told that, rather than being
	/// pointed at a squatter who does not exist.
	fn key_owner(id: KeyTypeIdBytes, bytes: &[u8]) -> Option<AccountId>;
}
