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

//! State-machine tests with an explicit verifier/ledger double. Host tests use the actual ledger.
use super::*;
use frame_support::{assert_noop, assert_ok, derive_impl};
use snowbridge_inbound_queue_primitives::{Log, Proof, VerificationError};
use sp_runtime::{BuildStorage, traits::TrailingZeroInput};
use std::cell::Cell;

frame_support::construct_runtime!(pub enum Test { System: frame_system, EthereumBridge: crate, EthereumBeaconClient: snowbridge_pallet_ethereum_client });
#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = frame_system::mocking::MockBlock<Test>;
}
thread_local! { static REAL_PROOF: Cell<bool> = const { Cell::new(false) }; static REJECT_PROOF: Cell<bool> = const { Cell::new(false) }; static FAIL_LEDGER: Cell<bool> = const { Cell::new(false) }; }
pub struct TestVerifier;
impl Verifier for TestVerifier {
	fn verify(log: &Log, proof: &Proof) -> Result<(), VerificationError> {
		if REAL_PROOF.get() {
			return EthereumBeaconClient::verify(log, proof);
		}
		if REJECT_PROOF.get() { Err(VerificationError::InvalidProof) } else { Ok(()) }
	}
}
pub struct TestLedger;
impl MidnightSystemTransactionEthereumExecutor for TestLedger {
	fn credit_ethereum_asset(credit: EthereumCredit) -> Result<[u8; 32], DispatchError> {
		sp_io::storage::set(b"test-issued", &credit.encode());
		if FAIL_LEDGER.get() { Err(DispatchError::Other("block full")) } else { Ok([9; 32]) }
	}
}
pub struct TestEscrow;
impl ValidateEscrow for TestEscrow {
	fn read(config: &EscrowConfig, sequence: u64) -> Result<Redemption, DispatchError> {
		Ok(Redemption {
			magic: *b"MNBR",
			version: 1,
			bridge_domain: config.bridge_domain,
			escrow: config.escrow,
			sequence,
			amount: 40,
			recipient: [5; 20],
			color: config.color,
			total_retired: 40,
			count: 1,
			ledger_balance: 40,
		})
	}
	fn validate(_: &EscrowConfig) -> DispatchResult {
		Ok(())
	}
}
pub struct TestQueue;
impl QueueRedemption for TestQueue {
	fn enqueue(_: &Registry, _: H256, _: &Redemption) -> DispatchResult {
		Ok(())
	}
}
impl Config for Test {
	type Verifier = TestVerifier;
	type Ledger = TestLedger;
	type EscrowValidator = TestEscrow;
	type Outbound = TestQueue;
}
fn run(f: impl FnOnce()) {
	REAL_PROOF.set(false);
	REJECT_PROOF.set(false);
	FAIL_LEDGER.set(false);
	sp_io::TestExternalities::new(
		frame_system::GenesisConfig::<Test>::default().build_storage().unwrap(),
	)
	.execute_with(|| {
		System::set_block_number(1);
		f();
	});
}
fn log(index: usize) -> Log {
	let data: serde_json::Value =
		serde_json::from_str(include_str!("../../../tests/snowbridge/fixtures/deposit-logs.json"))
			.unwrap();
	let item = &data[index];
	let bytes = |s: &str| hex::decode(s.trim_start_matches("0x")).unwrap();
	Log {
		address: H160::from_slice(&bytes(item["address"].as_str().unwrap())),
		topics: item["topics"]
			.as_array()
			.unwrap()
			.iter()
			.map(|v| H256::from_slice(&bytes(v.as_str().unwrap())))
			.collect(),
		data: bytes(item["data"].as_str().unwrap()),
		tx_index: u64::from_str_radix(
			item["transactionIndex"].as_str().unwrap().trim_start_matches("0x"),
			16,
		)
		.unwrap(),
	}
}
fn event(index: usize) -> Box<EventProof> {
	Box::new(EventProof {
		event_log: log(index),
		proof: Decode::decode(&mut TrailingZeroInput::zeroes()).unwrap(),
	})
}
fn registry() -> Registry {
	let message = Message::try_from(&log(1)).unwrap();
	let Payload::Raw(envelope) = message.payload else { panic!() };
	let EthereumAsset::NativeTokenERC20 { token_id, .. } = message.assets[0] else { panic!() };
	let mut config = Registry {
		gateway: message.gateway,
		token: token_id,
		escrow: EscrowConfig {
			bridge_domain: envelope[32..64].try_into().unwrap(),
			token_domain: [0; 32],
			escrow: [4; 32],
			color: [42; 32],
			retire_operation: [43; 32],
		},
	};
	config.escrow.token_domain = abi_hash(&[word(b"MN/SB/COLOR/V1"), asset_id(&config)]);
	config
}
fn activate() -> Registry {
	let config = registry();
	assert_ok!(EthereumBridge::activate(RuntimeOrigin::root(), config.clone()));
	config
}

