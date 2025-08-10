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

use crate::{AuraId, Hash, Runtime, RuntimeEvent};
use frame_support::{
	dispatch::{DispatchInfo, DispatchResult, Pays},
	pallet_prelude::BoundedVec,
};
use frame_system::EventRecord;
use hex::encode;
use pallet_midnight::pallet::{
	CallDetails, ClaimMintDetails, DeploymentDetails, MaintainDetails, PayoutDetails,
	TxAppliedDetails,
};
use pallet_scheduler::TaskAddress;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, Error};
use sp_core::{ByteArray, H256, crypto::AccountId32};
use sp_runtime::{DispatchError, serde::Serialize};

#[derive(Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, Debug, Serialize)]
pub enum DecodeEventsError {
	#[codec(index = 0)]
	CouldNotDecodeBytes,
}

pub type DecodeEventsResult = Result<Vec<EventType>, DecodeEventsError>;

#[derive(Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, Debug, Serialize)]
pub enum Phase {
	ApplyExtrinsic(u32),
	Finalization,
	Initialization,
}

#[derive(Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, Debug, Serialize)]
pub struct Topics {
	topics: Vec<String>,
}

#[derive(Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, Debug, Serialize)]
pub struct Account {
	id: String,
}

#[derive(Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, Debug, Serialize)]
pub enum DispatchClass {
	Normal,
	Operational,
	Mandatory,
}

#[derive(Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, Debug, Serialize)]
pub struct ErrorMsg {
	msg: String,
}

