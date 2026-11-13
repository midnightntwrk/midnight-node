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

//! # Candidate Registrations Pallet
//!
//! Maintains the permissionless part of Ariadne's candidate pool: Cardano SPOs
//! that have registered keys on Cardano and are eligible for a committee seat
//! on the strength of their ADA stake.
//!
//! Registrations are event-sourced. The node watches outputs appearing at the
//! candidates validator address since a stored cursor, keeps only those
//! carrying a Registration Token, decodes their datums, and hands a bounded
//! batch to [`Pallet::apply_registrations`] as an inherent. The cursor is a
//! Cardano transaction position; there is at most one registration output per
//! transaction, so nothing finer is needed.
//!
//! Everything upstream of this pallet is Cardano's: the Registration minting
//! policy has already checked the datum's shape and the pool's signature over
//! it, and will only mint for a real SPO. The token on the output is the
//! evidence, and it is also the cheap filter that makes depositing junk at a
//! permissionless address pointless. What Cardano cannot check is the session
//! keys — so proving possession of those is this pallet's job.
//!
//! ## Three rules decide what the map holds
//!
//! 1. **Newest attributed registration wins.** A registration attributed to
//!    pool `P` replaces `P`'s entry, even an invalid one over a valid one.
//! 2. **Validity is decided once**, from the facts at the position where the
//!    registration was observed, and never revisited.
//! 3. **Superseded is final.** A replaced record is gone. The latest event
//!    decides about the state.
//!
//! An invalid registration is still stored. A Registration Token is reserved
//! for SPOs, so the map holds at most one record per pool whether or not that
//! pool registeration is valid. User can read back what was wrong with it.
//!
//! ## Session keys
//!
//! Operators do not submit `pallet_session::set_keys` themselves; the runtime
//! does it on their behalf. This pallet calls it at admission, which is what
//! turns a key conflict into a property of the registration — decided once,
//! recorded, visible — rather than a committee hand-off that fails at
//! rotation. The pallet deliberately keeps no cross-chain-key-to-session-key
//! index of its own: `pallet_session` already is that index.
//!
//! `purge_keys` is never called, so a session key is not necessarily released
//! from the account it was bound to. Two consequences follow:
//! a key that has been registered once can, possibly, never be registered again by
//! anyone else, and rotating the cross-chain key — which moves the account the
//! keys hang on — requires rotating every session key with it. A registration
//! that rotates only the cross-chain key collides with the pool's *own*
//! previous registration.
//!
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame_support::{
	dispatch::{Pays, PostDispatchInfo},
	pallet_prelude::*,
};
use frame_system::pallet_prelude::*;
use midnight_primitives_candidate_registrations::{
	INHERENT_IDENTIFIER, InherentError, RegistrationsObservation,
};
pub use pallet::*;

pub mod config;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod validation;
pub mod weights;

#[frame_support::pallet]
pub mod pallet {
	use alloc::vec::Vec;
	use frame_support::sp_runtime::TransactionOutcome;
	use frame_support::storage::transactional::with_transaction;
	use midnight_primitives_candidate_registrations::{
		CARDANO_BECH32_ADDRESS_MAX_LENGTH, CardanoTxPosition, CrossChainAccount, KeyCollision,
		MAX_REGISTRATION_EVENTS_PER_BLOCK, ObservedRegistration, POLICY_ID_LENGTH,
		RegistrationDatum, RegistrationInvalidReason, RegistrationMessage, RegistrationRecord,
		RegistrationState, RegistrationStatus, SessionKeyAuthority,
	};
	use sidechain_domain::MainchainKeyHash;
	use sp_runtime::traits::OpaqueKeys;

	use crate::config::CandidateRegistrationsGenesis;
	use crate::validation;
	use crate::weights::WeightInfo;

	use super::*;

	/// The runtime's `SessionKeys`, reached through the authority that both
	/// assembles and binds them.
	pub type KeysOf<T> = <<T as Config>::SessionKeyAuthority as SessionKeyAuthority<
		<T as frame_system::Config>::AccountId,
	>>::Keys;