#[test]
fn activation_is_root_only_and_immutable() {
	run(|| {
		let config = registry();
		assert_noop!(
			EthereumBridge::activate(RuntimeOrigin::signed(1), config.clone()),
			sp_runtime::DispatchError::BadOrigin
		);
		let mut wrong = config.clone();
		wrong.escrow.token_domain[0] ^= 1;
		assert_noop!(
			EthereumBridge::activate(RuntimeOrigin::root(), wrong),
			Error::<Test>::Configuration
		);
		activate();
		assert_noop!(
			EthereumBridge::activate(RuntimeOrigin::root(), config),
			Error::<Test>::AlreadyActive
		);
	});
}

#[test]
fn verified_registration_deposit_and_at_most_once_materialization() {
	run(|| {
		let config = activate();
		assert_ok!(EthereumBridge::submit_deposit(RuntimeOrigin::signed(2), event(0)));
		assert!(Registered::<Test>::get());
		assert_eq!(Deposits::<Test>::iter().count(), 0);
		assert_ok!(EthereumBridge::submit_deposit(RuntimeOrigin::signed(3), event(1)));
		let message = Message::try_from(&log(1)).unwrap();
		let id = H256(deposit_id(&config, message.nonce));
		let pending = Deposits::<Test>::get(id).unwrap();
		assert_eq!(pending.credit.amount, 100);
		assert_eq!(pending.credit.recipient, uint(7));
		assert!(!pending.issued);
		assert_ok!(EthereumBridge::materialize_deposit(RuntimeOrigin::signed(4), id));
		assert!(Deposits::<Test>::get(id).unwrap().issued);
		assert_noop!(
			EthereumBridge::materialize_deposit(RuntimeOrigin::signed(5), id),
			Error::<Test>::AlreadyIssued
		);
		assert_noop!(
			EthereumBridge::submit_deposit(RuntimeOrigin::signed(6), event(1)),
			Error::<Test>::AlreadySeen
		);
		assert_eq!(MessageCount::<Test>::get(), 2);
	});
}

#[test]
fn invalid_proofs_and_wrong_gateway_cannot_create_pending_deposits() {
	run(|| {
		activate();
		REJECT_PROOF.set(true);
		assert_noop!(
			EthereumBridge::submit_deposit(RuntimeOrigin::signed(2), event(1)),
			Error::<Test>::InvalidProof
		);
		REJECT_PROOF.set(false);
		let mut wrong = event(1);
		wrong.event_log.address = H160::repeat_byte(88);
		assert_noop!(
			EthereumBridge::submit_deposit(RuntimeOrigin::signed(2), wrong),
			Error::<Test>::WrongGateway
		);
		assert_eq!(MessageCount::<Test>::get(), 0);
		assert_eq!(Deposits::<Test>::iter().count(), 0);
	});
}

#[test]
fn failed_materialization_rolls_back_all_writes_and_remains_retryable() {
	run(|| {
		let config = activate();
		assert_ok!(EthereumBridge::submit_deposit(RuntimeOrigin::signed(2), event(1)));
		let id = H256(deposit_id(&config, Message::try_from(&log(1)).unwrap().nonce));
		let events = System::events();
		FAIL_LEDGER.set(true);
		assert_noop!(
			EthereumBridge::materialize_deposit(RuntimeOrigin::signed(3), id),
			DispatchError::Other("block full")
		);
		assert!(sp_io::storage::get(b"test-issued").is_none());
		assert!(!Deposits::<Test>::get(id).unwrap().issued);
		assert_eq!(System::events(), events);
		FAIL_LEDGER.set(false);
		assert_ok!(EthereumBridge::materialize_deposit(RuntimeOrigin::signed(4), id));
	});
}

