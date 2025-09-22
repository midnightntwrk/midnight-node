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

#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]
// Needed for GetSidechainStatus (used inside of a macro, so can't apply directly)
#![allow(deprecated)]

#[cfg(feature = "runtime-benchmarks")]
#[macro_use]
extern crate frame_benchmarking;

use crate::authorship::current_block_author_aura_index;

extern crate alloc;
use alloc::{collections::BTreeMap, string::String};
use authority_selection_inherents::{
	CommitteeMember,
	authority_selection_inputs::AuthoritySelectionInputs,
	filter_invalid_candidates::{
		Candidate, PermissionedCandidateDataError, RegistrationDataError, StakeError,
		validate_permissioned_candidate_data,
	},
	select_authorities::select_authorities,
};

pub use frame_support::{
	BoundedVec, PalletId, StorageValue, construct_runtime,
	genesis_builder_helper::{build_state, get_preset},
	pallet_prelude::DispatchResult,
	parameter_types, storage,
	traits::{
		ConstBool, ConstU8, ConstU32, ConstU64, ConstU128, Contains, EqualPrivilegeOnly,
		InsideBoth, KeyOwnerProofSystem, NeverEnsureOrigin, Nothing, Randomness, StorageInfo,
	},
	weights::{
		IdentityFee, Weight,
		constants::{
			BlockExecutionWeight, ExtrinsicBaseWeight, ParityDbWeight, WEIGHT_PROOF_SIZE_PER_KB,
			WEIGHT_REF_TIME_PER_SECOND,
		},
	},
};
pub use frame_system::Call as SystemCall;
use frame_system::{EnsureNone, EnsureRoot};
use midnight_node_ledger::types::{GasCost, StorageCost, Tx, active_version::LedgerApiError};
use midnight_primitives_native_token_observation::CardanoPosition;
use opaque::{CrossChainKey, SessionKeys};
use pallet_grandpa::AuthorityId as GrandpaId;
pub use pallet_midnight::{TransactionTypeV2, pallet::Call as MidnightCall};
pub use pallet_midnight_system::Call as MidnightSystemCall;
pub use pallet_session_validator_management;
pub use pallet_timestamp::Call as TimestampCall;
pub use pallet_upgrade::pallet::Call as RuntimeUpgradeCall;
pub use pallet_version::VERSION_ID;
use parity_scale_codec::Encode;
use session_manager::ValidatorManagementSessionManager;
use sidechain_domain::{
	NativeTokenAmount, PermissionedCandidateData, RegistrationData, ScEpochNumber, ScSlotNumber,
	StakeDelegation, StakePoolPublicKey, UtxoId, byte_string::ByteString,
};
use sp_api::impl_runtime_apis;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_beefy::{
	OpaqueKeyOwnershipProof,
	ecdsa_crypto::{AuthorityId as BeefyId, Signature as BeefySignature},
	mmr::MmrLeafVersion,
};
use sp_core::{ByteArray, OpaqueMetadata, crypto::KeyTypeId};
use sp_governed_map::MainChainScriptsV1;

//#[cfg(feature = "experimental")]
//use sp_block_rewards::GetBlockRewardPoints;
#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;
use sp_runtime::traits::{Convert, Keccak256};
use sp_runtime::{
	ApplyExtrinsicResult, Cow, MultiSignature, OpaqueValue, generic, impl_opaque_keys,
	traits::{
		AccountIdLookup, BlakeTwo256, Block as BlockT, Get, IdentifyAccount, NumberFor, OpaqueKeys,
		Verify,
	},
	transaction_validity::{TransactionSource, TransactionValidity},
};
pub use sp_runtime::{Perbill, Permill};
#[allow(deprecated)]
use sp_sidechain::SidechainStatus;
// use sp_staking::SessionIndex;
use sp_std::prelude::*;
use sp_storage::well_known_keys;
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

#[cfg(test)]
mod mock;

/// Handover phase is 1/6th length of an epoch.
/// With committee size 5 we would like any validator to have two slots for signing certificates.
/// 5 * 2 * 6 = 60
/// (Needs to multiply cleanly into 24h)
pub const SLOTS_PER_EPOCH: u32 = 1200;

pub mod authorship;
mod check_call_filter;
mod constants;
mod currency;
mod governance;
mod session_manager;

use check_call_filter::CheckCallFilter;
use constants::time_units::DAYS;
use governance::MembershipHandler;

/// An index to a block.
pub type BlockNumber = u32;

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// Balance of an account.
pub type Balance = u128;

/// Index of a transaction in the chain.
pub type Nonce = u32;

/// A hash of some data used by the chain.
pub type Hash = sp_core::H256;

pub const CROSS_CHAIN: KeyTypeId = KeyTypeId(*b"crch");

/// Opaque types. These are used by the CLI to instantiate machinery that don't need to know
/// the specifics of the runtime. They can then be made to be agnostic over specific formats
/// of data like extrinsics, allowing for them to continue syncing the network through upgrades
/// to even the core data structures.
pub mod opaque {
	use super::*;
	use parity_scale_codec::MaxEncodedLen;
	use sp_core::{ed25519, sr25519};
	pub use sp_runtime::OpaqueExtrinsic as UncheckedExtrinsic;

	/// Opaque block header type.
	pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
	/// Opaque block type.
	pub type Block = generic::Block<Header, UncheckedExtrinsic>;
	/// Opaque block identifier type.
	pub type BlockId = generic::BlockId<Block>;

	pub const CROSS_CHAIN: KeyTypeId = KeyTypeId(*b"crch");
	pub struct CrossChainRuntimeAppPublic;

	pub mod cross_chain_app {
		use super::CROSS_CHAIN;
		use parity_scale_codec::MaxEncodedLen;
		use sidechain_domain::SidechainPublicKey;
		use sp_core::crypto::AccountId32;
		use sp_runtime::MultiSigner;
		use sp_runtime::app_crypto::{app_crypto, ecdsa};
		use sp_runtime::traits::IdentifyAccount;
		use sp_std::vec::Vec;

		app_crypto!(ecdsa, CROSS_CHAIN);
		impl MaxEncodedLen for Signature {
			fn max_encoded_len() -> usize {
				ecdsa::Signature::max_encoded_len()
			}
		}

		impl From<Signature> for Vec<u8> {
			fn from(value: Signature) -> Self {
				value.into_inner().0.to_vec()
			}
		}

		impl From<Public> for AccountId32 {
			fn from(value: Public) -> Self {
				MultiSigner::from(ecdsa::Public::from(value)).into_account()
			}
		}

		impl From<Public> for Vec<u8> {
			fn from(value: Public) -> Self {
				value.into_inner().0.to_vec()
			}
		}

		impl TryFrom<SidechainPublicKey> for Public {
			type Error = SidechainPublicKey;
			fn try_from(pubkey: SidechainPublicKey) -> Result<Self, Self::Error> {
				let cross_chain_public_key =
					Public::try_from(pubkey.0.as_slice()).map_err(|_| pubkey)?;
				Ok(cross_chain_public_key)
			}
		}
	}

	impl_opaque_keys! {
		#[derive(MaxEncodedLen, PartialOrd, Ord)]
		pub struct SessionKeys {
			pub aura: Aura,
			pub grandpa: Grandpa,
			// todo: add the beefy
			// pub beefy: Beefy,
		}
	}

	// todo: check possibililty of adding the beefy ecdsa public
	impl From<(sr25519::Public, ed25519::Public)> for SessionKeys {
		fn from((aura, grandpa): (sr25519::Public, ed25519::Public)) -> Self {
			Self { aura: aura.into(), grandpa: grandpa.into() }
		}
	}

	impl_opaque_keys! {
		pub struct CrossChainKey {
			pub account: CrossChainPublic,
		}
	}
}

pub type CrossChainPublic = opaque::cross_chain_app::Public;

