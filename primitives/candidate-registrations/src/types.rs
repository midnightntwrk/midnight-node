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
use sidechain_domain::{MainchainKeyHash, McEpochNumber, UtxoId};

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

/// Most keys one registration may carry: one per key type the runtime defines,
/// plus room for types a later runtime will define, which are carried but
/// ignored until then (MIP rule 13). A node with more is malformed.
pub const MAX_KEYS_PER_REGISTRATION: u32 = 16;

/// Compressed secp256k1 public key.
pub const CROSS_CHAIN_PUBLIC_KEY_LEN: usize = 33;

/// secp256k1 signature without a recovery id: r || s.
pub const CROSS_CHAIN_SIGNATURE_LEN: usize = 64;

/// The blake2b-256 hash of a node's `crch`: how the runtime identifies the
/// key the registration set keeps unique.
///
/// The contract leaves `crch` any length that fits a node (MIP rule 9, Nodes),
/// so the runtime keeps its hash, which has one size whatever the key. A
/// registration whose `crch` Midnight cannot read is still found when it is
/// deactivated, by the hash of the bytes its deactivated node keeps.
#[derive(
	Clone,
	Copy,
	Debug,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
)]
pub struct CrchHash(pub [u8; 32]);

impl CrchHash {
	/// The hash of a `crch` as the node's datum holds it.
	pub fn of(crch: &[u8]) -> Self {
		use sp_runtime::traits::{BlakeTwo256, Hash};
		Self(BlakeTwo256::hash(crch).0)
	}
}

/// The most bytes the datum of a registration node holds without its two list
/// links: `night_registration`'s `NODE_DATUM_MAX` (MIP rule 9, Nodes). No field of
/// a node the contract admits is longer.
pub const NODE_DATUM_MAX: u32 = 4_500;

/// A node's `attributes` field, as CBOR bytes exactly as they are on Cardano.
pub type AttributesBytes = BoundedVec<u8, ConstU32<NODE_DATUM_MAX>>;

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
	/// Proof of possession of `bytes` over the node's
	/// [`SessionKeyMessage`](crate::SessionKeyMessage).
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

/// Length of a Midnight reward address.
pub const REWARD_ADDRESS_LEN: usize = 32;

/// The Midnight address that claims a pool's rewards (MIP rule 9, Nodes, and
/// rule 23, Snapshot record).
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
pub struct RewardAddressBytes(pub [u8; REWARD_ADDRESS_LEN]);

/// secp256k1 signature over the [`CrossChainKeyMessage`](crate::CrossChainKeyMessage), `r || s`.
///
/// The datum carries no recovery id; the key it was made with is the
/// registration's `crch`, so verification tries both recovery ids against it.
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

/// Which state a registration node moved to.
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
pub enum RegistrationEventKind {
	/// An active node: a `Create` or an `Update`.
	Register,
	/// An inactive node: a `Deactivate`.
	Deactivate,
}

/// What Midnight reads of an active `Registration` node of the NIGHT
/// registration set (`lib/registration/types.ak` in midnight-reserve-contracts).
///
/// The node's other fields, the list links and `owner`, are Cardano's
/// bookkeeping and are not carried.
#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub struct RegistrationDatum {
	/// The pool's id: blake2b-224 of its cold verification key. The
	/// registering transaction is signed by that key, which the validator checks.
	pub pool_id: MainchainKeyHash,
	/// The key determining the candidate's `AccountId`: the node's `crch`.
	pub cross_chain_pub_key: CrossChainPublicKeyBytes,
	/// Proof that the registrant holds `cross_chain_pub_key`: the node's
	/// `crch_signature`.
	pub cross_chain_signature: CrossChainSignatureBytes,
	/// Session keys, each next to its own proof of possession: the node's
	/// `keys` joined with its `signatures` by key type id.
	pub keys: CandidateKeys,
	/// Where Midnight pays the pool's rewards.
	pub reward_address: RewardAddressBytes,
	/// STAR per Cardano epoch the pool takes before its margin. The meaning is
	/// the NIGHT rewards MIP's; this one only carries it.
	pub fixed_cost: u128,
	/// The node's `attributes`, as on Cardano: carried, not read. No attribute
	/// is defined yet (MIP rule 13), so admission ignores them and the record
	/// does not keep them. The node does not interpret them either: a node
	/// that did could read an old block differently from the one that made it.
	pub attributes: AttributesBytes,
}

