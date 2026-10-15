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

#![cfg_attr(not(feature = "std"), no_std)]
// `frame_support::runtime` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]
// Needed for GetSidechainStatus (used inside of a macro, so can't apply directly)
#![allow(deprecated)]

#[cfg(feature = "runtime-benchmarks")]
#[macro_use]
extern crate frame_benchmarking;

extern crate alloc;
use alloc::string::String;
use authority_selection_inherents::{
	AuthoritySelectionInputs, CommitteeMember, PermissionedCandidateDataError,
	RegistrationDataError, StakeError, select_authorities, validate_permissioned_candidate_data,
};

pub use frame_support::{
	BoundedVec, PalletId, StorageValue,
	genesis_builder_helper::{build_state, get_preset},
	pallet_prelude::DispatchResult,
	parameter_types, storage,
	traits::{
		ConstBool, ConstU8, ConstU32, ConstU64, ConstU128, Contains, EitherOfDiverse,
		EqualPrivilegeOnly, InsideBoth, KeyOwnerProofSystem, NeverEnsureOrigin, Nothing,
		Randomness, StorageInfo,
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
use frame_system::{EnsureNone, EnsureRoot, EnsureRootWithSuccess};
use midnight_node_ledger::types::{GasCost, Tx, active_version::LedgerApiError};
use midnight_primitives::BridgeRecipient;
use midnight_primitives_beefy::BeefyStakes;
use midnight_primitives_cnight_observation::CardanoPosition;
use opaque::{CrossChainKey, SessionKeys};
pub use pallet_cnight_observation::Call as CNightObservationCall;
use pallet_grandpa::AuthorityId as GrandpaId;
pub use pallet_midnight::{TransactionTypeV2, pallet::Call as MidnightCall};
pub use pallet_midnight_system::Call as MidnightSystemCall;
pub use pallet_session_validator_management::{self, Config};
pub use pallet_timestamp::Call as TimestampCall;
pub use pallet_version::VERSION_ID;
use parity_scale_codec::Encode;
use sidechain_domain::{
	DParameter, MainchainAddress, PermissionedCandidateData, PolicyId, RegistrationData,
	ScEpochNumber, ScSlotNumber, StakeDelegation, StakePoolPublicKey, UtxoId,
};
use sp_api::impl_runtime_apis;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_beefy::{
	OpaqueKeyOwnershipProof,
	ecdsa_crypto::{AuthorityId as BeefyId, Signature as BeefySignature},
	mmr::{BeefyAuthoritySet, BeefyNextAuthoritySet, MmrLeafVersion},
};
use sp_core::{ByteArray, OpaqueMetadata, crypto::KeyTypeId};
use sp_partner_chains_bridge::{BridgeDataCheckpoint, MainChainScripts as BridgeMainChainScripts};
#[cfg(feature = "runtime-benchmarks")]
use sp_partner_chains_bridge::{BridgeTransferV1, TransferRecipient};
use sp_runtime::SaturatedConversion;
use sp_runtime::traits::StaticLookup;

#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;
use sp_runtime::traits::{Convert, ConvertInto, Keccak256};
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
use crate::{constants::time_units::HOURS, currency::CurrencyWaiver};
use alloc::{vec, vec::Vec};
use sp_version::RuntimeVersion;

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

#[cfg(test)]
mod mock;

/// Number of slots per partner-chain epoch: 300 slots of 6-second blocks give 30-minute
/// epochs. The epoch length must divide 24h evenly.
pub const SLOTS_PER_EPOCH: u32 = 300;

pub mod beefy;
pub mod check_call_filter;
mod constants;
mod currency;
/// Fork-transition support. Off by default; see the module docs before
/// enabling -- a runtime built with this feature must never be released.
#[cfg(feature = "fork-transition")]
pub mod fork_transition;
mod migrations;
pub mod weights;

use check_call_filter::CheckCallFilter;
use constants::time_units::DAYS;
use pallet_federated_authority::{
	AuthorityBody, FederatedAuthorityEnsureProportionAtLeast, FederatedAuthorityOriginManager,
};
#[cfg(not(feature = "runtime-benchmarks"))]
use runtime_common::governance::AlwaysNo;
use runtime_common::governance::{MembershipHandler, MembershipObservationHandler};

use crate::beefy::{
	compute_current_authority_set, compute_next_authority_set, current_beefy_stakes,
	next_beefy_stakes,
};

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
	use authority_selection_inherents::MaybeFromCandidateKeys;
	use parity_scale_codec::MaxEncodedLen;
	use sp_core::{ecdsa, ed25519, sr25519};
	pub use sp_runtime::OpaqueExtrinsic as UncheckedExtrinsic;
	use sp_runtime::key_types::{AURA, BABE, BEEFY, GRANDPA};

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
		use alloc::vec::Vec;
		use parity_scale_codec::MaxEncodedLen;
		use sp_core::crypto::AccountId32;
		use sp_runtime::MultiSigner;
		use sp_runtime::app_crypto::{app_crypto, ecdsa};
		use sp_runtime::traits::IdentifyAccount;

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
	}

	impl_opaque_keys! {
		#[derive(MaxEncodedLen, PartialOrd, Ord)]
		pub struct SessionKeys {
			pub aura: Aura,
			pub grandpa: Grandpa,
			pub babe: Babe,
			pub beefy: Beefy,
		}
	}

	impl MaybeFromCandidateKeys for SessionKeys {
		fn maybe_from(keys: &sidechain_domain::CandidateKeys) -> Option<Self> {
			let aura = keys.find(AURA)?;
			let aura = sr25519::Public::from_raw(aura.try_into().ok()?);
			let grandpa = keys.find(GRANDPA)?;
			let grandpa = ed25519::Public::from_raw(grandpa.try_into().ok()?);
			let babe = keys.find(BABE)?;
			let babe = sr25519::Public::from_raw(babe.try_into().ok()?);
			let beefy = keys.find(BEEFY)?;
			let beefy = ecdsa::Public::from_raw(beefy.try_into().ok()?);
			Some(Self {
				aura: aura.into(),
				grandpa: grandpa.into(),
				babe: babe.into(),
				beefy: beefy.into(),
			})
		}
	}

	impl From<SessionKeys> for sidechain_domain::CandidateKeys {
		fn from(value: SessionKeys) -> Self {
			Self(vec![
				sidechain_domain::CandidateKey::new(
					AURA,
					value.aura.into_inner().to_raw().to_vec(),
				),
				sidechain_domain::CandidateKey::new(
					BABE,
					value.babe.into_inner().to_raw().to_vec(),
				),
				sidechain_domain::CandidateKey::new(
					GRANDPA,
					value.grandpa.into_inner().to_raw().to_vec(),
				),
				sidechain_domain::CandidateKey::new(BEEFY, value.beefy.into_inner().to_raw_vec()),
			])
		}
	}

	impl_opaque_keys! {
		pub struct CrossChainKey {
			pub account: CrossChainPublic,
		}
	}

	impl MaybeFromCandidateKeys for CrossChainKey {
		fn maybe_from(keys: &sidechain_domain::CandidateKeys) -> Option<Self> {
			let key = keys.find(CROSS_CHAIN)?;
			let account = CrossChainPublic::try_from(key.as_slice()).ok()?;
			Some(Self { account })
		}
	}
}

pub type CrossChainPublic = opaque::cross_chain_app::Public;

/// The `impl_name` of a runtime built with `fork-transition`, so one is
/// identifiable from `state_getRuntimeVersion` or from its wasm: release
/// builds check for this string (`scripts/assert-no-fork-transition.sh`).
pub const FORK_TRANSITION_IMPL_NAME: &str = "midnight-fork-transition-UNSAFE";

// `#[sp_version::runtime_version]` only accepts string literals, so `impl_name`
// is chosen per feature by expanding the one definition twice rather than by a
// const. `spec_version` must stay single-sourced: `codeSubstitutes` matches on
// it, so a fork runtime that drifted from the stock one would silently not apply.
macro_rules! runtime_version {
	($impl_name:tt) => {
		// To learn more about runtime versioning, see:
		// https://docs.substrate.io/main-docs/build/upgrade#runtime-versioning
		#[allow(clippy::zero_prefixed_literal)]
		#[sp_version::runtime_version]
		pub const VERSION: RuntimeVersion = RuntimeVersion {
			spec_name: Cow::Borrowed("midnight"),
			impl_name: Cow::Borrowed($impl_name),
			authoring_version: 1,
			// The version of the runtime specification. A full node will not attempt to use its native
			//   runtime in substitute for the on-chain Wasm runtime unless all of `spec_name`,
			//   `spec_version`, and `authoring_version` are the same between Wasm and native.
			spec_version: 003_000_000,
			impl_version: 0,
			apis: RUNTIME_API_VERSIONS,
			transaction_version: 4,
			system_version: 3,
		};
	};
}

#[cfg(not(feature = "fork-transition"))]
runtime_version!("midnight");
#[cfg(feature = "fork-transition")]
runtime_version!("midnight-fork-transition-UNSAFE");

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

pub const BABE_GENESIS_EPOCH_CONFIG: sp_consensus_babe::BabeEpochConfiguration =
	sp_consensus_babe::BabeEpochConfiguration {
		c: (1, 4),
		allowed_slots: sp_consensus_babe::AllowedSlots::PrimaryAndSecondaryVRFSlots,
	};

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
		::max_with_normal_ratio(1024 * 1024, NORMAL_DISPATCH_RATIO);
	pub const SS58Prefix: u8 = 42;
}

// Configure FRAME pallets to include in runtime.

impl frame_system::Config for Runtime {
	/// The basic call filter to use in dispatchable.
	type BaseCallFilter = InsideBoth<SafeMode, TxPause>;
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
	/// Converts a module to the index of the module in `runtime!`.
	///
	/// This type is being generated by `runtime!`.
	type PalletInfo = PalletInfo;
	/// What to do if a new account is created.
	type OnNewAccount = ();
	/// What to do if an account is fully reaped from the system.
	type OnKilledAccount = ();
	/// The data to be stored in an account.
	type AccountData = ();
	/// Weight information for the extrinsics of this pallet.
	type SystemWeightInfo = weights::frame_system::WeightInfo<Runtime>;
	/// This is used as an identifier of the chain. 42 is the generic substrate prefix.
	type SS58Prefix = SS58Prefix;
	/// The set code logic, just the default since we're not a parachain.
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
	type RuntimeTask = RuntimeTask;
	type SingleBlockMigrations = (
		// Initializes QueuedCommittee (v1 -> v2), adds BABE and BEEFY keys, and activates the
		// consensus-engine pallet (pre-seeds pallet-babe's GenesisSlot before its
		// `on_initialize` sees the first BABE pre-digest).
		crate::migrations::authority_keys::MigrateV1ToV2AddBabeAndBeefySessionKeys,
	);
	type MultiBlockMigrator = MultiBlockMigrations;
	type PreInherents = ();
	type PostInherents = ();
	type PostTransactions = ();
	type ExtensionsWeightInfo = weights::frame_system_extensions::WeightInfo<Runtime>;
}

impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
	type MaxAuthorities = MaxAuthorities;
	type AllowMultipleBlocksPerSlot = ConstBool<false>;
	type SlotDuration = ConstU64<SLOT_DURATION>;
}

impl pallet_babe::Config for Runtime {
	type EpochDuration = SidechainEpochDuration;
	type ExpectedBlockTime = ConstU64<SLOT_DURATION>;
	type EpochChangeTrigger = pallet_babe::ExternalTrigger;
	type DisabledValidators = ();
	// Default weights implemented by `()` are the best thing we have for BABE.
	// Pallet does not define benchmarks for methods in the `WeightInfo`, so
	// running benchmarks does not add any value.
	type WeightInfo = ();
	type MaxAuthorities = MaxAuthorities;
	type MaxNominators = ConstU32<5>;
	// Equivocation reporting is disabled, matching GRANDPA/BEEFY.
	type KeyOwnerProof = sp_core::Void;
	type EquivocationReportSystem = ();
}

/// BABE uses epoch lenght defined by pallet sidechain
pub struct SidechainEpochDuration;

impl Get<u64> for SidechainEpochDuration {
	fn get() -> u64 {
		Sidechain::slots_per_epoch().0.into()
	}
}

impl pallet_session::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type ValidatorIdOf = ConvertInto;
	type ShouldEndSession = SessionCommitteeManagement;
	type NextSessionRotation = ();
	// The wrapper records each session's validator trie root, which key-ownership proofs are
	// checked against; the inner manager is what rotates committees.
	type SessionManager =
		pallet_session::historical::NoteHistoricalRoot<Self, SessionCommitteeManagement>;
	type SessionHandler = <opaque::SessionKeys as OpaqueKeys>::KeyTypeIdProviders;
	type Keys = opaque::SessionKeys;
	type DisablingStrategy = pallet_session::disabling::UpToLimitWithReEnablingDisablingStrategy;
	type WeightInfo = pallet_session::weights::SubstrateWeight<Runtime>;
	type Currency = CurrencyWaiver;
	type KeyDeposit = ();
}

pub struct FullIdentificationOf;
impl sp_runtime::traits::Convert<AccountId, Option<()>> for FullIdentificationOf {
	fn convert(_: AccountId) -> Option<()> {
		Some(())
	}
}

impl pallet_session::historical::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type FullIdentification = ();
	type FullIdentificationOf = FullIdentificationOf;
}

impl pallet_grandpa::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;

	type WeightInfo = weights::pallet_grandpa::WeightInfo<Runtime>;
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
	type WeightInfo = weights::pallet_mmr::WeightInfo<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