// To learn more about runtime versioning, see:
// https://docs.substrate.io/main-docs/build/upgrade#runtime-versioning
#[cfg(all(not(hardfork_test), not(hardfork_test_rollback)))]
#[allow(clippy::zero_prefixed_literal)]
#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: Cow::Borrowed("midnight"),
	impl_name: Cow::Borrowed("midnight"),
	authoring_version: 1,
	// The version of the runtime specification. A full node will not attempt to use its native
	//   runtime in substitute for the on-chain Wasm runtime unless all of `spec_name`,
	//   `spec_version`, and `authoring_version` are the same between Wasm and native.
	// This value is set to 100 to notify Polkadot-JS App (https://polkadot.js.org/apps) to use
	//   the compatible custom types.
	spec_version: 000_016_002,
	impl_version: 0,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 2,
	system_version: 1,
};

#[cfg(hardfork_test)]
#[allow(clippy::zero_prefixed_literal)]
#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: Cow::Borrowed("midnight"),
	impl_name: Cow::Borrowed("midnight"),
	authoring_version: 1,
	// The version of the runtime specification. A full node will not attempt to use its native
	//   runtime in substitute for the on-chain Wasm runtime unless all of `spec_name`,
	//   `spec_version`, and `authoring_version` are the same between Wasm and native.
	// This value is set to 100 to notify Polkadot-JS App (https://polkadot.js.org/apps) to use
	//   the compatible custom types.
	spec_version: 100_006_002,

	impl_version: 0,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 2,
	system_version: 1,
};

#[cfg(hardfork_test_rollback)]
#[allow(clippy::zero_prefixed_literal)]
#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: Cow::Borrowed("midnight"),
	impl_name: Cow::Borrowed("midnight"),
	authoring_version: 1,
	spec_version: 100_006_002,

	impl_version: 0,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 2,
	system_version: 1,
};

/// This determines the average expected block time that we are targeting.
/// Blocks will be produced at a minimum duration defined by `SLOT_DURATION`.
/// `SLOT_DURATION` is picked up by `pallet_timestamp` which is in turn picked
/// up by `pallet_aura` to implement `fn slot_duration()`.
///
/// Change this to adjust the block time.
// NOTE: Currently it is not possible to change the slot duration after the chain has started.
//       Attempting to do so will brick block production.
// slot time set to 6s
pub const SLOT_DURATION: u64 = 6 * 1000;

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
	NativeVersion { runtime_version: VERSION, can_author_with: Default::default() }
}

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

//todo here
parameter_types! {
	pub const BlockHashCount: BlockNumber = 2400;
	pub const Version: RuntimeVersion = VERSION;
	/// We allow for 2 seconds of compute with a 6 second average block time.
	pub BlockWeights: frame_system::limits::BlockWeights =
	frame_system::limits::BlockWeights::with_sensible_defaults(
		Weight::from_parts(2u64 * WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
		NORMAL_DISPATCH_RATIO,
	);
	pub BlockLength: frame_system::limits::BlockLength = frame_system::limits::BlockLength
		::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
	pub const SS58Prefix: u8 = 42;
}

// Configure FRAME pallets to include in runtime.

impl frame_system::Config for Runtime {
	/// The basic call filter to use in dispatchable.
	type BaseCallFilter = TxPause;
	/// The block type for the runtime.
	type Block = Block;
	/// The type for storing how many extrinsics an account has signed.
	type Nonce = Nonce;
	/// Block & extrinsics weights: base values and limits.
	type BlockWeights = BlockWeights;
	/// The maximum length of a block (in bytes).
	type BlockLength = BlockLength;
	/// The identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The aggregated dispatch type that is available for extrinsics.
	type RuntimeCall = RuntimeCall;
	/// The lookup mechanism to get account ID from whatever is passed in dispatchers.
	type Lookup = AccountIdLookup<AccountId, ()>;
	/// The type for hashing blocks and tries.
	type Hash = Hash;
	/// The hashing algorithm used.
	type Hashing = BlakeTwo256;
	/// The ubiquitous event type.
	type RuntimeEvent = RuntimeEvent;
	/// The ubiquitous origin type.
	type RuntimeOrigin = RuntimeOrigin;
	/// Maximum number of block number to block hash mappings to keep (oldest pruned first).
	type BlockHashCount = BlockHashCount;
	/// The weight of database operations that the runtime can invoke.
	type DbWeight = ParityDbWeight;
	/// Version of the runtime.
	type Version = Version;
	/// Converts a module to the index of the module in `construct_runtime!`.
	///
	/// This type is being generated by `construct_runtime!`.
	type PalletInfo = PalletInfo;
	/// What to do if a new account is created.
	type OnNewAccount = ();
	/// What to do if an account is fully reaped from the system.
	type OnKilledAccount = ();
	/// The data to be stored in an account.
	type AccountData = ();
	/// Weight information for the extrinsics of this pallet.
	type SystemWeightInfo = ();
	/// This is used as an identifier of the chain. 42 is the generic substrate prefix.
	type SS58Prefix = SS58Prefix;
	/// The set code logic, just the default since we're not a parachain.
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
	type RuntimeTask = RuntimeTask;
	type SingleBlockMigrations = (
		// Needed if chain is upgradeing from before PC 1.6
		pallet_session_validator_management::migrations::v1::LegacyToV1Migration<Runtime>,
	);
	type MultiBlockMigrator = MultiBlockMigrations;
	type PreInherents = ();
	type PostInherents = ();
	type PostTransactions = ();
	type ExtensionsWeightInfo = ();
}

impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
	type MaxAuthorities = MaxAuthorities;
	type AllowMultipleBlocksPerSlot = ConstBool<false>;
	type SlotDuration = ConstU64<SLOT_DURATION>;
}

pallet_partner_chains_session::impl_pallet_session_config!(Runtime);

impl pallet_grandpa::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;

	type WeightInfo = ();
	type MaxAuthorities = MaxAuthorities;
	type MaxNominators = ConstU32<5>;
	type MaxSetIdSessionEntries = ConstU64<0>;

	type KeyOwnerProof = sp_core::Void;
	type EquivocationReportSystem = ();
}

impl pallet_beefy::Config for Runtime {
	type BeefyId = BeefyId;
	type MaxAuthorities = MaxAuthorities;
	type MaxNominators = ConstU32<5>;
	type MaxSetIdSessionEntries = ConstU64<0>;
	type OnNewValidatorSet = BeefyMmrLeaf;
	type AncestryHelper = BeefyMmrLeaf;
	type WeightInfo = ();
	type KeyOwnerProof = sp_core::Void;
	type EquivocationReportSystem = ();
}

impl pallet_mmr::Config for Runtime {
	const INDEXING_PREFIX: &'static [u8] = mmr::INDEXING_PREFIX;
	type Hashing = Keccak256;
	type LeafData = pallet_beefy_mmr::Pallet<Runtime>;
	type OnNewRoot = pallet_beefy_mmr::DepositBeefyDigest<Runtime>;
	type BlockHashProvider = pallet_mmr::DefaultBlockHashProvider<Runtime>;
	type WeightInfo = ();
}

/// MMR helper types.
pub mod mmr {
	use super::Runtime;
	pub use pallet_mmr::primitives::*;

	pub type Leaf = <<Runtime as pallet_mmr::Config>::LeafData as LeafDataProvider>::LeafData;
	pub type Hashing = <Runtime as pallet_mmr::Config>::Hashing;
	pub type Hash = <Hashing as sp_runtime::traits::Hash>::Output;
}

parameter_types! {
	/// Version of the produced MMR leaf.
	///
	/// The version consists of two parts;
	/// - `major` (3 bits)
	/// - `minor` (5 bits)
	///
	/// `major` should be updated only if decoding the previous MMR Leaf format from the payload
	/// is not possible (i.e. backward incompatible change).
	/// `minor` should be updated if fields are added to the previous MMR Leaf, which given SCALE
	/// encoding does not prevent old leafs from being decoded.
	///
	/// Hence we expect `major` to be changed really rarely (think never).
	/// See [`MmrLeafVersion`] type documentation for more details.
	pub LeafVersion: MmrLeafVersion = MmrLeafVersion::new(0, 0);
}

