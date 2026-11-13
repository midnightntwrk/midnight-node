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

//! The registration record as it crosses the Cardano/Midnight boundary.

use alloc::vec::Vec;
use frame_support::{storage::bounded_vec::BoundedVec, traits::ConstU32};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sidechain_domain::{MainchainKeyHash, StakePoolPublicKey, UtxoId};

/// A `sp_core::crypto::KeyTypeId` as it travels in a datum: four opaque bytes.
///
/// `KeyTypeId` itself is deliberately not used here — it implements neither
/// `MaxEncodedLen` nor `DecodeWithMemTracking`, both of which storage items and
/// extrinsic arguments need. The encoding is identical either way.
pub type KeyTypeIdBytes = [u8; 4];

/// Longest public key the decoder admits. The key types in play today are 32
/// bytes (sr25519, ed25519) or 33 (compressed ECDSA); the slack is for a future
/// scheme, not for the datum to grow freely.
pub const MAX_KEY_LENGTH: u32 = 64;

/// Longest proof of possession the decoder admits: 64 bytes for sr25519 and
/// ed25519, 65 for a recoverable ECDSA signature.
pub const MAX_KEY_PROOF_LENGTH: u32 = 96;

/// Most keys one registration may carry. A registration proves one key per key
/// type the runtime knows about, so this only has to stay ahead of `SessionKeys`.
pub const MAX_KEYS_PER_REGISTRATION: u32 = 16;

/// Compressed secp256k1 public key.
pub const CROSS_CHAIN_PUBLIC_KEY_LEN: usize = 33;

/// Recoverable secp256k1 signature: r || s || recovery id.
pub const CROSS_CHAIN_SIGNATURE_LEN: usize = 65;

/// Ed25519 signature produced by the pool's cold key.
pub const POOL_SIGNATURE_LEN: usize = 64;

/// Bytes of one public key, as they appeared in the datum.
pub type KeyBytes = BoundedVec<u8, ConstU32<MAX_KEY_LENGTH>>;

/// Bytes of one proof of possession, as they appeared in the datum.
pub type KeyProofBytes = BoundedVec<u8, ConstU32<MAX_KEY_PROOF_LENGTH>>;

/// A public key together with the proof that the registrant holds its private
/// half.
///
/// The two are stored as one record rather than as parallel lists so that they
/// cannot be misaligned, be of different lengths, or be reordered independently.
#[derive(
	Clone,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	PartialOrd,
	Ord,
)]
pub struct CandidateKey {
	/// Key type id, e.g. `*b"gran"`.
	pub id: KeyTypeIdBytes,
	/// The public key.
	pub bytes: KeyBytes,
	/// Proof of possession of `bytes` over the registration message.
	pub proof: KeyProofBytes,
}

/// The key list of one registration.
///
/// Canonical form is ascending by `id` with no duplicate `id`; a datum that is
/// not in canonical form is admitted as an invalid registration rather than
/// re-sorted.
pub type CandidateKeys = BoundedVec<CandidateKey, ConstU32<MAX_KEYS_PER_REGISTRATION>>;

/// Compressed secp256k1 key that determines the candidate's `AccountId`.
#[derive(
	Clone,
	Copy,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	PartialOrd,
	Ord,
)]
pub struct CrossChainPublicKeyBytes(pub [u8; CROSS_CHAIN_PUBLIC_KEY_LEN]);

/// Recoverable secp256k1 signature over the registration message.
#[derive(
	Clone,
	Copy,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
)]
pub struct CrossChainSignatureBytes(pub [u8; CROSS_CHAIN_SIGNATURE_LEN]);

/// Ed25519 signature of the pool's cold key over the registration message.
///
/// Midnight never checks it: the Registration minting policy already did, and
/// the token on the UTxO is the evidence. It is carried and stored so an
/// operator can see what was registered on their behalf.
#[derive(
	Clone,
	Copy,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
)]
pub struct PoolSignatureBytes(pub [u8; POOL_SIGNATURE_LEN]);

/// Which direction an observed deposit moves the registration state machine.
///
/// Both flags arrive the same way — as a deposit. Going inactive is not a
/// UTxO disappearing: A6 says the transaction spending a registration output
/// deposits a new registration output, which may carry the Inactive flag, so
/// the layers close to the indexer never have to track consumption at all.
/// The deposit that stands a pool down need not resemble the one it replaces.
#[derive(
	Clone,
	Copy,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum RegistrationStatus {
	/// The pool is claiming the keys in this datum.
	Active,
	/// The pool is standing down the registration it last posted.
	Inactive,
}

