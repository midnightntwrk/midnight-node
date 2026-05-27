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

extern crate alloc;

use alloc::vec::Vec;
use hex_literal::hex;
use midnight_node_ledger::types::{
	Hash, Tx,
	active_version::{BlockContext, LedgerApiError},
};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::DispatchError;

/// Maximum allowed drift, in seconds, between a transaction's declared block time and
/// the validating node's clock (`tblock_err` in a [`BlockContext`]).
///
/// Mirrors substrate's private `MAX_TIMESTAMP_DRIFT_MILLIS` (30_000 ms) from
/// `substrate/frame/timestamp/src/lib.rs`. Block authoring applies the same drift in
/// `LedgerBlockContextProvider::get_block_context` (see `pallet-midnight`), so any code
/// that builds a `BlockContext` for validation off-chain (e.g. the `midnight_validateTransaction`
/// RPC) must use this value to stay in sync with what the runtime accepts.
pub const MAX_TIMESTAMP_DRIFT_SECS: u32 = 30;

/// Cheap, runtime-provided context needed to validate a transaction off-chain (e.g. via the
/// `midnight_validateTransaction` RPC) without submitting it to the txpool.
///
/// Returned by `MidnightRuntimeApi::get_validation_context` so the node never has to reconstruct
/// the validation context from individual storage reads — a layout-fragile approach that can't be
/// caught at compile time. Bundling these together also guarantees they're all read at the same
/// block.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, Debug, TypeInfo)]
pub struct ValidationContext {
	/// Serialized ledger state key at the queried block.
	pub state_key: Vec<u8>,
	/// Block context (timestamps, parent hash, allowed drift) exactly as built by block authoring.
	pub block_context: BlockContext,
	/// Runtime spec version at the queried block.
	pub spec_version: u32,
	/// Maximum block weight (ref_time) the runtime will admit; used to reject oversized txs.
	pub max_block_weight: u64,
}

pub type LedgerMutFn<E> = fn(Vec<u8>) -> Result<Vec<u8>, E>;
/// Trait to allow pallets to mutate the Ledger state
pub trait LedgerStateProviderMut {
	/// Get the current ledger state key
	fn get_ledger_state_key() -> Vec<u8>;
	/// Mutate the ledger state - must return an updated ledger state key and may optionally return extra data
	fn mut_ledger_state<F, E, R>(f: F) -> Result<R, E>
	where
		F: FnOnce(Vec<u8>) -> Result<(Vec<u8>, R), E>;
}

pub trait LedgerBlockContextProvider {
	fn get_block_context() -> BlockContext;
}

pub trait MidnightSystemTransactionExecutor {
	/// Execute a Midnight System Transaction and return a SCALE-compatible result
	fn execute_system_transaction(
		serialized_system_transaction: Vec<u8>,
	) -> Result<Hash, DispatchError>;
}

#[derive(Clone, Encode, Decode, DecodeWithMemTracking, Debug, TypeInfo)]
pub enum TransactionType {
	MidnightTx(Vec<u8>, Option<Tx>),
	TimestampTx(u64),
	UnknownTx,
}

#[derive(Clone, Encode, Decode, DecodeWithMemTracking, Debug, TypeInfo)]
pub enum TransactionTypeV2 {
	MidnightTx(Vec<u8>, Result<Tx, LedgerApiError>),
	TimestampTx(u64),
	UnknownTx,
}

pub use bridge::{BridgeRecipient, BridgeRecipientError, BridgeRecipientMaxLen};

pub mod bridge {
	use super::*;
	use core::ops::Deref;
	use sp_core::{Get, H256, bounded::BoundedVec, crypto::UncheckedFrom};

	/// Maximum length (bytes) of a Midnight recipient encoded in the bridge datum.
	pub const BRIDGE_RECIPIENT_MAX_BYTES: u32 = 32;

	/// Type-level constant used to bound bridge recipient length.
	pub struct BridgeRecipientMaxLen;

	impl Get<u32> for BridgeRecipientMaxLen {
		fn get() -> u32 {
			BRIDGE_RECIPIENT_MAX_BYTES
		}
	}

	/// Error type returned when bridge recipient bytes cannot be converted.
	#[derive(Clone, Copy, PartialEq, Eq, Debug)]
	pub enum BridgeRecipientError {
		/// The encoded recipient exceeds the configured byte limit.
		TooLong,
	}

	/// Recipient type used by the bridge pallet and inherent data provider.
	#[derive(
		Clone,
		PartialEq,
		Eq,
		Encode,
		Decode,
		DecodeWithMemTracking,
		MaxEncodedLen,
		TypeInfo,
		Debug,
		Default,
	)]
	#[scale_info(skip_type_params(BridgeRecipientMaxLen))]
	pub struct BridgeRecipient(pub BoundedVec<u8, BridgeRecipientMaxLen>);

	impl BridgeRecipient {
		/// Returns the raw bytes.
		pub fn as_bytes(&self) -> &[u8] {
			self.0.as_slice()
		}

		/// Consumes the recipient and returns the bounded vector backing it.
		pub fn into_inner(self) -> BoundedVec<u8, BridgeRecipientMaxLen> {
			self.0
		}
	}

	impl Deref for BridgeRecipient {
		type Target = [u8];

		fn deref(&self) -> &Self::Target {
			self.as_bytes()
		}
	}

	impl AsRef<[u8]> for BridgeRecipient {
		fn as_ref(&self) -> &[u8] {
			self.as_bytes()
		}
	}

	impl TryFrom<&[u8]> for BridgeRecipient {
		type Error = BridgeRecipientError;

		fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
			BoundedVec::<u8, BridgeRecipientMaxLen>::try_from(value.to_vec())
				.map(BridgeRecipient)
				.map_err(|_| BridgeRecipientError::TooLong)
		}
	}

	impl TryFrom<Vec<u8>> for BridgeRecipient {
		type Error = BridgeRecipientError;

		fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
			BoundedVec::<u8, BridgeRecipientMaxLen>::try_from(value)
				.map(BridgeRecipient)
				.map_err(|_| BridgeRecipientError::TooLong)
		}
	}

	impl UncheckedFrom<H256> for BridgeRecipient {
		fn unchecked_from(value: H256) -> Self {
			let bytes = value.as_bytes();
			BoundedVec::<u8, BridgeRecipientMaxLen>::try_from(bytes.to_vec())
				.map(BridgeRecipient)
				.expect("H256 length fits within bridge recipient bounds; qed")
		}
	}

	impl From<BridgeRecipient> for Vec<u8> {
		fn from(value: BridgeRecipient) -> Self {
			value.0.into()
		}
	}
}

pub mod well_known_keys {
	use super::hex;

	pub const MIDNIGHT_STATE_KEY: &[u8] =
		&hex!["2a760f9a173a6df5cd4373ff49fa999bf39a107f2d8d3854c9aba9b021f43d9c"];

	pub const MIDNIGHT_NETWORK_ID_KEY: &[u8] =
		&hex!["2a760f9a173a6df5cd4373ff49fa999b47872dec514b30607df0c271efce9fc4"];
}
