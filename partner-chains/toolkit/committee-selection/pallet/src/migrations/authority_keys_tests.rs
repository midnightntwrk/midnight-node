//! Tests for [`super::authority_keys::AuthorityKeysMigration`].
//!
//! The crate-level mock in [`crate::mock`] uses `u64` as `AuthorityKeys`, which cannot satisfy
//! the migration's `pallet_session::Config<Keys = T::AuthorityKeys>` bound (session keys must be
//! [`sp_runtime::traits::OpaqueKeys`]). This module therefore defines its own mock runtime,
//! mirroring the real upgrade scenario the migration exists for: an opaque session keys type
//! gaining an additional key (here `OldSessionKeys { foo }` -> `NewSessionKeys { foo, bar }`,
//! with `bar` using a distinct `KeyTypeId`, like adding e.g. Beefy to Aura + Grandpa).

use super::authority_keys::{AuthorityKeysMigration, UpgradeCommitteeMember};
use crate::pallet::CommitteeInfo;
use frame_support::traits::{ConstU32, OnRuntimeUpgrade, StorageVersion};
use frame_support::{derive_impl, parameter_types};
use frame_system::EnsureRoot;
use parity_scale_codec::{Encode, MaxEncodedLen};
use sp_application_crypto::RuntimeAppPublic;
use sp_core::{ConstU128, crypto::key_types::DUMMY};
use sp_runtime::traits::OpaqueKeys;
use sp_runtime::{BoundedVec, BuildStorage, KeyTypeId, testing::UintAuthorityId};
use sp_session_validator_management::MainChainScripts;

type Block = frame_system::mocking::MockBlock<Test>;

type AccountId = u64;
type AuthorityId = u64;

/// Key type of the session key added by the tested upgrade.
const BAR: KeyTypeId = KeyTypeId(*b"barr");

mod bar {
	use sp_application_crypto::{app_crypto, sr25519};
	app_crypto!(sr25519, super::BAR);
}

sp_runtime::impl_opaque_keys! {
	/// Pre-upgrade session keys shape.
	pub struct OldSessionKeys {
		pub foo: UintAuthorityId,
	}
}

sp_runtime::impl_opaque_keys! {
	#[derive(MaxEncodedLen, PartialOrd, Ord)]
	/// Post-upgrade session keys shape: `foo` is preserved, `bar` is added.
	pub struct NewSessionKeys {
		pub foo: UintAuthorityId,
		pub bar: bar::Public,
	}
}

/// Derives the new `bar` key deterministically from the pre-existing `foo` key, so that each
/// validator gets a distinct `bar` key and `KeyOwner` assertions stay meaningful.
fn bar_key(foo: u64) -> bar::Public {
	UintAuthorityId(foo).to_public_key::<sp_core::sr25519::Public>().into()
}

impl From<OldSessionKeys> for NewSessionKeys {
	fn from(old: OldSessionKeys) -> Self {
		let bar = bar_key(old.foo.0);
		NewSessionKeys { foo: old.foo, bar }
	}
}

type OldCommitteeMember = (AuthorityId, OldSessionKeys);
type NewCommitteeMember = (AuthorityId, NewSessionKeys);

impl UpgradeCommitteeMember<Test> for OldCommitteeMember {
	fn upgrade(self) -> NewCommitteeMember {
		(self.0, self.1.into())
	}
}

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		SessionCommitteeManagement: crate,
		Session: pallet_session,
		Historical: pallet_session::historical,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u128>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = u128;
	type AccountStore = System;
}

impl crate::pallet::Config for Test {
	type MaxValidators = ConstU32<32>;
	type AuthorityId = AuthorityId;
	type AuthorityKeys = NewSessionKeys;
	type AuthoritySelectionInputs = BoundedVec<NewCommitteeMember, Self::MaxValidators>;
	type ScEpochNumber = u64;
	type CommitteeMember = NewCommitteeMember;
	type MainChainScriptsOrigin = EnsureRoot<AccountId>;

	fn select_authorities(
		input: Self::AuthoritySelectionInputs,
		_sidechain_epoch: Self::ScEpochNumber,
	) -> Option<BoundedVec<NewCommitteeMember, Self::MaxValidators>> {
		if input.is_empty() { None } else { Some(input) }
	}

	fn current_epoch_number() -> Self::ScEpochNumber {
		0
	}

	type WeightInfo = ();

	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

parameter_types! {
	pub const Period: u64 = 1;
	pub const Offset: u64 = 0;
}

pub struct TestSessionHandler;
impl pallet_session::SessionHandler<AccountId> for TestSessionHandler {
	const KEY_TYPE_IDS: &'static [KeyTypeId] = &[DUMMY, BAR];