#[test]
fn unsupported_authenticated_asset_is_quarantined_and_capacity_is_bounded() {
	run(|| {
		let mut config = registry();
		config.token = H160::repeat_byte(88);
		config.escrow.token_domain = abi_hash(&[word(b"MN/SB/COLOR/V1"), asset_id(&config)]);
		assert_ok!(EthereumBridge::activate(RuntimeOrigin::root(), config));
		assert_ok!(EthereumBridge::submit_deposit(RuntimeOrigin::signed(2), event(1)));
		let nonce = Message::try_from(&log(1)).unwrap().nonce;
		assert_eq!(Quarantine::<Test>::get(nonce).unwrap().1, QuarantineReason::Asset);
		assert_eq!(Deposits::<Test>::iter().count(), 0);
		assert_noop!(
			EthereumBridge::submit_deposit(RuntimeOrigin::signed(2), event(1)),
			Error::<Test>::AlreadySeen
		);
		MessageCount::<Test>::put(MAX_MESSAGES);
		assert_noop!(
			EthereumBridge::submit_deposit(RuntimeOrigin::signed(2), event(0)),
			Error::<Test>::Capacity
		);
	});
}

#[test]
fn deposit_capacity_is_independent_of_registration_delivery() {
	run(|| {
		activate();
		DepositCount::<Test>::put(64);
		assert_noop!(
			EthereumBridge::submit_deposit(RuntimeOrigin::signed(2), event(1)),
			Error::<Test>::Capacity
		);
		assert_eq!(MessageCount::<Test>::get(), 0);
		assert!(Seen::<Test>::iter().next().is_none());
		assert_ok!(EthereumBridge::submit_deposit(RuntimeOrigin::signed(2), event(0)));
		assert!(Registered::<Test>::get());
	});
}

frame_support::parameter_types! {
	pub const ForkVersions: snowbridge_beacon_primitives::ForkVersions = snowbridge_beacon_primitives::ForkVersions {
		genesis: snowbridge_beacon_primitives::Fork { version: [0,0,0,0], epoch: 0 },
		altair: snowbridge_beacon_primitives::Fork { version: [1,0,0,0], epoch: 0 },
		bellatrix: snowbridge_beacon_primitives::Fork { version: [2,0,0,0], epoch: 0 },
		capella: snowbridge_beacon_primitives::Fork { version: [3,0,0,0], epoch: 0 },
		deneb: snowbridge_beacon_primitives::Fork { version: [4,0,0,0], epoch: 0 },
		electra: snowbridge_beacon_primitives::Fork { version: [5,0,0,0], epoch: 0 },
		fulu: snowbridge_beacon_primitives::Fork { version: [6,0,0,0], epoch: 100_000_000 },
	};
}
impl snowbridge_pallet_ethereum_client::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type ForkVersions = ForkVersions;
	type FreeHeadersInterval = frame_support::traits::ConstU32<32>;
	type WeightInfo = ();
}

/// Pinned SDK cryptographic vectors, not receipts from the independent live Anvil run.
#[test]
fn real_beacon_client_checks_bls_updates_and_receipt_proofs() {
	run(|| {
		use snowbridge_pallet_ethereum_client_fixtures as fixtures;
		assert_ok!(EthereumBeaconClient::force_checkpoint(
			RuntimeOrigin::root(),
			fixtures::make_checkpoint()
		));
		assert_ok!(EthereumBeaconClient::submit(
			RuntimeOrigin::signed(1),
			fixtures::make_sync_committee_update()
		));
		assert_ok!(EthereumBeaconClient::submit(
			RuntimeOrigin::signed(1),
			fixtures::make_finalized_header_update()
		));
		let fixture = fixtures::make_inbound_fixture();
		assert_ok!(EthereumBeaconClient::verify(&fixture.event.event_log, &fixture.event.proof));
		let mut forged = fixture.event;
		forged.event_log.data[0] ^= 1;
		assert!(EthereumBeaconClient::verify(&forged.event_log, &forged.proof).is_err());
	});
}

#[test]
fn real_verifier_rejects_anvil_logs_without_finalized_receipt_proof() {
	run(|| {
		activate();
		REAL_PROOF.set(true);
		assert_noop!(
			EthereumBridge::submit_deposit(RuntimeOrigin::signed(2), event(1)),
			Error::<Test>::InvalidProof
		);
		assert_eq!(MessageCount::<Test>::get(), 0);
		assert!(Deposits::<Test>::iter().next().is_none());
	});
}

