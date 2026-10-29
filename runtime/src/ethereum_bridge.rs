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

//! Fresh-genesis local PoC ingress. Bootstrap the beacon client from the local PoS network.
use crate::{EthereumBeaconClient, Midnight, MidnightSystem, Runtime, RuntimeEvent};
use frame_support::{parameter_types, traits::ConstU32};
use midnight_node_ledger::{ethereum_bridge::EscrowConfig, types::active_ledger_bridge};
use midnight_primitives::LedgerStateProvider;
use snowbridge_beacon_primitives::{Fork, ForkVersions};

parameter_types! {
	pub const BridgeForkVersions: ForkVersions = ForkVersions {
		genesis: Fork { version: [0,0,0,0], epoch: 0 },
		altair: Fork { version: [1,0,0,0], epoch: 0 },
		bellatrix: Fork { version: [2,0,0,0], epoch: 0 },
		capella: Fork { version: [3,0,0,0], epoch: 0 },
		deneb: Fork { version: [4,0,0,0], epoch: 0 },
		electra: Fork { version: [5,0,0,0], epoch: 0 },
		fulu: Fork { version: [6,0,0,0], epoch: 100_000_000 },
	};
}
impl snowbridge_pallet_ethereum_client::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ForkVersions = BridgeForkVersions;
	type FreeHeadersInterval = ConstU32<32>;
	type WeightInfo = ();
}

pub struct CanonicalEscrow;
impl pallet_ethereum_bridge::ValidateEscrow for CanonicalEscrow {
	fn validate(config: &EscrowConfig) -> sp_runtime::DispatchResult {
		active_ledger_bridge::validate_bridge_escrow(
			&Midnight::get_ledger_state_key(),
			config.clone(),
		)
		.map_err(|_| sp_runtime::DispatchError::Other("invalid bridge escrow"))
	}
	fn read(
		config: &EscrowConfig,
		sequence: u64,
	) -> Result<midnight_node_ledger::ethereum_bridge::Redemption, sp_runtime::DispatchError> {
		active_ledger_bridge::get_bridge_redemption(
			&Midnight::get_ledger_state_key(),
			config.clone(),
			sequence,
		)
		.map_err(|_| sp_runtime::DispatchError::Other("invalid committed retirement"))
	}
}
impl pallet_ethereum_bridge::Config for Runtime {
	type Verifier = EthereumBeaconClient;
	type Ledger = MidnightSystem;
	type EscrowValidator = CanonicalEscrow;
	type Outbound = BridgeQueue;
}

use crate::{EthereumBridge, EthereumMessageQueue, EthereumOutboundQueue};
use frame_support::{
	traits::{EnqueueMessage, ProcessMessage, ProcessMessageError},
	weights::WeightMeter,
};
use pallet_ethereum_bridge::{ActiveRegistry, OutboundRoot, RedemptionPhase, Redemptions};
use parity_scale_codec::{DecodeAll, Encode};
use snowbridge_outbound_queue_primitives::v2::{Message, SendMessage};
use sp_core::{H160, H256};