pub struct RawBeefyId;
impl Convert<BeefyId, Vec<u8>> for RawBeefyId {
	fn convert(beefy_id: BeefyId) -> Vec<u8> {
		beefy_id.to_raw_vec()
	}
}
impl pallet_beefy_mmr::Config for Runtime {
	type LeafVersion = LeafVersion;
	type BeefyAuthorityToMerkleLeaf = RawBeefyId;
	type LeafExtra = Vec<u8>; // default
	type BeefyDataProvider = ();
	type WeightInfo = ();
}

impl pallet_timestamp::Config for Runtime {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = u64;
	type OnTimestampSet = Aura;
	type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
	type WeightInfo = ();
}

/// Existential deposit.
pub const EXISTENTIAL_DEPOSIT: u128 = 500;

impl pallet_sudo::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type WeightInfo = pallet_sudo::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	pub MbmServiceWeight: Weight = Perbill::from_percent(80) * BlockWeights::get().max_block;
}

impl pallet_migrations::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type Migrations = ();
	// Benchmarks need mocked migrations to guarantee that they succeed.
	#[cfg(feature = "runtime-benchmarks")]
	type Migrations = pallet_migrations::mock_helpers::MockedMigrations;
	type CursorMaxLen = ConstU32<65_536>;
	type IdentifierMaxLen = ConstU32<256>;
	type MigrationStatusHandler = ();
	type FailedMigrationHandler = frame_support::migrations::FreezeChainOnFailedMigration;
	type MaxServiceWeight = MbmServiceWeight;
	type WeightInfo = pallet_migrations::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	pub MaximumSchedulerWeight: Weight = Perbill::from_percent(80) *
		BlockWeights::get().max_block;
}

impl pallet_scheduler::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type PalletsOrigin = OriginCaller;
	type RuntimeCall = RuntimeCall;
	type MaximumWeight = MaximumSchedulerWeight;
	type ScheduleOrigin = EnsureRoot<AccountId>;
	#[cfg(feature = "runtime-benchmarks")]
	type MaxScheduledPerBlock = ConstU32<512>;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type MaxScheduledPerBlock = ConstU32<50>;
	type WeightInfo = pallet_scheduler::weights::SubstrateWeight<Runtime>;
	type OriginPrivilegeCmp = EqualPrivilegeOnly;
	type Preimages = Preimage;
	type BlockNumberProvider = frame_system::Pallet<Runtime>;
}

impl pallet_partner_chains_session::Config for Runtime {
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type ShouldEndSession = ValidatorManagementSessionManager<Runtime>;
	type NextSessionRotation = ();
	type SessionManager = ValidatorManagementSessionManager<Runtime>;
	type SessionHandler = <opaque::SessionKeys as OpaqueKeys>::KeyTypeIdProviders;
	type Keys = opaque::SessionKeys;
}

parameter_types! {
	pub const MaxAuthorities: u32 = 10_000;
}

/// If an override to the D-parameter is set onchain, select the next authorities according to the overridden d-parameter. Otherwise, perform the normal authority selection
fn select_authorities_optionally_overriding(
	mut input: AuthoritySelectionInputs,
	sidechain_epoch: ScEpochNumber,
) -> Option<Vec<Candidate<CrossChainPublic, SessionKeys>>> {
	let d_parameter_override = pallet_midnight::pallet::DParameterOverride::<Runtime>::get();
	if let Some(d_parameter_override) = d_parameter_override {
		input.d_parameter.num_permissioned_candidates = d_parameter_override.0;
		input.d_parameter.num_registered_candidates = d_parameter_override.1;
	}
	select_authorities(Sidechain::genesis_utxo(), input, sidechain_epoch)
}

impl pallet_session_validator_management::Config for Runtime {
	type MaxValidators = MaxAuthorities;
	type AuthorityId = CrossChainPublic;
	type AuthorityKeys = SessionKeys;
	type AuthoritySelectionInputs = AuthoritySelectionInputs;
	type ScEpochNumber = ScEpochNumber;

	fn select_authorities(
		input: AuthoritySelectionInputs,
		sidechain_epoch: ScEpochNumber,
	) -> Option<BoundedVec<Self::CommitteeMember, MaxAuthorities>> {
		Some(BoundedVec::truncate_from(
			select_authorities_optionally_overriding(input, sidechain_epoch)?
				.into_iter()
				.map(CommitteeMember::from)
				.collect(),
		))
	}

	fn current_epoch_number() -> ScEpochNumber {
		Sidechain::current_epoch_number()
	}

	// TODO: Benchmark all pallets
	type WeightInfo = ();

	type CommitteeMember = CommitteeMember<CrossChainPublic, SessionKeys>;

	type MainChainScriptsOrigin = EnsureRoot<Self::AccountId>;
}

pub struct LogBeneficiaries;
impl sp_sidechain::OnNewEpoch for LogBeneficiaries {
	#[cfg(feature = "experimental")]
	fn on_new_epoch(_old_epoch: ScEpochNumber, _new_epoch: ScEpochNumber) -> Weight {
		//let rewards = BlockRewards::get_rewards_and_clear();
		//log::info!("Rewards accrued in epoch {old_epoch}: {rewards:?}");

		ParityDbWeight::get().reads_writes(1, 1)
	}
	#[cfg(not(feature = "experimental"))]
	fn on_new_epoch(_old_epoch: ScEpochNumber, _new_epoch: ScEpochNumber) -> Weight {
		Weight::zero()
	}
}

impl pallet_sidechain::Config for Runtime {
	fn current_slot_number() -> ScSlotNumber {
		ScSlotNumber(*pallet_aura::CurrentSlot::<Self>::get())
	}
	type OnNewEpoch = LogBeneficiaries;
}

pub const BLOCK_REWARD_POINTS: u128 = 500_000;

pub type BeneficiaryId = midnight_node_ledger::types::Hash;
pub type BlockRewardPoints = u128;
pub type BlockReward = (BlockRewardPoints, Option<BeneficiaryId>);

/*
#[cfg(feature = "experimental")]
pub struct LedgerBlockRewardPoints;
#[cfg(feature = "experimental")]
impl GetBlockRewardPoints<BlockRewardPoints> for LedgerBlockRewardPoints {
	fn get_block_reward() -> BlockRewardPoints {
		BLOCK_REWARD_POINTS
	}
}
*/

pub struct LedgerBlockReward;
impl Get<BlockReward> for LedgerBlockReward {
	#[cfg(feature = "experimental")]
	fn get() -> BlockReward {
		/*
		(
			<Runtime as pallet_block_rewards::Config>::GetBlockRewardPoints::get_block_reward(),
			pallet_block_rewards::CurrentBlockBeneficiary::<Runtime>::get(),
		)
		*/
		(0, None)
	}
	#[cfg(not(feature = "experimental"))]
	fn get() -> BlockReward {
		(0, None)
	}
}

/*
#[cfg(feature = "experimental")]
impl pallet_block_rewards::Config for Runtime {
	type BeneficiaryId = BeneficiaryId;
	type BlockRewardPoints = BlockRewardPoints;
	type GetBlockRewardPoints = LedgerBlockRewardPoints;
}
*/

/// Configure the pallet-midnight in pallets/midnight.
impl pallet_midnight::Config for Runtime {
	type WeightInfo = pallet_midnight::weights::SubstrateWeight<Runtime>;
	type BlockReward = LedgerBlockReward;
	type SlotDuration = ConstU64<SLOT_DURATION>;
}

/// Configure the pallet-midnight in pallets/midnight.
impl pallet_midnight_system::Config for Runtime {
	type LedgerStateProviderMut = Midnight;
	type LedgerBlockContextProvider = Midnight;
}

pub struct ValidatorSet;
impl Get<BoundedVec<AuraId, MaxAuthorities>> for ValidatorSet {
	fn get() -> BoundedVec<AuraId, MaxAuthorities> {
		pallet_aura::Authorities::<Runtime>::get()
	}
}

parameter_types! {
	pub const MaxPreimageSize: u32 = pallet_upgrade::MAX_PREIMAGE_SIZE;
	// Max runtimes which can be voted on at one time
	pub const MaxVoteTargets: u8 = 5;
	pub const PalletUpgradeId: PalletId = PalletId(*b"hardfork");
	// TODO: Continue to adjust this value
	pub const SessionsPerVotingPeriod: u32 = 26;
	// Wait 5 blocks following a any check which certifies an upgrade, before performing the upgrade
	pub const UpgradeDelay: BlockNumber = 5;
	pub const UpgradeVoteThreshold: sp_arithmetic::Percent = sp_arithmetic::Percent::from_percent(50);
}