/// Consume proofs freshly generated from the owned Geth/Lodestar network.
/// The verifier is real; this test still substitutes escrow activation and issuance.
#[test]
#[ignore = "requires SNOWBRIDGE_POS_PROOFS from the live PoS harness"]
fn live_pos_receipt_is_authenticated_before_deposit_admission() {
	use snowbridge_pallet_ethereum_client::types::{CheckpointUpdate, Update};
	use std::path::PathBuf;
	let dir =
		PathBuf::from(std::env::var("SNOWBRIDGE_POS_PROOFS").expect("live PoS proof directory"));
	let read = |path: &str| std::fs::read(dir.join(path)).unwrap();
	let decode = |data: Vec<u8>| {
		let mut input = data.as_slice();
		let event = EventProof::decode(&mut input).unwrap();
		assert!(input.is_empty(), "trailing SCALE bytes");
		event
	};
	let checkpoint =
		CheckpointUpdate::decode(&mut read("bootstrap/checkpoint.scale").as_slice()).unwrap();
	let update = Update::decode(&mut read("receipt/update.scale").as_slice()).unwrap();
	let event = decode(read("receipt/event.scale"));
	let message = Message::try_from(&event.event_log).unwrap();
	let Payload::Raw(envelope) = message.payload else { panic!("expected token deposit") };
	let EthereumAsset::NativeTokenERC20 { token_id, value } = message.assets[0] else {
		panic!("expected ERC20")
	};
	assert_eq!(value, 100u128);
	run(|| {
		REAL_PROOF.set(true);
		assert_ok!(EthereumBeaconClient::force_checkpoint(
			RuntimeOrigin::root(),
			Box::new(checkpoint)
		));
		assert_ok!(EthereumBeaconClient::submit(RuntimeOrigin::signed(1), Box::new(update)));
		assert_ok!(EthereumBeaconClient::verify(&event.event_log, &event.proof));
		let mut config = registry();
		config.gateway = event.event_log.address;
		config.token = token_id;
		config.escrow.bridge_domain = envelope[32..64].try_into().unwrap();
		config.escrow.token_domain = abi_hash(&[word(b"MN/SB/COLOR/V1"), asset_id(&config)]);
		assert_ok!(EthereumBridge::activate(RuntimeOrigin::root(), config.clone()));
		for corrupt_index in 0..3 {
			let mut forged = event.clone();
			match corrupt_index {
				0 => forged.event_log.data[0] ^= 1,
				1 => forged.event_log.tx_index += 1,
				_ => forged.proof.receipt_proof[0][0] ^= 1,
			}
			assert_noop!(
				EthereumBridge::submit_deposit(RuntimeOrigin::signed(2), Box::new(forged)),
				Error::<Test>::InvalidProof
			);
		}
		assert_ok!(EthereumBridge::submit_deposit(
			RuntimeOrigin::signed(2),
			Box::new(event.clone())
		));
		let id = H256(deposit_id(&config, message.nonce));
		assert_eq!(Deposits::<Test>::get(id).unwrap().credit.amount, 100);
		assert_noop!(
			EthereumBridge::submit_deposit(RuntimeOrigin::signed(2), Box::new(event)),
			Error::<Test>::AlreadySeen
		);
		assert_eq!(DepositCount::<Test>::get(), 1);
	});
}