pub struct BridgeQueue;
impl pallet_ethereum_bridge::QueueRedemption for BridgeQueue {
	fn enqueue(
		config: &pallet_ethereum_bridge::Registry,
		id: H256,
		record: &midnight_node_ledger::ethereum_bridge::Redemption,
	) -> sp_runtime::DispatchResult {
		let message = pallet_ethereum_bridge::outbound_message(config, id, record);
		let ticket = EthereumOutboundQueue::validate(&message)
			.map_err(|_| pallet_ethereum_bridge::Error::<Runtime>::QueueFailure)?;
		let attempt = Redemptions::<Runtime>::get(id)
			.ok_or(pallet_ethereum_bridge::Error::<Runtime>::InvalidRedemption)?
			.attempt;
		// Tag the queue envelope with the attempt; the Ethereum topic remains the stable redemption ID.
		let payload: frame_support::BoundedVec<
			u8,
			<EthereumMessageQueue as EnqueueMessage<H256>>::MaxMessageLen,
		> = (attempt, ticket)
			.encode()
			.try_into()
			.map_err(|_| pallet_ethereum_bridge::Error::<Runtime>::QueueFailure)?;
		EthereumMessageQueue::enqueue_message(
			payload.as_bounded_slice(),
			pallet_ethereum_bridge::bridge_origin(config),
		);
		Ok(())
	}
}
impl ProcessMessage for BridgeQueue {
	type Origin = H256;
	fn process_message(
		bytes: &[u8],
		origin: H256,
		meter: &mut WeightMeter,
		id: &mut [u8; 32],
	) -> Result<bool, ProcessMessageError> {
		let overhead = frame_support::weights::Weight::from_parts(1_000_000_000, 64_000);
		meter
			.try_consume(overhead)
			.map_err(|_| ProcessMessageError::Overweight(overhead))?;
		if bytes.len() > 1024 {
			return Err(ProcessMessageError::Corrupt);
		}
		// The SDK processor writes before some error returns. Roll back all of its
		// storage/events along with our nonce binding; consumed weight is retained.
		frame_support::storage::transactional::with_transaction_opaque_err(|| {
			let result = (|| {
				if pallet_ethereum_bridge::OutboundHalted::<Runtime>::get() {
					return Err(ProcessMessageError::Yield);
				}
				let (attempt, message) = <(u32, Message)>::decode_all(&mut &bytes[..])
					.map_err(|_| ProcessMessageError::Corrupt)?;
				let config =
					ActiveRegistry::<Runtime>::get().ok_or(ProcessMessageError::Corrupt)?;
				let export =
					Redemptions::<Runtime>::get(message.id).ok_or(ProcessMessageError::Corrupt)?;
				if export.attempt != attempt
					|| export.phase != RedemptionPhase::Queued
					|| origin != pallet_ethereum_bridge::bridge_origin(&config)
					|| message
						!= pallet_ethereum_bridge::outbound_message(
							&config,
							message.id,
							&export.record,
						) {
					return Err(ProcessMessageError::Corrupt);
				}
				let done =
					EthereumOutboundQueue::process_message(&message.encode(), origin, meter, id)?;
				if !done {
					return Err(ProcessMessageError::Yield);
				}
				let nonce = snowbridge_pallet_outbound_queue_v2::Nonce::<Runtime>::get();
				let messages = snowbridge_pallet_outbound_queue_v2::Messages::<Runtime>::get();
				let accepted = messages.last().ok_or(ProcessMessageError::Corrupt)?;
				let hash = snowbridge_pallet_outbound_queue_v2::MessageLeaves::<Runtime>::get()
					.last()
					.copied()
					.ok_or(ProcessMessageError::Corrupt)?;
				if accepted.nonce != nonce
					|| accepted.topic != message.id
					|| accepted.origin != origin
					|| accepted.commands.len() != 1
					|| accepted.commands[0].kind != message.commands[0].index()
					|| accepted.commands[0].gas != 200_000
					|| accepted.commands[0].payload != message.commands[0].abi_encode()
				{
					return Err(ProcessMessageError::Corrupt);
				}
				EthereumBridge::accept(message.id, nonce, hash)
					.map_err(|_| ProcessMessageError::Corrupt)?;
				Ok(true)
			})();
			if result.is_ok() {
				frame_support::storage::TransactionOutcome::Commit(result)
			} else {
				frame_support::storage::TransactionOutcome::Rollback(result)
			}
		})
		.map_err(|_| ProcessMessageError::Corrupt)?
	}
}