/// The decoded datum of one output at the candidates validator address.
#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub struct RegistrationDatum {
	/// Whether this deposit claims a candidacy or stands one down.
	pub status: RegistrationStatus,
	/// The pool's cold verification key — the pool's identity.
	pub cold_pub_key: StakePoolPublicKey,
	/// That key's signature over the registration message. Checked on Cardano.
	pub pool_signature: PoolSignatureBytes,
	/// The key determining the candidate's `AccountId`.
	pub cross_chain_pub_key: CrossChainPublicKeyBytes,
	/// Proof that the registrant holds `cross_chain_pub_key`.
	pub cross_chain_signature: CrossChainSignatureBytes,
	/// Session keys, each next to its own proof of possession.
	pub keys: CandidateKeys,
}

impl RegistrationDatum {
	/// The pool id this registration is attributed to: blake2b-224 of the cold key.
	pub fn pool_id(&self) -> MainchainKeyHash {
		self.cold_pub_key.hash()
	}
}

/// Position of a Cardano transaction in the chain's total order.
///
/// At most one registration output exists per transaction, so a transaction is
/// a fine enough cursor: nothing inside it has to be addressed.
#[derive(
	Clone,
	Copy,
	Debug,
	Default,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	serde::Serialize,
	serde::Deserialize,
)]
pub struct CardanoTxPosition {
	/// Cardano block number.
	pub block_no: u32,
	/// Zero-based index of the transaction within `block_no`.
	pub tx_index_in_block: u32,
}

impl CardanoTxPosition {
	/// Creates a position from its parts.
	pub const fn new(block_no: u32, tx_index_in_block: u32) -> Self {
		Self { block_no, tx_index_in_block }
	}

	/// The position just past this one, which is what a cursor stores once this
	/// transaction has been applied.
	pub const fn next(self) -> Self {
		Self {
			block_no: self.block_no,
			tx_index_in_block: self.tx_index_in_block.saturating_add(1),
		}
	}
}

impl core::fmt::Display for CardanoTxPosition {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(f, "{}#{}", self.block_no, self.tx_index_in_block)
	}
}

/// One registration deposit, as the node hands it to the runtime.
#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub struct ObservedRegistration {
	/// Where the depositing transaction sits on Cardano.
	pub position: CardanoTxPosition,
	/// The deposited output. Kept so an operator can tie a stored record back
	/// to the UTxO they posted.
	pub utxo: UtxoId,
	/// The decoded datum.
	pub datum: RegistrationDatum,
}

/// Why a registration was admitted as invalid.
///
/// Stored alongside the record, so an operator who posted a bad registration
/// can read back what was wrong with it rather than guessing.
#[derive(
	Clone,
	Copy,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum RegistrationInvalidReason {
	/// `keys` was not ascending by `id`, or carried a duplicate `id`.
	NonCanonicalKeys,
	/// One of the runtime's session key types was absent from `keys`.
	MissingKeyProof(KeyTypeIdBytes),
	/// `keys` carried a key type the runtime cannot verify.
	UnknownKeyType(KeyTypeIdBytes),
	/// A key's proof of possession did not verify against it.
	InvalidKeyProof(KeyTypeIdBytes),
	/// `cross_chain_signature` did not verify against `cross_chain_pub_key`.
	InvalidCrossChainSignature,
	/// A session key in this registration is bound to another cross-chain key.
	SessionKeyReuse,
	/// `pallet_session` refused the keys although no key in the registration is
	/// held by anyone else — so the refusal was not key reuse at all.
	SessionBindingFailed,
}

/// The state a stored registration is in, per the registration state machine.
///
/// A registration first appears Active or, if it failed admission, Invalid.
/// A pool may register afresh from any of the three, so every state can move
/// to Active or Invalid; only a state that is not already Inactive can be
/// stood down. The pallet keeps only the state the latest event left behind,
/// and never drops a record — the map only ever grows, so a pool that has
/// registered once stays addressable whatever it did afterwards.
#[derive(
	Clone,
	Copy,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum RegistrationState {
	/// Admitted. The pool is a candidate as of the block this was applied in.
	Active,
	/// The pool stood this registration down on Cardano. Kept, not a candidate.
	Inactive,
	/// Admitted and kept for the operator to inspect, but not a candidate.
	Invalid(RegistrationInvalidReason),
}