	/// Bech32 Cardano address, bounded for storage.
	pub type BoundedCardanoAddress = BoundedVec<u8, ConstU32<CARDANO_BECH32_ADDRESS_MAX_LENGTH>>;

	/// Cardano minting policy id, bounded for storage.
	pub type BoundedPolicyId = BoundedVec<u8, ConstU32<POLICY_ID_LENGTH>>;

	/// One block's batch of registration events, bounded so a hostile inherent
	/// cannot make the runtime allocate without bound at decode time.
	pub type RegistrationBatch =
		BoundedVec<ObservedRegistration, ConstU32<MAX_REGISTRATION_EVENTS_PER_BLOCK>>;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Verifies a registration's session keys and binds them to its
		/// account.
		///
		/// The runtime owns both halves because it is the only place that
		/// knows what `SessionKeys` is made of. The pallet learns the session
		/// key types from `Keys::key_ids()` and never has to repeat the list.
		type SessionKeyAuthority: SessionKeyAuthority<Self::AccountId>;

		/// Ceiling on how many pools the map may hold.
		///
		/// Should be sized above Cardano's pool count, so it never binds in practice.
		/// Overflow is treated as a bug - processing of registrations stops
		/// and waits for fix.
		#[pallet::constant]
		type MaxRegistrations: Get<u32>;

		/// Derives a candidate's `AccountId` from their cross-chain key.
		type CrossChainAccount: CrossChainAccount<Self::AccountId>;

		/// Weight information for this pallet's extrinsics.
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A pool registered keys that passed admission. It is a candidate
		/// from this block on.
		Registered {
			/// blake2b-224 of the pool's cold key.
			pool_id: MainchainKeyHash,
			/// The output that carried the registration.
			utxo: sidechain_domain::UtxoId,
		},
		/// A pool registered keys that did not pass admission. The record is
		/// stored anyway, so the operator can see why.
		RegisteredInvalid {
			/// blake2b-224 of the pool's cold key.
			pool_id: MainchainKeyHash,
			/// The output that carried the registration.
			utxo: sidechain_domain::UtxoId,
			/// What was wrong with it.
			reason: RegistrationInvalidReason,
		},
		/// A pool inactivated its registration on Cardano. The record stays in
		/// the map, marked inactive.
		Inactivated {
			/// blake2b-224 of the pool's cold key.
			pool_id: MainchainKeyHash,
			/// The output carrying the inactive datum.
			utxo: sidechain_domain::UtxoId,
		},
		/// State transition that Cardano Cardano should not let to
		/// happen, so it deliberately stops registrations
		/// processing - it means there is a bug to fix in some place.
		IllegalStateTransition {
			/// blake2b-224 of the pool's cold key.
			pool_id: MainchainKeyHash,
			/// The output that carried the deposit.
			utxo: sidechain_domain::UtxoId,
			/// The pool's state before it. `None` would mean no record at all,
			/// which this state machine does not reach — an inactive deposit
			/// for an unknown pool is ignored, not halted.
			from: Option<RegistrationState>,
			/// The flag the deposit carried.
			deposited: RegistrationStatus,
		},
		/// The map is full and this pool is not already in it.
		RegistrationRejectedMapFull {
			/// blake2b-224 of the pool's cold key.
			pool_id: MainchainKeyHash,
			/// The output that carried the registration.
			utxo: sidechain_domain::UtxoId,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Only one registration inherent is allowed per block.
		InherentAlreadyExecuted,
		/// The inherent's cursor does not advance beyond the stored one.
		CursorRegression,
		/// The batch is larger than the per-block limit the runtime published.
		TooManyRegistrations,
		/// A Cardano address longer than the longest possible Bech32 one.
		MaxCardanoAddressLengthExceeded,
		/// A per-block limit of zero, or above the inherent's decode bound.
		InvalidRegistrationEventsPerBlock,
	}