	fn on_genesis_session<Ks: OpaqueKeys>(_validators: &[(AccountId, Ks)]) {}

	fn on_new_session<Ks: OpaqueKeys>(_: bool, _: &[(AccountId, Ks)], _: &[(AccountId, Ks)]) {}

	fn on_disabled(_: u32) {}
}

impl pallet_session::Config for Test {
	type ValidatorId = AuthorityId;
	type ValidatorIdOf = sp_runtime::traits::ConvertInto;
	type ShouldEndSession = crate::Pallet<Test>;
	type NextSessionRotation = ();
	type SessionManager = crate::Pallet<Test>;
	type SessionHandler = TestSessionHandler;
	type Keys = NewSessionKeys;
	type DisablingStrategy = ();
	type WeightInfo = ();
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type KeyDeposit = ConstU128<0>;
}

pub struct FullIdentificationOf;
impl sp_runtime::traits::Convert<AuthorityId, Option<()>> for FullIdentificationOf {
	fn convert(_: AuthorityId) -> Option<()> {
		Some(())
	}
}

impl pallet_session::historical::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type FullIdentification = ();
	type FullIdentificationOf = FullIdentificationOf;
}

/// Storage versions the migration is wired for in these tests.
const FROM: u16 = 1;
const TO: u16 = 2;

type Migration = AuthorityKeysMigration<Test, OldCommitteeMember, OldSessionKeys, FROM, TO>;

type OldCommitteeInfo = CommitteeInfo<u64, OldCommitteeMember, ConstU32<32>>;
type NewCommitteeInfo = CommitteeInfo<u64, NewCommitteeMember, ConstU32<32>>;

const ALICE: AuthorityId = 1;
const BOB: AuthorityId = 2;
/// A validator that rotated out of the committee but still has (stale) session keys registered:
/// `register_committee_keys` never removes keys, so such entries exist on real chains.
const CHARLIE: AuthorityId = 3;

fn old_keys(id: AuthorityId) -> OldSessionKeys {
	OldSessionKeys { foo: UintAuthorityId(id + 100) }
}

fn new_keys(id: AuthorityId) -> NewSessionKeys {
	old_keys(id).into()
}

fn old_committee(epoch: u64, members: &[AuthorityId]) -> OldCommitteeInfo {
	CommitteeInfo {
		epoch,
		committee: BoundedVec::truncate_from(
			members.iter().map(|id| (*id, old_keys(*id))).collect(),
		),
	}
}

fn new_committee(epoch: u64, members: &[AuthorityId]) -> NewCommitteeInfo {
	CommitteeInfo {
		epoch,
		committee: BoundedVec::truncate_from(
			members.iter().map(|id| (*id, new_keys(*id))).collect(),
		),
	}
}

fn assert_committees_eq(left: &NewCommitteeInfo, right: &NewCommitteeInfo) {
	assert_eq!(left.epoch, right.epoch);
	assert_eq!(left.committee.to_vec(), right.committee.to_vec());
}

fn new_test_ext() -> sp_io::TestExternalities {
	let session_committee_management = crate::GenesisConfig::<Test> {
		initial_authorities: vec![(ALICE, new_keys(ALICE)), (BOB, new_keys(BOB))],
		main_chain_scripts: MainChainScripts::default(),
	};
	RuntimeGenesisConfig { session_committee_management, ..Default::default() }
		.build_storage()
		.unwrap()
		.into()
}

/// Rewrites storage to the exact shape a chain running the pre-upgrade runtime would have:
/// old-shaped committees, old-shaped `NextKeys`/`QueuedKeys` and `KeyOwner` entries only for the
/// old key types.
fn seed_old_state(
	current: &OldCommitteeInfo,
	queued: &OldCommitteeInfo,
	next: Option<&OldCommitteeInfo>,
) {
	frame_support::storage::unhashed::put(&crate::CurrentCommittee::<Test>::hashed_key(), current);
	frame_support::storage::unhashed::put(&crate::QueuedCommittee::<Test>::hashed_key(), queued);
	match next {
		Some(next) => {
			frame_support::storage::unhashed::put(&crate::NextCommittee::<Test>::hashed_key(), next)
		},
		None => frame_support::storage::unhashed::kill(&crate::NextCommittee::<Test>::hashed_key()),
	}

	// Wipe whatever genesis registered in the new shape, then register the old shape.
	let _ = pallet_session::NextKeys::<Test>::clear(u32::MAX, None);
	let _ = pallet_session::KeyOwner::<Test>::clear(u32::MAX, None);
	for id in [ALICE, BOB, CHARLIE] {
		let keys = old_keys(id);
		frame_support::storage::unhashed::put(
			&pallet_session::NextKeys::<Test>::hashed_key_for(id),
			&keys,
		);
		pallet_session::KeyOwner::<Test>::insert((DUMMY, keys.foo.to_raw_vec()), id);
	}
	let queued: Vec<(AuthorityId, OldSessionKeys)> =
		vec![(ALICE, old_keys(ALICE)), (BOB, old_keys(BOB))];
	frame_support::storage::unhashed::put(
		&pallet_session::QueuedKeys::<Test>::hashed_key(),
		&queued,
	);

	StorageVersion::new(FROM).put::<crate::Pallet<Test>>();
}