impl pallet_upgrade::Config for Runtime {
	type AuthorityId = AuraId;
	type SessionsPerVotingPeriod = SessionsPerVotingPeriod;
	type MaxValidators = MaxAuthorities;
	type MaxVoteTargets = MaxVoteTargets;
	type PalletId = PalletUpgradeId;
	type PalletsOrigin = OriginCaller;
	type Scheduler = Scheduler;
	type UpgradeDelay = UpgradeDelay;
	type UpgradeVoteThreshold = UpgradeVoteThreshold;
	type ValidatorSet = ValidatorSet;
	type WeightInfo = ();

	fn spec_version() -> RuntimeVersion {
		System::runtime_version()
	}

	fn current_authority() -> Option<AuraId> {
		let index = current_block_author_aura_index::<Runtime>()
			.expect("Each aura block should have an author encoded in the digest");
		pallet_aura::Authorities::<Runtime>::get().get(index).cloned()
	}

	type Preimage = Preimage;
	type SetCode = ();
}

/// Configure the pallet-upgrade in pallets/upgrade.
impl pallet_version::Config for Runtime {
	type WeightInfo = pallet_version::VersionWeight<Runtime>;
	type RuntimeVersion = Version;
}

impl pallet_preimage::Config for Runtime {
	type WeightInfo = pallet_preimage::weights::SubstrateWeight<Runtime>;
	type RuntimeEvent = RuntimeEvent;
	type Currency = currency::CurrencyWaiver;
	type ManagerOrigin = EnsureRoot<AccountId>;
	type Consideration = ();
}

impl pallet_tx_pause::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type PauseOrigin = EnsureRoot<AccountId>;
	type UnpauseOrigin = EnsureRoot<AccountId>;
	type WhitelistedCalls = Nothing;
	type MaxNameLen = ConstU32<256>;
	type WeightInfo = pallet_tx_pause::weights::SubstrateWeight<Runtime>;
}

pub const MOTION_DURATION: BlockNumber = 5 * DAYS;
pub const MAX_PROPOSALS: u32 = 100;
pub const MAX_MEMBERS: u32 = 10;

parameter_types! {
	pub const MotionDuration: BlockNumber = MOTION_DURATION;
	pub MaxProposalWeight: Weight = Perbill::from_percent(50) * BlockWeights::get().max_block;
}

/// Council
impl pallet_collective::Config<pallet_collective::Instance1> for Runtime {
	type RuntimeOrigin = RuntimeOrigin;
	type Proposal = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type MotionDuration = MotionDuration;
	type MaxProposals = ConstU32<MAX_PROPOSALS>;
	type MaxMembers = ConstU32<MAX_PROPOSALS>; // Should be same as `pallet_membership`
	type DefaultVote = pallet_collective::MoreThanMajorityThenPrimeDefaultVote; // TODO: change
	type SetMembersOrigin = NeverEnsureOrigin<()>; // Should be managed from `pallet_membership`
	type MaxProposalWeight = MaxProposalWeight;
	type DisapproveOrigin = EnsureRoot<Self::AccountId>;
	type KillOrigin = EnsureRoot<Self::AccountId>;
	type Consideration = ();
	type WeightInfo = pallet_collective::weights::SubstrateWeight<Runtime>;
}

impl pallet_membership::Config<pallet_membership::Instance1> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type AddOrigin = NeverEnsureOrigin<()>; // Members only managed by `ResetOrigin`
	type RemoveOrigin = NeverEnsureOrigin<()>; // Members only managed by `ResetOrigin`
	type SwapOrigin = NeverEnsureOrigin<()>; // Members only managed by `ResetOrigin`
	type ResetOrigin = EnsureNone<Self::AccountId>; // To be called by an Inherent with `RawOrigin::None`
	type PrimeOrigin = NeverEnsureOrigin<()>; // Members only managed by `ResetOrigin`
	type MembershipInitialized = MembershipHandler<Runtime, Council>;
	type MembershipChanged = MembershipHandler<Runtime, Council>;
	type MaxMembers = ConstU32<MAX_MEMBERS>;
	type WeightInfo = pallet_membership::weights::SubstrateWeight<Runtime>;
}

/// Technical Authority
impl pallet_collective::Config<pallet_collective::Instance2> for Runtime {
	type RuntimeOrigin = RuntimeOrigin;
	type Proposal = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type MotionDuration = MotionDuration;
	type MaxProposals = ConstU32<MAX_PROPOSALS>;
	type MaxMembers = ConstU32<MAX_PROPOSALS>; // Should be same as `pallet_membership`
	type DefaultVote = pallet_collective::MoreThanMajorityThenPrimeDefaultVote; // TODO: change
	type SetMembersOrigin = NeverEnsureOrigin<()>; // Should be managed from `pallet_membership`
	type MaxProposalWeight = MaxProposalWeight;
	type DisapproveOrigin = EnsureRoot<Self::AccountId>;
	type KillOrigin = EnsureRoot<Self::AccountId>;
	type Consideration = ();
	type WeightInfo = pallet_collective::weights::SubstrateWeight<Runtime>;
}

impl pallet_membership::Config<pallet_membership::Instance2> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type AddOrigin = NeverEnsureOrigin<()>; // Members only managed by `ResetOrigin`
	type RemoveOrigin = NeverEnsureOrigin<()>; // Members only managed by `ResetOrigin`
	type SwapOrigin = NeverEnsureOrigin<()>; // Members only managed by `ResetOrigin`
	type ResetOrigin = EnsureNone<Self::AccountId>; // To be called by an Inherent with `RawOrigin::None`
	type PrimeOrigin = NeverEnsureOrigin<()>; // Members only managed by `ResetOrigin`
	type MembershipInitialized = MembershipHandler<Runtime, TechnicalAuthority>;
	type MembershipChanged = MembershipHandler<Runtime, TechnicalAuthority>;
	type MaxMembers = ConstU32<MAX_MEMBERS>;
	type WeightInfo = pallet_membership::weights::SubstrateWeight<Runtime>;
}

pub struct MidnightTokenTransferHandler;

// Replace with pc native token management pallet
impl pallet_native_token_management::TokenTransferHandler for MidnightTokenTransferHandler {
	fn handle_token_transfer(token_amount: NativeTokenAmount) -> DispatchResult {
		// TODO: Needs to have dedicated function on the ledger side for receiving block reward mints
		log::info!("Registered transfer of {} native tokens.", token_amount.0,);
		Ok(())
	}
}

impl pallet_native_token_management::Config for Runtime {
	type TokenTransferHandler = MidnightTokenTransferHandler;
	type WeightInfo = pallet_native_token_management::weights::SubstrateWeight<Runtime>;
	type MainChainScriptsOrigin = EnsureRoot<Self::AccountId>;
}

parameter_types! {
	pub const MaxRegistrationsPerCardanoAddress: u8 = 100;
}

impl pallet_native_token_observation::Config for Runtime {
	type MaxRegistrationsPerCardanoAddress = MaxRegistrationsPerCardanoAddress;
	type MidnightSystemTransactionExecutor = MidnightSystem;
}

parameter_types! {
	pub const MaxChanges: u32 = 16;
	pub const MaxKeyLength: u32 = 64;
	pub const MaxValueLength: u32 = 512;
}

impl pallet_governed_map::Config for Runtime {
	type MaxChanges = MaxChanges;
	type MaxKeyLength = MaxKeyLength;
	type MaxValueLength = MaxValueLength;
	type WeightInfo = pallet_governed_map::weights::SubstrateWeight<Runtime>;