/// An active node, as a `Create` or an `Update` leaves it.
#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub enum RegistrationNode {
	/// A node Midnight reads.
	Readable(RegistrationDatum),
	/// A node Midnight cannot read: the session keys, their signatures or the
	/// cross-chain key are not in the shape Midnight expects. The validator
	/// does not check those fields, so this is the registrant's error and is
	/// recorded against their pool, not a reason to stop observing.
	Malformed {
		/// The pool the node belongs to.
		pool_id: MainchainKeyHash,
		/// The hash of the node's `crch`, kept so its deactivation can be
		/// attributed.
		crch_hash: CrchHash,
	},
}

impl RegistrationNode {
	/// The pool the node belongs to.
	pub fn pool_id(&self) -> MainchainKeyHash {
		match self {
			Self::Readable(datum) => datum.pool_id,
			Self::Malformed { pool_id, .. } => *pool_id,
		}
	}

	/// The hash of the node's `crch`.
	pub fn crch_hash(&self) -> CrchHash {
		match self {
			Self::Readable(datum) => datum.crch_hash(),
			Self::Malformed { crch_hash, .. } => *crch_hash,
		}
	}
}

/// The state one observed NIGHT registration transaction left a node in, read
/// from the node's datum alone (MIP rule 12, Transaction filter).
///
/// The datum does not say whether a `Create` or an `Update` produced an active
/// node, and Midnight does not need to know: the node's newest state is the
/// pool's registration, and its validity is decided for that state (rule 20,
/// Registration validity).
#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub enum RegistrationEvent {
	/// An active node, left by a `Create` or an `Update`: the pool's
	/// registration from now on.
	Registered(RegistrationNode),
	/// An inactive node, left by a `Deactivate`: deactivated for good.
	///
	/// The deactivated node keeps only its `crch`, so that is what identifies
	/// it; the runtime knows which pool registered it.
	Deactivated {
		/// The hash of the deactivated node's `crch`.
		crch_hash: CrchHash,
	},
}

impl RegistrationDatum {
	/// The hash of the registration's `crch`, as the runtime indexes it.
	pub fn crch_hash(&self) -> CrchHash {
		CrchHash::of(&self.cross_chain_pub_key.0)
	}
}

impl RegistrationEvent {
	/// The hash of the node's `crch`.
	pub fn crch_hash(&self) -> CrchHash {
		match self {
			Self::Registered(node) => node.crch_hash(),
			Self::Deactivated { crch_hash } => *crch_hash,
		}
	}

	/// Which state the event moves the node to.
	pub fn kind(&self) -> RegistrationEventKind {
		match self {
			Self::Registered(_) => RegistrationEventKind::Register,
			Self::Deactivated { .. } => RegistrationEventKind::Deactivate,
		}
	}
}

/// A pool's active registration, as committee selection takes it.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct ActiveRegistration {
	/// The pool's id: blake2b-224 of its cold verification key.
	pub pool_id: MainchainKeyHash,
	/// The registrant's compressed secp256k1 key: the candidate's authority id.
	pub cross_chain_pub_key: CrossChainPublicKeyBytes,
	/// The session keys admission verified, raw, by key type id, in the
	/// runtime's key order.
	pub keys: Vec<(KeyTypeIdBytes, Vec<u8>)>,
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

/// One registration event, as the node hands it to the runtime.
#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub struct ObservedRegistration {
	/// Where the depositing transaction sits on Cardano.
	pub position: CardanoTxPosition,
	/// The Cardano epoch of the block holding the transaction.
	pub mc_epoch: McEpochNumber,
	/// The node output the transaction created, its first output. Kept so an
	/// operator can tie a stored record back to the UTxO they posted.
	pub utxo: UtxoId,
	/// What the transaction did.
	pub event: RegistrationEvent,
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
	/// The node's session keys, their signatures or its cross-chain key are not
	/// in the shape Midnight reads.
	MalformedDatum,
	/// `keys` was not ascending by `id`, or carried a duplicate `id`.
	NonCanonicalKeys,
	/// One of the runtime's session key types was absent from `keys`.
	MissingKeyProof(KeyTypeIdBytes),
	/// A key's proof of possession did not verify against it.
	InvalidKeyProof(KeyTypeIdBytes),
	/// `cross_chain_signature` did not verify against `cross_chain_pub_key`.
	InvalidCrossChainSignature,
	/// A session key in this registration belongs to another account: one a
	/// registration with another cross-chain key used, ever, or one
	/// `pallet_session` holds for another account.
	SessionKeyReuse,
}