/// MMR helper types.
mod mmr {
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
	type LeafExtra = Vec<u8>;
	type BeefyDataProvider = ();
	type WeightInfo = weights::pallet_beefy_mmr::WeightInfo<Runtime>;
}

impl pallet_timestamp::Config for Runtime {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = u64;
	type OnTimestampSet = ConsensusEngine;
	type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
	type WeightInfo = weights::pallet_timestamp::WeightInfo<Runtime>;
}

/// Existential deposit.
pub const EXISTENTIAL_DEPOSIT: u128 = 500;

parameter_types! {
	pub MbmServiceWeight: Weight = Perbill::from_percent(80) * BlockWeights::get().max_block;
}

impl pallet_migrations::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	#[cfg(not(feature = "runtime-benchmarks"))]
	// Append-only: `ActiveCursor.index` indexes this tuple.
	type Migrations = (
		pallet_cnight_observation::migrations::v1::MigrateV0ToV1<Runtime>,
		pallet_cnight_observation::migrations::v2::MigrateV1ToV2<Runtime>,
	);
	// Benchmarks need mocked migrations to guarantee that they succeed.
	#[cfg(feature = "runtime-benchmarks")]
	type Migrations = pallet_migrations::mock_helpers::MockedMigrations;
	type CursorMaxLen = ConstU32<65_536>;
	type IdentifierMaxLen = ConstU32<256>;
	type MigrationStatusHandler = ();
	type FailedMigrationHandler = migrations::EnterSafeModeAndUnstuckOnFailedMigration;
	type MaxServiceWeight = MbmServiceWeight;
	type WeightInfo = weights::pallet_migrations::WeightInfo<Runtime>;
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
	type WeightInfo = weights::pallet_scheduler::WeightInfo<Runtime>;
	type OriginPrivilegeCmp = EqualPrivilegeOnly;
	type Preimages = Preimage;
	type BlockNumberProvider = frame_system::Pallet<Runtime>;
}

parameter_types! {
	pub const MaxAuthorities: u32 = 10_000;
}

/// Select the next authorities using the D-parameter from the system-parameters pallet
fn select_authorities_optionally_overriding(
	mut input: AuthoritySelectionInputs,
	sidechain_epoch: ScEpochNumber,
) -> Option<BoundedVec<CommitteeMember<CrossChainPublic, SessionKeys>, MaxAuthorities>> {
	let d_parameter = SystemParameters::get_d_parameter();
	input.d_parameter.num_permissioned_candidates = d_parameter.num_permissioned_candidates;
	input.d_parameter.num_registered_candidates = d_parameter.num_registered_candidates;
	log_if_d_param_below_permissioned_candidates(&d_parameter, &input.permissioned_candidates);
	select_authorities(Sidechain::genesis_utxo(), input, sidechain_epoch)
}

/// Log an error when the D-parameter's permissioned slots are fewer than the available
/// permissioned candidates. In a federated network this means that no candidate has a
/// guaranteed committee seat, which risks liveness if any node is repeatedly selected.
/// See <https://github.com/midnightntwrk/midnight-node/issues/1481>.
pub fn log_if_d_param_below_permissioned_candidates(
	d_parameter: &DParameter,
	permissioned_candidates: &[PermissionedCandidateData],
) {
	let d = d_parameter.num_permissioned_candidates as usize;
	let p = permissioned_candidates.len();
	if d < p {
		log::error!(
			"D-parameter num_permissioned_candidates ({d}) is less than the number of available \
			 permissioned candidates ({p}). With D_P < n_P, candidates do not have guaranteed \
			 committee seats, risking liveness in a federated network. \
			 See https://github.com/midnightntwrk/midnight-node/issues/1481"
		);
	}
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
		select_authorities_optionally_overriding(input, sidechain_epoch)
	}

	fn current_epoch_number() -> ScEpochNumber {
		Sidechain::current_epoch_number()
	}

	type WeightInfo = weights::pallet_session_validator_management::WeightInfo<Runtime>;

	type CommitteeMember = CommitteeMember<CrossChainPublic, SessionKeys>;

	type MainChainScriptsOrigin = EnsureRoot<Self::AccountId>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

pub struct LogBeneficiaries;
impl sp_sidechain::OnNewEpoch for LogBeneficiaries {
	fn on_new_epoch(_old_epoch: ScEpochNumber, _new_epoch: ScEpochNumber) -> Weight {
		Weight::zero()
	}
}

impl pallet_sidechain::Config for Runtime {
	fn current_slot_number() -> ScSlotNumber {
		// Single source of truth: active engine `CurrentSlot`. Babe (3) and Aura (2)
		// both run before Sidechain (4), so storage is already updated for this block.
		ScSlotNumber(*ConsensusEngine::current_slot())
	}
	type OnNewEpoch = LogBeneficiaries;
}

pub const BLOCK_REWARD_POINTS: u128 = 500_000;

pub type BeneficiaryId = midnight_node_ledger::types::Hash;
pub type BlockRewardPoints = u128;
pub type BlockReward = (BlockRewardPoints, Option<BeneficiaryId>);

pub struct LedgerBlockReward;
impl Get<BlockReward> for LedgerBlockReward {
	fn get() -> BlockReward {
		(0, None)
	}
}

/// Configure the pallet-midnight in pallets/midnight.
impl pallet_midnight::Config for Runtime {
	type BlockReward = LedgerBlockReward;
	type SlotDuration = ConstU64<SLOT_DURATION>;
}

/// Configure the pallet-midnight in pallets/midnight.
impl pallet_midnight_system::Config for Runtime {
	type LedgerStateProviderMut = Midnight;
	type LedgerBlockContextProvider = Midnight;
}

/// Configure the pallet-upgrade in pallets/upgrade.
impl pallet_version::Config for Runtime {
	type WeightInfo = weights::pallet_version::WeightInfo<Runtime>;
	type RuntimeVersion = Version;
}

impl pallet_preimage::Config for Runtime {
	type WeightInfo = weights::pallet_preimage::WeightInfo<Runtime>;
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
	type WeightInfo = weights::pallet_tx_pause::WeightInfo<Runtime>;
}

parameter_types! {
	/// Nominal durations for the permissionless `enter`/`extend` calls. Inert in practice:
	/// `EnterDepositAmount`/`ExtendDepositAmount` are `None`, which disables permissionless
	/// entry entirely.
	pub const SafeModeEnterDuration: BlockNumber = DAYS;
	pub const SafeModeExtendDuration: BlockNumber = DAYS;
	/// How long a single Root `force_enter`/`force_extend` lasts.
	pub const SafeModeForceDuration: BlockNumber = 7 * DAYS;
}

impl pallet_safe_mode::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	// Deposits are disabled (`EnterDepositAmount`/`ExtendDepositAmount` = None), so the
	// Currency is typecheck-only.
	type Currency = CurrencyWaiver;
	type RuntimeHoldReason = RuntimeHoldReason;
	// While safe mode is entered only these calls (plus safe-mode's own, auto-exempted by
	// pallet name) pass `BaseCallFilter`: the governance recovery path (including the
	// motion calls the collectives dispatch internally) and the inherents block production
	// depends on. Root bypasses the filter entirely.
	type WhitelistedCalls = (
		check_call_filter::GovernanceAuthorityCallFilter,
		check_call_filter::InherentCalls,
		check_call_filter::FederatedMotionCalls,
	);
	type EnterDuration = SafeModeEnterDuration;
	type ExtendDuration = SafeModeExtendDuration;
	type EnterDepositAmount = ();
	type ExtendDepositAmount = ();
	type ForceEnterOrigin = EnsureRootWithSuccess<AccountId, SafeModeForceDuration>;
	type ForceExtendOrigin = EnsureRootWithSuccess<AccountId, SafeModeForceDuration>;
	type ForceExitOrigin = EnsureRoot<AccountId>;
	type ForceDepositOrigin = EnsureRoot<AccountId>;
	type Notify = ();
	type ReleaseDelay = ();
	type WeightInfo = weights::pallet_safe_mode::WeightInfo<Runtime>;
}

pub const MOTION_DURATION: BlockNumber = 5 * DAYS;
pub const MAX_PROPOSALS: u32 = 100;
pub const MAX_MEMBERS: u32 = 10;

parameter_types! {
	pub const MotionDuration: BlockNumber = MOTION_DURATION;
	pub MaxProposalWeight: Weight = Perbill::from_percent(50) * BlockWeights::get().max_block;
}

/// Council
type CouncilCollectiveInstance = pallet_collective::Instance1;
impl pallet_collective::Config<CouncilCollectiveInstance> for Runtime {
	type RuntimeOrigin = RuntimeOrigin;
	type Proposal = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type MotionDuration = MotionDuration;
	type MaxProposals = ConstU32<MAX_PROPOSALS>;
	type MaxMembers = ConstU32<MAX_MEMBERS>; // Should be same as `pallet_membership`
	#[cfg(not(feature = "runtime-benchmarks"))]
	type DefaultVote = AlwaysNo;
	#[cfg(feature = "runtime-benchmarks")]
	type DefaultVote = pallet_collective::PrimeDefaultVote;
	// Production: managed from `pallet_membership`. Benchmarks need an origin
	// whose `try_successful_origin` succeeds so setup helpers can install members.
	#[cfg(not(feature = "runtime-benchmarks"))]
	type SetMembersOrigin = NeverEnsureOrigin<()>;
	#[cfg(feature = "runtime-benchmarks")]
	type SetMembersOrigin = EnsureRoot<Self::AccountId>;
	type MaxProposalWeight = MaxProposalWeight;
	type DisapproveOrigin = EnsureRoot<Self::AccountId>;
	type KillOrigin = EnsureRoot<Self::AccountId>;
	type Consideration = ();
	type WeightInfo = weights::pallet_collective::WeightInfo<Runtime>;
}

type CouncilMembershipInstance = pallet_membership::Instance1;
impl pallet_membership::Config<CouncilMembershipInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	// Production: members managed only by `ResetOrigin`. Benchmarks need successful
	// origins so the upstream `set_members`/`set_prime` helpers don't panic.
	#[cfg(not(feature = "runtime-benchmarks"))]
	type AddOrigin = NeverEnsureOrigin<()>;
	#[cfg(feature = "runtime-benchmarks")]
	type AddOrigin = EnsureRoot<Self::AccountId>;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type RemoveOrigin = NeverEnsureOrigin<()>;
	#[cfg(feature = "runtime-benchmarks")]
	type RemoveOrigin = EnsureRoot<Self::AccountId>;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type SwapOrigin = NeverEnsureOrigin<()>;
	#[cfg(feature = "runtime-benchmarks")]
	type SwapOrigin = EnsureRoot<Self::AccountId>;
	type ResetOrigin = EnsureNone<Self::AccountId>; // To be called by an Inherent with `RawOrigin::None`
	#[cfg(not(feature = "runtime-benchmarks"))]
	type PrimeOrigin = NeverEnsureOrigin<()>;
	#[cfg(feature = "runtime-benchmarks")]
	type PrimeOrigin = EnsureRoot<Self::AccountId>;
	type MembershipInitialized = MembershipHandler<Runtime, Council>;
	type MembershipChanged = MembershipHandler<Runtime, Council>;
	type MaxMembers = ConstU32<MAX_MEMBERS>;
	type WeightInfo = weights::pallet_membership::WeightInfo<Runtime>;
}

/// Technical Committee
type TechnicalCommitteeCollectiveInstance = pallet_collective::Instance2;
impl pallet_collective::Config<TechnicalCommitteeCollectiveInstance> for Runtime {
	type RuntimeOrigin = RuntimeOrigin;
	type Proposal = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type MotionDuration = MotionDuration;
	type MaxProposals = ConstU32<MAX_PROPOSALS>;
	type MaxMembers = ConstU32<MAX_MEMBERS>; // Should be same as `pallet_membership`
	#[cfg(not(feature = "runtime-benchmarks"))]
	type DefaultVote = AlwaysNo;
	#[cfg(feature = "runtime-benchmarks")]
	type DefaultVote = pallet_collective::PrimeDefaultVote;
	// See Council instance above for rationale.
	#[cfg(not(feature = "runtime-benchmarks"))]
	type SetMembersOrigin = NeverEnsureOrigin<()>;
	#[cfg(feature = "runtime-benchmarks")]
	type SetMembersOrigin = EnsureRoot<Self::AccountId>;
	type MaxProposalWeight = MaxProposalWeight;
	type DisapproveOrigin = EnsureRoot<Self::AccountId>;
	type KillOrigin = EnsureRoot<Self::AccountId>;
	type Consideration = ();
	type WeightInfo = weights::pallet_collective::WeightInfo<Runtime>;
}

type TechnicalCommitteeMembershipInstance = pallet_membership::Instance2;
impl pallet_membership::Config<TechnicalCommitteeMembershipInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	// See CouncilMembership instance above for rationale.
	#[cfg(not(feature = "runtime-benchmarks"))]
	type AddOrigin = NeverEnsureOrigin<()>;
	#[cfg(feature = "runtime-benchmarks")]
	type AddOrigin = EnsureRoot<Self::AccountId>;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type RemoveOrigin = NeverEnsureOrigin<()>;
	#[cfg(feature = "runtime-benchmarks")]
	type RemoveOrigin = EnsureRoot<Self::AccountId>;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type SwapOrigin = NeverEnsureOrigin<()>;
	#[cfg(feature = "runtime-benchmarks")]
	type SwapOrigin = EnsureRoot<Self::AccountId>;
	type ResetOrigin = EnsureNone<Self::AccountId>; // To be called by an Inherent with `RawOrigin::None`
	#[cfg(not(feature = "runtime-benchmarks"))]
	type PrimeOrigin = NeverEnsureOrigin<()>;
	#[cfg(feature = "runtime-benchmarks")]
	type PrimeOrigin = EnsureRoot<Self::AccountId>;
	type MembershipInitialized = MembershipHandler<Runtime, TechnicalCommittee>;
	type MembershipChanged = MembershipHandler<Runtime, TechnicalCommittee>;
	type MaxMembers = ConstU32<MAX_MEMBERS>;
	type WeightInfo = weights::pallet_membership::WeightInfo<Runtime>;
}

