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

//! Experimental single-asset Ethereum ingress. Every deposit is verified before admission.
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;
use alloc::boxed::Box;
use frame_support::{pallet_prelude::*, transactional};
use frame_system::pallet_prelude::*;
use midnight_node_ledger::ethereum_bridge::{EscrowConfig, EthereumCredit, Redemption};
use midnight_primitives::MidnightSystemTransactionEthereumExecutor;
pub use pallet::*;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use snowbridge_inbound_queue_primitives::{
	EventProof, Verifier,
	v2::{EthereumAsset, Message, Payload},
};
use sp_core::{H160, H256};
use sp_io::hashing::keccak_256;

const MAX_AMOUNT: u128 = 1_000_000_000_000;
const MAX_MESSAGES: u32 = 65; // One registration and the Gateway's 64 lifetime deposits.

pub trait ValidateEscrow {
	fn validate(config: &EscrowConfig) -> DispatchResult;
	fn read(config: &EscrowConfig, sequence: u64) -> Result<Redemption, DispatchError>;
}

pub trait QueueRedemption {
	fn enqueue(config: &Registry, id: H256, record: &Redemption) -> DispatchResult;
}

#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub enum RedemptionPhase {
	Queued,
	AwaitingResult(u64),
	RetryEligible,
	Completed,
}
#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct Export {
	pub record: Redemption,
	pub attempt: u32,
	pub phase: RedemptionPhase,
}
#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct NonceBinding<BlockNumber> {
	pub redemption: H256,
	pub attempt: u32,
	pub message_hash: H256,
	pub accepted_block: BlockNumber,
	pub result: Option<bool>,
}

#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct Registry {
	pub gateway: H160,
	pub token: H160,
	pub escrow: EscrowConfig,
}

#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub struct Deposit {
	pub credit: EthereumCredit,
	pub nonce: u64,
	pub origin: H160,
	pub issued: bool,
}

#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub enum QuarantineReason {
	Fees,
	Asset,
	Envelope,
	Registration,
}