#[derive(Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, Debug, Serialize)]
pub enum EventType {
	#[codec(index = 0)]
	MidnightDeployContract { phase: Phase, topics: Topics, address: String, tx_hash: String },
	#[codec(index = 1)]
	MidnightCallContract { phase: Phase, topics: Topics, address: String, tx_hash: String },
	#[codec(index = 2)]
	MidnightOnlyGuaranteedTxApplied { phase: Phase, topics: Topics, tx_hash: String },
	#[codec(index = 3)]
	MidnightTxApplied { phase: Phase, topics: Topics, tx_hash: String },
	#[codec(index = 4)]
	ExtrinsicSuccess {
		phase: Phase,
		topics: Topics,
		dispatch_class: DispatchClass,
		ref_time: u64,
		proof_size: u64,
		pay_fees: bool,
	},
	#[codec(index = 5)]
	ExtrinsicFail {
		phase: Phase,
		topics: Topics,
		dispatch_class: DispatchClass,
		error: ErrorMsg,
		ref_time: u64,
		proof_size: u64,
		pay_fees: bool,
	},
	#[codec(index = 6)]
	SystemCodeUpdated { phase: Phase, topics: Topics },
	#[codec(index = 7)]
	SystemNewAccount { account_id: Account, phase: Phase, topics: Topics },
	#[codec(index = 8)]
	SystemKilledAccount { account_id: Account, phase: Phase, topics: Topics },
	#[codec(index = 9)]
	SystemRemarked { sender_id: Account, remarks_hash: String, phase: Phase, topics: Topics },
	#[codec(index = 10)]
	GrandpaEvent { phase: Phase, topics: Topics },
	#[codec(index = 11)]
	SessionCommitteeManagement {},
	#[codec(index = 12)]
	Session {},
	#[codec(index = 13)]
	BalanceEvent {},
	#[codec(index = 14)]
	Sudo {},
	#[codec(index = 15)]
	RuntimeUpgrade { phase: Phase, topics: Topics, spec_version: u32, runtime_hash: H256 },
	#[codec(index = 16)]
	Preimage {},
	#[codec(index = 17)]
	CouldNotUpgrade {},
	#[codec(index = 18)]
	NoConsensusOnUpgrade {},
	#[codec(index = 19)]
	CouldNotScheduleRuntimeUpgrade,
	#[codec(index = 20)]
	NoUpgradePreimageMissing {},
	#[codec(index = 21)]
	UpgradeNoVotes {},
	#[codec(index = 22)]
	UpgradeVotedOn { voter: AuraId, spec_version: u32, runtime_hash: H256 },
	#[codec(index = 23)]
	NoUpgradePreimageNotRequested,
	#[codec(index = 24)]
	UpgradeStarted { migrations: u32 },
	#[codec(index = 25)]
	UpgradeCompleted,
	#[codec(index = 26)]
	UpgradeFailed,
	#[codec(index = 27)]
	MigrationSkipped { index: u32 },
	#[codec(index = 28)]
	MigrationAdvanced { index: u32, took: crate::BlockNumber },
	#[codec(index = 29)]
	MigrationCompleted { index: u32, took: crate::BlockNumber },
	#[codec(index = 30)]
	MigrationFailed { index: u32, took: crate::BlockNumber },
	#[codec(index = 31)]
	HistoricCleared { next_cursor: Option<Vec<u8>> },
	#[codec(index = 32)]
	MidnightMaintainContract { phase: Phase, topics: Topics, address: String, tx_hash: String },
	#[codec(index = 33)]
	MidnightClaimMint {
		phase: Phase,
		topics: Topics,
		coin_type: String,
		value: u128,
		tx_hash: String,
	},
	#[codec(index = 34)]
	MidnightPayoutMinted { phase: Phase, topics: Topics, amount: u128, receiver: String },
	#[codec(index = 35)]
	Scheduled { when: crate::BlockNumber, index: u32 },
	#[codec(index = 36)]
	Canceled { index: u32, when: crate::BlockNumber },
	#[codec(index = 37)]
	Dispatched {
		task: TaskAddress<crate::BlockNumber>,
		id: Option<[u8; 32]>,
		result: DispatchResult,
	},
	#[codec(index = 38)]
	RetrySet {
		task: TaskAddress<crate::BlockNumber>,
		id: Option<[u8; 32]>,
		retries: u8,
		period: crate::BlockNumber,
	},
	#[codec(index = 39)]
	RetryCancelled { task: TaskAddress<crate::BlockNumber>, id: Option<[u8; 32]> },
	#[codec(index = 40)]
	CallUnavailable { task: TaskAddress<crate::BlockNumber>, id: Option<[u8; 32]> },
	#[codec(index = 41)]
	PeriodicFailed { task: TaskAddress<crate::BlockNumber>, id: Option<[u8; 32]> },
	#[codec(index = 42)]
	RetryFailed { task: TaskAddress<crate::BlockNumber>, id: Option<[u8; 32]> },
	#[codec(index = 43)]
	PermanentlyOverweight { task: TaskAddress<crate::BlockNumber>, id: Option<[u8; 32]> },
	#[codec(index = 44)]
	UpgradeCallTooLarge,
	#[codec(index = 45)]
	PalletSession,
	#[codec(index = 46)]
	TxPause {
		full_name: (
			BoundedVec<u8, <Runtime as pallet_tx_pause::Config>::MaxNameLen>,
			BoundedVec<u8, <Runtime as pallet_tx_pause::Config>::MaxNameLen>,
		),
	},
	#[codec(index = 47)]
	UnknownEvent { phase: Phase, topics: Topics },
}

impl From<&AccountId32> for Account {
	fn from(value: &AccountId32) -> Self {
		Account { id: encode(value.as_slice()) }
	}
}

impl From<&frame_support::dispatch::DispatchClass> for DispatchClass {
	fn from(value: &frame_support::dispatch::DispatchClass) -> Self {
		match value {
			frame_support::dispatch::DispatchClass::Normal => DispatchClass::Normal,
			frame_support::dispatch::DispatchClass::Operational => DispatchClass::Operational,
			frame_support::dispatch::DispatchClass::Mandatory => DispatchClass::Mandatory,
		}
	}
}