pub struct BridgeCommitment;
impl snowbridge_pallet_outbound_queue_v2::OnNewCommitment for BridgeCommitment {
	fn on_new_commitment(root: H256) {
		OutboundRoot::<Runtime>::put(root);
	}
}
impl sp_consensus_beefy::mmr::BeefyDataProvider<H256> for BridgeCommitment {
	fn extra_data() -> H256 {
		OutboundRoot::<Runtime>::get()
	}
}
pub struct GatewayAddress;
impl frame_support::traits::Get<H160> for GatewayAddress {
	fn get() -> H160 {
		ActiveRegistry::<Runtime>::get().map(|r| r.gateway).unwrap_or_default()
	}
}
pub struct NoRewards;
impl bp_relayers::RewardLedger<crate::AccountId, (), u128> for NoRewards {
	fn register_reward(_: &crate::AccountId, _: (), _: u128) {}
}
parameter_types! {
	pub BridgeServiceWeight: Option<frame_support::weights::Weight> = Some(frame_support::weights::Weight::from_parts(10_000_000_000, 300_000));
	pub BridgeEthereumNetwork: staging_xcm::latest::NetworkId = staging_xcm::latest::NetworkId::Ethereum { chain_id: 31337 };
}
impl pallet_message_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type MessageProcessor = BridgeQueue;
	type Size = u32;
	type QueueChangeHandler = ();
	type HeapSize = ConstU32<16_384>;
	type MaxStale = ConstU32<4>;
	type ServiceWeight = BridgeServiceWeight;
	type IdleMaxServiceWeight = ();
	type QueuePausedQuery = ();
}
impl snowbridge_pallet_outbound_queue_v2::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Hashing = sp_runtime::traits::Keccak256;
	type AggregateMessageOrigin = H256;
	type MessageQueue = EthereumMessageQueue;
	type GasMeter = snowbridge_outbound_queue_primitives::v2::ConstantGasMeter;
	type Balance = u128;
	type MaxMessagePayloadSize = ConstU32<1024>;
	type MaxMessagesPerBlock = ConstU32<1>;
	type OnNewCommitment = BridgeCommitment;
	type WeightToFee = frame_support::weights::IdentityFee<u128>;
	type WeightInfo = ();
	type Verifier = EthereumBeaconClient;
	type GatewayAddress = GatewayAddress;
	type RewardKind = ();
	type DefaultRewardKind = ();
	type RewardPayment = NoRewards;
	type EthereumNetwork = BridgeEthereumNetwork;
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::{assert_ok, traits::Hooks};
	use midnight_node_ledger::ethereum_bridge::Redemption;
	use pallet_ethereum_bridge::{ByNonce, Export, NonceBinding};
	use parity_scale_codec::Encode;
	use sp_consensus_beefy::mmr::BeefyDataProvider;
	fn fixture() -> (pallet_ethereum_bridge::Registry, H256, Redemption) {
		let config = pallet_ethereum_bridge::Registry {
			gateway: H160::repeat_byte(1),
			token: H160::repeat_byte(2),
			escrow: EscrowConfig {
				bridge_domain: [3; 32],
				token_domain: [4; 32],
				escrow: [5; 32],
				color: [6; 32],
				retire_operation: [7; 32],
			},
		};
		let record = Redemption {
			magic: *b"MNBR",
			version: 1,
			bridge_domain: [3; 32],
			escrow: [5; 32],
			sequence: 0,
			amount: 40,
			recipient: [8; 20],
			color: [6; 32],
			total_retired: 40,
			count: 1,
			ledger_balance: 40,
		};
		let id = pallet_ethereum_bridge::redemption_id(&config, 0);
		(config, id, record)
	}
	#[test]
	fn outbound_processor_rolls_back_sdk_writes_without_refunding_weight() {
		sp_io::TestExternalities::default().execute_with(|| {
			crate::System::set_block_number(10);
			let (config, id, record) = fixture();
			ActiveRegistry::<Runtime>::put(&config);
			Redemptions::<Runtime>::insert(
				id,
				Export { record: record.clone(), attempt: 0, phase: RedemptionPhase::Queued },
			);
			// Force a failure after the SDK has appended its message, leaf and pending order.
			ByNonce::<Runtime>::insert(
				1,
				NonceBinding {
					redemption: H256::repeat_byte(99),
					attempt: 0,
					message_hash: H256::zero(),
					accepted_block: 9,
					result: None,
				},
			);
			let bytes =
				(0u32, pallet_ethereum_bridge::outbound_message(&config, id, &record)).encode();
			let mut meter = WeightMeter::new();
			let mut message_id = [0; 32];
			assert!(
				BridgeQueue::process_message(
					&bytes,
					pallet_ethereum_bridge::bridge_origin(&config),
					&mut meter,
					&mut message_id
				)
				.is_err()
			);
			assert!(meter.consumed().ref_time() > 0);
			assert_eq!(snowbridge_pallet_outbound_queue_v2::Nonce::<Runtime>::get(), 0);
			assert!(snowbridge_pallet_outbound_queue_v2::Messages::<Runtime>::get().is_empty());
			assert!(
				snowbridge_pallet_outbound_queue_v2::MessageLeaves::<Runtime>::get().is_empty()
			);
			assert!(!snowbridge_pallet_outbound_queue_v2::PendingOrders::<Runtime>::contains_key(
				1
			));
			assert_eq!(Redemptions::<Runtime>::get(id).unwrap().phase, RedemptionPhase::Queued);
			ByNonce::<Runtime>::remove(1);
			assert_ok!(BridgeQueue::process_message(
				&bytes,
				pallet_ethereum_bridge::bridge_origin(&config),
				&mut meter,
				&mut message_id
			));
			let binding = ByNonce::<Runtime>::get(1).unwrap();
			assert_eq!(binding.accepted_block, 10);
			EthereumOutboundQueue::on_finalize(10);
			assert_eq!(BridgeCommitment::extra_data(), binding.message_hash);
			assert_eq!(snowbridge_pallet_outbound_queue_v2::Messages::<Runtime>::get().len(), 1);
			assert!(
				snowbridge_pallet_outbound_queue_v2::MessageLeaves::<Runtime>::get().is_empty()
			);
			EthereumBridge::on_initialize(11);
			assert_eq!(BridgeCommitment::extra_data(), H256::zero());
		});
	}
	#[test]
	fn outbound_processor_rejects_origin_amount_and_trailing_bytes() {
		sp_io::TestExternalities::default().execute_with(|| {
			crate::System::set_block_number(10);
			let (config, id, record) = fixture();
			ActiveRegistry::<Runtime>::put(&config);
			Redemptions::<Runtime>::insert(
				id,
				Export { record: record.clone(), attempt: 0, phase: RedemptionPhase::Queued },
			);
			let origin = pallet_ethereum_bridge::bridge_origin(&config);
			let bytes =
				(0u32, pallet_ethereum_bridge::outbound_message(&config, id, &record)).encode();
			let mut meter = WeightMeter::new();
			let mut message_id = [0; 32];
			assert!(
				BridgeQueue::process_message(&bytes, H256::zero(), &mut meter, &mut message_id)
					.is_err()
			);
			let mut wrong = record;
			wrong.amount += 1;
			assert!(
				BridgeQueue::process_message(
					&(0u32, pallet_ethereum_bridge::outbound_message(&config, id, &wrong)).encode(),
					origin,
					&mut meter,
					&mut message_id
				)
				.is_err()
			);
			let mut trailing = bytes.clone();
			trailing.push(0);
			assert!(
				BridgeQueue::process_message(&trailing, origin, &mut meter, &mut message_id)
					.is_err()
			);
			assert_eq!(snowbridge_pallet_outbound_queue_v2::Nonce::<Runtime>::get(), 0);
			Redemptions::<Runtime>::mutate(id, |export| export.as_mut().unwrap().attempt = 1);
			assert!(
				BridgeQueue::process_message(&bytes, origin, &mut meter, &mut message_id).is_err()
			);
			let (_, message) = <(u32, Message)>::decode_all(&mut &bytes[..]).unwrap();
			let retry = (1u32, message).encode();
			assert_ok!(BridgeQueue::process_message(&retry, origin, &mut meter, &mut message_id));
			assert!(
				BridgeQueue::process_message(&bytes, origin, &mut meter, &mut message_id).is_err()
			);
		});
	}
}