fn word(tag: &[u8]) -> [u8; 32] {
	let mut word = [0; 32];
	word[..tag.len()].copy_from_slice(tag);
	word
}
fn abi_hash(words: &[[u8; 32]]) -> [u8; 32] {
	keccak_256(&words.concat())
}
fn uint(value: u128) -> [u8; 32] {
	let mut out = [0; 32];
	out[16..].copy_from_slice(&value.to_be_bytes());
	out
}
fn address(value: H160) -> [u8; 32] {
	let mut out = [0; 32];
	out[12..].copy_from_slice(value.as_bytes());
	out
}
pub fn asset_id(config: &Registry) -> [u8; 32] {
	abi_hash(&[word(b"MN/SB/ASSET/V1"), config.escrow.bridge_domain, address(config.token)])
}
pub fn deposit_id(config: &Registry, nonce: u64) -> [u8; 32] {
	abi_hash(&[word(b"MN/SB/DEPOSIT/V1"), config.escrow.bridge_domain, uint(nonce.into())])
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	#[pallet::pallet]
	pub struct Pallet<T>(_);
	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		type Verifier: Verifier;
		type Ledger: MidnightSystemTransactionEthereumExecutor;
		type EscrowValidator: ValidateEscrow;
		type Outbound: QueueRedemption;
	}
	#[pallet::storage]
	pub type ActiveRegistry<T> = StorageValue<_, Registry>;
	#[pallet::storage]
	pub type Seen<T> = StorageMap<_, Blake2_128Concat, u64, H256>;
	#[pallet::storage]
	pub type MessageCount<T> = StorageValue<_, u32, ValueQuery>;
	#[pallet::storage]
	pub type DepositCount<T> = StorageValue<_, u32, ValueQuery>;
	#[pallet::storage]
	pub type Deposits<T> = StorageMap<_, Blake2_128Concat, H256, Deposit>;
	#[pallet::storage]
	pub type Quarantine<T> = StorageMap<_, Blake2_128Concat, u64, (H256, QuarantineReason)>;
	#[pallet::storage]
	pub type Registered<T> = StorageValue<_, bool, ValueQuery>;
	#[pallet::storage]
	pub type Redemptions<T> = StorageMap<_, Blake2_128Concat, H256, Export>;
	#[pallet::storage]
	pub type ByNonce<T: Config> =
		StorageMap<_, Blake2_128Concat, u64, NonceBinding<BlockNumberFor<T>>>;
	#[pallet::storage]
	pub type OutboundRoot<T> = StorageValue<_, H256, ValueQuery>;
	#[pallet::storage]
	pub type OutboundHalted<T> = StorageValue<_, bool, ValueQuery>;
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			// MMR runs earlier and has already read the previous block's commitment.
			OutboundRoot::<T>::kill();
			T::DbWeight::get().writes(1)
		}
	}
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		Activated { gateway: H160 },
		Registered,
		RedemptionQueued { id: H256, attempt: u32 },
		RedemptionAccepted { id: H256, attempt: u32, nonce: u64 },
		RedemptionResult { id: H256, nonce: u64, success: bool },
		ConflictingReceipt { nonce: u64 },
		Pending { id: H256 },
		Issued { id: H256, transaction_hash: [u8; 32] },
		Quarantined { nonce: u64, reason: QuarantineReason },
	}
	#[pallet::error]
	pub enum Error<T> {
		Inactive,
		AlreadyActive,
		Configuration,
		InvalidProof,
		WrongGateway,
		TooLarge,
		Capacity,
		InvalidNonce,
		AlreadySeen,
		ConflictingMessage,
		MissingDeposit,
		AlreadyIssued,
		InvalidRedemption,
		AlreadyExported,
		NotRetryEligible,
		Halted,
		InvalidReceipt,
		QueueFailure,
	}
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// One-shot disposable-network setup. No mint is available to the activator.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(5_000_000_000, 300_000))]
		#[transactional]
		pub fn activate(origin: OriginFor<T>, config: Registry) -> DispatchResult {
			ensure_root(origin)?;
			ensure!(!ActiveRegistry::<T>::exists(), Error::<T>::AlreadyActive);
			ensure!(
				config.gateway != H160::zero() && config.token != H160::zero(),
				Error::<T>::Configuration
			);
			let namespace = abi_hash(&[word(b"MN/SB/COLOR/V1"), asset_id(&config)]);
			ensure!(namespace == config.escrow.token_domain, Error::<T>::Configuration);
			T::EscrowValidator::validate(&config.escrow)?;
			Self::deposit_event(Event::Activated { gateway: config.gateway });
			ActiveRegistry::<T>::put(config);
			Ok(())
		}

		/// Proofs are authenticated by the configured Ethereum light client, not the relayer.
		#[pallet::call_index(1)]
		// Conservative PoC weight; requires benchmarked weights before any supported deployment.
		#[pallet::weight(Weight::from_parts(10_000_000_000, 300_000))]
		#[transactional]
		pub fn submit_deposit(origin: OriginFor<T>, event: Box<EventProof>) -> DispatchResult {
			ensure_signed(origin)?;
			ensure!(
				event.encoded_size() <= 262_144
					&& event.event_log.data.len() <= 4096
					&& event.event_log.topics.len() <= 4,
				Error::<T>::TooLarge
			);
			let config = ActiveRegistry::<T>::get().ok_or(Error::<T>::Inactive)?;
			ensure!(event.event_log.address == config.gateway, Error::<T>::WrongGateway);
			T::Verifier::verify(&event.event_log, &event.proof)
				.map_err(|_| Error::<T>::InvalidProof)?;
			// An undecodable event has no trustworthy nonce, so cannot enter the source stream.
			let message =
				Message::try_from(&event.event_log).map_err(|_| Error::<T>::InvalidNonce)?;
			let nonce = message.nonce;
			ensure!(nonce != 0, Error::<T>::InvalidNonce);
			let digest = H256(keccak_256(&event.event_log.encode()));
			if let Some(previous) = Seen::<T>::get(nonce) {
				ensure!(previous == digest, Error::<T>::ConflictingMessage);
				return Err(Error::<T>::AlreadySeen.into());
			}
			ensure!(MessageCount::<T>::get() < MAX_MESSAGES, Error::<T>::Capacity);
			Seen::<T>::insert(nonce, digest);
			MessageCount::<T>::mutate(|n| *n += 1);
			match Self::admit(&config, message) {
				Ok(Some(deposit)) => {
					ensure!(DepositCount::<T>::get() < 64, Error::<T>::Capacity);
					DepositCount::<T>::mutate(|n| *n += 1);
					let id = H256(deposit.credit.deposit_id);
					Deposits::<T>::insert(id, deposit);
					Self::deposit_event(Event::Pending { id });
				},
				Ok(None) => {
					Registered::<T>::put(true);
					Self::deposit_event(Event::Registered);
				},
				Err(reason) => {
					Quarantine::<T>::insert(nonce, (digest, reason.clone()));
					Self::deposit_event(Event::Quarantined { nonce, reason });
				},
			}
			Ok(())
		}

		/// Only fields already stored from a verified event reach the ledger capability.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::from_parts(20_000_000_000, 300_000))]
		#[transactional]
		pub fn materialize_deposit(origin: OriginFor<T>, id: H256) -> DispatchResult {
			ensure_signed(origin)?;
			let mut deposit = Deposits::<T>::get(id).ok_or(Error::<T>::MissingDeposit)?;
			ensure!(!deposit.issued, Error::<T>::AlreadyIssued);
			let transaction_hash = T::Ledger::credit_ethereum_asset(deposit.credit.clone())?;
			deposit.issued = true;
			Deposits::<T>::insert(id, deposit);
			Self::deposit_event(Event::Issued { id, transaction_hash });
			Ok(())
		}
		/// Export only an immutable retirement read from the committed ledger.
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::from_parts(20_000_000_000, 300_000))]
		#[transactional]
		pub fn export_redemption(origin: OriginFor<T>, sequence: u64) -> DispatchResult {
			ensure_signed(origin)?;
			ensure!(!OutboundHalted::<T>::get(), Error::<T>::Halted);
			ensure!(sequence < 64, Error::<T>::Capacity);
			let config = ActiveRegistry::<T>::get().ok_or(Error::<T>::Inactive)?;
			let id = redemption_id(&config, sequence);
			ensure!(!Redemptions::<T>::contains_key(id), Error::<T>::AlreadyExported);
			let record = T::EscrowValidator::read(&config.escrow, sequence)?;
			ensure!(
				record.sequence == sequence
					&& record.amount > 0
					&& record.amount <= MAX_AMOUNT
					&& record.recipient != [0; 20]
					&& record.escrow == config.escrow.escrow
					&& record.color == config.escrow.color
					&& record.bridge_domain == config.escrow.bridge_domain,
				Error::<T>::InvalidRedemption
			);
			Redemptions::<T>::insert(
				id,
				Export { record: record.clone(), attempt: 0, phase: RedemptionPhase::Queued },
			);
			T::Outbound::enqueue(&config, id, &record)?;
			Self::deposit_event(Event::RedemptionQueued { id, attempt: 0 });
			Ok(())
		}
		/// A retry is possible only after a finalized, authenticated failure.
		#[pallet::call_index(4)]
		#[pallet::weight(Weight::from_parts(5_000_000_000, 300_000))]
		#[transactional]
		pub fn retry_redemption(origin: OriginFor<T>, id: H256) -> DispatchResult {
			ensure_signed(origin)?;
			ensure!(!OutboundHalted::<T>::get(), Error::<T>::Halted);
			let config = ActiveRegistry::<T>::get().ok_or(Error::<T>::Inactive)?;
			let mut export = Redemptions::<T>::get(id).ok_or(Error::<T>::InvalidRedemption)?;
			ensure!(export.phase == RedemptionPhase::RetryEligible, Error::<T>::NotRetryEligible);
			ensure!(export.attempt < 3, Error::<T>::Capacity);
			export.attempt += 1;
			export.phase = RedemptionPhase::Queued;
			Redemptions::<T>::insert(id, &export);
			T::Outbound::enqueue(&config, id, &export.record)?;
			Self::deposit_event(Event::RedemptionQueued { id, attempt: export.attempt });
			Ok(())
		}
		/// Permanent nonce bindings survive SDK reward-order cleanup.
		#[pallet::call_index(5)]
		#[pallet::weight(Weight::from_parts(10_000_000_000, 300_000))]
		#[transactional]
		pub fn submit_redemption_receipt(
			origin: OriginFor<T>,
			event: Box<EventProof>,
		) -> DispatchResult {
			ensure_signed(origin)?;
			ensure!(
				event.encoded_size() <= 262_144
					&& event.event_log.data.len() == 96
					&& event.event_log.topics.len() == 2,
				Error::<T>::TooLarge
			);
			let config = ActiveRegistry::<T>::get().ok_or(Error::<T>::Inactive)?;
			ensure!(event.event_log.address == config.gateway, Error::<T>::WrongGateway);
			T::Verifier::verify(&event.event_log, &event.proof)
				.map_err(|_| Error::<T>::InvalidProof)?;
			let receipt = snowbridge_outbound_queue_primitives::v2::DeliveryReceipt::try_from(
				&event.event_log,
			)
			.map_err(|_| Error::<T>::InvalidReceipt)?;
			let mut binding = ByNonce::<T>::get(receipt.nonce).ok_or(Error::<T>::InvalidReceipt)?;
			ensure!(receipt.topic == binding.redemption, Error::<T>::InvalidReceipt);
			if let Some(previous) = binding.result {
				if previous != receipt.success {
					// Return success so the safety halt is committed, rather than rolled back.
					OutboundHalted::<T>::put(true);
					Self::deposit_event(Event::ConflictingReceipt { nonce: receipt.nonce });
					return Ok(());
				}
				return Err(Error::<T>::AlreadySeen.into());
			}
			let mut export =
				Redemptions::<T>::get(binding.redemption).ok_or(Error::<T>::InvalidRedemption)?;
			ensure!(
				export.attempt == binding.attempt
					&& export.phase == RedemptionPhase::AwaitingResult(receipt.nonce),
				Error::<T>::InvalidReceipt
			);
			export.phase = if receipt.success {
				RedemptionPhase::Completed
			} else {
				RedemptionPhase::RetryEligible
			};
			binding.result = Some(receipt.success);
			Redemptions::<T>::insert(binding.redemption, export);
			ByNonce::<T>::insert(receipt.nonce, &binding);
			Self::deposit_event(Event::RedemptionResult {
				id: binding.redemption,
				nonce: receipt.nonce,
				success: receipt.success,
			});
			Ok(())
		}
	}
	impl<T: Config> Pallet<T> {
		pub fn accept(id: H256, nonce: u64, message_hash: H256) -> DispatchResult {
			let mut export = Redemptions::<T>::get(id).ok_or(Error::<T>::InvalidRedemption)?;
			ensure!(
				export.phase == RedemptionPhase::Queued && !ByNonce::<T>::contains_key(nonce),
				Error::<T>::InvalidRedemption
			);
			export.phase = RedemptionPhase::AwaitingResult(nonce);
			ByNonce::<T>::insert(
				nonce,
				NonceBinding {
					redemption: id,
					attempt: export.attempt,
					message_hash,
					accepted_block: frame_system::Pallet::<T>::block_number(),
					result: None,
				},
			);
			Self::deposit_event(Event::RedemptionAccepted { id, attempt: export.attempt, nonce });
			Redemptions::<T>::insert(id, export);
			Ok(())
		}
		fn admit(config: &Registry, message: Message) -> Result<Option<Deposit>, QuarantineReason> {
			if message.value != 0
				|| message.execution_fee != 0
				|| message.relayer_fee != 0
				|| message.claimer.is_some()
			{
				return Err(QuarantineReason::Fees);
			}
			let envelope = match message.payload {
				Payload::CreateAsset { token, .. } => {
					if token != config.token || !message.assets.is_empty() || Registered::<T>::get()
					{
						return Err(QuarantineReason::Registration);
					}
					return Ok(None);
				},
				Payload::Raw(bytes) => bytes,
			};
			let amount = match message.assets.as_slice() {
				[EthereumAsset::NativeTokenERC20 { token_id, value }]
					if *token_id == config.token && *value > 0 && *value <= MAX_AMOUNT =>
				{
					*value
				},
				_ => return Err(QuarantineReason::Asset),
			};
			if envelope.len() != 96
				|| envelope[..32] != uint(1)
				|| envelope[32..64] != config.escrow.bridge_domain
				|| envelope[64..] == [0; 32]
			{
				return Err(QuarantineReason::Envelope);
			}
			let mut recipient = [0; 32];
			recipient.copy_from_slice(&envelope[64..]);
			let deposit = deposit_id(config, message.nonce);
			let output = abi_hash(&[
				word(b"MN/SB/OUTPUT/V1"),
				deposit,
				asset_id(config),
				config.escrow.color,
				recipient,
				uint(amount),
			]);
			Ok(Some(Deposit {
				nonce: message.nonce,
				origin: message.origin,
				issued: false,
				credit: EthereumCredit {
					deposit_id: deposit,
					output_id: output,
					color: config.escrow.color,
					amount,
					recipient,
				},
			}))
		}
	}
}

#[cfg(test)]
mod tests;

pub fn bridge_origin(config: &Registry) -> H256 {
	H256(abi_hash(&[word(b"MN/SB/ORIGIN/V1"), config.escrow.bridge_domain]))
}
pub fn redemption_id(config: &Registry, sequence: u64) -> H256 {
	H256(abi_hash(&[
		word(b"MN/SB/REDEEM/V1"),
		config.escrow.bridge_domain,
		config.escrow.escrow,
		uint(sequence.into()),
	]))
}
pub fn outbound_message(
	config: &Registry,
	id: H256,
	record: &Redemption,
) -> snowbridge_outbound_queue_primitives::v2::Message {
	use snowbridge_outbound_queue_primitives::v2::{Command, Message};
	Message {
		origin: bridge_origin(config),
		id,
		fee: 0,
		commands: alloc::vec![Command::UnlockNativeToken {
			token: config.token,
			recipient: H160(record.recipient),
			amount: record.amount
		}]
		.try_into()
		.expect("one command fits"),
	}
}
