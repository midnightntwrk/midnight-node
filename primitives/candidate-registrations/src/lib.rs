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

//! # Candidate registration primitives
//!
//! Types shared by the node and the runtime for the permissionless part of
//! Ariadne's candidate pool: Cardano SPOs that register keys on Cardano and
//! earn a chance at a committee seat from their ADA stake.
//!
//! A registration is an output at the candidates validator address carrying a
//! Registration Token. The node observes deposits of such outputs since a
//! stored cursor, decodes their datums into [`RegistrationDatum`], and hands a
//! bounded batch to `pallet-candidate-registrations`, which decides each one's
//! validity and maintains the registration set.
//!
//! What lives here is only what both sides must agree on: the datum, the
//! message its signatures cover, the cursor, and the inherent carrying them.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use parity_scale_codec::Encode;
use sp_api::decl_runtime_apis;

#[cfg(feature = "std")]
use parity_scale_codec::{Decode, DecodeWithMemTracking};

pub mod message;
pub mod traits;
pub mod types;

pub use message::RegistrationMessage;
pub use traits::{CrossChainAccount, KeyCollision, SessionKeyAuthority};
pub use types::*;

/// How many registration events one block may carry.
///
/// The value runtimes give `Config::MaxRegistrationEventsPerBlock`. It is both
/// the operating limit and the inherent's decode bound. The node reads it from
/// [`CandidateRegistrationsApi`] rather than holding its own copy, so every
/// node bounds its observation the way the runtime will enforce it.
///
/// It changes only when the runtime blob does. Registrations are rare — one
/// Cardano transaction each, from a population capped by Cardano's pool count
/// — so it would be hard for attackers to make the pallet lag in observability.
pub const MAX_REGISTRATION_EVENTS_PER_BLOCK: u32 = 64;

/// Default ceiling on how many pools the registration map may hold.
///
/// Sized comfortably above Cardano's pool count so it never binds in practice.
/// It exists only so the map has a stated bound.
pub const DEFAULT_MAX_REGISTRATIONS: u32 = 100_000;

/// Length of a Cardano minting policy id.
pub const POLICY_ID_LENGTH: u32 = 28;

/// Longest Bech32 Cardano address, as in
/// `midnight_primitives_cnight_observation::CARDANO_BECH32_ADDRESS_MAX_LENGTH`.
pub const CARDANO_BECH32_ADDRESS_MAX_LENGTH: u32 = 108;

/// Inherent carrying one block's worth of observed registrations.
pub const INHERENT_IDENTIFIER: sp_inherents::InherentIdentifier = *b"mncandrg";

/// What can go wrong between the node's observation and the runtime's view of it.
#[derive(Encode, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(Decode, DecodeWithMemTracking, thiserror::Error))]
pub enum InherentError {
	/// The block carries no registration inherent although the observation
	/// produced one.
	#[cfg_attr(feature = "std", error("Candidate registration inherent data is missing"))]
	Missing,
	/// The inherent data could not be decoded.
	#[cfg_attr(feature = "std", error("Candidate registration inherent data failed to decode"))]
	DecodeFailed,
	/// The proposer's inherent does not match what this node observed.
	#[cfg_attr(
		feature = "std",
		error("Candidate registration inherent does not match the local observation")
	)]
	Mismatch,
	/// The block carries a registration inherent although the observation
	/// produced none.
	#[cfg_attr(feature = "std", error("Unexpected candidate registration inherent"))]
	Unexpected,
}

impl sp_inherents::IsFatalError for InherentError {
	fn is_fatal_error(&self) -> bool {
		true
	}
}

decl_runtime_apis! {
	/// What the node needs from the runtime to observe registrations, and what
	/// an operator needs to find out whether a registration will be admitted.
	pub trait CandidateRegistrationsApi {
		/// Bech32 address of the candidates validator on Cardano.
		fn validator_address() -> Vec<u8>;

		/// Policy id of the Registration Token. An output at the validator
		/// address without this token is not a registration.
		fn registration_policy_id() -> Vec<u8>;

		/// Cursor: the first Cardano transaction not yet applied.
		fn next_cardano_position() -> CardanoTxPosition;

		/// How many registration events the runtime will accept in one block.
		/// The node uses it to size its indexer query.
		fn max_registration_events_per_block() -> u32;

		/// Runs the full admission check against current state without
		/// changing it, so an operator can find out whether a registration
		/// will be admitted before — or after — posting it.
		///
		/// `None` means it would be admitted as valid.
		fn validate_registration(datum: RegistrationDatum) -> Option<RegistrationInvalidReason>;
	}
}