/// The state a stored registration is in, per the registration state machine.
///
/// A registration first appears Active or, if it failed admission, Invalid.
/// A pool may register afresh from any of the three, so every state can move
/// to Active or Invalid; only a state that is not already Inactive can be
/// inactivated. The pallet keeps the state the latest event left behind, and
/// the past states selection may still read ([`RegistrationHistory`]). It never
/// drops a pool — the map only ever grows, so a pool that has registered once
/// stays addressable whatever it did afterwards.
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
/// the runtime's `SessionKeys`.
#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub enum RegistrationRecord<Keys> {
	/// Admitted. The pool is a valid candidate.
	Active {
		/// The key the candidate's `AccountId` is derived from.
		cross_chain_pub_key: CrossChainPublicKeyBytes,
		/// The session keys this registration proved possession of, as they
		/// were assembled and bound at admission.
		session_keys: Keys,
		/// Where Midnight pays the pool's rewards.
		reward_address: RewardAddressBytes,
		/// STAR per Cardano epoch the pool takes before its margin.
		fixed_cost: u128,
		/// Where the registration was observed.
		observed_at: CardanoTxPosition,
		/// The output that carried it, and still carries the datum.
		utxo: UtxoId,
	},
	/// Invalid registration trace, kept for the operator to inspect, but not a candidate.
	Invalid {
		/// The hash of the registration's `crch`.
		crch_hash: CrchHash,
		/// What was wrong with it. Data is to be read from Cardano.
		reason: RegistrationInvalidReason,
		/// Where the registration was observed.
		observed_at: CardanoTxPosition,
		/// The output that carried it.
		utxo: UtxoId,
	},
	/// The pool inactivated its registration on Cardano.
	Inactive {
		/// The hash of the deactivated registration's `crch`.
		crch_hash: CrchHash,
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
		crch_hash: CrchHash,
		outcome: Result<(&RegistrationDatum, Keys), RegistrationInvalidReason>,
	) -> Self {
		match outcome {
			Ok((datum, session_keys)) => Self::Active {
				cross_chain_pub_key: datum.cross_chain_pub_key,
				session_keys,
				reward_address: datum.reward_address,
				fixed_cost: datum.fixed_cost,
				observed_at: observed.position,
				utxo: observed.utxo,
			},
			Err(reason) => Self::Invalid {
				crch_hash,
				reason,
				observed_at: observed.position,
				utxo: observed.utxo,
			},
		}
	}

	/// The record an inactivation leaves behind.
	pub fn inactivated(observed: &ObservedRegistration, crch_hash: CrchHash) -> Self {
		Self::Inactive { crch_hash, observed_at: observed.position, utxo: observed.utxo }
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

	/// The hash of the `crch` of the registration behind this record.
	pub fn crch_hash(&self) -> CrchHash {
		match self {
			Self::Active { cross_chain_pub_key, .. } => CrchHash::of(&cross_chain_pub_key.0),
			Self::Invalid { crch_hash, .. } | Self::Inactive { crch_hash, .. } => *crch_hash,
		}
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

/// How many Cardano epochs before the epoch it selects for committee selection reads the
/// registrations: selection for epoch `E` takes them as they stood at the start of epoch
/// `E - 3`, where Cardano fixed the ADA stake Ariadne reads for `E` (MIP rule 16, Snapshot
/// point).
pub const SNAPSHOT_EPOCH_OFFSET: u32 = 3;

/// Most past states [`RegistrationHistory`] keeps: the state before the oldest snapshot
/// still in use, and one per Cardano epoch from that snapshot to the current state's.
pub const MAX_PREVIOUS_REGISTRATIONS: u32 = SNAPSHOT_EPOCH_OFFSET + 1;

/// A pool's registration state, and the Cardano epoch of the event that left it.
#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub struct RegistrationEntry<Keys> {
	/// The Cardano epoch of the block whose event left this state.
	pub mc_epoch: McEpochNumber,
	/// The state.
	pub record: RegistrationRecord<Keys>,
}

/// A pool's registration now, and as it stood at the starts of the Cardano epochs
/// committee selection may still read.
#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub struct RegistrationHistory<Keys> {
	/// The latest state: what admission, deactivation and the key indices work from.
	pub current: RegistrationEntry<Keys>,
	/// Earlier states, newest first, each from an earlier Cardano epoch than the one
	/// before it in the list.
	pub previous: BoundedVec<RegistrationEntry<Keys>, ConstU32<MAX_PREVIOUS_REGISTRATIONS>>,
}

