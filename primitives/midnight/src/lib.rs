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

//! # Midnight node primitives
//!
//! Shared types and traits that define the boundary between the Midnight node
//! and the Midnight ledger.
//!
//! The node frames, validates, weighs, and routes transactions; the ledger
//! decodes and interprets the opaque payload carried by a ledger transaction.
//! Everything in this crate sits on the node side of that boundary.
//!
//! The crate exposes:
//!
//! - The [`TransactionType`] and [`TransactionTypeV2`] classification
//!   vocabulary, published to off-node consumers through runtime metadata.
//! - The [`MidnightSystemTransactionExecutor`] seam, by which other pallets
//!   apply a serialized system transaction to the ledger.
//! - The [`LedgerStateProviderMut`] and [`LedgerBlockContextProvider`] seams
//!   for reading and mutating ledger state.
//! - The [`bridge`] module's [`BridgeRecipient`] type, used by the bridge
//!   inherent and the Cardano-to-Midnight bridge pallet.

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

/// Seam by which a pallet applies a serialized system transaction to the ledger.
///
/// `pallet-midnight-system` implements this trait so that other pallets — notably
/// the Cardano-to-Midnight bridge — can apply a system transaction through the
/// privileged ledger path without depending on the system pallet directly. A
/// bridge transfer uses this seam to turn an observed Cardano transfer into a
/// system transaction.
///
/// The argument is the opaque, serialized ledger system transaction; the node
/// passes it to the ledger and does not interpret its contents.
pub trait MidnightSystemTransactionExecutor {
	/// Apply a serialized system transaction and return its ledger transaction hash.
	///
	/// # Errors
	///
	/// Returns a [`DispatchError`] if the ledger rejects the system transaction
	/// (for example, a deserialization or transaction error surfaced through the
	/// ledger API).
	fn execute_system_transaction(
		serialized_system_transaction: Vec<u8>,
	) -> Result<Hash, DispatchError>;
}

/// Classification vocabulary for a decoded transaction, consumed off-node.
///
/// This enumeration is a published vocabulary, exposed to off-node consumers
/// (indexers, the toolkit, and downstream tooling) through runtime metadata. It
/// labels a decoded transaction; it is **not** an in-node dispatch mechanism.
/// The node never matches on these variants to decide how to process a
/// transaction — dispatch happens through the FRAME pallet [`Call`] enums and
/// inherents. The variants exist so a downstream decoder can classify a
/// transaction without re-implementing the dispatch logic.
///
/// [`TransactionTypeV2`] supersedes this type: where this version carries an
/// `Option<Tx>` (the decoded ledger transaction if decoding succeeded), the V2
/// vocabulary carries a `Result<Tx, LedgerApiError>` so a consumer can see why a
/// payload failed to decode.
///
/// [`Call`]: https://docs.rs/frame-support/latest/frame_support/pallet_macros/attr.call.html
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, Debug, TypeInfo)]
pub enum TransactionType {
	/// A ledger-carrying transaction: the opaque ledger payload and, if it
	/// decoded, the decoded ledger transaction.
	MidnightTx(Vec<u8>, Option<Tx>),
	/// The block timestamp, surfaced as a first-class transaction value.
	TimestampTx(u64),
	/// A transaction the classifier does not recognize (for example, a future
	/// or unknown variant).
	UnknownTx,
}

/// Current classification vocabulary for a decoded transaction, consumed off-node.
///
/// Like [`TransactionType`], this enumeration is a published vocabulary exposed
/// to off-node consumers through runtime metadata to label a decoded
/// transaction. It is **not** an in-node dispatch mechanism — the node never
/// matches on these variants; dispatch happens through the FRAME pallet `Call`
/// enums and inherents.
///
/// This version supersedes [`TransactionType`] by carrying the ledger decode
/// `Result` rather than an `Option`, so a consumer can observe a decode failure
/// ([`LedgerApiError`]) instead of only the absence of a decoded transaction.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, Debug, TypeInfo)]
pub enum TransactionTypeV2 {
	/// A ledger-carrying transaction: the opaque ledger payload and the result
	/// of decoding it.
	MidnightTx(Vec<u8>, Result<Tx, LedgerApiError>),
	/// The block timestamp, surfaced as a first-class transaction value.
	TimestampTx(u64),
	/// A transaction the classifier does not recognize (for example, a future
	/// or unknown variant).
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

	/// A Midnight recipient address carried by a bridge transfer.
	///
	/// The bridge pallet and the bridge inherent data provider use this type to
	/// name the beneficiary of a Cardano-to-Midnight transfer. The address is
	/// bounded to [`BRIDGE_RECIPIENT_MAX_BYTES`] bytes so the recipient encoded
	/// in the bridge datum has a fixed maximum size and a predictable encoded
	/// length; an address exceeding the bound is rejected with
	/// [`BridgeRecipientError::TooLong`].
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