	/// What the pallet knows about each pool, keyed by blake2b-224 of its cold
	/// key — the same pool id Cardano's stake distribution is keyed by, so the
	/// per-epoch stake join needs no translation.
	#[pallet::storage]
	pub type Registrations<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		MainchainKeyHash,
		RegistrationRecord<KeysOf<T>>,
		OptionQuery,
	>;

	/// Size of [`Registrations`], kept so the map bound can be enforced
	/// without counting the map.
	#[pallet::storage]
	pub type RegistrationCount<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// The first Cardano transaction not yet applied.
	#[pallet::storage]
	pub type NextCardanoPosition<T: Config> = StorageValue<_, CardanoTxPosition, ValueQuery>;

	/// Bech32 address of the candidates validator on Cardano.
	#[pallet::storage]
	pub type ValidatorAddress<T: Config> = StorageValue<_, BoundedCardanoAddress, ValueQuery>;

	/// Policy id of the Registration Token.
	#[pallet::storage]
	pub type RegistrationPolicyId<T: Config> = StorageValue<_, BoundedPolicyId, ValueQuery>;

	#[pallet::type_value]
	/// Default for [`RegistrationEventsPerBlock`].
	pub fn DefaultRegistrationEventsPerBlock() -> u32 {
		midnight_primitives_candidate_registrations::DEFAULT_REGISTRATION_EVENTS_PER_BLOCK
	}

	/// Registration events the runtime accepts per block.
	///
	/// The node reads this to size its indexer query, so the limit lives in
	/// consensus state rather than in the node's configuration: every node
	/// bounds its observation the same way, and governance can retune it
	/// without a release.
	///
	/// Bounding the query is the node's side of the contract. An observation
	/// that exceeds this limit is refused whole rather than trimmed to fit —
	/// see [`Pallet::proposable_batch`].
	#[pallet::storage]
	pub type RegistrationEventsPerBlock<T: Config> =
		StorageValue<_, u32, ValueQuery, DefaultRegistrationEventsPerBlock>;

	#[pallet::storage]
	pub type InherentExecutedThisBlock<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Where on Cardano to observe, and how much per block.
		pub config: CandidateRegistrationsGenesis,
		#[serde(skip)]
		/// Unused; present so the config can be generic over `T`.
		pub _marker: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			// Substrate genesis fail-fast convention: `build` returns `()`, so
			// invalid chain-spec input panics rather than producing state that
			// is quietly wrong. The panic names the camelCase JSON path the
			// operator edits.
			ValidatorAddress::<T>::set(
				self.config.validator_address.as_bytes().to_vec().try_into().unwrap_or_else(
					|v: Vec<u8>| {
						panic!(
							"genesis: candidateRegistrations.config.validatorAddress length {} \
							 bytes exceeds maximum {}",
							v.len(),
							BoundedCardanoAddress::bound(),
						)
					},
				),
			);

			// Infallible: the array length equals the BoundedVec bound.
			RegistrationPolicyId::<T>::set(BoundedVec::truncate_from(
				self.config.registration_policy_id.to_vec(),
			));

			let limit = self.config.registration_events_per_block;
			assert!(
				limit > 0 && limit <= MAX_REGISTRATION_EVENTS_PER_BLOCK,
				"genesis: candidateRegistrations.config.registrationEventsPerBlock must be in \
				 1..={MAX_REGISTRATION_EVENTS_PER_BLOCK}, got {limit}",
			);
			RegistrationEventsPerBlock::<T>::set(limit);