pub const MAX_NUM_BODIES: u32 = 2; // TechnicalCommittee + Council
pub const MAX_MOTIONS_PER_BLOCK: u32 = 10;

type CouncilApproval = AuthorityBody<
	Council,
	pallet_collective::EnsureProportionAtLeast<AccountId, CouncilCollectiveInstance, 2, 3>,
>;
type TechnicalCommitteeApproval = AuthorityBody<
	TechnicalCommittee,
	pallet_collective::EnsureProportionAtLeast<
		AccountId,
		TechnicalCommitteeCollectiveInstance,
		2,
		3,
	>,
>;

type CouncilRevoke = AuthorityBody<
	Council,
	pallet_collective::EnsureProportionAtLeast<AccountId, CouncilCollectiveInstance, 2, 3>,
>;
type TechnicalCommitteeRevoke = AuthorityBody<
	TechnicalCommittee,
	pallet_collective::EnsureProportionAtLeast<
		AccountId,
		TechnicalCommitteeCollectiveInstance,
		2,
		3,
	>,
>;

impl pallet_federated_authority::Config for Runtime {
	type MotionCall = RuntimeCall;
	type MaxAuthorityBodies = ConstU32<MAX_NUM_BODIES>;
	type MotionDuration = ConstU32<MOTION_DURATION>;
	type MotionApprovalProportion = FederatedAuthorityEnsureProportionAtLeast<1, 1>;
	type MotionApprovalOrigin =
		FederatedAuthorityOriginManager<(CouncilApproval, TechnicalCommitteeApproval)>;
	type MotionRevokeOrigin =
		FederatedAuthorityOriginManager<(CouncilRevoke, TechnicalCommitteeRevoke)>;
	type WeightInfo = weights::pallet_federated_authority::WeightInfo<Runtime>;
}

impl pallet_federated_authority_observation::Config for Runtime {
	type CouncilMaxMembers = ConstU32<MAX_MEMBERS>; // Should be same as its `pallet_membership` instance
	type TechnicalCommitteeMaxMembers = ConstU32<MAX_MEMBERS>; // Should be same as its `pallet_membership` instance
	type CouncilMembershipHandler =
		MembershipObservationHandler<Runtime, CouncilMembershipInstance>;
	type TechnicalCommitteeMembershipHandler =
		MembershipObservationHandler<Runtime, TechnicalCommitteeMembershipInstance>;
	type WeightInfo = weights::pallet_federated_authority_observation::WeightInfo<Runtime>;
}

impl pallet_system_parameters::Config for Runtime {
	type SystemOrigin = EnsureRoot<AccountId>;
	type WeightInfo = weights::pallet_system_parameters::WeightInfo<Runtime>;
}

parameter_types! {
	/// Maximum bytes a single account can submit within a throttle window (10 MB).
	pub const MaxBytes: u64 = 10 * 1024 * 1024;
	/// Maximum transactions a single account can submit within a throttle window
	pub const MaxTxs: u64 = 100;
	/// Number of blocks that define a throttle window (1 hour: 600 blocks at 6 s/block).
	pub const WindowSize: u32 = HOURS;
}

impl pallet_throttle::Config for Runtime {
	type MaxBytes = MaxBytes;
	type MaxTxs = MaxTxs;
	type WindowSize = WindowSize;
}

parameter_types! {
	pub BabeEpochConfigurationValue: sp_consensus_babe::BabeEpochConfiguration =
		BABE_GENESIS_EPOCH_CONFIG;
}

impl pallet_consensus_engine::Config for Runtime {
	// Some state transitions are governance-driven: federated-authority motions dispatch approved
	// calls as root.
	type GovernanceOrigin = EnsureRoot<AccountId>;
	type EpochDuration = SidechainEpochDuration;
	type EpochConfiguration = BabeEpochConfigurationValue;
	// Unit weights for now. Issue #1863.
	type WeightInfo = ();
}

parameter_types! {
	pub const BridgeMaxTransfersPerBlock: u32 = 256;
}

impl pallet_cnight_observation::Config for Runtime {
	type MidnightSystemTransactionExecutor = MidnightSystem;
	type LedgerStateProvider = Midnight;
	type LedgerBlockContextProvider = Midnight;
	type WeightInfo = weights::pallet_cnight_observation::WeightInfo<Runtime>;
}

impl pallet_partner_chains_bridge::Config for Runtime {
	type GovernanceOrigin = EnsureRoot<Self::AccountId>;
	type Recipient = BridgeRecipient;
	type TransferHandler = C2MBridge;
	type MaxTransfersPerBlock = BridgeMaxTransfersPerBlock;
	type WeightInfo = weights::pallet_partner_chains_bridge::WeightInfo<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = MidnightBridgeBenchmarkHelper;
}

/// Registers a `LedgerStorageExt` on the current externalities (via a host fn)
/// so the `C2MBridge` transfer handler can resolve `LedgerApi` calls during benchmark dispatch.
#[cfg(feature = "runtime-benchmarks")]
pub struct MidnightBridgeBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_partner_chains_bridge::benchmarking::BenchmarkHelper<Runtime>
	for MidnightBridgeBenchmarkHelper
{
	fn transfers(
		n: u32,
	) -> BoundedVec<BridgeTransferV1<BridgeRecipient>, BridgeMaxTransfersPerBlock> {
		midnight_node_ledger::types::active_ledger_bridge::register_benchmark_ledger_storage();
		let _ = midnight_node_ledger::types::active_ledger_bridge::ensure_storage_initialized();
		let transfers = (1..=n)
			.map(|i| {
				let bytes = i.to_le_bytes();
				let mut buf = [0u8; 32];
				buf[0..4].copy_from_slice(&bytes[0..4]);

				let mc_tx_hash = sidechain_domain::McTxHash(buf);

				pallet_c2m_bridge::ApprovedMcTxHashes::<Runtime>::insert(mc_tx_hash, ());

				// UserTransfer is the most expensive (and common) case, so it is used for the benchmark
				let recipient = TransferRecipient::Address {
					recipient: BridgeRecipient(BoundedVec::truncate_from(buf.to_vec())),
				};

				BridgeTransferV1 { amount: 1000, mc_tx_hash, recipient }
			})
			.collect();

		BoundedVec::truncate_from(transfers)
	}

	fn data_checkpoint() -> sp_partner_chains_bridge::BridgeDataCheckpoint {
		<() as pallet_partner_chains_bridge::benchmarking::BenchmarkHelper<Runtime>>::data_checkpoint()
	}
}

/// Provider for the minimum bridge transfer amount from the Midnight ledger.
pub struct MidnightMinBridgeAmount;
impl pallet_c2m_bridge::pallet::MinBridgeAmountProvider for MidnightMinBridgeAmount {
	fn get_c_to_m_bridge_min_amount()
	-> Result<u128, midnight_node_ledger::types::active_version::LedgerApiError> {
		Midnight::get_c_to_m_bridge_min_amount()
	}
}

impl pallet_c2m_bridge::Config for Runtime {
	type MidnightSystemTransactionExecutor = MidnightSystem;
	/// Provides access to the ledger's `c_to_m_bridge_min_amount` parameter.
	type MinBridgeAmountProvider = MidnightMinBridgeAmount;
	type GovernanceOrigin = EnsureRoot<Self::AccountId>;
	type WeightInfo = weights::pallet_c2m_bridge::WeightInfo<Runtime>;
}

// Create the runtime by composing the FRAME pallets that were previously configured.
#[frame_support::runtime]
mod runtime {
	use super::*;

	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeSlashReason,
		RuntimeLockId,
		RuntimeTask,
		RuntimeViewFunction
	)]
	pub struct Runtime;

	#[runtime::pallet_index(0)]
	pub type System = frame_system::Pallet<Runtime>;
	#[runtime::pallet_index(1)]
	pub type Timestamp = pallet_timestamp::Pallet<Runtime>;
	#[runtime::pallet_index(2)]
	pub type Aura = pallet_aura::Pallet<Runtime>;
	// BABE immediately after AURA so both engines write `CurrentSlot` before
	// `Sidechain` (4) reads it via `ConsensusEngine::current_slot()`.
	#[runtime::pallet_index(3)]
	pub type Babe = pallet_babe::Pallet<Runtime>;
	#[runtime::pallet_index(4)]
	pub type Sidechain = pallet_sidechain::Pallet<Runtime>;

	// Midnight pallets:
	#[runtime::pallet_index(5)]
	pub type Midnight = pallet_midnight::Pallet<Runtime>;
	#[runtime::pallet_index(6)]
	pub type MidnightSystem = pallet_midnight_system::Pallet<Runtime>;

	#[runtime::pallet_index(7)]
	pub type Grandpa = pallet_grandpa::Pallet<Runtime>;

	#[runtime::pallet_index(8)]
	pub type SessionCommitteeManagement = pallet_session_validator_management::Pallet<Runtime>;

	// Consensus engine transition state machine. Hook order (pallet index order) is
	// load-bearing: its `on_initialize` digest guards must run after Babe (which
	// consumes BABE pre-digests) but before anything that mutates the state they
	// check against — Scheduler (18) can dispatch `schedule_flip` from
	// its own `on_initialize`, and Session (30) rotates `pallet_aura::Authorities`,
	// which the `authority_index == slot % n` transition guard compares with. Both
	// the block author and the AURA seal verifier work from the parent state, so
	// the guards must too.
	#[runtime::pallet_index(10)]
	pub type ConsensusEngine = pallet_consensus_engine::Pallet<Runtime>;

	#[runtime::pallet_index(30)]
	#[runtime::disable_call]
	pub type Session = pallet_session::Pallet<Runtime>;
	#[runtime::pallet_index(31)]
	pub type Historical = pallet_session::historical::Pallet<Runtime>;

	#[runtime::pallet_index(11)]
	pub type NodeVersion = pallet_version::Pallet<Runtime>;

	#[runtime::pallet_index(13)]
	pub type CNightObservation = pallet_cnight_observation::Pallet<Runtime>;

	// Utility
	#[runtime::pallet_index(15)]
	pub type Preimage = pallet_preimage::Pallet<Runtime>;

	#[runtime::pallet_index(16)]
	pub type MultiBlockMigrations = pallet_migrations::Pallet<Runtime>;

	#[runtime::pallet_index(18)]
	pub type Scheduler = pallet_scheduler::Pallet<Runtime>;
	#[runtime::pallet_index(19)]
	pub type TxPause = pallet_tx_pause::Pallet<Runtime>;
	#[runtime::pallet_index(20)]
	pub type SafeMode = pallet_safe_mode::Pallet<Runtime>;

	// BEEFY Bridges support.
	#[runtime::pallet_index(21)]
	pub type Beefy = pallet_beefy::Pallet<Runtime>;
	// MMR leaf construction must be after session in order to have a leaf's next_auth_set
	// refer to block<N>. See issue polkadot-fellows/runtimes#160 for details.
	#[runtime::pallet_index(22)]
	pub type Mmr = pallet_mmr::Pallet<Runtime>;
	#[runtime::pallet_index(23)]
	pub type BeefyMmrLeaf = pallet_beefy_mmr::Pallet<Runtime>;

	#[runtime::pallet_index(32)]
	pub type Bridge = pallet_partner_chains_bridge::Pallet<Runtime>;

	#[runtime::pallet_index(33)]
	pub type C2MBridge = pallet_c2m_bridge::Pallet<Runtime>;

	// Governance
	#[runtime::pallet_index(40)]
	pub type Council = pallet_collective::Pallet<Runtime, Instance1>;
	#[runtime::pallet_index(41)]
	pub type CouncilMembership = pallet_membership::Pallet<Runtime, Instance1>;

	#[runtime::pallet_index(42)]
	pub type TechnicalCommittee = pallet_collective::Pallet<Runtime, Instance2>;
	#[runtime::pallet_index(43)]
	pub type TechnicalCommitteeMembership = pallet_membership::Pallet<Runtime, Instance2>;

	#[runtime::pallet_index(44)]
	pub type FederatedAuthority = pallet_federated_authority::Pallet<Runtime>;
	#[runtime::pallet_index(45)]
	pub type FederatedAuthorityObservation =
		pallet_federated_authority_observation::Pallet<Runtime>;

	// System Parameters
	#[runtime::pallet_index(50)]
	pub type SystemParameters = pallet_system_parameters::Pallet<Runtime>;

	// Throttling
	#[runtime::pallet_index(51)]
	pub type Throttle = pallet_throttle::Pallet<Runtime>;
}

/// The address format for describing accounts.
pub type Address = sp_runtime::MultiAddress<AccountId, ()>;
/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
/// The TransactionExtension to the basic transaction logic.
pub type TxExtension = (
	frame_system::AuthorizeCall<Runtime>,
	frame_system::CheckNonZeroSender<Runtime>,
	frame_system::CheckSpecVersion<Runtime>,
	frame_system::CheckTxVersion<Runtime>,
	frame_system::CheckGenesis<Runtime>,
	frame_system::CheckEra<Runtime>,
	frame_system::CheckNonce<Runtime>,
	frame_system::CheckWeight<Runtime>,
	CheckCallFilter,
	pallet_throttle::CheckThrottle<Runtime>,
	frame_system::WeightReclaim<Runtime>,
);

/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic =
	generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, TxExtension>;