impl<Keys> RegistrationHistory<Keys> {
	/// A pool's first state.
	pub fn new(entry: RegistrationEntry<Keys>) -> Self {
		Self { current: entry, previous: BoundedVec::new() }
	}

	/// Makes `entry` the pool's latest state.
	///
	/// A state from the same Cardano epoch as the current one replaces it: only the
	/// last state of an epoch can be the one at the start of a later epoch. Past states
	/// no snapshot can read any more are dropped: events arrive in Cardano order, and
	/// selection reads the snapshot at the start of `E - SNAPSHOT_EPOCH_OFFSET` only
	/// while Cardano is in epoch `E - 1` or `E`, so no later read needs a snapshot older
	/// than `SNAPSHOT_EPOCH_OFFSET` epochs before `entry`'s.
	pub fn record(&mut self, entry: RegistrationEntry<Keys>) {
		if entry.mc_epoch <= self.current.mc_epoch {
			self.current = entry;
			return;
		}
		let oldest_snapshot = entry.mc_epoch.0.saturating_sub(SNAPSHOT_EPOCH_OFFSET);
		let previous = core::mem::replace(&mut self.previous, BoundedVec::new());
		let replaced = core::mem::replace(&mut self.current, entry);
		let mut kept: Vec<RegistrationEntry<Keys>> = Vec::new();
		for past in core::iter::once(replaced).chain(previous) {
			let before_oldest_snapshot = past.mc_epoch.0 < oldest_snapshot;
			kept.push(past);
			if before_oldest_snapshot {
				break;
			}
		}
		// At most one state per epoch from `oldest_snapshot` up to `entry`'s, and one
		// before it: never more than the bound.
		self.previous = BoundedVec::truncate_from(kept);
	}

	/// The latest state.
	pub fn current(&self) -> &RegistrationRecord<Keys> {
		&self.current.record
	}

	/// The state at the start of Cardano epoch `mc_epoch`: the one the last event of an
	/// earlier epoch left, or `None` when the pool had not registered by then.
	pub fn at_start_of(&self, mc_epoch: McEpochNumber) -> Option<&RegistrationRecord<Keys>> {
		core::iter::once(&self.current)
			.chain(self.previous.iter())
			.find(|entry| entry.mc_epoch < mc_epoch)
			.map(|entry| &entry.record)
	}
}

/// Everything one block's observation hands to the runtime.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct RegistrationsObservation {
	/// Events since the stored cursor, in Cardano order, valid or not. Bounded
	/// by the per-block limit the runtime publishes.
	pub registrations: Vec<ObservedRegistration>,
	/// Where the cursor moves once these have been applied.
	pub next_position: CardanoTxPosition,
	/// The Cardano epoch of the last block read in full: the block before
	/// `next_position`'s. `None` when no block has been read in full yet.
	///
	/// An epoch ends with no event at all as often as not, so the events alone
	/// cannot say that an epoch has been read through; this does (see
	/// [`snapshot_complete`]).
	pub read_through_epoch: Option<McEpochNumber>,
}

/// Whether the registrations snapshot for Cardano epoch `for_mc_epoch` is
/// complete, given the epoch of the last Cardano block read in full.
///
/// The snapshot is the state at the start of `for_mc_epoch -
/// SNAPSHOT_EPOCH_OFFSET`; a block of that epoch or a later one having been read
/// in full means every block before that start has been read (MIP rule 17,
/// Snapshot finality, and rule 27, Bounded units).
pub fn snapshot_complete(
	for_mc_epoch: McEpochNumber,
	read_through_epoch: Option<McEpochNumber>,
) -> bool {
	read_through_epoch
		.is_some_and(|read| read.0 >= for_mc_epoch.0.saturating_sub(SNAPSHOT_EPOCH_OFFSET))
}