	type OnGovernedMappingChange = ();
	type MainChainScriptsOrigin = EnsureRoot<Self::AccountId>;

	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

// Create the runtime by composing the FRAME pallets that were previously configured.
construct_runtime!(
	pub struct Runtime {
		System: frame_system = 0,
		Timestamp: pallet_timestamp = 1,
		Aura: pallet_aura = 2,
		Grandpa: pallet_grandpa = 3,
		Sidechain: pallet_sidechain = 4,

		// Midnight pallets:
		Midnight: pallet_midnight = 5,
        MidnightSystem: pallet_midnight_system = 6,

		Sudo: pallet_sudo = 7,
		SessionCommitteeManagement: pallet_session_validator_management = 8,
		//#[cfg(feature = "experimental")]
		//BlockRewards: pallet_block_rewards = 9,
		RuntimeUpgrade: pallet_upgrade = 10,
		NodeVersion: pallet_version = 11,

		NativeTokenManagement: pallet_native_token_management = 12,
		NativeTokenObservation: pallet_native_token_observation = 13,

		// Utility
		Preimage: pallet_preimage = 15,

		MultiBlockMigrations: pallet_migrations = 16,
		// Only stub implementation of pallet_session should be wired.
		// Partner Chains session_manager ValidatorManagementSessionManager writes to pallet_session::pallet::CurrentIndex.
		// ValidatorManagementSessionManager is wired in by pallet_partner_chains_session.
		PalletSession: pallet_session = 17,

		Scheduler: pallet_scheduler = 18,
		TxPause: pallet_tx_pause = 19,
		// SafeMode: pallet_safe_mode = 20,

        // BEEFY Bridges support.
        Beefy: pallet_beefy = 21,
        // MMR leaf construction must be after session in order to have a leaf's next_auth_set
		// refer to block<N>. See issue polkadot-fellows/runtimes#160 for details.
        Mmr: pallet_mmr = 22,
        BeefyMmrLeaf: pallet_beefy_mmr = 23,

		// The order matters!! pallet_partner_chains_session needs to come last for correct initialization order
		Session: pallet_partner_chains_session = 30,
        GovernedMap: pallet_governed_map = 31,

        // Governance
        Council: pallet_collective::<Instance1> = 40,
        CouncilMembership: pallet_membership::<Instance1> = 41,

        TechnicalAuthority: pallet_collective::<Instance2> = 43,
        TechnicalAuthorityMembership: pallet_membership::<Instance2> = 42,

	}
);

/// The address format for describing accounts.
pub type Address = sp_runtime::MultiAddress<AccountId, ()>;
/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
/// The SignedExtension to the basic transaction logic.
pub type SignedExtra = (
	frame_system::CheckNonZeroSender<Runtime>,
	frame_system::CheckSpecVersion<Runtime>,
	frame_system::CheckTxVersion<Runtime>,
	frame_system::CheckGenesis<Runtime>,
	frame_system::CheckEra<Runtime>,
	frame_system::CheckNonce<Runtime>,
	frame_system::CheckWeight<Runtime>,
	CheckCallFilter,
);

/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic =
	generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;
/// The payload being signed in transactions.
pub type SignedPayload = generic::SignedPayload<RuntimeCall, SignedExtra>;
/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
	Runtime,
	Block,
	frame_system::ChainContext<Runtime>,
	Runtime,
	AllPalletsWithSystem,
>;

#[cfg(feature = "runtime-benchmarks")]
mod benches {
	define_benchmarks!(
		[frame_benchmarking, BaselineBench::<Runtime>]
		[frame_system, SystemBench::<Runtime>]
		[pallet_beefy_mmr, BeefyMmrLeaf]
		[pallet_timestamp, Timestamp]
		[pallet_sudo, Sudo]
		[pallet_migrations, MultiBlockMigrations]
		[pallet_session_validator_management, SessionValidatorManagementBench::<Runtime>]
		[pallet_upgrade, RuntimeUpgrade]
		[pallet_midnight, Midnight]
	);
}

impl_runtime_apis! {
	impl sp_native_token_management::NativeTokenManagementApi<Block> for Runtime {
		fn get_main_chain_scripts() -> Option<sp_native_token_management::MainChainScripts> {
			NativeTokenManagement::get_main_chain_scripts()
		}
		fn initialized() -> bool {
			NativeTokenManagement::initialized()
		}
	}

	impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
		fn build_state(config: Vec<u8>) -> sp_genesis_builder::Result {
			build_state::<RuntimeGenesisConfig>(config)
		}

		fn get_preset(id: &Option<sp_genesis_builder::PresetId>) -> Option<Vec<u8>> {
			get_preset::<RuntimeGenesisConfig>(id, |_| None)
		}

		fn preset_names() -> Vec<sp_genesis_builder::PresetId> {
			vec![]
		}
	}

	impl sp_api::Core<Block> for Runtime {
		fn version() -> RuntimeVersion {
			VERSION
		}

		fn execute_block(block: Block) {
			Executive::execute_block(block);
		}

		fn initialize_block(header: &<Block as BlockT>::Header) -> sp_runtime::ExtrinsicInclusionMode {
			Executive::initialize_block(header)
		}
	}

	impl pallet_midnight::MidnightRuntimeApi<Block> for Runtime {
		fn get_contract_state(contract_address: Vec<u8>) -> Result<Vec<u8>, LedgerApiError> {
			Midnight::get_contract_state(&contract_address)
		}
		fn get_decoded_transaction(midnight_transaction: Vec<u8>) -> Result<Tx, LedgerApiError>  {
			Midnight::get_decoded_transaction(&midnight_transaction)
		}
		fn get_zswap_chain_state(contract_address: Vec<u8>) -> Result<Vec<u8>, LedgerApiError> {
			Midnight::get_zswap_chain_state(&contract_address)
		}
		fn get_network_id() -> Vec<u8> {
			Midnight::get_network_id()
		}
		fn get_ledger_version() -> Vec<u8> {
			Midnight::get_ledger_version()
		}
		fn get_unclaimed_amount(beneficiary: Vec<u8>) -> Result<u128, LedgerApiError> {
			Midnight::get_unclaimed_amount(&beneficiary)
		}
		fn get_ledger_parameters() -> Result<Vec<u8>, LedgerApiError> {
			Midnight::get_ledger_parameters()
		}
		fn get_transaction_cost(midnight_transaction: Vec<u8>) -> Result<(StorageCost, GasCost), LedgerApiError> {
			Midnight::get_transaction_cost(&midnight_transaction)
		}
		fn get_zswap_state_root() -> Result<Vec<u8>, LedgerApiError> {
			Midnight::get_zswap_state_root()
		}
	}

	impl midnight_primitives_upgrade_api::UpgradeApi<Block> for Runtime {
		fn get_current_version_info() -> (u32, Hash) {
			use sp_core::Hasher;
			let spec_version = System::runtime_version().spec_version;
			let runtime_bytes = storage::unhashed::get_raw(well_known_keys::CODE).expect("Runtime code exists");
			let runtime_hash = BlakeTwo256::hash(&runtime_bytes);
			(spec_version, runtime_hash)
		}
	}

	impl sp_api::Metadata<Block> for Runtime {
		fn metadata() -> OpaqueMetadata {
			OpaqueMetadata::new(Runtime::metadata().into())
		}

		fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
			Runtime::metadata_at_version(version)
		}

