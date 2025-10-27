// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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

use authority_selection_inherents::{
	AriadneInherentDataProvider, AuthoritySelectionInputs, CommitteeMember, MaybeFromCandidateKeys,
	RegisterValidatorSignedMessage, filter_trustless_candidates_registrations,
};
use frame_support::{
	Hashable,
	pallet_prelude::*,
	parameter_types,
	traits::{ConstBool, ConstU64},
};
use frame_system::EnsureRoot;
use sidechain_domain::*;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::crypto::CryptoType;
use sp_core::{ByteArray, H256, Pair, crypto::AccountId32};
use sp_core::{ecdsa, ed25519, sr25519};
use sp_runtime::{
	BuildStorage, Digest, DigestItem, MultiSigner, impl_opaque_keys,
	key_types::{AURA, GRANDPA},
	traits::{BlakeTwo256, IdentifyAccount, IdentityLookup, OpaqueKeys},
};
use sp_std::vec::Vec;
use std::cmp::max;

pub const MILLISECS_PER_BLOCK: u64 = 6000;
pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;

pub const DUMMY_EPOCH_NONCE: &[u8] = &[1u8, 2u8, 3u8];

type Block = frame_system::mocking::MockBlock<Test>;

#[derive(
	PartialOrd,
	Ord,
	PartialEq,
	Eq,
	Debug,
	Clone,
	MaxEncodedLen,
	Encode,
	Decode,
	frame_support::Serialize,
	frame_support::Deserialize,
	TypeInfo,
)]
pub struct AccountKeys {
	pub aura: [u8; 32],
	pub grandpa: [u8; 32],
}

impl AccountKeys {
	pub fn from_seed(seed: &str) -> AccountKeys {
		let mut aura = format!("aura-{seed}").into_bytes();
		aura.resize(32, 0);
		let mut grandpa = format!("grandpa-{seed}").into_bytes();
		grandpa.resize(32, 0);
		AccountKeys { aura: aura.try_into().unwrap(), grandpa: grandpa.try_into().unwrap() }
	}
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct AccountId(ecdsa::Public);

pallet_partner_chains_session::impl_pallet_session_config!(Test);

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Sidechain: pallet_sidechain,
		SessionCommitteeManagement: pallet_session_validator_management,
		Aura: pallet_aura,
		Grandpa: pallet_grandpa,
		PalletSession: pallet_session,
		Session: pallet_partner_chains_session,
	}
);

parameter_types! {
	pub const SS58Prefix: u8 = 42;
}
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId32;
	type Lookup = IdentityLookup<Self::AccountId>;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = ConstU64<250>;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
	type Nonce = u64;
	type Block = Block;
	type RuntimeTask = RuntimeTask;
	type SingleBlockMigrations = ();
	type MultiBlockMigrator = ();
	type PreInherents = ();
	type PostInherents = ();
	type PostTransactions = ();
	type ExtensionsWeightInfo = ();
}

use sp_consensus_aura::AURA_ENGINE_ID;

pub const SLOTS_PER_EPOCH: u32 = 7;

impl_opaque_keys! {
	#[derive(MaxEncodedLen, PartialOrd, Ord)]
	pub struct TestSessionKeys {
		pub aura: Aura,
		pub grandpa: Grandpa,
	}
}
impl MaybeFromCandidateKeys for TestSessionKeys {
	fn maybe_from(keys: &sidechain_domain::CandidateKeys) -> Option<Self> {
		let aura = keys.find(AURA)?;
		let aura = sr25519::Public::from_raw(aura.try_into().ok()?);
		let grandpa = keys.find(GRANDPA)?;
		let grandpa = ed25519::Public::from_raw(grandpa.try_into().ok()?);
		Some(Self { aura: aura.into(), grandpa: grandpa.into() })
	}
}

impl pallet_partner_chains_session::Config for Test {
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type ShouldEndSession = ValidatorManagementSessionManager<Test>;
	type NextSessionRotation = ();
	type SessionManager = ValidatorManagementSessionManager<Test>;
	type SessionHandler = <TestSessionKeys as OpaqueKeys>::KeyTypeIdProviders;
	type Keys = TestSessionKeys;
}

impl pallet_sidechain::Config for Test {
	fn current_slot_number() -> ScSlotNumber {
		ScSlotNumber(*pallet_aura::CurrentSlot::<Test>::get())
	}
	type OnNewEpoch = ();
}