impl From<&DispatchError> for ErrorMsg {
	fn from(value: &DispatchError) -> Self {
		match value {
			DispatchError::Other(err) => {
				ErrorMsg { msg: format!("Extrinsic dispatch error: other: {}", err) }
			},
			DispatchError::CannotLookup => {
				ErrorMsg { msg: String::from("Extrinsic dispatch error: fail to lookup data") }
			},
			DispatchError::BadOrigin => {
				ErrorMsg { msg: String::from("Extrinsic dispatch error: bad extrinsic origin") }
			},
			DispatchError::Module(err) => ErrorMsg {
				msg: format!("Extrinsic dispatch error: module related error {:?}", err.message),
			},
			DispatchError::ConsumerRemaining => {
				ErrorMsg { msg: String::from("Extrinsic dispatch error: cannot destroy account") }
			},
			DispatchError::NoProviders => {
				ErrorMsg { msg: String::from("Extrinsic dispatch error: cannot create account") }
			},
			DispatchError::TooManyConsumers => {
				ErrorMsg { msg: String::from("Extrinsic dispatch error: cannot create account") }
			},
			DispatchError::Token(err) => ErrorMsg {
				msg: format!(
					"Extrinsic dispatch error: unable to complete token related operation: {:?}",
					err
				),
			},
			DispatchError::Arithmetic(err) => ErrorMsg {
				msg: format!("Extrinsic dispatch error: because of arithmetic error {:?}", err),
			},
			DispatchError::Transactional(err) => ErrorMsg {
				msg: format!(
					"Extrinsic dispatch error: unable to execute storage transaction {:?}",
					err
				),
			},
			DispatchError::Exhausted => {
				ErrorMsg { msg: String::from("Extrinsic dispatch error: exhausted resources") }
			},
			DispatchError::Corruption => ErrorMsg {
				msg: String::from(
					"Extrinsic dispatch error: corrupted node state, external fix required",
				),
			},
			DispatchError::Unavailable => ErrorMsg {
				msg: String::from("Extrinsic dispatch error: unavailable resource try later"),
			},
			DispatchError::RootNotAllowed => {
				ErrorMsg { msg: String::from("Extrinsic can't be called with root origin.") }
			},
		}
	}
}

impl From<&frame_system::Phase> for Phase {
	fn from(value: &frame_system::Phase) -> Self {
		match value {
			frame_system::Phase::ApplyExtrinsic(ex) => Phase::ApplyExtrinsic(*ex),
			frame_system::Phase::Finalization => Phase::Finalization,
			frame_system::Phase::Initialization => Phase::Initialization,
		}
	}
}

impl From<&Vec<H256>> for Topics {
	fn from(value: &Vec<H256>) -> Self {
		let hashes: Vec<_> = value.iter().map(|h| encode(h.0)).collect();
		Topics { topics: hashes }
	}
}

struct PaysWrapper(Pays);
impl From<PaysWrapper> for bool {
	fn from(value: PaysWrapper) -> Self {
		match value {
			PaysWrapper(Pays::Yes) => true,
			PaysWrapper(Pays::No) => false,
		}
	}
}

impl From<Error> for DecodeEventsError {
	fn from(_value: Error) -> Self {
		DecodeEventsError::CouldNotDecodeBytes
	}
}