fn receipt(nonce: u64, id: H256, success: bool) -> Box<EventProof> {
	Box::new(EventProof {
		event_log: Log {
			address: registry().gateway,
			topics: vec![
				H256(keccak_256(b"InboundMessageDispatched(uint64,bytes32,bool,bytes32)")),
				H256(uint(nonce.into())),
			],
			data: [id.0, uint(success as u128), [0; 32]].concat(),
			tx_index: 0,
		},
		proof: Decode::decode(&mut TrailingZeroInput::zeroes()).unwrap(),
	})
}
#[test]
fn retirement_retry_requires_authenticated_failure_and_never_changes_money_fields() {
	run(|| {
		let config = activate();
		let id = redemption_id(&config, 0);
		assert_ok!(EthereumBridge::export_redemption(RuntimeOrigin::signed(1), 0));
		let initial = Redemptions::<Test>::get(id).unwrap();
		assert_noop!(
			EthereumBridge::export_redemption(RuntimeOrigin::signed(1), 0),
			Error::<Test>::AlreadyExported
		);
		assert_noop!(
			EthereumBridge::retry_redemption(RuntimeOrigin::signed(1), id),
			Error::<Test>::NotRetryEligible
		);
		assert_ok!(EthereumBridge::accept(id, 1, H256::repeat_byte(9)));
		REJECT_PROOF.set(true);
		assert_noop!(
			EthereumBridge::submit_redemption_receipt(
				RuntimeOrigin::signed(1),
				receipt(1, id, false)
			),
			Error::<Test>::InvalidProof
		);
		REJECT_PROOF.set(false);
		assert_noop!(
			EthereumBridge::submit_redemption_receipt(
				RuntimeOrigin::signed(1),
				receipt(1, H256::zero(), false)
			),
			Error::<Test>::InvalidReceipt
		);
		assert_noop!(
			EthereumBridge::submit_redemption_receipt(
				RuntimeOrigin::signed(1),
				receipt(2, id, false)
			),
			Error::<Test>::InvalidReceipt
		);
		assert_ok!(EthereumBridge::submit_redemption_receipt(
			RuntimeOrigin::signed(1),
			receipt(1, id, false)
		));
		assert_ok!(EthereumBridge::retry_redemption(RuntimeOrigin::signed(1), id));
		assert_ok!(EthereumBridge::accept(id, 2, H256::repeat_byte(10)));
		assert_noop!(
			EthereumBridge::submit_redemption_receipt(
				RuntimeOrigin::signed(1),
				receipt(1, id, false)
			),
			Error::<Test>::AlreadySeen
		);
		assert_eq!(Redemptions::<Test>::get(id).unwrap().phase, RedemptionPhase::AwaitingResult(2));
		assert_ok!(EthereumBridge::submit_redemption_receipt(
			RuntimeOrigin::signed(1),
			receipt(2, id, true)
		));
		assert_noop!(
			EthereumBridge::retry_redemption(RuntimeOrigin::signed(1), id),
			Error::<Test>::NotRetryEligible
		);
		assert_eq!(Redemptions::<Test>::get(id).unwrap().record, initial.record);
		assert_eq!(Redemptions::<Test>::get(id).unwrap().phase, RedemptionPhase::Completed);
		assert_eq!(ByNonce::<Test>::get(1).unwrap().result, Some(false));
		assert_eq!(ByNonce::<Test>::get(2).unwrap().result, Some(true));
	});
}
#[test]
fn contradictory_authenticated_receipt_commits_a_permanent_halt() {
	run(|| {
		let config = activate();
		let id = redemption_id(&config, 0);
		assert_ok!(EthereumBridge::export_redemption(RuntimeOrigin::signed(1), 0));
		assert_ok!(EthereumBridge::accept(id, 1, H256::repeat_byte(9)));
		assert_ok!(EthereumBridge::submit_redemption_receipt(
			RuntimeOrigin::signed(1),
			receipt(1, id, false)
		));
		assert_ok!(EthereumBridge::submit_redemption_receipt(
			RuntimeOrigin::signed(1),
			receipt(1, id, true)
		));
		assert!(OutboundHalted::<Test>::get());
		assert_eq!(ByNonce::<Test>::get(1).unwrap().result, Some(false));
		assert_noop!(
			EthereumBridge::retry_redemption(RuntimeOrigin::signed(1), id),
			Error::<Test>::Halted
		);
		assert_noop!(
			EthereumBridge::export_redemption(RuntimeOrigin::signed(1), 1),
			Error::<Test>::Halted
		);
	});
}
#[test]
fn retry_limit_is_permanent_even_when_every_attempt_fails() {
	run(|| {
		let config = activate();
		let id = redemption_id(&config, 0);
		assert_ok!(EthereumBridge::export_redemption(RuntimeOrigin::signed(1), 0));
		for nonce in 1..=4 {
			assert_ok!(EthereumBridge::accept(id, nonce, H256::repeat_byte(nonce as u8)));
			assert_ok!(EthereumBridge::submit_redemption_receipt(
				RuntimeOrigin::signed(1),
				receipt(nonce, id, false)
			));
			if nonce < 4 {
				assert_ok!(EthereumBridge::retry_redemption(RuntimeOrigin::signed(1), id));
			}
		}
		assert_noop!(
			EthereumBridge::retry_redemption(RuntimeOrigin::signed(1), id),
			Error::<Test>::Capacity
		);
		assert_noop!(
			EthereumBridge::export_redemption(RuntimeOrigin::signed(1), 64),
			Error::<Test>::Capacity
		);
		assert_eq!(ByNonce::<Test>::iter().count(), 4);
	});
}