impl pallet_session_validator_management::Config for Test {
	type MaxValidators = ConstU32<32>;
	type AuthorityId = CrossChainPublic;
	type AuthorityKeys = TestSessionKeys;
	type AuthoritySelectionInputs = AuthoritySelectionInputs;
	type ScEpochNumber = ScEpochNumber;
	type CommitteeMember = CommitteeMember<Self::AuthorityId, Self::AuthorityKeys>;

	/// Mock simply selects all valid registered candidates as validators.
	fn select_authorities(
		input: AuthoritySelectionInputs,
		_sidechain_epoch: ScEpochNumber,
	) -> Option<BoundedVec<Self::CommitteeMember, Self::MaxValidators>> {
		let candidates: Vec<_> = filter_trustless_candidates_registrations(
			input.registered_candidates,
			Sidechain::genesis_utxo(),
		)
		.into_iter()
		.map(|c| c.0.into())
		.collect();
		if candidates.is_empty() { None } else { Some(BoundedVec::truncate_from(candidates)) }
	}

	fn current_epoch_number() -> ScEpochNumber {
		Sidechain::current_epoch_number()
	}

	type WeightInfo = ();

	type MainChainScriptsOrigin = EnsureRoot<Self::AccountId>;
}

impl pallet_timestamp::Config for Test {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = u64;
	type OnTimestampSet = Aura;
	type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
	type WeightInfo = ();
}

impl pallet_aura::Config for Test {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
	type MaxAuthorities = ConstU32<32>;
	type AllowMultipleBlocksPerSlot = ConstBool<false>;
	type SlotDuration = ConstU64<6000>;
}

impl pallet_grandpa::Config for Test {
	type RuntimeEvent = RuntimeEvent;

	type WeightInfo = ();
	type MaxAuthorities = ConstU32<32>;
	type MaxNominators = ConstU32<0>;
	type MaxSetIdSessionEntries = ConstU64<0>;

	type KeyOwnerProof = sp_core::Void;
	type EquivocationReportSystem = ();
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

	let initial_authorities: Vec<_> = vec![
		(alice().cross_chain.public(), alice().session()).into(),
		(bob().cross_chain.public(), bob().session()).into(),
	];

	let session_keys: Vec<_> =
		vec![(alice().account(), alice().session()), (bob().account(), bob().session())];
	let main_chain_scripts = MainChainScripts::default();
	pallet_session_validator_management::GenesisConfig::<Test> {
		initial_authorities,
		main_chain_scripts,
	}
	.assimilate_storage(&mut t)
	.unwrap();

	pallet_partner_chains_session::GenesisConfig::<Test> { initial_validators: session_keys }
		.assimilate_storage(&mut t)
		.unwrap();