#[test]
fn upgrades_committees_session_keys_and_storage_version() {
	new_test_ext().execute_with(|| {
		seed_old_state(
			&old_committee(5, &[ALICE, BOB]),
			&old_committee(5, &[ALICE, BOB]),
			Some(&old_committee(6, &[BOB])),
		);

		Migration::on_runtime_upgrade();

		assert_eq!(
			StorageVersion::get::<crate::Pallet<Test>>(),
			StorageVersion::new(TO),
			"storage version should be bumped"
		);

		assert_committees_eq(
			&crate::CurrentCommittee::<Test>::get(),
			&new_committee(5, &[ALICE, BOB]),
		);
		assert_committees_eq(
			&crate::QueuedCommittee::<Test>::get(),
			&new_committee(5, &[ALICE, BOB]),
		);
		assert_committees_eq(
			&crate::NextCommittee::<Test>::get().expect("next committee should be preserved"),
			&new_committee(6, &[BOB]),
		);

		// All `NextKeys` entries are upgraded, including CHARLIE's stale one.
		assert_eq!(pallet_session::NextKeys::<Test>::iter_keys().count(), 3);
		for id in [ALICE, BOB, CHARLIE] {
			let keys = new_keys(id);
			assert_eq!(pallet_session::NextKeys::<Test>::get(id), Some(keys.clone()));
			// Ownership of the preserved key is re-registered, ownership of the added key is
			// registered for the first time.
			assert_eq!(
				pallet_session::KeyOwner::<Test>::get((DUMMY, keys.foo.to_raw_vec())),
				Some(id)
			);
			assert_eq!(
				pallet_session::KeyOwner::<Test>::get((BAR, keys.bar.to_raw_vec())),
				Some(id)
			);
		}

		assert_eq!(
			pallet_session::QueuedKeys::<Test>::get(),
			vec![(ALICE, new_keys(ALICE)), (BOB, new_keys(BOB))]
		);
	});
}

#[test]
fn handles_missing_next_committee() {
	new_test_ext().execute_with(|| {
		seed_old_state(&old_committee(5, &[ALICE, BOB]), &old_committee(4, &[ALICE]), None);

		Migration::on_runtime_upgrade();

		assert_eq!(StorageVersion::get::<crate::Pallet<Test>>(), StorageVersion::new(TO));
		assert_committees_eq(
			&crate::CurrentCommittee::<Test>::get(),
			&new_committee(5, &[ALICE, BOB]),
		);
		assert_committees_eq(&crate::QueuedCommittee::<Test>::get(), &new_committee(4, &[ALICE]));
		assert!(crate::NextCommittee::<Test>::get().is_none());
	});
}

#[test]
fn is_noop_when_storage_version_is_not_from() {
	new_test_ext().execute_with(|| {
		// A chain that already upgraded (or was genesis-reset at the new version) stores
		// new-shaped values. Running the migration again must not touch them.
		StorageVersion::new(TO).put::<crate::Pallet<Test>>();
		let state_before = (
			crate::CurrentCommittee::<Test>::get().encode(),
			pallet_session::NextKeys::<Test>::iter().collect::<Vec<_>>(),
			pallet_session::QueuedKeys::<Test>::get(),
		);

		Migration::on_runtime_upgrade();

		assert_eq!(StorageVersion::get::<crate::Pallet<Test>>(), StorageVersion::new(TO));
		let state_after = (
			crate::CurrentCommittee::<Test>::get().encode(),
			pallet_session::NextKeys::<Test>::iter().collect::<Vec<_>>(),
			pallet_session::QueuedKeys::<Test>::get(),
		);
		assert_eq!(state_before, state_after);
	});
}

#[cfg(feature = "try-runtime")]
#[test]
fn try_runtime_hooks_pass() {
	new_test_ext().execute_with(|| {
		seed_old_state(
			&old_committee(5, &[ALICE, BOB]),
			&old_committee(5, &[ALICE, BOB]),
			Some(&old_committee(6, &[BOB])),
		);

		Migration::try_on_runtime_upgrade(true).expect("pre/post upgrade hooks should pass");

		assert_eq!(StorageVersion::get::<crate::Pallet<Test>>(), StorageVersion::new(TO));
	});
}