pub fn export_events(events: Vec<EventRecord<RuntimeEvent, Hash>>) -> DecodeEventsResult {
	type SystemEvent = frame_system::Event<Runtime>;
	type GrandpaEvent = pallet_grandpa::Event;
	type MidnightEvent = pallet_midnight::Event;
	type RuntimeUpgradeEvent = pallet_upgrade::Event<Runtime>;
	type MultiBlockMigrationEvent = pallet_migrations::Event<Runtime>;
	type SchedulerEvent = pallet_scheduler::Event<Runtime>;
	type TxPauseEvent = pallet_tx_pause::Event<Runtime>;

	let translated: Vec<EventType> = events
		.iter()
		.map(|event| match event {
			EventRecord {
				phase,
				event:
					RuntimeEvent::System(SystemEvent::ExtrinsicSuccess {
						dispatch_info: DispatchInfo { weight, class, pays_fee },
					}),
				topics,
			} => EventType::ExtrinsicSuccess {
				phase: phase.into(),
				topics: topics.into(),
				dispatch_class: class.into(),
				ref_time: weight.ref_time(),
				proof_size: weight.proof_size(),
				pay_fees: PaysWrapper(*pays_fee).into(),
			},
			EventRecord {
				phase,
				event:
					RuntimeEvent::System(SystemEvent::ExtrinsicFailed {
						dispatch_error: error,
						dispatch_info: DispatchInfo { weight, class, pays_fee },
					}),
				topics,
			} => EventType::ExtrinsicFail {
				phase: phase.into(),
				topics: topics.into(),
				dispatch_class: class.into(),
				error: error.into(),
				ref_time: weight.ref_time(),
				proof_size: weight.proof_size(),
				pay_fees: PaysWrapper(*pays_fee).into(),
			},
			EventRecord {
				phase,
				event: RuntimeEvent::System(SystemEvent::CodeUpdated),
				topics,
			} => EventType::SystemCodeUpdated { phase: phase.into(), topics: topics.into() },
			EventRecord {
				phase,
				event: RuntimeEvent::System(SystemEvent::NewAccount { account: id }),
				topics,
			} => EventType::SystemNewAccount {
				account_id: id.into(),
				phase: phase.into(),
				topics: topics.into(),
			},
			EventRecord {
				phase,
				event: RuntimeEvent::System(SystemEvent::KilledAccount { account: id }),
				topics,
			} => EventType::SystemKilledAccount {
				account_id: id.into(),
				phase: phase.into(),
				topics: topics.into(),
			},
			EventRecord {
				phase,
				event: RuntimeEvent::System(SystemEvent::Remarked { sender: id, hash: remark_hash }),
				topics,
			} => EventType::SystemRemarked {
				sender_id: id.into(),
				remarks_hash: encode(remark_hash.0),
				phase: phase.into(),
				topics: topics.into(),
			},

			EventRecord { phase, event: RuntimeEvent::System(_), topics } => {
				EventType::UnknownEvent { phase: phase.into(), topics: topics.into() }
			},

			EventRecord { phase, event: RuntimeEvent::Grandpa(GrandpaEvent::Paused), topics } => {
				EventType::GrandpaEvent { phase: phase.into(), topics: topics.into() }
			},
			EventRecord { phase, event: RuntimeEvent::Grandpa(GrandpaEvent::Resumed), topics } => {
				EventType::GrandpaEvent { phase: phase.into(), topics: topics.into() }
			},
			EventRecord {
				phase,
				event:
					RuntimeEvent::Grandpa(GrandpaEvent::NewAuthorities { authority_set: _authorities }),
				topics,
			} => EventType::GrandpaEvent { phase: phase.into(), topics: topics.into() },

			EventRecord {
				phase,
				event:
					RuntimeEvent::Midnight(MidnightEvent::ContractDeploy(DeploymentDetails {
						tx_hash: hash,
						contract_address: address,
					})),
				topics,
			} => EventType::MidnightDeployContract {
				phase: phase.into(),
				topics: topics.into(),
				address: to_hex(address),
				tx_hash: to_hex(hash),
			},
			EventRecord {
				phase,
				event:
					RuntimeEvent::Midnight(MidnightEvent::ContractMaintain(MaintainDetails {
						tx_hash: hash,
						contract_address: address,
					})),
				topics,
			} => EventType::MidnightMaintainContract {
				phase: phase.into(),
				topics: topics.into(),
				address: to_hex(address),
				tx_hash: to_hex(hash),
			},
			EventRecord {
				phase,
				event:
					RuntimeEvent::Midnight(MidnightEvent::ClaimMint(ClaimMintDetails {
						tx_hash: hash,
						coin_type,
						value,
					})),
				topics,
			} => EventType::MidnightClaimMint {
				phase: phase.into(),
				topics: topics.into(),
				coin_type: to_hex(coin_type),
				value: *value,
				tx_hash: to_hex(hash),
			},
			EventRecord {
				phase,
				event:
					RuntimeEvent::Midnight(MidnightEvent::PayoutMinted(PayoutDetails {
						amount,
						receiver,
					})),
				topics,
			} => EventType::MidnightPayoutMinted {
				phase: phase.into(),
				topics: topics.into(),
				amount: *amount,
				receiver: to_hex(receiver),
			},
			EventRecord {
				phase,
				event:
					RuntimeEvent::Midnight(MidnightEvent::ContractCall(CallDetails {
						tx_hash: hash,
						contract_address: address,
					})),
				topics,
			} => EventType::MidnightCallContract {
				phase: phase.into(),
				topics: topics.into(),
				address: to_hex(address),
				tx_hash: to_hex(hash),
			},
			EventRecord {
				phase,
				event:
					RuntimeEvent::Midnight(MidnightEvent::TxApplied(TxAppliedDetails { tx_hash: hash })),
				topics,
			} => EventType::MidnightTxApplied {
				phase: phase.into(),
				topics: topics.into(),
				tx_hash: to_hex(hash),
			},
			EventRecord {
				phase,
				event:
					RuntimeEvent::Midnight(MidnightEvent::TxOnlyGuaranteedApplied(TxAppliedDetails {
						tx_hash: hash,
					})),
				topics,
			} => EventType::MidnightOnlyGuaranteedTxApplied {
				phase: phase.into(),
				topics: topics.into(),
				tx_hash: to_hex(hash),
			},
			EventRecord { event: RuntimeEvent::SessionCommitteeManagement(_), .. } => {
				EventType::SessionCommitteeManagement {}
			},
			EventRecord { event: RuntimeEvent::Session(_), .. } => EventType::Session {},
			EventRecord { event: RuntimeEvent::Balances(_), .. } => EventType::BalanceEvent {},
			EventRecord { event: RuntimeEvent::Sudo(_), .. } => EventType::Sudo {},
			EventRecord {
				phase,
				event:
					RuntimeEvent::RuntimeUpgrade(RuntimeUpgradeEvent::UpgradeScheduled {
						runtime_hash,
						spec_version,
						..
					}),
				topics,
			} => EventType::RuntimeUpgrade {
				phase: phase.into(),
				topics: topics.into(),
				spec_version: *spec_version,
				runtime_hash: *runtime_hash,
			},
			EventRecord {
				event: RuntimeEvent::RuntimeUpgrade(RuntimeUpgradeEvent::NoConsensusOnUpgrade),
				..
			} => EventType::NoConsensusOnUpgrade {},
			EventRecord {
				event:
					RuntimeEvent::RuntimeUpgrade(RuntimeUpgradeEvent::CouldNotScheduleRuntimeUpgrade {
						..
					}),
				..
			} => EventType::CouldNotScheduleRuntimeUpgrade {},
			EventRecord {
				event:
					RuntimeEvent::RuntimeUpgrade(RuntimeUpgradeEvent::NoUpgradePreimageMissing {
						..
					}),
				..
			} => EventType::NoUpgradePreimageMissing {},
			EventRecord {
				event: RuntimeEvent::RuntimeUpgrade(RuntimeUpgradeEvent::NoVotes),
				..
			} => EventType::UpgradeNoVotes {},
			EventRecord {
				event: RuntimeEvent::RuntimeUpgrade(RuntimeUpgradeEvent::Voted { voter, target }),
				..
			} => EventType::UpgradeVotedOn {
				voter: voter.clone(),
				spec_version: target.spec_version,
				runtime_hash: target.runtime_hash,
			},
			EventRecord {
				event:
					RuntimeEvent::RuntimeUpgrade(RuntimeUpgradeEvent::NoUpgradePreimageNotRequested {
						..
					}),
				..
			} => EventType::NoUpgradePreimageNotRequested {},
			EventRecord {
				event: RuntimeEvent::RuntimeUpgrade(RuntimeUpgradeEvent::UpgradeCallTooLarge { .. }),
				..
			} => EventType::UpgradeCallTooLarge {},
			EventRecord {
				event: RuntimeEvent::Scheduler(pallet_scheduler::Event::__Ignore(_, _)),
				..
			} => unreachable!("__Ignore cannot be used"),
			EventRecord { event: RuntimeEvent::Preimage(_), .. } => EventType::Preimage {},
			EventRecord { event: RuntimeEvent::PalletSession(_), .. } => {
				EventType::PalletSession {}
			},
			EventRecord {
				event:
					RuntimeEvent::MultiBlockMigrations(MultiBlockMigrationEvent::UpgradeStarted {
						migrations,
					}),
				..
			} => EventType::UpgradeStarted { migrations: *migrations },
			EventRecord {
				event:
					RuntimeEvent::MultiBlockMigrations(MultiBlockMigrationEvent::UpgradeCompleted),
				..
			} => EventType::UpgradeCompleted,
			EventRecord {
				event: RuntimeEvent::MultiBlockMigrations(MultiBlockMigrationEvent::UpgradeFailed),
				..
			} => EventType::UpgradeFailed,
			EventRecord {
				event:
					RuntimeEvent::MultiBlockMigrations(MultiBlockMigrationEvent::MigrationSkipped {
						index,
					}),
				..
			} => EventType::MigrationSkipped { index: *index },
			EventRecord {
				event:
					RuntimeEvent::MultiBlockMigrations(MultiBlockMigrationEvent::MigrationAdvanced {
						index,
						took,
					}),
				..
			} => EventType::MigrationAdvanced { index: *index, took: *took },
			EventRecord {
				event:
					RuntimeEvent::MultiBlockMigrations(MultiBlockMigrationEvent::MigrationCompleted {
						index,
						took,
					}),
				..
			} => EventType::MigrationCompleted { index: *index, took: *took },
			EventRecord {
				event:
					RuntimeEvent::MultiBlockMigrations(MultiBlockMigrationEvent::MigrationFailed {
						index,
						took,
					}),
				..
			} => EventType::MigrationFailed { index: *index, took: *took },
			EventRecord {
				event:
					RuntimeEvent::MultiBlockMigrations(MultiBlockMigrationEvent::HistoricCleared {
						next_cursor,
					}),
				..
			} => EventType::HistoricCleared { next_cursor: next_cursor.clone() },
			EventRecord {
				event: RuntimeEvent::MultiBlockMigrations(MultiBlockMigrationEvent::__Ignore(_, _)),
				..
			} => unreachable!("__Ignore cannot be used"),
			EventRecord {
				event: RuntimeEvent::Scheduler(SchedulerEvent::Scheduled { when, index }),
				..
			} => EventType::Scheduled { when: *when, index: *index },
			EventRecord {
				event: RuntimeEvent::Scheduler(SchedulerEvent::Canceled { when, index }),
				..
			} => EventType::Canceled { when: *when, index: *index },
			EventRecord {
				event: RuntimeEvent::Scheduler(SchedulerEvent::Dispatched { task, id, result }),
				..
			} => EventType::Dispatched { task: *task, id: *id, result: *result },
			EventRecord {
				event:
					RuntimeEvent::Scheduler(SchedulerEvent::RetrySet { task, id, period, retries }),
				..
			} => EventType::RetrySet { period: *period, retries: *retries, id: *id, task: *task },

			EventRecord {
				event: RuntimeEvent::Scheduler(SchedulerEvent::RetryCancelled { task, id }),
				..
			} => EventType::RetryCancelled { id: *id, task: *task },
			EventRecord {
				event: RuntimeEvent::Scheduler(SchedulerEvent::CallUnavailable { task, id }),
				..
			} => EventType::CallUnavailable { id: *id, task: *task },
			EventRecord {
				event: RuntimeEvent::Scheduler(SchedulerEvent::PeriodicFailed { task, id }),
				..
			} => EventType::PeriodicFailed { id: *id, task: *task },
			EventRecord {
				event: RuntimeEvent::Scheduler(SchedulerEvent::RetryFailed { task, id }),
				..
			} => EventType::RetryFailed { id: *id, task: *task },
			EventRecord {
				event: RuntimeEvent::Scheduler(SchedulerEvent::PermanentlyOverweight { task, id }),
				..
			} => EventType::PermanentlyOverweight { task: *task, id: *id },
			EventRecord {
				event: RuntimeEvent::TxPause(TxPauseEvent::CallPaused { full_name }),
				..
			} => EventType::TxPause { full_name: full_name.clone() },
			EventRecord {
				event: RuntimeEvent::TxPause(TxPauseEvent::CallUnpaused { full_name }),
				..
			} => EventType::TxPause { full_name: full_name.clone() },
			&EventRecord {
				event: RuntimeEvent::TxPause(pallet_tx_pause::Event::__Ignore(_, _)),
				..
			} => unimplemented!(),
			&EventRecord {
				event: RuntimeEvent::RuntimeUpgrade(pallet_upgrade::Event::__Ignore(_, _)),
				..
			} => unimplemented!(),
			&EventRecord { event: RuntimeEvent::NativeTokenManagement(_), .. } => unimplemented!(),
			&EventRecord { event: RuntimeEvent::NativeTokenObservation(_), .. } => unimplemented!(),
		})
		.collect();

	Ok(translated)
}

fn to_hex(value: impl AsRef<[u8]>) -> String {
	hex::encode(value)
}