	pallet_sidechain::GenesisConfig::<Test> {
		genesis_utxo: UtxoId::default(),
		slots_per_epoch: SlotsPerEpoch(SLOTS_PER_EPOCH),
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	t.into()
}

pub fn slots_to_epoch(epoch: u64, slots_per_epoch: u32) -> u64 {
	let epoch = ARBITRARY_FIRST_EPOCH + epoch;
	let current_slot = pallet_aura::CurrentSlot::<Test>::get();
	let to_slot = epoch * (slots_per_epoch as u64);
	to_slot - *current_slot
}

pub fn advance_block() {
	finalize_block();
	initialize_block();
}

// in real life first slot will be something much bigger than 0, that's why it is here
pub const ARBITRARY_FIRST_SLOT: u64 = 389374234;
pub const ARBITRARY_FIRST_EPOCH: u64 = ARBITRARY_FIRST_SLOT / (SLOTS_PER_EPOCH as u64);

pub fn initialize_block() {
	let slot = *pallet_aura::CurrentSlot::<Test>::get() + 1;
	let slot = if slot == 1 { slot + ARBITRARY_FIRST_SLOT } else { slot };
	initialize_with_slot_digest_and_increment_block_number(slot);

	System::on_initialize(System::block_number());
	Aura::on_initialize(System::block_number());
	Grandpa::on_initialize(System::block_number());
	SessionCommitteeManagement::on_initialize(System::block_number());
	Session::on_initialize(System::block_number());

	let block_number = System::block_number();
	let epoch = Sidechain::current_epoch_number();
	assert_eq!(slot, *pallet_aura::CurrentSlot::<Test>::get());
	println!("(slot {slot}, epoch {epoch}) Initialized block {block_number}");
}

pub fn finalize_block() {
	if System::block_number() > 0 {
		Session::on_finalize(System::block_number());
		SessionCommitteeManagement::on_finalize(System::block_number());
		Grandpa::on_finalize(System::block_number());
		Aura::on_finalize(System::block_number());
		System::on_finalize(System::block_number());
	}
}

pub fn for_next_n_blocks(n: u32, f: &dyn Fn()) {
	for _ in 1..=n {
		f();
		advance_block();
	}
}

pub fn until_epoch(epoch: u64, f: &dyn Fn()) {
	let n = slots_to_epoch(epoch, SLOTS_PER_EPOCH);
	for_next_n_blocks(u32::try_from(n).unwrap(), f)
}

pub fn until_epoch_after_finalizing(epoch: u64, f: &dyn Fn()) {
	let n = slots_to_epoch(epoch, SLOTS_PER_EPOCH);
	for_next_n_blocks_after_finalizing(u32::try_from(n).unwrap(), f)
}

pub fn for_next_n_blocks_after_finalizing(n: u32, f: &dyn Fn()) {
	for _ in 1..=n {
		finalize_block();
		f();
		initialize_block()
	}
}

pub fn create_inherent_data_struct(
	expected_authorities: &[TestKeys],
) -> AriadneInherentDataProvider {
	let genesis_utxo = pallet_sidechain::Pallet::<Test>::genesis_utxo();

	let candidates: Vec<CandidateRegistrations> = expected_authorities
		.iter()
		.map(|validator| {
			let validator_pub_key: [u8; 33] =
				validator.cross_chain.public().to_raw_vec().try_into().unwrap();
			let mainchain_pub_key_seed: [u8; 32] = validator_pub_key.blake2_256();
			let dummy_mainchain_pub_key: ed25519::Pair = Pair::from_seed(&mainchain_pub_key_seed);
			let registration_utxo = UtxoId::default();
			let signed_message = RegisterValidatorSignedMessage {
				genesis_utxo,
				sidechain_pub_key: validator.cross_chain.public().into_inner().0.to_vec(),
				registration_utxo,
			};

			let signed_message_encoded = plutus::to_datum_cbor_bytes(signed_message);

			let mainchain_signature = dummy_mainchain_pub_key.sign(&signed_message_encoded[..]);
			let sidechain_signature = validator.cross_chain.sign(&signed_message_encoded[..]);

			let registration_data = RegistrationData {
				registration_utxo,
				sidechain_signature: SidechainSignature(
					sidechain_signature.into_inner().0[..64].to_vec(),
				),
				mainchain_signature: MainchainSignature(mainchain_signature.0),
				cross_chain_signature: CrossChainSignature(vec![]),
				sidechain_pub_key: SidechainPublicKey(
					validator.cross_chain.public().into_inner().0.to_vec(),
				),
				cross_chain_pub_key: CrossChainPublicKey(vec![]),
				keys: validator.candidate_keys(),
				utxo_info: UtxoInfo::default(),
				tx_inputs: vec![registration_utxo],
			};

			CandidateRegistrations {
				stake_pool_public_key: StakePoolPublicKey(dummy_mainchain_pub_key.public().0),
				registrations: vec![registration_data],
				stake_delegation: Some(StakeDelegation(7)),
			}
		})
		.collect();

	AriadneInherentDataProvider {
		data: Some(AuthoritySelectionInputs {
			d_parameter: DParameter {
				num_permissioned_candidates: 0,
				num_registered_candidates: max(candidates.len() as u16, 1),
			},
			permissioned_candidates: vec![],
			registered_candidates: candidates,
			epoch_nonce: EpochNonce(DUMMY_EPOCH_NONCE.to_vec()),
		}),
	}
}

pub type CrossChainPair = <CrossChainPublic as CryptoType>::Pair;

const ALICE_SEED: &str = "//1";
const BOB_SEED: &str = "//2";

#[derive(Clone)]
pub struct TestKeys {
	pub cross_chain: CrossChainPair,
	pub aura: sp_consensus_aura::sr25519::AuthorityPair,
	pub grandpa: sp_consensus_grandpa::AuthorityPair,
}

impl TestKeys {
	pub fn from_seed(s: &str) -> Self {
		Self { cross_chain: pair_from_seed(s), aura: pair_from_seed(s), grandpa: pair_from_seed(s) }
	}
	pub fn account(&self) -> AccountId32 {
		MultiSigner::from(sp_core::ecdsa::Public::from(self.cross_chain.public())).into_account()
	}
	pub fn session(&self) -> TestSessionKeys {
		TestSessionKeys { aura: self.aura.public(), grandpa: self.grandpa.public() }
	}
	pub fn candidate_keys(&self) -> CandidateKeys {
		CandidateKeys(vec![
			CandidateKey::new(AURA, self.aura.public().as_slice().into()),
			CandidateKey::new(GRANDPA, self.grandpa.public().as_slice().into()),
		])
	}
}

pub fn pair_from_seed<P: Pair>(seed: &str) -> P {
	<P as Pair>::from_string(seed, None).expect("static values are valid; qed")
}

pub fn alice() -> TestKeys {
	TestKeys::from_seed(ALICE_SEED)
}

pub fn bob() -> TestKeys {
	TestKeys::from_seed(BOB_SEED)
}

#[derive(Clone)]
pub(crate) struct MockValidator {
	pub seed: &'static str,
	pub stake: u64,
}

impl MockValidator {
	pub const fn new(_name: &'static str, seed: &'static str, stake: u64) -> Self {
		Self { seed, stake }
	}
	pub fn account_id(&self) -> AccountId {
		AccountId(self.ecdsa_pair().public())
	}