/// The payload being signed in transactions.
pub type SignedPayload = generic::SignedPayload<RuntimeCall, TxExtension>;
/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
	Runtime,
	Block,
	frame_system::ChainContext<Runtime>,
	Runtime,
	AllPalletsWithSystem,
	Migrations,
>;

/// Extrinsic type that has already been checked.
pub type CheckedExtrinsic = generic::CheckedExtrinsic<AccountId, RuntimeCall, TxExtension>;
/// Migrations to apply on runtime upgrade.
pub type Migrations = (
	pallet_throttle::migrations::v1::MigrateV0ToV1<Runtime>,
	// MUST precede the pallet-midnight translation below: it captures the
	// still-untranslated v8 state key, which the cNIGHT dust generation replay
	// (`pallet_cnight_observation::migrations::v2::MigrateV1ToV2`) reads the
	// wiped entries' values and owners from.
	pallet_cnight_observation::migrations::v2::RecordPreForkState<Runtime>,
	// Ledger v8 -> v9 state translation (the ledger 8->9 hardfork). Runs once,
	// when a ledger-8 runtime (pallet-midnight storage version 1) upgrades to
	// this ledger-9 runtime (storage version 2).
	pallet_midnight::migrations::v2::MigrateV1ToV2<Runtime>,
	// Resets the BEEFY genesis block to `None`, disabling BEEFY: the voters have been stuck
	// on the unsigned mandatory block 1 since launch, and the session-key migration above
	// does not unstick it. Governance re-enables BEEFY later with `set_new_genesis`, once a
	// session rotation has made `pallet_beefy::Authorities` track the committee. One-shot:
	// no-op once unset. Remove after it has landed on all live networks and before BEEFY is
	// re-enabled (see `migrations::beefy_genesis`).
	migrations::beefy_genesis::ResetBeefyGenesis,
);

impl<LocalCall> frame_system::offchain::CreateTransaction<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	type Extension = TxExtension;

	fn create_transaction(call: RuntimeCall, extension: TxExtension) -> UncheckedExtrinsic {
		generic::UncheckedExtrinsic::new_transaction(call, extension)
	}
}

impl<LocalCall> frame_system::offchain::CreateBare<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	fn create_bare(call: RuntimeCall) -> UncheckedExtrinsic {
		generic::UncheckedExtrinsic::new_bare(call)
	}
}

impl frame_system::offchain::SigningTypes for Runtime {
	type Public = <Signature as sp_runtime::traits::Verify>::Signer;
	type Signature = Signature;
}

impl<C> frame_system::offchain::CreateTransactionBase<C> for Runtime
where
	RuntimeCall: From<C>,
{
	type Extrinsic = UncheckedExtrinsic;
	type RuntimeCall = RuntimeCall;
}

impl<C> frame_system::offchain::CreateAuthorizedTransaction<C> for Runtime
where
	RuntimeCall: From<C>,
{
	fn create_extension() -> Self::Extension {
		(
			frame_system::AuthorizeCall::<Runtime>::new(),
			frame_system::CheckNonZeroSender::<Runtime>::new(),
			frame_system::CheckSpecVersion::<Runtime>::new(),
			frame_system::CheckTxVersion::<Runtime>::new(),
			frame_system::CheckGenesis::<Runtime>::new(),
			frame_system::CheckEra::<Runtime>::from(generic::Era::Immortal),
			frame_system::CheckNonce::<Runtime>::from(0),
			frame_system::CheckWeight::<Runtime>::new(),
			CheckCallFilter,
			pallet_throttle::CheckThrottle::<Runtime>::new(),
			frame_system::WeightReclaim::<Runtime>::new(),
		)
	}
}

impl<LocalCall> frame_system::offchain::CreateSignedTransaction<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	fn create_signed_transaction<
		C: frame_system::offchain::AppCrypto<Self::Public, Self::Signature>,
	>(
		call: RuntimeCall,
		public: <Signature as sp_runtime::traits::Verify>::Signer,
		account: AccountId,
		nonce: Nonce,
	) -> Option<UncheckedExtrinsic> {
		// take the biggest period possible.
		let period =
			BlockHashCount::get().checked_next_power_of_two().map(|c| c / 2).unwrap_or(2) as u64;
		let current_block = System::block_number()
			.saturated_into::<u64>()
			// The `System::block_number` is initialized with `n+1`,
			// so the actual block number is `n`.
			.saturating_sub(1);
		let era = generic::Era::mortal(period, current_block);
		let tx_ext: TxExtension = (
			frame_system::AuthorizeCall::<Runtime>::new(),
			frame_system::CheckNonZeroSender::<Runtime>::new(),
			frame_system::CheckSpecVersion::<Runtime>::new(),
			frame_system::CheckTxVersion::<Runtime>::new(),
			frame_system::CheckGenesis::<Runtime>::new(),
			frame_system::CheckEra::<Runtime>::from(era),
			frame_system::CheckNonce::<Runtime>::from(nonce),
			frame_system::CheckWeight::<Runtime>::new(),
			CheckCallFilter,
			pallet_throttle::CheckThrottle::<Runtime>::new(),
			frame_system::WeightReclaim::<Runtime>::new(),
		);

		let raw_payload = SignedPayload::new(call, tx_ext)
			.map_err(|e| {
				log::warn!("Unable to create signed payload: {:?}", e);
			})
			.ok()?;
		let signature = raw_payload.using_encoded(|payload| C::sign(payload, public))?;
		let address = <Runtime as frame_system::Config>::Lookup::unlookup(account);
		let (call, tx_ext, _) = raw_payload.deconstruct();
		let transaction = generic::UncheckedExtrinsic::new_signed(call, address, signature, tx_ext);
		Some(transaction)
	}
}

#[cfg(feature = "runtime-benchmarks")]
mod benches {
	define_benchmarks!(
		[frame_benchmarking, BaselineBench::<Runtime>]
		[frame_system, SystemBench::<Runtime>]
		// The `TxExtension` checks every signed transaction pays for. Without this
		// entry `frame_system::Config::ExtensionsWeightInfo` falls back to upstream's
		// `()` impl, whose numbers are Parity's reference hardware and RocksDB —
		// this runtime is ParityDb.
		[frame_system_extensions, SystemExtensionsBench::<Runtime>]
		[pallet_beefy_mmr, BeefyMmrLeaf]
		[pallet_grandpa, Grandpa]
		[pallet_timestamp, Timestamp]
		[pallet_mmr, Mmr]
		[pallet_migrations, MultiBlockMigrations]
		[pallet_preimage, Preimage]
		[pallet_scheduler, Scheduler]
		[pallet_tx_pause, TxPause]
		// Load-bearing since #2079 made safe mode the failed-migration handler.
		// `enter`/`extend`/`release_deposit` report Weightless here: deposits are
		// disabled in this runtime, so those calls are unusable by design.
		[pallet_safe_mode, SafeMode]
		[pallet_collective, Council]
		[pallet_collective, TechnicalCommittee]
		[pallet_membership, CouncilMembership]
		[pallet_membership, TechnicalCommitteeMembership]
		[pallet_session_validator_management, SessionCommitteeManagement]
		[pallet_federated_authority, FederatedAuthority]
		[pallet_federated_authority_observation, FederatedAuthorityObservation]
		[pallet_system_parameters, SystemParameters]
		[pallet_cnight_observation, CNightObservation]
		[pallet_c2m_bridge, C2MBridge]
		[pallet_partner_chains_bridge, Bridge]
		// No extrinsics; the benchmark measures the per-block `on_initialize` digest write.
		[pallet_version, NodeVersion]
	);
}

