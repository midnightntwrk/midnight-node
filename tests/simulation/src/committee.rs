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

//! Committee rotation on a virtual clock: the real `pallet-session-validator-management` and
//! `pallet-session` in a minimal runtime, with sidechain epochs and the availability of the
//! mainchain selection data at each block under the test's control. Weeks of epochs replay in
//! milliseconds.
//!
//! On a node the selection data reaches the runtime through the Ariadne inherent data provider,
//! which supplies nothing while the mainchain epoch it needs is not yet stable. [`SimCommittee::author_block`]
//! reproduces the author's side of that: it runs `create_inherent` with or without data and
//! dispatches whatever call comes back, reporting a panic instead of unwinding the test.

use frame_support::{
	construct_runtime, derive_impl,
	inherent::{InherentData, ProvideInherent},
	pallet_prelude::*,
	parameter_types,
	traits::{ConstU64, Hooks, UnfilteredDispatchable},
};
use frame_system::EnsureRoot;
use pallet_session_validator_management as committee;
use sp_core::{ConstU128, H256};
use sp_runtime::{
	BuildStorage, KeyTypeId,
	key_types::DUMMY,
	testing::UintAuthorityId,
	traits::{BlakeTwo256, IdentityLookup},
};
use sp_session_validator_management::MainChainScripts;
use std::{
	cell::Cell,
	panic::{AssertUnwindSafe, catch_unwind},
	sync::Once,
};

pub type AuthorityId = u64;
pub type AuthorityKeys = u64;
pub type Validator = (AuthorityId, AuthorityKeys);
pub type ScEpochNumber = u64;
type Block = frame_system::mocking::MockBlock<Test>;

sp_runtime::impl_opaque_keys! {
	pub struct SessionKeys {
		pub dummy: UintAuthorityId,
	}
}

impl From<u64> for SessionKeys {
	fn from(value: u64) -> Self {
		SessionKeys { dummy: UintAuthorityId::from(value) }
	}
}

/// Holds the sidechain epoch the runtime believes it is in.
#[frame_support::pallet]
pub mod epoch_pallet {
	use frame_support::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::storage]
	pub type CurrentEpoch<T: Config> = StorageValue<_, u64, ValueQuery>;
}

construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		Committee: committee,
		Session: pallet_session,
		Historical: pallet_session::historical,
		Epoch: epoch_pallet,
	}
);

impl epoch_pallet::Config for Test {}

parameter_types! {
	pub const SS58Prefix: u8 = 42;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountData = pallet_balances::AccountData<u128>;
	type BlockHashCount = ConstU64<250>;
	type SS58Prefix = SS58Prefix;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = u128;
	type AccountStore = System;
}

impl committee::Config for Test {
	type MaxValidators = ConstU32<32>;
	type AuthorityId = AuthorityId;
	type AuthorityKeys = AuthorityKeys;
	type AuthoritySelectionInputs = BoundedVec<Validator, Self::MaxValidators>;
	type ScEpochNumber = ScEpochNumber;
	type CommitteeMember = Validator;
	type MainChainScriptsOrigin = EnsureRoot<Self::AccountId>;

	/// Stands in for Ariadne: the candidates named in the selection data are the committee;
	/// no candidates means no committee can be selected.
	fn select_authorities(
		input: Self::AuthoritySelectionInputs,
		_sidechain_epoch: Self::ScEpochNumber,
	) -> Option<BoundedVec<Validator, Self::MaxValidators>> {
		if input.is_empty() { None } else { Some(input) }
	}

	fn current_epoch_number() -> Self::ScEpochNumber {
		epoch_pallet::CurrentEpoch::<Test>::get()
	}

	type WeightInfo = ();
}

pub struct NoopSessionHandler;
impl pallet_session::SessionHandler<u64> for NoopSessionHandler {
	const KEY_TYPE_IDS: &'static [KeyTypeId] = &[DUMMY];
	fn on_genesis_session<Ks: sp_runtime::traits::OpaqueKeys>(_: &[(u64, Ks)]) {}
	fn on_new_session<Ks: sp_runtime::traits::OpaqueKeys>(
		_: bool,
		_: &[(u64, Ks)],
		_: &[(u64, Ks)],
	) {
	}
	fn on_disabled(_: u32) {}
}

impl pallet_session::Config for Test {
	type ValidatorId = AuthorityId;
	type ValidatorIdOf = sp_runtime::traits::ConvertInto;
	type ShouldEndSession = committee::Pallet<Test>;
	type NextSessionRotation = ();
	type SessionManager = committee::Pallet<Test>;
	type SessionHandler = NoopSessionHandler;
	type Keys = SessionKeys;
	type DisablingStrategy = ();
	type WeightInfo = ();
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type KeyDeposit = ConstU128<0>;
}

pub struct NoIdentification;
impl sp_runtime::traits::Convert<AuthorityId, Option<()>> for NoIdentification {
	fn convert(_: AuthorityId) -> Option<()> {
		Some(())
	}
}