	pub fn ecdsa_pair(&self) -> ecdsa::Pair {
		ecdsa::Pair::from_string(self.seed, None).expect("static values are valid; qed")
	}

	pub fn sidechain_pub_key(&self) -> SidechainPublicKey {
		SidechainPublicKey(self.account_id().0.0.to_vec())
	}

	pub fn account_keys(&self) -> AccountKeys {
		AccountKeys::from_seed(self.seed)
	}

	pub fn session_keys(&self) -> CandidateKeys {
		let keys = self.account_keys();
		CandidateKeys(vec![
			CandidateKey::new(AURA, keys.aura.to_vec()),
			CandidateKey::new(GRANDPA, keys.grandpa.to_vec()),
		])
	}
}

pub fn alice_mock_validator() -> MockValidator {
	MockValidator::new("alice", "//1", 100)
}

pub fn bob_mock_validator() -> MockValidator {
	MockValidator::new("bob", "//2", 100)
}

pub fn charlie_mock_validator() -> MockValidator {
	MockValidator::new("charlie", "//3", 100)
}

fn initialize_with_slot_digest_and_increment_block_number(slot_number: u64) {
	let slot = sp_consensus_slots::Slot::from(slot_number);
	let pre_digest = Digest { logs: vec![DigestItem::PreRuntime(AURA_ENGINE_ID, slot.encode())] };

	System::reset_events();
	System::initialize(&(System::block_number() + 1), &System::parent_hash(), &pre_digest);
}

macro_rules! assert_current_epoch {
	($epoch:expr) => {{
		assert_eq!(Sidechain::current_epoch_number().0, $epoch + ARBITRARY_FIRST_EPOCH);
	}};
}
pub(crate) use assert_current_epoch;

macro_rules! assert_next_committee {
	([$($member:expr),*]) => {{
		let next = SessionCommitteeManagement::next_committee().unwrap().into_inner();
		assert_eq!(next, vec![$($member.cross_chain.public()),*])
	}};
}
pub(crate) use assert_next_committee;

macro_rules! assert_grandpa_authorities {
	([$($member:expr),*]) => {{
		let expected_authorities = HashSet::from([$($member.grandpa.public()),*]);
		let actual_authorities: Vec<sp_consensus_grandpa::AuthorityId> = Grandpa::grandpa_authorities()
			.into_iter()
			.map(|(authority_id, _)| authority_id)
			.collect();
		let actual_authorities = HashSet::<_>::from_iter(actual_authorities);
		assert_eq!(actual_authorities, expected_authorities);
	}};
}
pub(crate) use assert_grandpa_authorities;

macro_rules! assert_aura_authorities {
    ([$($member:expr),*]) => {{
		let expected_authorities = vec![$($member.aura.public()),*];
		let actual_authorities = pallet_aura::Authorities::<Test>::get();
		assert_eq!(actual_authorities, expected_authorities);
	}};
}
use crate::session_manager::ValidatorManagementSessionManager;
pub(crate) use assert_aura_authorities;
use sidechain_slots::SlotsPerEpoch;
use sp_session_validator_management::MainChainScripts;

use crate::CrossChainPublic;