impl_runtime_apis! {

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

		fn execute_block(block: <Block as BlockT>::LazyBlock) {
			// The fork block carries a storage delta instead of the inherents a
			// proposer would have produced; applying it is the whole execution.
			#[cfg(feature = "fork-transition")]
			if crate::fork_transition::is_fork_block(&block) {
				crate::fork_transition::execute_fork_block(&block);
				return;
			}

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
		fn get_network_id() -> String {
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
		fn get_transaction_cost(
			midnight_transaction: Vec<u8>,
		) -> Result<GasCost, LedgerApiError> {
			Midnight::get_transaction_cost(&midnight_transaction)
		}
		fn get_zswap_state_root() -> Result<Vec<u8>, LedgerApiError> {
			Midnight::get_zswap_state_root()
		}
		fn get_ledger_state_root() -> Result<Vec<u8>, LedgerApiError> {
			Midnight::get_ledger_state_root()
		}
	}

	impl sp_partner_chains_bridge::TokenBridgeIDPRuntimeApi<Block> for Runtime {
		fn get_pallet_version() -> u32 {
			Bridge::get_pallet_version()
		}

		fn get_main_chain_scripts() -> Option<BridgeMainChainScripts> {
			Bridge::get_main_chain_scripts()
		}

		fn get_max_transfers_per_block() -> u32 {
			Bridge::get_max_transfers_per_block()
		}

		fn get_last_data_checkpoint() -> Option<BridgeDataCheckpoint> {
			Bridge::get_data_checkpoint()
		}
	}

	impl pallet_c2m_bridge::C2MBridgeApi<Block> for Runtime {
		fn get_approved_mc_tx_hashes() -> Vec<sidechain_domain::McTxHash> {
			C2MBridge::get_approved_mc_tx_hashes()
		}
	}

	impl sp_api::Metadata<Block> for Runtime {
		fn metadata() -> OpaqueMetadata {
			OpaqueMetadata::new(Runtime::metadata().into())
		}

		fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
			Runtime::metadata_at_version(version)
		}

		fn metadata_versions() -> Vec<u32> {
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
			block: <Block as BlockT>::LazyBlock,
			data: sp_inherents::InherentData,
		) -> sp_inherents::CheckInherentsResult {
			// The fork block has no inherents to check -- it was never proposed.
			#[cfg(feature = "fork-transition")]
			if crate::fork_transition::is_fork_block(&block) {
				return sp_inherents::CheckInherentsResult::new();
			}

			// Verifiers check the first block after a runtime upgrade with this, the new,
			// runtime (the node calls this API in the on-chain context, which resolves the
			// upgrade staged in `:pending_code`), against the parent state the old runtime
			// left behind. Apply the pending migrations first, so the checks read storage in
			// the layout they expect. The runtime storage writes are discarded with the
			// runtime API call; the block's own execution runs the migrations again, for real.
			//
			// Host-side effects are NOT discarded: anything a migration does through a host
			// function (e.g. the ledger v8->v9 translation persisting the translated state in
			// the ledger arena) happens once per run, and a verifying node runs the migrations
			// twice for the first post-upgrade block (here, then in `execute_block`). Every
			// migration must therefore be deterministic and idempotent on the host side. This
			// was already required (competing children of the upgrade block, abandoned
			// proposals and resyncs re-run them on the same host state), but it is now the
			// normal path rather than an edge case.
			let upgraded = frame_system::LastRuntimeUpgrade::<Runtime>::get()
				.map(|last| last.was_upgraded(&VERSION))
				.unwrap_or(true);
			if upgraded {
				Executive::execute_on_runtime_upgrade();
			}

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
			// At the fork block's parent, report the fork's mock set so the
			// fork block's seal verifies. Every other height reads state.
			#[cfg(feature = "fork-transition")]
			if let Some(authorities) =
				crate::fork_transition::aura_authorities_at(System::block_number())
			{
				return authorities;
			}

			pallet_aura::Authorities::<Runtime>::get().into_inner()
		}
	}

	impl sp_consensus_babe::BabeApi<Block> for Runtime {
		fn configuration() -> sp_consensus_babe::BabeConfiguration {
			let epoch_config = Babe::epoch_config().unwrap_or(BABE_GENESIS_EPOCH_CONFIG);
			sp_consensus_babe::BabeConfiguration {
				slot_duration: Babe::slot_duration(),
				epoch_length: SidechainEpochDuration::get(),
				c: epoch_config.c,
				authorities: Babe::authorities().to_vec(),
				randomness: Babe::randomness(),
				allowed_slots: epoch_config.allowed_slots,
			}
		}

		fn current_epoch_start() -> sp_consensus_babe::Slot {
			Babe::current_epoch_start()
		}

		fn current_epoch() -> sp_consensus_babe::Epoch {
			Babe::current_epoch()
		}

		fn next_epoch() -> sp_consensus_babe::Epoch {
			Babe::next_epoch()
		}

		fn generate_key_ownership_proof(
			_slot: sp_consensus_babe::Slot,
			_authority_id: sp_consensus_babe::AuthorityId,
		) -> Option<sp_consensus_babe::OpaqueKeyOwnershipProof> {
			// Equivocation reporting is disabled, so no proof can be generated.
			None
		}

		fn submit_report_equivocation_unsigned_extrinsic(
			_equivocation_proof: sp_consensus_babe::EquivocationProof<<Block as BlockT>::Header>,
			_key_owner_proof: sp_consensus_babe::OpaqueKeyOwnershipProof,
		) -> Option<()> {
			None
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
	}

	// Collects the (Current BeefyStakes, Next BeefyStakes)
	impl midnight_primitives_beefy::BeefyStakesApi<Block, Hash, BeefyId> for Runtime {
		/// Gets the current beefy stakes
		fn current_beefy_stakes() -> BeefyStakes<BeefyId> {
			current_beefy_stakes(None)
		}

		/// Gets the next beefy stakes
		fn next_beefy_stakes() -> Option<BeefyStakes<BeefyId>> {
			next_beefy_stakes(None)
		}

		/// Returns the authority set based on the current beef stakes
		fn compute_current_authority_set(
			beefy_stakes: BeefyStakes<BeefyId>,
		) ->  BeefyAuthoritySet<Hash> {
			compute_current_authority_set(beefy_stakes)
		}

		/// Returns the authority set based on the next beef stakes
		fn compute_next_authority_set(
			beefy_stakes: BeefyStakes<BeefyId>,
		) -> BeefyNextAuthoritySet<Hash> {
			compute_next_authority_set(beefy_stakes)
		}
	}

	#[api_version(3)]
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

		fn generate_ancestry_proof(
			prev_block_number: BlockNumber,
			best_known_block_number: Option<BlockNumber>,
		) -> Result<mmr::AncestryProof<mmr::Hash>, mmr::Error> {
			Mmr::generate_ancestry_proof(prev_block_number, best_known_block_number)
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

	impl pallet_beefy_mmr::BeefyMmrApi<Block, Hash> for Runtime {
		fn authority_set_proof() -> sp_consensus_beefy::mmr::BeefyAuthoritySet<Hash> {
			BeefyMmrLeaf::authority_set_proof()
		}

		fn next_authority_set_proof() -> sp_consensus_beefy::mmr::BeefyNextAuthoritySet<Hash> {
			BeefyMmrLeaf::next_authority_set_proof()
		}
	}

	impl sp_session::SessionKeys<Block> for Runtime {
		fn generate_session_keys(
			owner: Vec<u8>,
			seed: Option<Vec<u8>>,
		) -> sp_session::OpaqueGeneratedSessionKeys {
			// despite being named "generate" this function also adds generated keys to local keystore
			let _ = CrossChainKey::generate(&owner, seed.clone());
			SessionKeys::generate(&owner, seed).into()
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
			use frame_benchmarking::{baseline, BenchmarkList};
			use frame_support::traits::StorageInfoTrait;
			use frame_system_benchmarking::Pallet as SystemBench;
			use frame_system_benchmarking::extensions::Pallet as SystemExtensionsBench;
			use baseline::Pallet as BaselineBench;

			let mut list = Vec::<BenchmarkList>::new();
			list_benchmarks!(list, extra);

			let storage_info = AllPalletsWithSystem::storage_info();

			(list, storage_info)
		}

		#[allow(non_local_definitions)]
		fn dispatch_benchmark(
			config: frame_benchmarking::BenchmarkConfig
		) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
			use frame_benchmarking::{baseline, BenchmarkBatch};
			use sp_storage::TrackedStorageKey;

			use frame_system_benchmarking::Pallet as SystemBench;
			use frame_system_benchmarking::extensions::Pallet as SystemExtensionsBench;
			use baseline::Pallet as BaselineBench;

			impl frame_system_benchmarking::Config for Runtime {}
			impl baseline::Config for Runtime {}

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
			block: <Block as BlockT>::LazyBlock,
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
				slot: <Runtime as pallet_sidechain::Config>::current_slot_number(),
				slots_per_epoch: Sidechain::slots_per_epoch().0,
			}
		}
	}

	impl midnight_primitives_session_info::SessionInfoApi<Block> for Runtime {
		fn current_session_index() -> u32 {
			Session::current_index()
		}
	}

	impl midnight_primitives_consensus_engine::ConsensusEngineApi<Block> for Runtime {
		fn active_engine() -> midnight_primitives_consensus_engine::ActiveEngine {
			ConsensusEngine::active_engine()
		}
	}

	impl sp_sidechain::GetGenesisUtxo<Block> for Runtime {
		fn genesis_utxo() -> UtxoId {
			Sidechain::genesis_utxo()
		}
	}

	impl sidechain_slots::SlotApi<Block> for Runtime {
		// The AURA slot duration needs no engine dispatch: it is the `SlotDuration`
		// config constant, and BABE's (`MinimumPeriod * 2`) is the same 6s, so the
		// value stays correct after the flip. `slot_duration_is_the_same_for_both_engines`
		// guards that equality.
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

	impl authority_selection_inherents::CandidateValidationApi<Block> for Runtime {
		fn validate_registered_candidate_data(stake_pool_pub_key: &StakePoolPublicKey, registration_data: &RegistrationData) -> Option<RegistrationDataError> {
			authority_selection_inherents::validate_registration_data::<SessionKeys>(stake_pool_pub_key, registration_data, Sidechain::genesis_utxo()).err()
		}
		fn validate_stake(stake: Option<StakeDelegation>) -> Option<StakeError> {
			authority_selection_inherents::validate_stake(stake).err()
		}
		fn validate_permissioned_candidate_data(candidate: PermissionedCandidateData) -> Option<PermissionedCandidateDataError> {
			validate_permissioned_candidate_data::<opaque::SessionKeys>(candidate).err()
		}
	}

	impl midnight_primitives_cnight_observation::CNightObservationApi<Block> for Runtime {
		fn get_mapping_validator_address() -> Vec<u8> {
			pallet_cnight_observation::MainChainMappingValidatorAddress::<Runtime>::get().into_inner()
		}

		fn get_next_cardano_position() -> CardanoPosition {
			pallet_cnight_observation::NextCardanoPosition::<Runtime>::get()
		}

		fn get_utxo_capacity_per_block() -> u32 {
			pallet_cnight_observation::CardanoTxCapacityPerBlock::<Runtime>::get()
		}

		fn get_cardano_block_window_size() -> u32 {
			pallet_cnight_observation::CardanoBlockWindowSize::<Runtime>::get()
		}

		fn get_cnight_token_identifier() -> (Vec<u8>, Vec<u8>) {
			let (policy_id, asset_name) = pallet_cnight_observation::CNightIdentifier::<Runtime>::get();
			(policy_id.into_inner(), asset_name.into_inner())
		}

		fn get_auth_token_asset_name() -> Vec<u8> {
			pallet_cnight_observation::MainChainAuthTokenAssetName::<Runtime>::get().into_inner()
		}
	}

	impl midnight_primitives_federated_authority_observation::FederatedAuthorityObservationApi<Block> for Runtime {
		fn get_council_address() -> MainchainAddress {
			pallet_federated_authority_observation::MainChainCouncilAddress::<Runtime>::get()
		}

		fn get_council_policy_id() -> PolicyId {
			pallet_federated_authority_observation::MainChainCouncilPolicyId::<Runtime>::get()
		}

		fn get_technical_committee_address() -> MainchainAddress {
			pallet_federated_authority_observation::MainChainTechnicalCommitteeAddress::<Runtime>::get()
		}

		fn get_technical_committee_policy_id() -> PolicyId {
			pallet_federated_authority_observation::MainChainTechnicalCommitteePolicyId::<Runtime>::get()
		}
	}

	impl pallet_system_parameters::SystemParametersApi<Block, Hash> for Runtime {
		fn get_terms_and_conditions() -> Option<pallet_system_parameters::TermsAndConditionsResponse<Hash>> {
			SystemParameters::get_terms_and_conditions().map(|tc| {
				pallet_system_parameters::TermsAndConditionsResponse {
					hash: tc.hash,
					url: tc.url.to_vec(),
				}
			})
		}

		fn get_d_parameter() -> sidechain_domain::DParameter {
			SystemParameters::get_d_parameter()
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::mock::*;
	use crate::{SystemParameters, select_authorities_optionally_overriding};
	use authority_selection_inherents::{AuthoritySelectionInputs, RegisterValidatorSignedMessage};
	use frame_support::{
		assert_ok,
		dispatch::PostDispatchInfo,
		inherent::ProvideInherent,
		traits::{FindAuthor, UnfilteredDispatchable, WhitelistedStorageKeys},
	};
	use frame_system::RawOrigin;
	use midnight_primitives_consensus_engine::ActiveEngine;
	use parity_scale_codec::Decode;
	use sidechain_domain::{
		CandidateRegistrations, CrossChainPublicKey, CrossChainSignature, DParameter, EpochNonce,
		MainchainSignature, PermissionedCandidateData, RegistrationData, ScEpochNumber,
		SidechainSignature, StakeDelegation, StakePoolPublicKey, UtxoId, UtxoInfo,
	};
	use sp_consensus_aura::AURA_ENGINE_ID;
	use sp_consensus_aura::sr25519::AuthorityId as AuraId;
	use sp_consensus_babe::{BABE_ENGINE_ID, BabeAuthorityWeight, ConsensusLog};
	use sp_core::crypto::AccountId32;
	use sp_core::{Pair, ed25519, hexdisplay::HexDisplay};
	use sp_inherents::InherentData;
	use sp_runtime::traits::Zero;
	use sp_session_validator_management::CommitteeMember as _;
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

			// Scheduled committee goes into effect after a 2-epoch delay
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
				assert_grandpa_authorities!([alice(), bob()]);
			});
			set_committee_through_inherent_data(&[alice()]);
			for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
				assert_current_epoch!(2);
				assert_grandpa_weights();
				assert_grandpa_authorities!([alice()]);
			});
			set_committee_through_inherent_data(&[alice(), bob()]);
			for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
				assert_current_epoch!(3);
				assert_grandpa_weights();
				assert_grandpa_authorities!([bob()]);
			});
			set_committee_through_inherent_data(&[bob(), alice()]);
			for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
				assert_current_epoch!(4);
				assert_grandpa_weights();
				assert_grandpa_authorities!([alice()]);
			});
			set_committee_through_inherent_data(&[alice()]);
			for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
				assert_current_epoch!(5);
				assert_grandpa_weights();
				assert_grandpa_authorities!([alice(), bob()]);
			});

			// When there's no new committees being scheduled, the last committee stays in power
			for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH * 3, &|| {
				assert_grandpa_weights();
				assert_grandpa_authorities!([bob(), alice()]);
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
			// Needs to be run to initialize first slot and epoch numbers;
			advance_block();
			// Scheduled committee goes into effect after a 2-epoch delay
			set_committee_through_inherent_data(&[alice()]);
			until_epoch_after_finalizing(1, &|| {
				assert_current_epoch!(0);
				assert_aura_authorities!([alice(), bob()]);
			});

			set_committee_through_inherent_data(&[bob()]);
			for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
				assert_current_epoch!(1);
				assert_aura_authorities!([alice(), bob()]);
			});
			set_committee_through_inherent_data(&[alice()]);
			for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
				assert_current_epoch!(2);
				assert_aura_authorities!([alice()]);
			});
			set_committee_through_inherent_data(&[alice(), bob()]);
			for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
				assert_current_epoch!(3);
				assert_aura_authorities!([bob()]);
			});
			set_committee_through_inherent_data(&[bob(), alice()]);
			for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
				assert_current_epoch!(4);
				assert_aura_authorities!([alice()]);
			});
			set_committee_through_inherent_data(&[alice()]);
			for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
				assert_current_epoch!(5);
				assert_aura_authorities!([alice(), bob()]);
			});

			// When there's no new committees being scheduled, the last committee stays in power
			for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH * 3, &|| {
				assert_aura_authorities!([bob(), alice()]);
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

	/// What an observer of the chain sees across the flip must agree, block by block: the engine
	/// the header's pre-runtime digests name, the signer those digests resolve to in every
	/// authority view readable at the parent state (`AuraApi::authorities`, `BabeApi::current_epoch`
	/// and `next_epoch`, the committee pallet's current and queued committees, `pallet_session`'s
	/// validators), the author the runtime credits, and the epoch BABE announces for the session to
	/// come. The committee changes at every rotation, so a stale or wrong set is a distinct answer.
	///
	/// Post-flip, a block that opens an epoch is signed under the epoch BABE announced one epoch
	/// earlier, i.e. `BabeApi::next_epoch` at its parent; every other block under
	/// `BabeApi::current_epoch` at its parent. At the flip block both name the first BABE epoch.
	#[test]
	fn observers_see_coherent_signers_and_epochs_across_the_flip() {
		new_test_ext().execute_with(|| {
			// As the upgrade migration leaves an existing network.
			ConsensusEngine::activate();
			advance_block();

			// A committee set in epoch N serves epoch N + 2. The flip commits at the end of epoch
			// 1, so `[bob]` serves the first BABE epoch while `[alice, bob]` still signs epoch 1.
			let plan = [
				vec![bob()],
				vec![alice()],
				vec![alice(), bob()],
				vec![bob(), alice()],
				vec![alice()],
			];
			let mut planned = None;
			let mut seen = std::collections::BTreeMap::<Kind, u32>::new();
			let mut last = None;
			// The activation block is observed too: the parent view is the genesis state.
			let mut parent = View::capture();
			loop {
				advance_block();
				let kind = match (last, observe(&parent)) {
					(Some(Kind::Flip), Kind::BabeOpening) => Kind::FirstBabe,
					(_, kind) => kind,
				};
				*seen.entry(kind).or_default() += 1;
				last = Some(kind);
				parent = View::capture();

				let epoch = (Sidechain::current_epoch_number().0 - ARBITRARY_FIRST_EPOCH) as usize;
				if epoch > plan.len() {
					break;
				}
				if planned != Some(epoch) {
					if let Some(committee) = plan.get(epoch) {
						set_committee_through_inherent_data(committee);
					}
					if epoch == 1 {
						assert_ok!(ConsensusEngine::schedule_flip(RawOrigin::Root.into()));
					}
					planned = Some(epoch);
				}
			}

			// Every kind of block the migration produces was checked, the singular ones once.
			assert_eq!(seen[&Kind::Flip], 1);
			assert_eq!(seen[&Kind::FirstBabe], 1);
			assert!(seen[&Kind::PreFlip] > 1, "{seen:?}");
			assert!(seen[&Kind::PreFlipOpening] >= 1, "{seen:?}");
			assert!(seen[&Kind::BabeOpening] >= 3, "{seen:?}");
			assert!(seen[&Kind::Babe] > 1, "{seen:?}");
			assert_eq!(parent.engine, ActiveEngine::Babe);

			// Every session the chain went through has its validator trie root on record.
			for index in 0..=Session::current_index() {
				assert!(
					pallet_session::historical::HistoricalSessions::<Test>::get(index).is_some(),
					"session {index} has no historical root"
				);
			}
		});

		/// The kinds of block the migration produces, each with its own observer rules.
		#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
		enum Kind {
			PreFlip,
			PreFlipOpening,
			Flip,
			FirstBabe,
			BabeOpening,
			Babe,
		}

		type Member = <Test as pallet_session_validator_management::Config>::CommitteeMember;
		type BabeAuthorities = Vec<(sp_consensus_babe::AuthorityId, BabeAuthorityWeight)>;

		/// The views an observer can read at a block's state, used for the block's child.
		struct View {
			engine: ActiveEngine,
			session: u32,
			aura: Vec<AuraId>,
			babe: BabeAuthorities,
			current_epoch: Option<sp_consensus_babe::Epoch>,
			next_epoch: BabeAuthorities,
			validators: Vec<AccountId32>,
			queued_validators: Vec<AccountId32>,
			committee: Vec<Member>,
			queued_committee: Vec<Member>,
		}

		impl View {
			fn capture() -> Self {
				// `BabeApi::current_epoch` needs the epoch config the flip writes.
				let current_epoch =
					pallet_babe::EpochConfig::<Test>::get().map(|_| Babe::current_epoch());
				View {
					engine: ConsensusEngine::active_engine(),
					session: Session::current_index(),
					aura: pallet_aura::Authorities::<Test>::get().to_vec(),
					babe: pallet_babe::Authorities::<Test>::get().to_vec(),
					current_epoch,
					next_epoch: pallet_babe::NextAuthorities::<Test>::get().to_vec(),
					validators: Session::validators(),
					queued_validators: pallet_session::QueuedKeys::<Test>::get()
						.into_iter()
						.map(|(validator, _)| validator)
						.collect(),
					committee: SessionCommitteeManagement::current_committee_storage()
						.committee
						.to_vec(),
					queued_committee: SessionCommitteeManagement::queued_committee_storage()
						.committee
						.to_vec(),
				}
			}
		}

		/// Check the block just initialized against the views at its parent.
		fn observe(parent: &View) -> Kind {
			let block = System::block_number();
			let logs = System::digest().logs;
			let engines = logs
				.iter()
				.filter_map(|log| log.as_pre_runtime())
				.map(|(engine, _)| engine)
				.collect::<Vec<_>>();
			let expected = match parent.engine {
				ActiveEngine::Aura => vec![AURA_ENGINE_ID, BABE_ENGINE_ID],
				ActiveEngine::Babe => vec![BABE_ENGINE_ID],
			};
			assert_eq!(engines, expected, "block {block}: pre-runtime digests");

			let index = <ConsensusEngine as FindAuthor<u32>>::find_author(
				logs.iter().filter_map(|log| log.as_pre_runtime()),
			)
			.expect("the digests name the author") as usize;
			let opens_epoch = babe_next_epoch_data().is_some();
			let rotated = Session::current_index() != parent.session;
			assert_eq!(
				opens_epoch, rotated,
				"block {block}: BABE announces an epoch iff the session rotates"
			);

			use frame_support::traits::KeyOwnerProofSystem;
			// `pallet_session::historical` has a root for the current session sized to its set,
			// and the session's members can prove their membership against it.
			let (_, count) = pallet_session::historical::HistoricalSessions::<Test>::get(
				Session::current_index(),
			)
			.expect("the current session has a historical root");
			assert_eq!(count as usize, Session::validators().len(), "block {block}");
			for validator in Session::validators() {
				let keys = Session::load_keys(&validator).expect("a validator has keys");
				let key: (sp_runtime::KeyTypeId, &[u8]) =
					(sp_consensus_babe::KEY_TYPE, keys.babe.as_ref());
				let proof = Historical::prove(key).expect("a member proves its BABE key");
				assert_eq!(proof.session, Session::current_index(), "block {block}");
				assert_eq!(
					Historical::check_proof(key, proof),
					Some((validator, ())),
					"block {block}: the proof names its owner"
				);
			}

			// The signer: who the author index names under the set the block is signed in.
			let (signer_babe, member, session_view) = match (parent.engine, opens_epoch) {
				// AURA: `slot % n` in the AURA list; the BABE digest names the same member.
				(ActiveEngine::Aura, _) => {
					assert_eq!(parent.aura[index], parent.committee[index].authority_keys().aura);
					(parent.babe[index].0.clone(), &parent.committee[index], &parent.validators)
				},
				(ActiveEngine::Babe, false) => {
					let epoch = parent.current_epoch.as_ref().expect("post-flip epoch");
					(
						epoch.authorities[index].0.clone(),
						&parent.committee[index],
						&parent.validators,
					)
				},
				(ActiveEngine::Babe, true) => (
					parent.next_epoch[index].0.clone(),
					&parent.queued_committee[index],
					&parent.queued_validators,
				),
			};
			let signer: AccountId32 = member.authority_id().into();
			assert_eq!(signer_babe, member.authority_keys().babe, "block {block}: committee view");
			assert_eq!(session_view[index], signer, "block {block}: pallet_session view");

			// Post-flip the signer's set is this block's current set everywhere.
			if parent.engine == ActiveEngine::Babe {
				assert_eq!(Session::validators()[index], signer, "block {block}");
				assert_eq!(
					pallet_babe::Authorities::<Test>::get()[index].0,
					signer_babe,
					"block {block}"
				);
				let current: AccountId32 = current_committee()[index].authority_id().into();
				assert_eq!(current, signer, "block {block}");
			}

			// What BABE announces is the set the next rotation installs.
			if let Some(announced) = babe_next_epoch_data() {
				assert_eq!(
					announced,
					pallet_babe::NextAuthorities::<Test>::get().to_vec(),
					"block {block}"
				);
				assert_eq!(announced, session_queued_babe_authorities(), "block {block}");
				assert_eq!(
					announced,
					committee_babe_authorities(queued_committee()),
					"block {block}"
				);
				if pallet_babe::EpochConfig::<Test>::get().is_some() {
					assert_eq!(announced, Babe::next_epoch().authorities, "block {block}");
				}
			}

			// The flip block: AURA-authored (its header says so) while its state already reports
			// BABE, so observers must take the engine from the header. Its state seeds epoch 0 with
			// the committee queued for the epoch it covers.
			let flipped = parent.engine == ActiveEngine::Aura
				&& ConsensusEngine::active_engine() == ActiveEngine::Babe;
			if flipped {
				assert!(!opens_epoch, "block {block}: the flip block ends an epoch");
				let epoch_zero = Babe::current_epoch();
				assert_eq!(
					epoch_zero.authorities,
					pallet_babe::NextAuthorities::<Test>::get().to_vec()
				);
				assert_eq!(epoch_zero.authorities, session_queued_babe_authorities());
				assert_eq!(epoch_zero.authorities, committee_babe_authorities(queued_committee()));
				assert_eq!(epoch_zero.authorities, Babe::next_epoch().authorities);
				assert_ne!(epoch_zero.authorities, committee_babe_authorities(current_committee()));
			}
			match (parent.engine, opens_epoch, flipped) {
				(_, _, true) => Kind::Flip,
				(ActiveEngine::Aura, true, _) => Kind::PreFlipOpening,
				(ActiveEngine::Aura, false, _) => Kind::PreFlip,
				(ActiveEngine::Babe, true, _) => Kind::BabeOpening,
				(ActiveEngine::Babe, false, _) => Kind::Babe,
			}
		}

		/// The BABE keys `pallet_session` activates at its next rotation.
		fn session_queued_babe_authorities() -> BabeAuthorities {
			pallet_session::QueuedKeys::<Test>::get()
				.into_iter()
				.map(|(_, keys)| (keys.babe, 1))
				.collect()
		}

		fn queued_committee() -> Vec<Member> {
			SessionCommitteeManagement::queued_committee_storage().committee.to_vec()
		}

		fn current_committee() -> Vec<Member> {
			SessionCommitteeManagement::current_committee_storage().committee.to_vec()
		}

		fn committee_babe_authorities(committee: Vec<Member>) -> BabeAuthorities {
			committee.into_iter().map(|member| (member.authority_keys().babe, 1)).collect()
		}

		/// The authorities of the `NextEpochData` BABE consensus log in the current block, if any.
		fn babe_next_epoch_data() -> Option<BabeAuthorities> {
			System::digest().logs.iter().find_map(|log| {
				let (engine, mut data) = log.as_consensus()?;
				if engine != BABE_ENGINE_ID {
					return None;
				}
				match ConsensusLog::decode(&mut data).ok()? {
					ConsensusLog::NextEpochData(descriptor) => Some(descriptor.authorities),
					_ => None,
				}
			})
		}
	}

	/// Observers read the chain through runtime APIs. Each one must be the pallet state the
	/// coherence test above checks, with nothing in between.
	mod runtime_api_views {
		use crate::mock::{TestKeys, alice, bob};
		use crate::{
			AuraId, BABE_GENESIS_EPOCH_CONFIG, BeefyId, Block, GrandpaId, Hash, Runtime,
			SLOT_DURATION, Sidechain, opaque,
		};
		use authority_selection_inherents::{AuthoritySelectionInputs, CommitteeMember};
		use frame_support::{BoundedVec, WeakBoundedVec};
		use midnight_primitives_consensus_engine::ActiveEngine;
		use pallet_consensus_engine::{EngineState, State};
		use pallet_session_validator_management::{
			CommitteeInfo, CurrentCommittee, MainChainScriptsConfiguration, NextCommittee,
			QueuedCommittee,
		};
		use parity_scale_codec::Encode;
		use sidechain_domain::{MainchainAddress, PolicyId, ScEpochNumber};
		use sp_consensus_beefy::mmr::BeefyAuthoritySet;
		use sp_consensus_slots::{Slot, SlotDuration};
		use sp_core::{ByteArray, Pair};
		use sp_runtime::key_types::{AURA, BABE, BEEFY, GRANDPA};
		use sp_session_validator_management::MainChainScripts;

		type CommitteeMemberOf =
			<Runtime as pallet_session_validator_management::Config>::CommitteeMember;

		fn ext() -> sp_io::TestExternalities {
			sp_io::TestExternalities::default()
		}

		fn babe_set(
			seed: u8,
		) -> WeakBoundedVec<
			(sp_consensus_babe::AuthorityId, sp_consensus_babe::BabeAuthorityWeight),
			<Runtime as pallet_babe::Config>::MaxAuthorities,
		> {
			let id = sp_consensus_babe::AuthorityId::from(sp_core::sr25519::Public::from_raw(
				[seed; 32],
			));
			WeakBoundedVec::try_from(vec![(id, 1)]).expect("one authority fits")
		}

		fn beefy_id(keys: &TestKeys) -> BeefyId {
			keys.cross_chain.public().into_inner().into()
		}

		fn member(keys: &TestKeys) -> CommitteeMemberOf {
			CommitteeMember::permissioned(
				keys.cross_chain.public(),
				opaque::SessionKeys {
					aura: keys.aura.public(),
					grandpa: keys.grandpa.public(),
					babe: keys.babe.public(),
					beefy: beefy_id(keys),
				},
			)
		}

		fn committee(
			epoch: u64,
			members: &[TestKeys],
		) -> CommitteeInfo<
			ScEpochNumber,
			CommitteeMemberOf,
			<Runtime as pallet_session_validator_management::Config>::MaxValidators,
		> {
			CommitteeInfo {
				epoch: ScEpochNumber(epoch),
				committee: BoundedVec::truncate_from(members.iter().map(member).collect()),
			}
		}

		#[test]
		fn aura_api_reports_the_pallet_authorities_and_slot_duration() {
			ext().execute_with(|| {
				let authorities = vec![alice().aura.public(), bob().aura.public()];
				pallet_aura::Authorities::<Runtime>::put(BoundedVec::truncate_from(
					authorities.clone(),
				));

				use sp_consensus_aura::runtime_decl_for_aura_api::AuraApi;
				assert_eq!(<Runtime as AuraApi<Block, AuraId>>::authorities(), authorities);
				assert_eq!(
					<Runtime as AuraApi<Block, AuraId>>::slot_duration(),
					SlotDuration::from_millis(SLOT_DURATION)
				);
			});
		}

		#[test]
		fn babe_api_reports_the_pallet_epochs_and_configuration() {
			ext().execute_with(|| {
				pallet_babe::EpochConfig::<Runtime>::put(BABE_GENESIS_EPOCH_CONFIG);
				pallet_babe::Authorities::<Runtime>::put(babe_set(1));
				pallet_babe::NextAuthorities::<Runtime>::put(babe_set(2));
				pallet_babe::Randomness::<Runtime>::put([7; 32]);
				pallet_babe::GenesisSlot::<Runtime>::put(Slot::from(700u64));
				pallet_babe::EpochIndex::<Runtime>::put(2);
				let epoch_length = u64::from(Sidechain::slots_per_epoch().0);

				use sp_consensus_babe::runtime_decl_for_babe_api::BabeApi;
				let current = <Runtime as BabeApi<Block>>::current_epoch();
				assert_eq!(current.authorities, babe_set(1).to_vec());
				assert_eq!(current.epoch_index, 2);
				assert_eq!(current.start_slot, Slot::from(700 + 2 * epoch_length));
				assert_eq!(current.duration, epoch_length);
				assert_eq!(current.config, BABE_GENESIS_EPOCH_CONFIG);
				let next = <Runtime as BabeApi<Block>>::next_epoch();
				assert_eq!(next.authorities, babe_set(2).to_vec());
				assert_eq!(next.epoch_index, 3);
				assert_eq!(next.start_slot, Slot::from(700 + 3 * epoch_length));
				assert_eq!(
					<Runtime as BabeApi<Block>>::current_epoch_start(),
					Slot::from(700 + 2 * epoch_length)
				);
				let configuration = <Runtime as BabeApi<Block>>::configuration();
				assert_eq!(configuration.authorities, babe_set(1).to_vec());
				assert_eq!(configuration.randomness, [7; 32]);
				assert_eq!(configuration.epoch_length, epoch_length);
				assert_eq!(configuration.slot_duration, SLOT_DURATION);
				assert_eq!(configuration.c, BABE_GENESIS_EPOCH_CONFIG.c);
				assert_eq!(configuration.allowed_slots, BABE_GENESIS_EPOCH_CONFIG.allowed_slots);
				assert!(
					<Runtime as BabeApi<Block>>::generate_key_ownership_proof(
						Slot::from(700u64),
						babe_set(1)[0].0.clone()
					)
					.is_none()
				);
			});
		}

		#[test]
		fn grandpa_api_reports_the_pallet_authorities_and_set_id() {
			ext().execute_with(|| {
				let authorities =
					vec![(alice().grandpa.public(), 1u64), (bob().grandpa.public(), 1)];
				pallet_grandpa::Authorities::<Runtime>::put(
					WeakBoundedVec::try_from(authorities.clone()).expect("two authorities fit"),
				);
				pallet_grandpa::CurrentSetId::<Runtime>::put(3);

				use sp_consensus_grandpa::runtime_decl_for_grandpa_api::GrandpaApi;
				assert_eq!(<Runtime as GrandpaApi<Block>>::grandpa_authorities(), authorities);
				assert_eq!(<Runtime as GrandpaApi<Block>>::current_set_id(), 3);
				assert!(
					<Runtime as GrandpaApi<Block>>::generate_key_ownership_proof(
						3,
						GrandpaId::from(alice().grandpa.public())
					)
					.is_none()
				);
			});
		}

		#[test]
		fn beefy_api_reports_the_pallet_validator_set_and_genesis() {
			ext().execute_with(|| {
				let validators = vec![beefy_id(&alice()), beefy_id(&bob())];
				pallet_beefy::Authorities::<Runtime>::put(BoundedVec::truncate_from(
					validators.clone(),
				));
				pallet_beefy::ValidatorSetId::<Runtime>::put(2);
				pallet_beefy::GenesisBlock::<Runtime>::put(Some(5));

				use sp_consensus_beefy::runtime_decl_for_beefy_api::BeefyApi;
				let set = <Runtime as BeefyApi<Block, BeefyId>>::validator_set().expect("a set");
				assert_eq!(set.validators(), &validators[..]);
				assert_eq!(set.id(), 2);
				assert_eq!(<Runtime as BeefyApi<Block, BeefyId>>::beefy_genesis(), Some(5));
				assert!(
					<Runtime as BeefyApi<Block, BeefyId>>::generate_key_ownership_proof(
						2,
						beefy_id(&alice())
					)
					.is_none()
				);
			});
		}

		#[test]
		fn beefy_stakes_api_reports_the_current_and_queued_sets() {
			ext().execute_with(|| {
				pallet_beefy::Authorities::<Runtime>::put(BoundedVec::truncate_from(vec![
					beefy_id(&alice()),
				]));
				pallet_beefy::NextAuthorities::<Runtime>::put(BoundedVec::truncate_from(vec![
					beefy_id(&bob()),
				]));
				CurrentCommittee::<Runtime>::put(committee(7, &[alice()]));
				QueuedCommittee::<Runtime>::put(committee(7, &[bob()]));

				use midnight_primitives_beefy::runtime_decl_for_beefy_stakes_api::BeefyStakesApi;
				assert_eq!(
					<Runtime as BeefyStakesApi<Block, Hash, BeefyId>>::current_beefy_stakes(),
					vec![(beefy_id(&alice()), 1)]
				);
				assert_eq!(
					<Runtime as BeefyStakesApi<Block, Hash, BeefyId>>::next_beefy_stakes(),
					Some(vec![(beefy_id(&bob()), 1)])
				);
			});
		}

		#[test]
		fn beefy_mmr_api_reports_the_authority_set_proofs() {
			ext().execute_with(|| {
				let current =
					BeefyAuthoritySet { id: 2, len: 1, keyset_commitment: Hash::repeat_byte(2) };
				let next =
					BeefyAuthoritySet { id: 3, len: 2, keyset_commitment: Hash::repeat_byte(3) };
				pallet_beefy_mmr::BeefyAuthorities::<Runtime>::put(current.clone());
				pallet_beefy_mmr::BeefyNextAuthorities::<Runtime>::put(next.clone());

				use pallet_beefy_mmr::runtime_decl_for_beefy_mmr_api::BeefyMmrApi;
				assert_eq!(<Runtime as BeefyMmrApi<Block, Hash>>::authority_set_proof(), current);
				assert_eq!(<Runtime as BeefyMmrApi<Block, Hash>>::next_authority_set_proof(), next);
			});
		}

		#[test]
		fn session_keys_api_decodes_the_opaque_keys_in_declaration_order() {
			ext().execute_with(|| {
				let keys = alice();
				let encoded = opaque::SessionKeys {
					aura: keys.aura.public(),
					grandpa: keys.grandpa.public(),
					babe: keys.babe.public(),
					beefy: beefy_id(&keys),
				}
				.encode();

				use sp_session::runtime_decl_for_session_keys::SessionKeys;
				assert_eq!(
					<Runtime as SessionKeys<Block>>::decode_session_keys(encoded),
					Some(vec![
						(keys.aura.public().to_raw_vec(), AURA),
						(keys.grandpa.public().to_raw_vec(), GRANDPA),
						(keys.babe.public().to_raw_vec(), BABE),
						(beefy_id(&keys).to_raw_vec(), BEEFY),
					])
				);
			});
		}

		#[test]
		fn session_info_api_reports_the_session_index() {
			ext().execute_with(|| {
				pallet_session::CurrentIndex::<Runtime>::put(5);

				use midnight_primitives_session_info::runtime_decl_for_session_info_api::SessionInfoApi;
				assert_eq!(<Runtime as SessionInfoApi<Block>>::current_session_index(), 5);
			});
		}

		#[test]
		fn consensus_engine_api_reports_the_engine_state() {
			ext().execute_with(|| {
				use midnight_primitives_consensus_engine::runtime_decl_for_consensus_engine_api::ConsensusEngineApi;
				assert_eq!(
					<Runtime as ConsensusEngineApi<Block>>::active_engine(),
					ActiveEngine::Aura
				);
				EngineState::<Runtime>::put(State::ScheduledFlip);
				assert_eq!(
					<Runtime as ConsensusEngineApi<Block>>::active_engine(),
					ActiveEngine::Aura
				);
				EngineState::<Runtime>::put(State::Babe);
				assert_eq!(
					<Runtime as ConsensusEngineApi<Block>>::active_engine(),
					ActiveEngine::Babe
				);
			});
		}

		#[test]
		fn committee_api_reports_the_committees_epochs_and_scripts() {
			ext().execute_with(|| {
				let current = committee(7, &[alice()]);
				let next = committee(8, &[alice(), bob()]);
				CurrentCommittee::<Runtime>::put(current.clone());
				QueuedCommittee::<Runtime>::put(committee(7, &[bob()]));
				let scripts = MainChainScripts {
					committee_candidate_address: MainchainAddress::default(),
					d_parameter_policy_id: PolicyId([1; 28]),
					permissioned_candidates_policy_id: PolicyId([2; 28]),
				};
				MainChainScriptsConfiguration::<Runtime>::put(scripts.clone());

				use sp_session_validator_management::runtime_decl_for_session_validator_management_api::SessionValidatorManagementApi;
				macro_rules! api {
					($method:ident) => {
						<Runtime as SessionValidatorManagementApi<
							Block,
							CommitteeMemberOf,
							AuthoritySelectionInputs,
							ScEpochNumber,
						>>::$method()
					};
				}
				assert_eq!(api!(get_current_committee), (ScEpochNumber(7), current.committee.to_vec()));
				assert_eq!(api!(get_next_committee), None);
				// With nothing selected yet, the next epoch to fill follows the queued one.
				assert_eq!(api!(get_next_unset_epoch_number), ScEpochNumber(8));
				NextCommittee::<Runtime>::put(next.clone());
				assert_eq!(api!(get_next_committee), Some((ScEpochNumber(8), next.committee.to_vec())));
				assert_eq!(api!(get_next_unset_epoch_number), ScEpochNumber(9));
				assert_eq!(api!(get_main_chain_scripts), scripts);
			});
		}
	}

	#[test]
	// The effects of setting the d parameter are already well-tested, so we will not check that. We will check the selection to ensure that it simply respects d-parameter overriding
	fn check_overridden_d_param_committee_rotation() {
		new_test_ext().execute_with(|| {
			let permissioned_validators = vec![alice_mock_validator(), bob_mock_validator()];
			let registered_validators = vec![charlie_mock_validator()];

			let d_parameter =
				DParameter { num_permissioned_candidates: 1, num_registered_candidates: 0 };

			// Set initial D-parameter in SystemParameters pallet
			assert_ok!(SystemParameters::update_d_parameter(RawOrigin::Root.into(), 1, 0));

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

			// Override the D-parameter via SystemParameters pallet
			assert_ok!(SystemParameters::update_d_parameter(RawOrigin::Root.into(), 20, 2));

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
		let slot = *ConsensusEngine::current_slot();
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
				keys: c.session_keys(),
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
					keys: validator.session_keys(),
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

	mod sidechain_slot_number {
		use crate::Runtime;
		use frame_support::traits::Hooks;
		use pallet_consensus_engine::{EngineState, State};
		use parity_scale_codec::Encode;
		use sidechain_domain::ScSlotNumber;
		use sp_consensus_babe::BABE_ENGINE_ID;
		use sp_consensus_babe::digests::{PreDigest, SecondaryPlainPreDigest};
		use sp_consensus_slots::Slot;
		use sp_runtime::{Digest, DigestItem};

		fn babe_pre_digest(slot: u64) -> DigestItem {
			DigestItem::PreRuntime(
				BABE_ENGINE_ID,
				PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
					authority_index: 0,
					slot: Slot::from(slot),
				})
				.encode(),
			)
		}

		fn initialize_block_with_logs(logs: Vec<DigestItem>) {
			frame_system::Pallet::<Runtime>::initialize(&1, &Default::default(), &Digest { logs });
		}

		fn current_slot_number() -> ScSlotNumber {
			<Runtime as pallet_sidechain::Config>::current_slot_number()
		}

		#[test]
		fn is_read_from_aura_storage_pre_flip() {
			sp_io::TestExternalities::default().execute_with(|| {
				pallet_aura::CurrentSlot::<Runtime>::put(Slot::from(7u64));
				pallet_babe::CurrentSlot::<Runtime>::put(Slot::from(99u64));
				assert_eq!(current_slot_number(), ScSlotNumber(7));
			});
		}

		#[test]
		fn is_read_from_babe_storage_post_flip() {
			sp_io::TestExternalities::default().execute_with(|| {
				EngineState::<Runtime>::put(State::Babe);
				pallet_aura::CurrentSlot::<Runtime>::put(Slot::from(7u64));
				pallet_babe::CurrentSlot::<Runtime>::put(Slot::from(42u64));
				assert_eq!(current_slot_number(), ScSlotNumber(42));
			});
		}

		/// Regression: with Babe before Sidechain, Babe's `on_initialize` copies the
		/// pre-digest into `CurrentSlot` before Sidechain reads it — so a stale
		/// storage value is overwritten for this block.
		#[test]
		fn babe_hook_refreshes_storage_before_sidechain_reads() {
			sp_io::TestExternalities::default().execute_with(|| {
				EngineState::<Runtime>::put(State::Babe);
				// Suppress premature genesis init so Babe only updates CurrentSlot.
				pallet_babe::GenesisSlot::<Runtime>::put(Slot::from(1u64));
				pallet_babe::CurrentSlot::<Runtime>::put(Slot::from(41u64));
				initialize_block_with_logs(vec![babe_pre_digest(42)]);

				pallet_babe::Pallet::<Runtime>::on_initialize(1);

				assert_eq!(current_slot_number(), ScSlotNumber(42));
			});
		}
	}

	/// Tests for the slot reported by the `GetSidechainStatus` runtime API across the
	/// AURA to BABE consensus flip.
	mod sidechain_status {
		use crate::Runtime;
		use pallet_consensus_engine::{EngineState, State};
		use sidechain_domain::{ScEpochNumber, ScSlotNumber};
		use sp_consensus_slots::Slot;
		use sp_sidechain::SidechainStatus;

		/// Slot the AURA storage is left frozen at, standing in for the value
		/// `pallet_aura::CurrentSlot` keeps forever once BABE takes over.
		const STALE_AURA_SLOT: u64 = 41;
		const BABE_SLOT: u64 = 4242;

		fn get_sidechain_status() -> SidechainStatus {
			<Runtime as sp_sidechain::runtime_decl_for_get_sidechain_status::GetSidechainStatus<
				crate::Block,
			>>::get_sidechain_status()
		}

		fn slots_per_epoch() -> u64 {
			u64::from(crate::Sidechain::slots_per_epoch().0)
		}

		#[test]
		fn slot_is_read_from_aura_storage_pre_flip() {
			sp_io::TestExternalities::default().execute_with(|| {
				// Default engine state is Aura.
				pallet_aura::CurrentSlot::<Runtime>::put(Slot::from(STALE_AURA_SLOT));
				pallet_babe::CurrentSlot::<Runtime>::put(Slot::from(BABE_SLOT));

				let status = get_sidechain_status();

				assert_eq!(status.slot, ScSlotNumber(STALE_AURA_SLOT));
				assert_eq!(status.epoch, ScEpochNumber(STALE_AURA_SLOT / slots_per_epoch()));
			});
		}

		// Regression test: this API used to read `pallet_aura::CurrentSlot`
		// unconditionally, which stops advancing once BABE produces blocks, so the
		// reported slot (and the epoch derived from it) froze at the flip.
		#[test]
		fn slot_is_read_from_babe_storage_post_flip() {
			sp_io::TestExternalities::default().execute_with(|| {
				EngineState::<Runtime>::put(State::Babe);
				pallet_aura::CurrentSlot::<Runtime>::put(Slot::from(STALE_AURA_SLOT));
				pallet_babe::CurrentSlot::<Runtime>::put(Slot::from(BABE_SLOT));

				let status = get_sidechain_status();

				assert_eq!(status.slot, ScSlotNumber(BABE_SLOT));
				assert_eq!(status.epoch, ScEpochNumber(BABE_SLOT / slots_per_epoch()));
			});
		}

		// The scheduled state still produces AURA blocks, so the slot must keep
		// coming from AURA until the flip actually completes.
		#[test]
		fn slot_is_read_from_aura_storage_while_the_flip_is_pending() {
			sp_io::TestExternalities::default().execute_with(|| {
				EngineState::<Runtime>::put(State::ScheduledFlip);
				pallet_aura::CurrentSlot::<Runtime>::put(Slot::from(STALE_AURA_SLOT));
				pallet_babe::CurrentSlot::<Runtime>::put(Slot::from(BABE_SLOT));

				assert_eq!(get_sidechain_status().slot, ScSlotNumber(STALE_AURA_SLOT));
			});
		}
	}

	/// Nodes distrust the AURA/BABE pre-runtime digest layout of blocks executed by runtimes from
	/// before `pallet-consensus-engine`, identified by `spec_version` via the `pallet-version`
	/// digest, so this runtime must not report a version below the activation one.
	mod consensus_engine_activation {
		use crate::VERSION;
		use midnight_primitives_consensus_engine::ACTIVATION_SPEC_VERSION;

		#[test]
		fn this_runtime_is_at_or_past_the_activation_version() {
			const {
				assert!(
					VERSION.spec_version >= ACTIVATION_SPEC_VERSION,
					"pallet-consensus-engine is in this runtime, so its spec_version must not be \
					 below ACTIVATION_SPEC_VERSION",
				)
			};
		}
	}

	/// The slot duration reported by `SlotApi` comes from AURA's config, so it must not
	/// diverge from BABE's once BABE produces the blocks.
	mod slot_config {
		use crate::{Block, Runtime};

		fn slot_config() -> sidechain_slots::ScSlotConfig {
			<Runtime as sidechain_slots::runtime_decl_for_slot_api::SlotApi<Block>>::slot_config()
		}

		#[test]
		fn slot_duration_is_the_same_for_both_engines() {
			sp_io::TestExternalities::default().execute_with(|| {
				// Both are config constants — AURA's `SlotDuration` and BABE's
				// `MinimumPeriod * 2` — so `slot_config` needs no engine dispatch as
				// long as they agree. This test fails if a future config change makes
				// them diverge.
				assert_eq!(
					crate::Aura::slot_duration(),
					crate::Babe::slot_duration(),
					"AURA and BABE slot durations diverged; SlotApi::slot_config must \
					 dispatch on the active engine"
				);
				assert_eq!(slot_config().slot_duration.as_millis(), crate::SLOT_DURATION);
			});
		}
	}

	mod failed_mbm_recovery {
		use crate::{AccountId, BlockNumber, Executive, Runtime, RuntimeCall, VERSION};
		use frame_support::{
			assert_ok, dispatch::GetDispatchInfo, migrations::MultiStepMigrator, traits::Contains,
		};
		use parity_scale_codec::Encode;
		use sp_runtime::{
			BuildStorage, ExtrinsicInclusionMode,
			traits::{Dispatchable, Hash as _, Header as _},
		};

		fn ongoing() -> bool {
			<Runtime as frame_system::Config>::MultiBlockMigrator::ongoing()
		}

		fn can_set_code() -> frame_system::CanSetCodeResult<Runtime> {
			frame_system::Pallet::<Runtime>::can_set_code(&[], false)
		}

		/// Fail migration 0 (cnight v1 `MigrateV0ToV1`) by planting an active cursor whose
		/// 1-byte inner cursor fails to SCALE-decode as the migration's fixed-size cursor
		/// type -> `InvalidCursor` -> `FailedMigrationHandler`, then step the MBMs the way
		/// Executive does after inherent application. Leaves the chain in safe mode with
		/// the cursor unstuck.
		fn fail_mbm_and_enter_safe_mode() {
			// Mark the runtime as already upgraded so a later `initialize_block` doesn't
			// onboard the MBMs again.
			frame_system::LastRuntimeUpgrade::<Runtime>::put(
				frame_system::LastRuntimeUpgradeInfo::from(VERSION),
			);
			frame_system::Pallet::<Runtime>::set_block_number(1);
			assert!(can_set_code().into_result().is_ok());

			assert_ok!(crate::MultiBlockMigrations::force_set_active_cursor(
				crate::RuntimeOrigin::root(),
				0,
				Some(vec![0u8].try_into().unwrap()),
				Some(1),
			));
			assert!(ongoing());
			assert!(matches!(
				can_set_code(),
				frame_system::CanSetCodeResult::MultiBlockMigrationsOngoing
			));

			Executive::inherents_applied();

			// Safe mode entered indefinitely, cursor unstuck, upgrades unblocked.
			assert_eq!(pallet_safe_mode::EnteredUntil::<Runtime>::get(), Some(BlockNumber::MAX));
			assert!(!ongoing());
			assert!(can_set_code().into_result().is_ok());
		}

		/// A failed multi-block migration must enter safe mode and unstuck the cursor
		/// instead of freezing the chain (`can_set_code` blocked forever with no on-chain
		/// recovery on a standalone chain).
		#[test]
		fn failed_mbm_enters_safe_mode_and_unstucks() {
			let t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
			sp_io::TestExternalities::from(t).execute_with(|| {
				fail_mbm_and_enter_safe_mode();

				// `BaseCallFilter` now blocks user calls...
				type Filter = <Runtime as frame_system::Config>::BaseCallFilter;
				assert!(!Filter::contains(&RuntimeCall::Midnight(
					pallet_midnight::Call::send_mn_transaction { midnight_tx: vec![] }
				)));
				// ...but keeps the inherents, governance, and safe-mode recovery calls.
				assert!(Filter::contains(&RuntimeCall::Timestamp(pallet_timestamp::Call::set {
					now: 0
				})));
				assert!(Filter::contains(&RuntimeCall::Council(pallet_collective::Call::vote {
					proposal: crate::Hash::default(),
					index: 0,
					approve: true,
				})));
				assert!(Filter::contains(&RuntimeCall::SafeMode(
					pallet_safe_mode::Call::force_exit {}
				)));

				// The next block admits normal (non-inherent) extrinsics again. Like any
				// AURA block it must carry the AURA pre-digest followed by the matching BABE
				// `SecondaryPlain` one, which `pallet-consensus-engine` checks against the
				// AURA authority set (`authority_index == slot % n`).
				let authority = sp_consensus_aura::sr25519::AuthorityId::from(
					sp_core::sr25519::Public::from_raw([1u8; 32]),
				);
				pallet_aura::Authorities::<Runtime>::put(frame_support::BoundedVec::truncate_from(
					vec![authority],
				));
				let slot = sp_consensus_slots::Slot::from(1);
				let digest = sp_runtime::Digest {
					logs: vec![
						sp_runtime::DigestItem::PreRuntime(
							sp_consensus_aura::AURA_ENGINE_ID,
							slot.encode(),
						),
						sp_runtime::DigestItem::PreRuntime(
							sp_consensus_babe::BABE_ENGINE_ID,
							sp_consensus_babe::digests::PreDigest::SecondaryPlain(
								sp_consensus_babe::digests::SecondaryPlainPreDigest {
									authority_index: 0,
									slot,
								},
							)
							.encode(),
						),
					],
				};
				let header = crate::Header::new(
					2,
					Default::default(),
					Default::default(),
					frame_system::Pallet::<Runtime>::parent_hash(),
					digest,
				);
				assert_eq!(
					Executive::initialize_block(&header),
					ExtrinsicInclusionMode::AllExtrinsics
				);
			});
		}

		fn account(b: u8) -> AccountId {
			AccountId::from([b; 32])
		}

		/// Dispatch `call` the way an applied extrinsic would: through the signed origin,
		/// which carries `BaseCallFilter`. This is what makes the test end-to-end — a call
		/// filtered in safe mode fails here with `CallFiltered`.
		fn dispatch_signed(call: RuntimeCall, who: AccountId) {
			assert_ok!(call.dispatch(crate::RuntimeOrigin::signed(who)));
		}

		/// Governance must be able to execute the full set-code recovery flow while the
		/// chain sits in post-failure safe mode: both collectives approve a
		/// `FederatedAuthority` motion, and closing the motion dispatches
		/// `System::authorize_upgrade` as Root (the fixed runtime is then applied via the
		/// whitelisted `System::apply_authorized_upgrade`). The collectives dispatch
		/// `motion_approve` internally with their (non-Root) `Members` origin, which goes
		/// through `BaseCallFilter` — the safe-mode whitelist must let it through or
		/// recovery dead-ends.
		#[test]
		fn governance_can_authorize_set_code_in_safe_mode() {
			let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
			// Seed both governance bodies (membership genesis forwards to the collectives).
			pallet_membership::GenesisConfig::<Runtime, pallet_membership::Instance1> {
				members: vec![account(1), account(2)].try_into().unwrap(),
				..Default::default()
			}
			.assimilate_storage(&mut t)
			.unwrap();
			pallet_membership::GenesisConfig::<Runtime, pallet_membership::Instance2> {
				members: vec![account(1), account(2)].try_into().unwrap(),
				..Default::default()
			}
			.assimilate_storage(&mut t)
			.unwrap();

			sp_io::TestExternalities::from(t).execute_with(|| {
				fail_mbm_and_enter_safe_mode();

				// The motion governance wants dispatched as Root: authorize the fixed
				// runtime's code hash.
				let motion = RuntimeCall::System(frame_system::Call::authorize_upgrade {
					code_hash: crate::Hash::repeat_byte(7),
				});
				let motion_hash = <Runtime as frame_system::Config>::Hashing::hash_of(&motion);
				assert!(frame_system::Pallet::<Runtime>::authorized_upgrade().is_none());

				// Council: propose motion_approve, both members vote aye, close dispatches
				// it with the Council's `Members` origin.
				let proposal = RuntimeCall::FederatedAuthority(
					pallet_federated_authority::Call::motion_approve {
						call: alloc::boxed::Box::new(motion.clone()),
					},
				);
				let len = proposal.encoded_size() as u32;
				let weight = proposal.get_dispatch_info().call_weight;
				let hash = <Runtime as frame_system::Config>::Hashing::hash_of(&proposal);

				dispatch_signed(
					RuntimeCall::Council(pallet_collective::Call::propose {
						threshold: 2,
						proposal: alloc::boxed::Box::new(proposal.clone()),
						length_bound: len,
					}),
					account(1),
				);
				for who in [account(1), account(2)] {
					dispatch_signed(
						RuntimeCall::Council(pallet_collective::Call::vote {
							proposal: hash,
							index: 0,
							approve: true,
						}),
						who,
					);
				}
				dispatch_signed(
					RuntimeCall::Council(pallet_collective::Call::close {
						proposal_hash: hash,
						index: 0,
						proposal_weight_bound: weight,
						length_bound: len,
					}),
					account(1),
				);
				// The inner motion_approve must have landed (do_approve_proposal swallows a
				// filtered dispatch into an event, so check the motion actually exists).
				assert!(
					pallet_federated_authority::Motions::<Runtime>::get(motion_hash).is_some(),
					"Council's motion_approve was not executed — blocked by BaseCallFilter?"
				);

				// Technical Committee: same flow.
				dispatch_signed(
					RuntimeCall::TechnicalCommittee(pallet_collective::Call::propose {
						threshold: 2,
						proposal: alloc::boxed::Box::new(proposal.clone()),
						length_bound: len,
					}),
					account(1),
				);
				for who in [account(1), account(2)] {
					dispatch_signed(
						RuntimeCall::TechnicalCommittee(pallet_collective::Call::vote {
							proposal: hash,
							index: 0,
							approve: true,
						}),
						who,
					);
				}
				dispatch_signed(
					RuntimeCall::TechnicalCommittee(pallet_collective::Call::close {
						proposal_hash: hash,
						index: 0,
						proposal_weight_bound: weight,
						length_bound: len,
					}),
					account(1),
				);

				// Both bodies approved: closing the motion dispatches it as Root.
				dispatch_signed(
					RuntimeCall::FederatedAuthority(
						pallet_federated_authority::Call::motion_close {
							motion_hash,
							proposal_weight_bound: motion.get_dispatch_info().call_weight,
						},
					),
					account(1),
				);
				assert!(
					frame_system::Pallet::<Runtime>::authorized_upgrade().is_some(),
					"authorize_upgrade did not execute — motion dispatch failed"
				);

				// And governance can lift safe mode once the fixed runtime is applied.
				assert_ok!(crate::SafeMode::force_exit(crate::RuntimeOrigin::root()));
				assert_eq!(pallet_safe_mode::EnteredUntil::<Runtime>::get(), None);
			});
		}
	}
}