impl RegistrationState {
	/// Whether this record makes its pool eligible for committee selection.
	pub fn is_active(&self) -> bool {
		matches!(self, Self::Active)
	}
}

/// What the pallet keeps for one pool.
///
/// One variant per state the registration state machine can leave behind, so
/// a record only ever carries fields that describe the same event. `Keys` is
/// the runtime's `SessionKeys`, reached through
/// [`SessionKeyAuthority`](crate::SessionKeyAuthority).
#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub enum RegistrationRecord<Keys> {
	/// Admitted. The pool is a valid candidate.
	Active {
		/// The pool's cold verification key, whose hash keys this record.
		cold_pub_key: StakePoolPublicKey,
		/// The key the candidate's `AccountId` is derived from.
		cross_chain_pub_key: CrossChainPublicKeyBytes,
		/// The session keys this registration proved possession of, as they
		/// were assembled and bound at admission.
		session_keys: Keys,
		/// Where the registration was observed.
		observed_at: CardanoTxPosition,
		/// The output that carried it, and still carries the datum.
		utxo: UtxoId,
	},
	/// Invalid registration trace, kept for the operator to inspect, but not a candidate.
	Invalid {
		/// The pool's cold verification key, whose hash keys this record.
		cold_pub_key: StakePoolPublicKey,
		/// What was wrong with it. Data is to be read from Cardano.
		reason: RegistrationInvalidReason,
		/// Where the registration was observed.
		observed_at: CardanoTxPosition,
		/// The output that carried it.
		utxo: UtxoId,
	},
	/// The pool inactivated its registration on Cardano.
	Inactive {
		/// The pool's cold verification key, whose hash keys this record.
		cold_pub_key: StakePoolPublicKey,
		/// Where the inactivation was observed.
		observed_at: CardanoTxPosition,
		/// The output that carried the inactivation.
		utxo: UtxoId,
	},
}

impl<Keys> RegistrationRecord<Keys> {
	/// The record an admitted deposit leaves behind, whichever way it went.
	pub fn admitted(
		observed: &ObservedRegistration,
		outcome: Result<Keys, RegistrationInvalidReason>,
	) -> Self {
		let datum = &observed.datum;
		match outcome {
			Ok(session_keys) => Self::Active {
				cold_pub_key: datum.cold_pub_key.clone(),
				cross_chain_pub_key: datum.cross_chain_pub_key,
				session_keys,
				observed_at: observed.position,
				utxo: observed.utxo,
			},
			Err(reason) => Self::Invalid {
				cold_pub_key: datum.cold_pub_key.clone(),
				reason,
				observed_at: observed.position,
				utxo: observed.utxo,
			},
		}
	}

	/// The record an inactivation leaves behind.
	pub fn inactivated(observed: &ObservedRegistration) -> Self {
		Self::Inactive {
			cold_pub_key: observed.datum.cold_pub_key.clone(),
			observed_at: observed.position,
			utxo: observed.utxo,
		}
	}

	/// Which state this record is in, for events and for callers that only
	/// need the verdict. A projection, not a second source of truth.
	pub fn state(&self) -> RegistrationState {
		match self {
			Self::Active { .. } => RegistrationState::Active,
			Self::Invalid { reason, .. } => RegistrationState::Invalid(*reason),
			Self::Inactive { .. } => RegistrationState::Inactive,
		}
	}

	/// Whether this record makes its pool eligible for committee selection.
	pub fn is_active(&self) -> bool {
		matches!(self, Self::Active { .. })
	}

	/// Where the event behind this record was observed.
	pub fn observed_at(&self) -> CardanoTxPosition {
		match self {
			Self::Active { observed_at, .. }
			| Self::Invalid { observed_at, .. }
			| Self::Inactive { observed_at, .. } => *observed_at,
		}
	}

	/// The output that carried the event behind this record.
	pub fn utxo(&self) -> UtxoId {
		match self {
			Self::Active { utxo, .. }
			| Self::Invalid { utxo, .. }
			| Self::Inactive { utxo, .. } => *utxo,
		}
	}
}

/// Everything one block's observation hands to the runtime.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct RegistrationsObservation {
	/// Deposits since the stored cursor, in Cardano order, both flags, valid or
	/// not. Bounded by the per-block limit the runtime publishes.
	pub registrations: Vec<ObservedRegistration>,
	/// Where the cursor moves once these have been applied.
	pub next_position: CardanoTxPosition,
}