			NextCardanoPosition::<T>::set(self.config.next_cardano_position);
		}
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = InherentError;
		const INHERENT_IDENTIFIER: InherentIdentifier =
			midnight_primitives_candidate_registrations::INHERENT_IDENTIFIER;

		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			let observation = match Self::observation_from_inherent_data(data) {
				Ok(Some(observation)) => observation,
				Ok(None) => return None,
				Err(e) => {
					log::error!(target: "candidate-registrations", "Failed to decode inherent data: {e:?}");
					return None;
				},
			};

			let (registrations, next_position) = Self::proposable_batch(observation)?;
			Some(Call::apply_registrations { registrations, next_position })
		}

		fn check_inherent(call: &Self::Call, data: &InherentData) -> Result<(), Self::Error> {
			let Call::apply_registrations { registrations, next_position } = call else {
				return Ok(());
			};

			let observation =
				Self::observation_from_inherent_data(data)?.ok_or(InherentError::Unexpected)?;

			// Compare against what this node would itself have proposed, not
			// against the raw observation: an observation this node would not
			// propose is one it will not accept either.
			let (expected, expected_position) =
				Self::proposable_batch(observation).ok_or(InherentError::Unexpected)?;

			if expected.as_slice() != registrations.as_slice()
				|| expected_position != *next_position
			{
				return Err(InherentError::Mismatch);
			}
			Ok(())
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::apply_registrations { .. })
		}

		fn is_inherent_required(data: &InherentData) -> Result<Option<Self::Error>, Self::Error> {
			let Some(observation) = Self::observation_from_inherent_data(data)? else {
				return Ok(None);
			};

			// Required exactly when it is proposable. The two have to answer
			// from the same condition: an inherent this node drops but still
			// reports as required is one it cannot author a block without, so
			// the block it just built is rejected — including by itself.
			Ok(Self::proposable_batch(observation).map(|_| InherentError::Missing))
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			// Pre-account for the on_finalize write that clears the flag.
			T::DbWeight::get().writes(1)
		}

		fn on_finalize(_n: BlockNumberFor<T>) {
			InherentExecutedThisBlock::<T>::kill();
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Applies one block's worth of observed registration deposits and
		/// advances the cursor.
		#[pallet::call_index(0)]
		#[pallet::weight((
			T::WeightInfo::apply_registrations(MAX_REGISTRATION_EVENTS_PER_BLOCK),
			DispatchClass::Mandatory
		))]
		pub fn apply_registrations(
			origin: OriginFor<T>,
			registrations: RegistrationBatch,
			next_position: CardanoTxPosition,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			// Unreachable through the inherent: `proposable_batch` refuses an
			// unbounded observation before it can become a call, precisely so
			// this check cannot fail a mandatory call and stop the author
			// producing a block. Kept as the assertion that says so.
			let count = registrations.len() as u32;
			ensure!(
				count <= RegistrationEventsPerBlock::<T>::get(),
				Error::<T>::TooManyRegistrations
			);
			ensure!(!InherentExecutedThisBlock::<T>::get(), Error::<T>::InherentAlreadyExecuted);

			// Unreachable through the inherent:
			// `proposable_batch` refuses a batch that does not start at the
			// cursor and ascend from there. Kept as the assertion that says so.
			let prev_position = NextCardanoPosition::<T>::get();
			ensure!(next_position >= prev_position, Error::<T>::CursorRegression);

			// Every rejection is above this line, so a refused batch leaves no
			// trace at all.
			InherentExecutedThisBlock::<T>::put(true);

			// A halt leaves the cursor on the offending deposit rather than
			// past it.
			let mut applied = 0u32;
			let mut cursor = next_position;
			for registration in registrations.iter() {
				if Self::apply_one(registration) == Applied::Halt {
					cursor = registration.position.max(prev_position);
					break;
				}
				applied += 1;
			}

			NextCardanoPosition::<T>::set(cursor);

			Ok(PostDispatchInfo {
				actual_weight: Some(T::WeightInfo::apply_registrations(applied)),
				pays_fee: Pays::No,
			})
		}
	}

	/// Whether the batch may carry on past a deposit.
	#[derive(PartialEq, Eq)]
	enum Applied {
		/// Everthing works as expected, continue.
		Continue,
		/// A deposit contradicted something Cardano was supposed to guarantee.
		/// Stop, and leave the cursor pointing at it.
		Halt,
	}

	/// Admission's own failure type.
	///
	/// [`with_transaction`] insists on an error it can build from a
	/// [`DispatchError`]; admission's verdicts are not dispatch errors, so the
	/// two are carried side by side and the dispatch arm is unreachable.
	enum AdmissionError {
		/// `bind` refused. Which key, and whose it is, is decided after the
		/// storage layer has been rolled back — see
		/// [`Pallet::classify_collision`].
		Collision,
		Dispatch(DispatchError),
	}

	impl From<DispatchError> for AdmissionError {
		fn from(value: DispatchError) -> Self {
			Self::Dispatch(value)
		}
	}

	impl<T: Config> Pallet<T> {
		fn observation_from_inherent_data(
			data: &InherentData,
		) -> Result<Option<RegistrationsObservation>, InherentError> {
			data.get_data::<RegistrationsObservation>(&INHERENT_IDENTIFIER)
				.map_err(|_| InherentError::DecodeFailed)
		}

		/// The call this observation should become, if it should become one.
		///
		/// The single condition behind `create_inherent`, `check_inherent` and
		/// `is_inherent_required`. They must agree: an inherent that is not
		/// proposed must not be required, or a node that drops one cannot
		/// author at all, and an observation a node would not propose is one it
		/// must not accept from someone else either.
		///
		/// `None` means the node handed over an observation it was not supposed
		/// to build.
		fn proposable_batch(
			observation: RegistrationsObservation,
		) -> Option<(RegistrationBatch, CardanoTxPosition)> {
			let limit = RegistrationEventsPerBlock::<T>::get();
			let count = observation.registrations.len();
			if count > limit as usize {
				log::error!(
					target: "candidate-registrations",
					"Observation carries {count} events against a published limit of {limit}: \
					 this node did not bound its query by `registration_events_per_block`. \
					 Refusing the observation. Registration processing stalls at {} until the \
					 node is fixed; block production is unaffected.",
					NextCardanoPosition::<T>::get(),
				);
				return None;
			}

			// Keep ordering, prevent double observations.
			let cursor = NextCardanoPosition::<T>::get();
			let mut expected_from = cursor;
			for registration in observation.registrations.iter() {
				if registration.position < expected_from {
					log::error!(
						target: "candidate-registrations",
						"Observation carries a deposit at {} where {expected_from} or later was \
						 expected (cursor {cursor}): this node re-sent an applied deposit, or \
						 did not order the batch. Refusing the observation.",
						registration.position,
					);
					return None;
				}
				expected_from = registration.position.next();
			}

			if observation.next_position < expected_from {
				log::error!(
					target: "candidate-registrations",
					"Observation moves the cursor to {} without clearing its own last deposit \
					 ({expected_from} or later). Refusing the observation.",
					observation.next_position,
				);
				return None;
			}

			let next_position = observation.next_position;
			// Infallible while the limit is within the decode bound, which
			// genesis enforces. If it ever is not, the conversion fails and the
			// observation is refused the same way.
			RegistrationBatch::try_from(observation.registrations)
				.ok()
				.map(|registrations| (registrations, next_position))
		}

		/// The record a pool currently holds, valid or not.
		pub fn registration(pool_id: &MainchainKeyHash) -> Option<RegistrationRecord<KeysOf<T>>> {
			Registrations::<T>::get(pool_id)
		}

		/// Whether this pool currently holds an active registration.
		pub fn is_active_candidate(pool_id: &MainchainKeyHash) -> bool {
			Registrations::<T>::get(pool_id).is_some_and(|record| record.is_active())
		}

		/// Every pool that currently holds an active registration, with what
		/// committee selection needs from this pallet.
		///
		/// Stake is not here. It is Cardano's, it changes every epoch, and it
		/// arrives through the selection inherent — which is also why a pool
		/// the stake list does not name is simply not a candidate that epoch,
		/// without anything here having to change.
		/// The keys are the ones admission assembled and bound, not a fresh
		/// reading of the datum's bytes. Rebuilding them here would be a second
		/// assembly that could disagree with what `pallet_session` holds, and
		/// the committee hand-off binds whatever this returns.
		pub fn active_candidates() -> Vec<(MainchainKeyHash, T::AccountId, KeysOf<T>)> {
			Registrations::<T>::iter()
				.filter_map(|(pool_id, record)| match record {
					RegistrationRecord::Active { cross_chain_pub_key, session_keys, .. } => {
						let account = T::CrossChainAccount::account_id(&cross_chain_pub_key);
						Some((pool_id, account, session_keys))
					},
					_ => None,
				})
				.collect()
		}

		/// Runs admission against current state without changing it.
		///
		/// `None` means the datum would be admitted as valid. This is the
		/// answer an operator wants before posting a registration, and the one
		/// they want afterwards when it did not take.
		pub fn validate_registration(
			datum: &RegistrationDatum,
		) -> Option<RegistrationInvalidReason> {
			let outcome = with_transaction(|| {
				TransactionOutcome::Rollback(Ok::<_, DispatchError>(Self::check(datum).err()))
			});

			match outcome {
				Ok(reason) => reason,
				// `with_transaction` only returns `Err` for what the closure
				// returns, and the closure returns `Ok` unconditionally.
				Err(_) => Some(RegistrationInvalidReason::SessionBindingFailed),
			}
		}

		/// Applies one observed deposit, as the registration state machine
		/// allows or does not.
		///
		/// The match is the state machine written out. An active deposit is
		/// always allowed: a pool may register, re-register over whatever it
		/// last posted, or come back after being inactive. An inactive deposit
		/// is applied when the pool has something to inactivate, and ignored
		/// when this chain has never heard of it. What is left is an inactive
		/// deposit against an already-inactive pool, which is against assumptions,
		/// so processing stops.
		fn apply_one(observed: &ObservedRegistration) -> Applied {
			let pool_id = observed.datum.pool_id();
			let current = Registrations::<T>::get(pool_id);

			match (&current, observed.datum.status) {
				// Newest attributed registration wins, whatever it replaces:
				// nothing, a live registration, one this pallet refused, or
				// one the pool inactivated.
				(_, RegistrationStatus::Active) => {
					Self::register(pool_id, observed, current.is_some())
				},

				// Inactivating a pool that is already down.
				(Some(RegistrationRecord::Inactive { .. }), RegistrationStatus::Inactive) => {
					Self::halt_on_illegal_transition(pool_id, observed, current)
				},

				// Inactivating whatever the pool last registered.
				(Some(_), RegistrationStatus::Inactive) => Self::inactivate(pool_id, observed),

				// Inactivating something this chain never saw. Covers scenario when someone
				// registered before the observability started and inactivated after.
				(None, RegistrationStatus::Inactive) => {
					log::debug!(
						target: "candidate-registrations",
						"Ignoring inactivation at {} for unknown pool {pool_id}",
						observed.position,
					);
					Applied::Continue
				},
			}
		}

		/// An inactive deposit. The pallet keeps only the state the latest
		/// event left behind, and for this event that is `Inactive`.
		fn inactivate(pool_id: MainchainKeyHash, observed: &ObservedRegistration) -> Applied {
			Registrations::<T>::insert(
				pool_id,
				RegistrationRecord::<KeysOf<T>>::inactivated(observed),
			);
			Self::deposit_event(Event::<T>::Inactivated { pool_id, utxo: observed.utxo });
			Applied::Continue
		}

		fn halt_on_illegal_transition(
			pool_id: MainchainKeyHash,
			observed: &ObservedRegistration,
			current: Option<RegistrationRecord<KeysOf<T>>>,
		) -> Applied {
			let from = current.map(|record| record.state());
			log::error!(
				target: "candidate-registrations",
				"Deposit at {} moves pool {pool_id} from {from:?} to {:?}, which the registration \
				 state machine does not allow; Cardano should have made this impossible. \
				 Halting registration processing.",
				observed.position,
				observed.datum.status,
			);
			Self::deposit_event(Event::<T>::IllegalStateTransition {
				pool_id,
				utxo: observed.utxo,
				from,
				deposited: observed.datum.status,
			});
			Applied::Halt
		}

		fn register(
			pool_id: MainchainKeyHash,
			observed: &ObservedRegistration,
			replaces_existing: bool,
		) -> Applied {
			if !replaces_existing && RegistrationCount::<T>::get() >= T::MaxRegistrations::get() {
				log::error!(
					target: "candidate-registrations",
					"Registration map is full ({}) at {}; the bound was sized above Cardano's \
					 pool count, so this should not happen. Halting registration processing.",
					T::MaxRegistrations::get(),
					observed.position,
				);
				Self::deposit_event(Event::<T>::RegistrationRejectedMapFull {
					pool_id,
					utxo: observed.utxo,
				});
				return Applied::Halt;
			}

			let outcome = Self::check(&observed.datum);
			let event = match &outcome {
				Ok(_) => Event::<T>::Registered { pool_id, utxo: observed.utxo },
				Err(reason) => {
					Event::<T>::RegisteredInvalid { pool_id, utxo: observed.utxo, reason: *reason }
				},
			};

			Registrations::<T>::insert(pool_id, RegistrationRecord::admitted(observed, outcome));
			if !replaces_existing {
				RegistrationCount::<T>::mutate(|count| *count = count.saturating_add(1));
			}

			Self::deposit_event(event);
			Applied::Continue
		}

		/// The checks, in order. The order is a security property, each step is more expensive
		/// than the last, and only the last one writes.
		fn check(datum: &RegistrationDatum) -> Result<KeysOf<T>, RegistrationInvalidReason> {
			// Phase A — free. No crypto, no state read, no state change.
			validation::check_canonical_keys(&datum.keys)?;

			let session_key_types = KeysOf::<T>::key_ids();
			validation::check_session_keys_present(&datum.keys, session_key_types)?;
			validation::check_no_unknown_key_types(&datum.keys, session_key_types)?;

			// Phase B — one proof of possession per key, plus one ECDSA
			// verification. All of them cover the same message, rebuilt here
			// from canonical form rather than taken as the datum laid it out.
			let message = RegistrationMessage::from_datum(datum).to_signed_bytes();
			let session_keys = T::SessionKeyAuthority::verify_and_assemble(&datum.keys, &message)?;
			validation::verify_cross_chain_signature(
				&datum.cross_chain_pub_key,
				&datum.cross_chain_signature,
				&message,
			)?;

			// Phase C — the only step that writes, and the only one that can
			// see a conflict. `pallet_session` is the index of who holds which
			// session key; asking it to bind these keys is how that question
			// gets answered, and a rejection here is the conflict that would
			// otherwise surface as a failed committee hand-off.
			Self::bind_session_keys(datum, session_keys.clone())?;
			Ok(session_keys)
		}

		fn bind_session_keys(
			datum: &RegistrationDatum,
			session_keys: KeysOf<T>,
		) -> Result<(), RegistrationInvalidReason> {
			let account = T::CrossChainAccount::account_id(&datum.cross_chain_pub_key);

			let outcome =
				with_transaction(|| match T::SessionKeyAuthority::bind(&account, session_keys) {
					Ok(()) => TransactionOutcome::Commit(Ok(())),
					Err(KeyCollision) => {
						TransactionOutcome::Rollback(Err(AdmissionError::Collision))
					},
				});

			match outcome {
				Ok(()) => Ok(()),
				Err(AdmissionError::Collision) => Err(Self::classify_collision(datum, &account)),
				Err(AdmissionError::Dispatch(e)) => {
					// Unreachable: the closure never returns a dispatch error.
					log::error!(
						target: "candidate-registrations",
						"Unexpected dispatch error binding session keys: {e:?}",
					);
					Err(RegistrationInvalidReason::SessionBindingFailed)
				},
			}
		}

		fn classify_collision(
			datum: &RegistrationDatum,
			account: &T::AccountId,
		) -> RegistrationInvalidReason {
			for key in datum.keys.iter() {
				match T::SessionKeyAuthority::key_owner(key.id, key.bytes.as_slice()) {
					Some(owner) if owner == *account => continue,
					Some(_) => return RegistrationInvalidReason::SessionKeyReuse,
					None => continue,
				}
			}

			// Nothing in the registration is held elsewhere, so whatever the
			// session pallet refused for, it was not key reuse.
			RegistrationInvalidReason::SessionBindingFailed
		}
	}
}