impl pallet_session::historical::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type FullIdentification = ();
	type FullIdentificationOf = NoIdentification;
}

/// What the block author did with the committee inherent in one block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Authoring {
	/// `create_inherent` produced no call: the next committee was already stored.
	NoInherent,
	/// A `set` call was produced and dispatched for the next epoch with this committee.
	Set(Vec<AuthorityId>),
	/// The runtime panicked while building the block; on a node this slot produces no block.
	Panicked(String),
}

thread_local! {
	static QUIET_PANICS: Cell<bool> = const { Cell::new(false) };
}

/// The panic hook is process-global and tests run on parallel threads, so instead of swapping
/// it per call, one hook silences only the thread inside [`SimCommittee::author_block`].
fn install_quiet_panic_hook() {
	static INSTALLED: Once = Once::new();
	INSTALLED.call_once(|| {
		let default = std::panic::take_hook();
		std::panic::set_hook(Box::new(move |info| {
			if !QUIET_PANICS.with(Cell::get) {
				default(info);
			}
		}));
	});
}

pub struct SimCommittee {
	ext: sp_io::TestExternalities,
	block: u64,
}

impl SimCommittee {
	/// A chain whose genesis committee is `initial`, at block 1 of epoch 0.
	pub fn new(initial: &[Validator]) -> Self {
		let genesis = RuntimeGenesisConfig {
			committee: committee::GenesisConfig {
				initial_authorities: initial.to_vec(),
				main_chain_scripts: MainChainScripts::default(),
			},
			..Default::default()
		};
		let mut ext: sp_io::TestExternalities = genesis.build_storage().expect("genesis").into();
		ext.execute_with(|| {
			System::set_block_number(1);
			<Session as Hooks<u64>>::on_initialize(1);
		});
		Self { ext, block: 1 }
	}

	pub fn epoch(&mut self) -> ScEpochNumber {
		self.ext.execute_with(epoch_pallet::CurrentEpoch::<Test>::get)
	}

	/// The committee serving the current epoch, as the pallet records it. (The stock
	/// `pallet-session` used here applies a queued validator set one session late; the node's
	/// partner-chains session pallet follows this storage directly.)
	pub fn current_committee(&mut self) -> Vec<AuthorityId> {
		self.ext.execute_with(|| {
			Committee::current_committee_storage()
				.committee
				.iter()
				.map(|(id, _)| *id)
				.collect()
		})
	}

	pub fn next_committee_stored(&mut self) -> bool {
		self.ext.execute_with(committee::NextCommittee::<Test>::exists)
	}

	/// The author's side of block building for the committee inherent: `create_inherent` with
	/// the selection data the Ariadne provider would supply (`None` while the mainchain epoch
	/// it needs is not stable), then dispatch of the produced call.
	pub fn author_block(&mut self, selection_data: Option<&[Validator]>) -> Authoring {
		let mut data = InherentData::new();
		if let Some(candidates) = selection_data {
			let candidates: BoundedVec<Validator, ConstU32<32>> =
				BoundedVec::truncate_from(candidates.to_vec());
			data.put_data(<Committee as ProvideInherent>::INHERENT_IDENTIFIER, &candidates)
				.expect("inherent data");
		}
		let ext = &mut self.ext;
		install_quiet_panic_hook();
		QUIET_PANICS.with(|q| q.set(true));
		let result = catch_unwind(AssertUnwindSafe(|| {
			ext.execute_with(|| match <Committee as ProvideInherent>::create_inherent(&data) {
				None => Authoring::NoInherent,
				Some(call) => {
					let committee = match &call {
						committee::Call::set { validators, .. } => {
							validators.iter().map(|(id, _)| *id).collect()
						},
						_ => vec![],
					};
					call.dispatch_bypass_filter(RuntimeOrigin::none()).expect("set dispatches");
					Authoring::Set(committee)
				},
			})
		}));
		QUIET_PANICS.with(|q| q.set(false));
		result.unwrap_or_else(|payload| {
			let msg = payload
				.downcast_ref::<String>()
				.cloned()
				.or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
				.unwrap_or_else(|| "non-string panic".into());
			Authoring::Panicked(msg)
		})
	}

	/// Closes the open block and opens the next one in the same epoch.
	pub fn advance_block(&mut self) {
		let n = self.block;
		self.ext.execute_with(|| {
			<Session as Hooks<u64>>::on_finalize(n);
			System::finalize();
			System::reset_events();
			System::initialize(&(n + 1), &Default::default(), &Default::default());
			System::set_block_number(n + 1);
			<Session as Hooks<u64>>::on_initialize(n + 1);
		});
		self.block = n + 1;
	}

	/// Moves the virtual clock into the next sidechain epoch and opens its first block, which
	/// is where `pallet-session` asks whether the session ends and the committee rotates.
	pub fn advance_epoch(&mut self) {
		self.ext.execute_with(|| {
			epoch_pallet::CurrentEpoch::<Test>::mutate(|e| *e += 1);
		});
		self.advance_block();
	}
}