		fn metadata_versions() -> sp_std::vec::Vec<u32> {
			Runtime::metadata_versions()
		}
	}

	impl sp_block_builder::BlockBuilder<Block> for Runtime {
		fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
			Executive::apply_extrinsic(extrinsic)
		}

		fn finalize_block() -> <Block as BlockT>::Header {
			Executive::finalize_block()
		}

		fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
			data.create_extrinsics()
		}

		fn check_inherents(
			block: Block,
			data: sp_inherents::InherentData,
		) -> sp_inherents::CheckInherentsResult {
			data.check_extrinsics(&block)
		}
	}

	impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
		fn validate_transaction(
			source: TransactionSource,
			tx: <Block as BlockT>::Extrinsic,
			block_hash: <Block as BlockT>::Hash,
		) -> TransactionValidity {
			Executive::validate_transaction(source, tx, block_hash)
		}
	}

	impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
		fn offchain_worker(header: &<Block as BlockT>::Header) {
			Executive::offchain_worker(header)
		}
	}

	impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
		fn slot_duration() -> sp_consensus_aura::SlotDuration {
			sp_consensus_aura::SlotDuration::from_millis(Aura::slot_duration())
		}

		fn authorities() -> Vec<AuraId> {
			pallet_aura::Authorities::<Runtime>::get().into_inner()
		}
	}

	impl sp_consensus_beefy::BeefyApi<Block, BeefyId> for Runtime {
		fn beefy_genesis() -> Option<BlockNumber> {
			pallet_beefy::GenesisBlock::<Runtime>::get()
		}

		fn validator_set() -> Option<sp_consensus_beefy::ValidatorSet<BeefyId>> {
			Beefy::validator_set()
		}

		fn generate_key_ownership_proof(
			_set_id: sp_consensus_beefy::ValidatorSetId,
			_authority_id: BeefyId,
		) -> Option<OpaqueKeyOwnershipProof> {
			None
		}

		fn submit_report_double_voting_unsigned_extrinsic(
			_equivocation_proof: sp_consensus_beefy::DoubleVotingProof<BlockNumber, BeefyId, BeefySignature>,
			_key_owner_proof: OpaqueValue,
		) -> Option<()> {
			None
		}

		fn submit_report_fork_voting_unsigned_extrinsic(
			_equivocation_proof: sp_consensus_beefy::ForkVotingProof<Header, BeefyId, OpaqueValue>,
			_key_owner_proof: OpaqueKeyOwnershipProof,
		) -> Option<()> {
			None
		}

		fn submit_report_future_block_voting_unsigned_extrinsic(
			_equivocation_proof: sp_consensus_beefy::FutureBlockVotingProof<BlockNumber,BeefyId> ,
			_key_owner_proof: OpaqueKeyOwnershipProof,
		) -> Option<()> {
			None
		}

		fn generate_ancestry_proof(
			prev_block_number: BlockNumber,
			best_known_block_number: Option<BlockNumber>,
		) -> Option<OpaqueValue> {
			Mmr::generate_ancestry_proof(prev_block_number, best_known_block_number)
				.map(|p| p.encode())
				.map(OpaqueKeyOwnershipProof::new)
				.ok()
		}
	}

	impl mmr::MmrApi<Block, Hash, BlockNumber> for Runtime {
		fn mmr_root() -> Result<mmr::Hash, mmr::Error> {
			Ok(Mmr::mmr_root())
		}

		fn mmr_leaf_count() -> Result<mmr::LeafIndex, mmr::Error> {
			Ok(Mmr::mmr_leaves())
		}

		fn generate_proof(
			block_numbers: Vec<BlockNumber>,
			best_known_block_number: Option<BlockNumber>,
		) -> Result<(Vec<mmr::EncodableOpaqueLeaf>, mmr::LeafProof<mmr::Hash>), mmr::Error> {
			Mmr::generate_proof(block_numbers, best_known_block_number).map(
				|(leaves, proof)| {
					(
						leaves
							.into_iter()
							.map(|leaf| mmr::EncodableOpaqueLeaf::from_leaf(&leaf))
							.collect(),
						proof,
					)
				},
			)
		}

		fn verify_proof(leaves: Vec<mmr::EncodableOpaqueLeaf>, proof: mmr::LeafProof<mmr::Hash>)
			-> Result<(), mmr::Error>
		{
			let leaves = leaves.into_iter().map(|leaf|
				leaf.into_opaque_leaf()
				.try_decode()
				.ok_or(mmr::Error::Verify)).collect::<Result<Vec<mmr::Leaf>, mmr::Error>>()?;
			Mmr::verify_leaves(leaves, proof)
		}

		fn verify_proof_stateless(
			root: mmr::Hash,
			leaves: Vec<mmr::EncodableOpaqueLeaf>,
			proof: mmr::LeafProof<mmr::Hash>
		) -> Result<(), mmr::Error> {
			let nodes = leaves.into_iter().map(|leaf|mmr::DataOrHash::Data(leaf.into_opaque_leaf())).collect();
			pallet_mmr::verify_leaves_proof::<mmr::Hashing, _>(root, nodes, proof)
		}
	}

	impl pallet_beefy_mmr::BeefyMmrApi<Block, Hash> for RuntimeApi {
		fn authority_set_proof() -> sp_consensus_beefy::mmr::BeefyAuthoritySet<Hash> {
			BeefyMmrLeaf::authority_set_proof()
		}

		fn next_authority_set_proof() -> sp_consensus_beefy::mmr::BeefyNextAuthoritySet<Hash> {
			BeefyMmrLeaf::next_authority_set_proof()
		}
	}

	impl sp_session::SessionKeys<Block> for Runtime {
		fn generate_session_keys(seed: Option<Vec<u8>>) -> Vec<u8> {
			// despite being named "generate" this function also adds generated keys to local keystore
			CrossChainKey::generate(seed.clone());
			SessionKeys::generate(seed)
		}

		fn decode_session_keys(
			encoded: Vec<u8>,
		) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
			SessionKeys::decode_into_raw_public_keys(&encoded)
		}
	}

	impl sp_consensus_grandpa::GrandpaApi<Block> for Runtime {
		fn grandpa_authorities() -> sp_consensus_grandpa::AuthorityList {
			Grandpa::grandpa_authorities()
		}

		fn current_set_id() -> sp_consensus_grandpa::SetId {
			Grandpa::current_set_id()
		}

		fn submit_report_equivocation_unsigned_extrinsic(
			_equivocation_proof: sp_consensus_grandpa::EquivocationProof<
				<Block as BlockT>::Hash,
				NumberFor<Block>,
			>,
			_key_owner_proof: sp_consensus_grandpa::OpaqueKeyOwnershipProof,
		) -> Option<()> {
			None
		}

		fn generate_key_ownership_proof(
			_set_id: sp_consensus_grandpa::SetId,
			_authority_id: GrandpaId,
		) -> Option<sp_consensus_grandpa::OpaqueKeyOwnershipProof> {
			// NOTE: this is the only implementation possible since we've
			// defined our key owner proof type as a bottom type (i.e. a type
			// with no values).
			None
		}
	}

	impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce> for Runtime {
		fn account_nonce(account: AccountId) -> Nonce {
			System::account_nonce(account)
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl frame_benchmarking::Benchmark<Block> for Runtime {
		fn benchmark_metadata(extra: bool) -> (
			Vec<frame_benchmarking::BenchmarkList>,
			Vec<frame_support::traits::StorageInfo>,
		) {
			use frame_benchmarking::{baseline, Benchmarking, BenchmarkList};
			use frame_support::traits::StorageInfoTrait;
			use frame_system_benchmarking::Pallet as SystemBench;
			use baseline::Pallet as BaselineBench;
			use pallet_session_validator_management_benchmarking::Pallet as SessionValidatorManagementBench;

			let mut list = Vec::<BenchmarkList>::new();
			list_benchmarks!(list, extra);

			let storage_info = AllPalletsWithSystem::storage_info();

			(list, storage_info)
		}

		fn dispatch_benchmark(
			config: frame_benchmarking::BenchmarkConfig
		) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
			use frame_benchmarking::{baseline, Benchmarking, BenchmarkBatch};
			use sp_storage::TrackedStorageKey;

			use frame_system_benchmarking::Pallet as SystemBench;
			use baseline::Pallet as BaselineBench;
			use pallet_session_validator_management_benchmarking::Pallet as SessionValidatorManagementBench;

			impl frame_system_benchmarking::Config for Runtime {}
			impl baseline::Config for Runtime {}
			impl pallet_session_validator_management_benchmarking::Config for Runtime {}

			use frame_support::traits::WhitelistedStorageKeys;
			let whitelist: Vec<TrackedStorageKey> = AllPalletsWithSystem::whitelisted_storage_keys();

			let mut batches = Vec::<BenchmarkBatch>::new();
			let params = (&config, &whitelist);
			add_benchmarks!(params, batches);

			Ok(batches)
		}
	}

	#[cfg(feature = "try-runtime")]
	impl frame_try_runtime::TryRuntime<Block> for Runtime {
		fn on_runtime_upgrade(checks: frame_try_runtime::UpgradeCheckSelect) -> (Weight, Weight) {
			// NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
			// have a backtrace here. If any of the pre/post migration checks fail, we shall stop
			// right here and right now.
			let weight = Executive::try_runtime_upgrade(checks).unwrap();
			(weight, BlockWeights::get().max_block)
		}

		fn execute_block(
			block: Block,
			state_root_check: bool,
			signature_check: bool,
			select: frame_try_runtime::TryStateSelect
		) -> Weight {
			// NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
			// have a backtrace here.
			Executive::try_execute_block(block, state_root_check, signature_check, select).expect("execute-block failed")
		}
	}

	impl sp_sidechain::GetSidechainStatus<Block> for Runtime {
		fn get_sidechain_status() -> SidechainStatus {
			SidechainStatus {
				epoch: Sidechain::current_epoch_number(),
				slot: ScSlotNumber(*pallet_aura::CurrentSlot::<Runtime>::get()),
				slots_per_epoch: Sidechain::slots_per_epoch().0,
			}
		}
	}

	impl sp_sidechain::GetGenesisUtxo<Block> for Runtime {
		fn genesis_utxo() -> UtxoId {
			Sidechain::genesis_utxo()
		}
	}

	impl sidechain_slots::SlotApi<Block> for Runtime {
		fn slot_config() -> sidechain_slots::ScSlotConfig {
			sidechain_slots::ScSlotConfig {
				slots_per_epoch: Sidechain::slots_per_epoch(),
				slot_duration: <Self as sp_consensus_aura::runtime_decl_for_aura_api::AuraApi<Block, AuraId>>::slot_duration()
			}
		}
	}

	impl sp_session_validator_management::SessionValidatorManagementApi<
		Block,
		<Runtime as pallet_session_validator_management::Config>::CommitteeMember,
		AuthoritySelectionInputs,
		sidechain_domain::ScEpochNumber
	> for Runtime {
		fn get_current_committee() -> (ScEpochNumber, sidechain_domain::Vec<authority_selection_inherents::CommitteeMember<CrossChainPublic, opaque::SessionKeys>>) {
			SessionCommitteeManagement::current_committee_storage().as_pair()
		}
		fn get_next_committee() -> Option<(ScEpochNumber, sidechain_domain::Vec<authority_selection_inherents::CommitteeMember<CrossChainPublic, opaque::SessionKeys>>)>  {
			Some(SessionCommitteeManagement::next_committee_storage()?.as_pair())
		}
		fn get_next_unset_epoch_number() -> sidechain_domain::ScEpochNumber {
			SessionCommitteeManagement::get_next_unset_epoch_number()
		}
		fn calculate_committee(authority_selection_inputs: AuthoritySelectionInputs, sidechain_epoch: sidechain_domain::ScEpochNumber) -> Option<Vec<authority_selection_inherents::CommitteeMember<CrossChainPublic, opaque::SessionKeys>>> {
			SessionCommitteeManagement::calculate_committee(authority_selection_inputs, sidechain_epoch)
		}
		fn get_main_chain_scripts() -> sp_session_validator_management::MainChainScripts {
			SessionCommitteeManagement::get_main_chain_scripts()
		}
	}

	impl authority_selection_inherents::filter_invalid_candidates::CandidateValidationApi<Block> for Runtime {
		fn validate_registered_candidate_data(stake_pool_pub_key: &StakePoolPublicKey,registration_data: &RegistrationData) -> Option<RegistrationDataError> {
			authority_selection_inherents::filter_invalid_candidates::validate_registration_data(stake_pool_pub_key, registration_data, Sidechain::genesis_utxo()).err()
		}
		fn validate_stake(stake: Option<StakeDelegation>) -> Option<StakeError> {
			authority_selection_inherents::filter_invalid_candidates::validate_stake(stake).err()
		}
		fn validate_permissioned_candidate_data(candidate: PermissionedCandidateData) -> Option<PermissionedCandidateDataError> {
			validate_permissioned_candidate_data::<CrossChainPublic>(candidate).err()
		}
	}

	impl midnight_primitives_native_token_observation::NativeTokenObservationApi<Block> for Runtime {
		fn get_redemption_validator_address() -> Vec<u8> {
			pallet_native_token_observation::MainChainRedemptionValidatorAddress::<Runtime>::get().into_inner()
		}

		fn get_mapping_validator_address() -> Vec<u8> {
			pallet_native_token_observation::MainChainMappingValidatorAddress::<Runtime>::get().into_inner()
		}

		fn get_next_cardano_position() -> CardanoPosition {
			pallet_native_token_observation::NextCardanoPosition::<Runtime>::get()
		}

		fn get_utxo_capacity_per_block() -> u32 {
			pallet_native_token_observation::CardanoTxCapacityPerBlock::<Runtime>::get()
		}

		fn get_cardano_block_window_size() -> u32 {
			pallet_native_token_observation::CardanoBlockWindowSize::<Runtime>::get()
		}

		fn get_native_token_identifier() -> (Vec<u8>, Vec<u8>) {
			let (policy_id, asset_name) = pallet_native_token_observation::NativeAssetIdentifier::<Runtime>::get();
			(policy_id.into_inner(), asset_name.into_inner())
		}
	}

	impl sp_governed_map::GovernedMapIDPApi<Block> for Runtime {
		fn is_initialized() -> bool {
			GovernedMap::is_initialized()
		}
		fn get_current_state() -> BTreeMap<String, ByteString> {
			GovernedMap::get_all_key_value_pairs_unbounded().collect()
		}
		fn get_main_chain_scripts() -> Option<MainChainScriptsV1> {
			GovernedMap::get_main_chain_scripts()
		}
		fn get_pallet_version() -> u32 {
			GovernedMap::get_version()
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::mock::*;
	use crate::{Midnight, select_authorities_optionally_overriding};
	use authority_selection_inherents::authority_selection_inputs::AuthoritySelectionInputs;
	use authority_selection_inherents::filter_invalid_candidates::RegisterValidatorSignedMessage;
	use frame_support::{
		assert_ok,
		dispatch::PostDispatchInfo,
		inherent::ProvideInherent,
		traits::{UnfilteredDispatchable, WhitelistedStorageKeys},
	};
	use frame_system::RawOrigin;
	use sidechain_domain::{
		CandidateRegistrations, CrossChainPublicKey, CrossChainSignature, DParameter, EpochNonce,
		MainchainSignature, PermissionedCandidateData, RegistrationData, ScEpochNumber,
		SidechainSignature, StakeDelegation, StakePoolPublicKey, UtxoId, UtxoInfo,
	};
	use sp_core::{Pair, ed25519, hexdisplay::HexDisplay};
	use sp_inherents::InherentData;
	use sp_runtime::traits::Zero;
	use std::collections::HashSet;

	#[test]
	fn check_whitelist() {
		let whitelist: HashSet<String> = super::AllPalletsWithSystem::whitelisted_storage_keys()
			.iter()
			.map(|e| HexDisplay::from(&e.key).to_string())
			.collect();

		// Block Number
		assert!(
			whitelist.contains("26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac")
		);
		// Execution Phase
		assert!(
			whitelist.contains("26aa394eea5630e07c48ae0c9558cef7ff553b5a9862a516939d82b3d3d8661a")
		);
		// Event Count
		assert!(
			whitelist.contains("26aa394eea5630e07c48ae0c9558cef70a98fdbe9ce6c55837576c60c7af3850")
		);
		// System Events
		assert!(
			whitelist.contains("26aa394eea5630e07c48ae0c9558cef780d41e5e16056765bc8461851072c9d7")
		);
	}

	// The set committee takes effect next session. Committee can be set for 1 session in advance.
	#[test]
	fn check_grandpa_authorities_rotation() {
		new_test_ext().execute_with(|| {
			// Needs to be run to initialize first slot and epoch numbers;
			advance_block();
			set_committee_through_inherent_data(&[alice()]);
			until_epoch_after_finalizing(1, &|| {
				assert_current_epoch!(0);
				assert_grandpa_weights();
				assert_grandpa_authorities!([alice(), bob()]);
			});

			set_committee_through_inherent_data(&[bob()]);
			for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
				assert_current_epoch!(1);
				assert_grandpa_weights();
				assert_grandpa_authorities!([alice()]);
			});

			for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
				assert_current_epoch!(2);
				assert_grandpa_weights();
				assert_grandpa_authorities!([bob()]);
			});

			// Authorities can be set as late as in the first block of new epoch, but it makes session last 1 block longer
			set_committee_through_inherent_data(&[alice()]);
			advance_block();
			assert_current_epoch!(3);
			assert_grandpa_authorities!([bob()]);
			set_committee_through_inherent_data(&[alice(), bob()]);
			for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH - 1, &|| {
				assert_current_epoch!(3);
				assert_grandpa_weights();
				assert_grandpa_authorities!([alice()]);
			});

			for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH * 3, &|| {
				assert_grandpa_weights();
				assert_grandpa_authorities!([alice(), bob()]);
			});
		});

		fn assert_grandpa_weights() {
			Grandpa::grandpa_authorities()
				.into_iter()
				.for_each(|(_, weight)| assert_eq!(weight, 1))
		}
	}

	// The set committee takes effect next session. Committee can be set for 1 session in advance.
	#[test]
	fn check_aura_authorities_rotation() {
		new_test_ext().execute_with(|| {
			advance_block();
			set_committee_through_inherent_data(&[alice()]);
			until_epoch(1, &|| {
				assert_current_epoch!(0);
				assert_aura_authorities!([alice(), bob()]);
			});

			for_next_n_blocks(SLOTS_PER_EPOCH, &|| {
				assert_current_epoch!(1);
				assert_aura_authorities!([alice()]);
			});

			// Authorities can be set as late as in the first block of new epoch, but it makes session last 1 block longer
			set_committee_through_inherent_data(&[bob()]);
			assert_current_epoch!(2);
			assert_aura_authorities!([alice()]);
			advance_block();
			set_committee_through_inherent_data(&[alice(), bob()]);
			for_next_n_blocks(SLOTS_PER_EPOCH - 1, &|| {
				assert_current_epoch!(2);
				assert_aura_authorities!([bob()]);
			});

			set_committee_through_inherent_data(&[alice(), bob()]);
			for_next_n_blocks(SLOTS_PER_EPOCH * 3, &|| {
				assert_aura_authorities!([alice(), bob()]);
			});
		});
	}

	// The set committee takes effect at next session. Committee can be set for 1 session in advance.
	#[test]
	fn check_cross_chain_committee_rotation() {
		new_test_ext().execute_with(|| {
			advance_block();
			set_committee_through_inherent_data(&[alice()]);
			until_epoch(1, &|| {
				assert_current_epoch!(0);
				assert_next_committee!([alice()]);
			});

			set_committee_through_inherent_data(&[bob()]);
			for_next_n_blocks(SLOTS_PER_EPOCH, &|| {
				assert_current_epoch!(1);
				assert_next_committee!([bob()]);
			});

			set_committee_through_inherent_data(&[]);
			for_next_n_blocks(SLOTS_PER_EPOCH, &|| {
				assert_current_epoch!(2);
				assert_next_committee!([bob()]);
			});
		});
	}

	#[test]
	// The effects of setting the d parameter are already well-tested, so we will not check that. We will check the selection to ensure that it simply respects d-parameter overriding
	fn check_overridden_d_param_committee_rotation() {
		new_test_ext().execute_with(|| {
			let permissioned_validators = vec![alice_mock_validator(), bob_mock_validator()];
			let registered_validators = vec![charlie_mock_validator()];

			let d_parameter =
				DParameter { num_permissioned_candidates: 1, num_registered_candidates: 0 };

			let authority_selection_inputs = create_authority_selection_inputs(
				&permissioned_validators,
				&registered_validators,
				d_parameter,
			);

			let initially_selected_authorities = select_authorities_optionally_overriding(
				authority_selection_inputs.clone(),
				ScEpochNumber::zero(),
			);

			assert_eq!(initially_selected_authorities.unwrap().len(), 1);

			// Override the committee manually
			assert_ok!(Midnight::override_d_parameter(RawOrigin::Root.into(), Some((20, 2))));

			let selected_authorities_override = select_authorities_optionally_overriding(
				authority_selection_inputs,
				ScEpochNumber::zero(),
			);

			assert_eq!(selected_authorities_override.unwrap().len(), 22);
		})
	}

	pub fn set_committee_through_inherent_data(
		expected_authorities: &[TestKeys],
	) -> PostDispatchInfo {
		let epoch = Sidechain::current_epoch_number();
		let slot = *pallet_aura::CurrentSlot::<Test>::get();
		println!(
			"(slot {slot}, epoch {epoch}) Setting {} authorities for next epoch",
			expected_authorities.len()
		);
		let inherent_data_struct = create_inherent_data_struct(expected_authorities);
		let mut inherent_data = InherentData::new();
		inherent_data
			.put_data(
				SessionCommitteeManagement::INHERENT_IDENTIFIER,
				&inherent_data_struct.data.unwrap(),
			)
			.expect("Setting inherent data should not fail");
		let call = <SessionCommitteeManagement as ProvideInherent>::create_inherent(&inherent_data)
			.expect("Creating test inherent should not fail");
		println!("    inherent: {call:?}");
		call.dispatch_bypass_filter(RuntimeOrigin::none())
			.expect("dispatching test call should work")
	}

	pub fn create_authority_selection_inputs(
		permissioned_candidates: &[MockValidator],
		validators: &[MockValidator],
		d_parameter: DParameter,
	) -> AuthoritySelectionInputs {
		let epoch_candidates = create_epoch_candidates_idp(validators);

		let permissioned_candidates_data: Vec<PermissionedCandidateData> = permissioned_candidates
			.iter()
			.map(|c| PermissionedCandidateData {
				sidechain_public_key: c.sidechain_pub_key(),
				aura_public_key: c.aura_pub_key(),
				grandpa_public_key: c.grandpa_pub_key(),
			})
			.collect();
		AuthoritySelectionInputs {
			d_parameter,
			permissioned_candidates: permissioned_candidates_data,
			registered_candidates: epoch_candidates,
			epoch_nonce: EpochNonce(DUMMY_EPOCH_NONCE.to_vec()),
		}
	}

	fn create_epoch_candidates_idp(validators: &[MockValidator]) -> Vec<CandidateRegistrations> {
		let mainchain_key_pair: ed25519::Pair = ed25519::Pair::from_seed_slice(&[7u8; 32]).unwrap();

		let candidates: Vec<CandidateRegistrations> = validators
			.iter()
			.map(|validator| {
				let signed_message = RegisterValidatorSignedMessage {
					genesis_utxo: UtxoId::default(),
					sidechain_pub_key: validator.sidechain_pub_key().0,
					registration_utxo: UtxoId::default(),
				};

				let mock_mainchain_signature = mainchain_key_pair.sign(&[]);

				let sidechain_signature_bytes_no_recovery =
					mock_mainchain_signature.0[..64].to_vec();

				let registration_data = RegistrationData {
					registration_utxo: signed_message.registration_utxo,
					sidechain_signature: SidechainSignature(
						sidechain_signature_bytes_no_recovery.clone(),
					),
					mainchain_signature: MainchainSignature(mock_mainchain_signature.0),
					cross_chain_signature: CrossChainSignature(
						sidechain_signature_bytes_no_recovery,
					),
					sidechain_pub_key: validator.sidechain_pub_key(),
					aura_pub_key: validator.aura_pub_key(),
					grandpa_pub_key: validator.grandpa_pub_key(),
					cross_chain_pub_key: CrossChainPublicKey(validator.sidechain_pub_key().0),
					utxo_info: UtxoInfo::default(),
					tx_inputs: vec![signed_message.registration_utxo],
				};

				CandidateRegistrations {
					registrations: vec![registration_data],
					stake_delegation: Some(StakeDelegation(validator.stake)),
					stake_pool_public_key: StakePoolPublicKey(mainchain_key_pair.public().0),
				}
			})
			.collect();

		candidates
	}
}
