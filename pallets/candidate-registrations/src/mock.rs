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

//! Test runtime.
//!
//! The mock [`SessionKeyAuthority`] is deliberately backed by runtime storage
//! rather than by a thread-local: the pallet rolls a rejected registration's
//! key bindings back through a storage layer, and a mock that writes outside
//! storage would silently pass tests the real wiring fails.

use frame_support::{construct_runtime, derive_impl, parameter_types, storage::unhashed};
use midnight_primitives_candidate_registrations::{
	CandidateKeys, CrossChainAccount, CrossChainPublicKeyBytes, KeyCollision, KeyTypeIdBytes,
	RegistrationInvalidReason, SessionKeyAuthority,
};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{
	ByteArray,
	crypto::{
		KeyTypeId,
		key_types::{AURA, BABE, BEEFY, GRANDPA},
	},
	ecdsa, ed25519, sr25519,
};
use sp_runtime::{BuildStorage, traits::OpaqueKeys};

use crate as pallet_candidate_registrations;
use crate::config::CandidateRegistrationsGenesis;
use crate::validation::{find_key, verify_ecdsa, verify_ed25519, verify_sr25519};

pub type AccountId = u64;

construct_runtime! {
	pub enum Test {
		System: frame_system,
		CandidateRegistrations: pallet_candidate_registrations,
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = frame_system::mocking::MockBlock<Test>;
	type AccountId = AccountId;
	type Lookup = sp_runtime::traits::IdentityLookup<AccountId>;
}

parameter_types! {
	pub const MaxRegistrations: u32 = 3;
	pub const MaxRegistrationEventsPerBlock: u32 = 4;
}

/// Stands in for the runtime's `impl_opaque_keys!` struct.
///
/// Hand-rolled rather than generated, because `impl_opaque_keys!` wants a
/// `BoundToRuntimeAppPublic` per field and this test runtime has no consensus
/// pallets. The fields mirror the real one's schemes: sr25519 for Aura and
/// Babe, ed25519 for Grandpa, ECDSA for Beefy.
#[derive(
	Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct MockSessionKeys {
	pub aura: sr25519::Public,
	pub babe: sr25519::Public,
	pub grandpa: ed25519::Public,
	pub beefy: ecdsa::Public,
}

impl OpaqueKeys for MockSessionKeys {
	type KeyTypeIdProviders = ();

	fn key_ids() -> &'static [KeyTypeId] {
		&[AURA, BABE, GRANDPA, BEEFY]
	}

	fn get_raw(&self, i: KeyTypeId) -> &[u8] {
		match i {
			AURA => self.aura.as_slice(),
			BABE => self.babe.as_slice(),
			GRANDPA => self.grandpa.as_slice(),
			BEEFY => self.beefy.as_slice(),
			_ => &[],
		}
	}

	fn ownership_proof_is_valid(&self, _owner: &[u8], _proof: &[u8]) -> bool {
		true
	}
}

/// Collapses a 33-byte cross-chain key to a test account id.
pub struct TruncatingAccount;

impl CrossChainAccount<AccountId> for TruncatingAccount {
	fn account_id(cross_chain_pub_key: &CrossChainPublicKeyBytes) -> AccountId {
		let mut bytes = [0u8; 8];
		bytes.copy_from_slice(&cross_chain_pub_key.0[..8]);
		AccountId::from_le_bytes(bytes)
	}
}

/// Stands in for the runtime: assembles `MockSessionKeys` the same way
/// `runtime/src/candidate_registrations.rs` assembles the real ones, then
/// remembers which account each key is bound to and refuses a key already
/// bound to a different one.
pub struct MockSessionKeyAuthority;

impl MockSessionKeyAuthority {
	fn binding_key(id: [u8; 4], bytes: &[u8]) -> Vec<u8> {
		(b"mock_session_keys", id, bytes).encode()
	}
}

impl SessionKeyAuthority<AccountId> for MockSessionKeyAuthority {
	type Keys = MockSessionKeys;

	fn verify_and_assemble(
		keys: &CandidateKeys,
		message: &[u8],
	) -> Result<Self::Keys, RegistrationInvalidReason> {
		Ok(MockSessionKeys {
			aura: verify_sr25519(find_key(keys, AURA)?, message)?,
			babe: verify_sr25519(find_key(keys, BABE)?, message)?,
			grandpa: verify_ed25519(find_key(keys, GRANDPA)?, message)?,
			beefy: verify_ecdsa(find_key(keys, BEEFY)?, message)?,
		})
	}

	fn bind(account: &AccountId, keys: Self::Keys) -> Result<(), KeyCollision> {
		for id in MockSessionKeys::key_ids() {
			let binding = Self::binding_key(id.0, keys.get_raw(*id));
			match unhashed::get::<AccountId>(&binding) {
				Some(owner) if owner != *account => return Err(KeyCollision),
				_ => unhashed::put(&binding, account),
			}
		}
		Ok(())
	}

	fn key_owner(id: KeyTypeIdBytes, bytes: &[u8]) -> Option<AccountId> {
		unhashed::get::<AccountId>(&Self::binding_key(id, bytes))
	}
}

impl pallet_candidate_registrations::Config for Test {
	type SessionKeyAuthority = MockSessionKeyAuthority;
	type MaxRegistrations = MaxRegistrations;
	type MaxRegistrationEventsPerBlock = MaxRegistrationEventsPerBlock;
	type CrossChainAccount = TruncatingAccount;
	type WeightInfo = ();
}

/// A genesis that points at a plausible validator address.
pub fn genesis_config() -> CandidateRegistrationsGenesis {
	CandidateRegistrationsGenesis {
		validator_address: "addr_test1wztestvalidatoraddress".into(),
		registration_policy_id: [7u8; 28],
		next_cardano_position: Default::default(),
	}
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

	pallet_candidate_registrations::GenesisConfig::<Test> {
		config: genesis_config(),
		_marker: Default::default(),
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	let mut ext = sp_io::TestExternalities::from(storage);
	ext.execute_with(|| System::set_block_number(1));
	ext
}
