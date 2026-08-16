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

//! Test mock for the pallet-midnight-system safe mode.

use alloc::vec::Vec;
use frame_support::{derive_impl, traits::Contains};
use sp_core::H256;
use sp_runtime::{
	BuildStorage,
	traits::{BlakeTwo256, IdentityLookup},
};

use crate as pallet_midnight_system;

/// A minimal pallet that exposes a single `DispatchClass::Mandatory` call, standing in
/// for an inherent so we can exercise the safe-mode filter's mandatory carve-out.
#[frame_support::pallet]
pub mod mandatory {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight((Weight::zero(), DispatchClass::Mandatory))]
		pub fn noop(origin: OriginFor<T>) -> DispatchResult {
			ensure_none(origin)?;
			Ok(())
		}
	}
}

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub struct Test {
		System: frame_system = 0,
		MidnightSystem: pallet_midnight_system = 1,
		Mandatory: mandatory = 2,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
}

impl mandatory::Config for Test {}

/// Trivial ledger providers. The safe-mode surface never touches the ledger, so
/// `get_block_context` is unreachable in these tests.
pub struct MockLedger;
impl midnight_primitives::LedgerStateProviderMut for MockLedger {
	fn get_ledger_state_key() -> Vec<u8> {
		Vec::new()
	}
	fn mut_ledger_state<F, E, R>(f: F) -> Result<R, E>
	where
		F: FnOnce(Vec<u8>) -> Result<(Vec<u8>, R), E>,
	{
		f(Vec::new()).map(|(_state_key, r)| r)
	}
}
impl midnight_primitives::LedgerBlockContextProvider for MockLedger {
	fn get_block_context() -> midnight_node_ledger::types::active_version::BlockContext {
		unimplemented!("block context is not needed for safe mode tests")
	}
}

/// Only `MidnightSystem` calls are whitelisted while safe mode is active.
pub struct MockWhitelist;
impl Contains<RuntimeCall> for MockWhitelist {
	fn contains(call: &RuntimeCall) -> bool {
		matches!(call, RuntimeCall::MidnightSystem(_))
	}
}

impl pallet_midnight_system::Config for Test {
	type LedgerStateProviderMut = MockLedger;
	type LedgerBlockContextProvider = MockLedger;
	type WhitelistedCalls = MockWhitelist;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext: sp_io::TestExternalities = t.into();
	ext.execute_with(|| System::set_block_number(1));
	ext
}
