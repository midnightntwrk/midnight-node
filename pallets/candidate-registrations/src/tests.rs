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

use frame_support::inherent::{InherentData, ProvideInherent};
use frame_support::traits::Hooks;
use frame_support::{assert_noop, assert_ok, pallet_prelude::DispatchResultWithPostInfo};
use midnight_primitives_candidate_registrations::{
	CandidateKey, CandidateKeys, CardanoTxPosition, CrossChainAccount, CrossChainPublicKeyBytes,
	CrossChainSignatureBytes, INHERENT_IDENTIFIER, InherentError, ObservedRegistration,
	PoolSignatureBytes, RegistrationDatum, RegistrationInvalidReason, RegistrationRecord,
	RegistrationState, RegistrationStatus, RegistrationsObservation, SessionKeyAuthority,
};
use sidechain_domain::{MainchainKeyHash, StakePoolPublicKey, UtxoId};
use sp_core::{
	Pair,
	crypto::{
		ByteArray,
		key_types::{AURA, BABE, BEEFY, GRANDPA},
	},
	ecdsa, ed25519,
	proof_of_possession::ProofOfPossessionGenerator,
	sr25519,
};
use sp_runtime::traits::OpaqueKeys;

use crate::mock::*;
use crate::pallet::{Config, Error, Event, NextCardanoPosition, RegistrationCount, Registrations};
use crate::{RegistrationBatch, pallet};

/// Registration events one block may carry, as the mock runtime configures it.
fn limit() -> u32 {
	<Test as Config>::MaxRegistrationEventsPerBlock::get()
}

/// One operator's full key set, as the off-chain registration tool would hold it.
struct Operator {
	spo_cold: ed25519::Pair,
	cross_chain: ecdsa::Pair,
	aura: sr25519::Pair,
	babe: sr25519::Pair,
	grandpa: ed25519::Pair,
	beefy: ecdsa::Pair,
}

impl Operator {
	fn from_seed(seed: u8) -> Self {
		let derive = |tag: u8| {
			let mut bytes = [seed; 32];
			bytes[31] = tag;
			bytes
		};
		Self {
			spo_cold: ed25519::Pair::from_seed(&derive(0)),
			cross_chain: ecdsa::Pair::from_seed(&derive(1)),
			aura: sr25519::Pair::from_seed(&derive(2)),
			babe: sr25519::Pair::from_seed(&derive(3)),
			grandpa: ed25519::Pair::from_seed(&derive(4)),
			beefy: ecdsa::Pair::from_seed(&derive(5)),
		}
	}

	fn pool_id(&self) -> MainchainKeyHash {
		StakePoolPublicKey(self.spo_cold.public().0).hash()
	}

	fn account_id(&self) -> AccountId {
		TruncatingAccount::account_id(&CrossChainPublicKeyBytes(self.cross_chain.public().0))
	}

	/// A registration signed end to end, exactly as the off-chain tool builds
	/// one: lay out the keys, derive the message from them, then fill in every
	/// proof over that message.
	fn register(&self) -> RegistrationDatum {
		self.register_with_keys(self.candidate_keys())
	}

	fn candidate_keys(&self) -> Vec<(KeyMaterial, [u8; 4])> {
		vec![
			(KeyMaterial::Sr25519(self.aura.clone()), AURA.0),
			(KeyMaterial::Sr25519(self.babe.clone()), BABE.0),
			(KeyMaterial::Ecdsa(self.beefy.clone()), BEEFY.0),
			(KeyMaterial::Ed25519(self.grandpa), GRANDPA.0),
		]
	}

	fn register_with_keys(&self, keys: Vec<(KeyMaterial, [u8; 4])>) -> RegistrationDatum {
		// Proofs are projected out of the message, so placeholders here do not
		// affect the bytes everything is signed over.
		let unproven: Vec<CandidateKey> = keys
			.iter()
			.map(|(material, id)| CandidateKey {
				id: *id,
				bytes: material.public_bytes().try_into().unwrap(),
				proof: vec![0u8; 64].try_into().unwrap(),
			})
			.collect();

		let mut datum = RegistrationDatum {
			status: RegistrationStatus::Active,
			cold_pub_key: StakePoolPublicKey(self.spo_cold.public().0),
			pool_signature: PoolSignatureBytes([0u8; 64]),
			cross_chain_pub_key: CrossChainPublicKeyBytes(self.cross_chain.public().0),
			cross_chain_signature: CrossChainSignatureBytes([0u8; 65]),
			keys: CandidateKeys::try_from(unproven).unwrap(),
		};

		let message =
			midnight_primitives_candidate_registrations::RegistrationMessage::from_datum(&datum)
				.to_signed_bytes();

		datum.pool_signature = PoolSignatureBytes(self.spo_cold.sign(&message).0);
		datum.cross_chain_signature = CrossChainSignatureBytes(self.cross_chain.sign(&message).0);

		let proven: Vec<CandidateKey> = keys
			.iter()
			.map(|(material, id)| CandidateKey {
				id: *id,
				bytes: material.public_bytes().try_into().unwrap(),
				proof: material.proof_of_possession(&message).try_into().unwrap(),
			})
			.collect();
		datum.keys = CandidateKeys::try_from(proven).unwrap();

		datum
	}
}

#[derive(Clone)]
#[allow(clippy::large_enum_variant)] // Test keys; the enum is never stored.
enum KeyMaterial {
	Sr25519(sr25519::Pair),
	Ed25519(ed25519::Pair),
	Ecdsa(ecdsa::Pair),
	// For invalid keys tests
	Raw(Vec<u8>),
}

impl KeyMaterial {
	fn public_bytes(&self) -> Vec<u8> {
		match self {
			Self::Sr25519(pair) => pair.public().to_raw_vec(),
			Self::Ed25519(pair) => pair.public().to_raw_vec(),
			Self::Ecdsa(pair) => pair.public().to_raw_vec(),
			Self::Raw(bytes) => bytes.clone(),
		}
	}

	fn proof_of_possession(&self, message: &[u8]) -> Vec<u8> {
		match self {
			Self::Sr25519(pair) => pair.clone().generate_proof_of_possession(message).to_raw_vec(),
			Self::Ed25519(pair) => pair.clone().generate_proof_of_possession(message).to_raw_vec(),
			Self::Ecdsa(pair) => pair.clone().generate_proof_of_possession(message).to_raw_vec(),
			// No secret to prove with. Right length for sr25519/ed25519, so the
			// key check is what fails rather than this.
			Self::Raw(_) => vec![0u8; 64],
		}
	}
}

/// A deposit that inactivates a registration.
///
/// A6 allows any registration datum with the inactive flag, not only a copy of
/// the one being replaced; this builds the copy because that is the ordinary
/// case. `an_inactivation_need_not_resemble_what_it_inactivates` covers the
/// other one.
fn inactive_deposit_of(operator: &Operator) -> RegistrationDatum {
	RegistrationDatum { status: RegistrationStatus::Inactive, ..operator.register() }
}

fn observed(datum: RegistrationDatum, position: CardanoTxPosition) -> ObservedRegistration {
	ObservedRegistration {
		position,
		utxo: UtxoId::new([position.block_no as u8; 32], position.tx_index_in_block as u16),
		datum,
	}
}

fn apply(events: Vec<ObservedRegistration>) -> DispatchResultWithPostInfo {
	let next = events
		.last()
		.map(|event| event.position.next())
		.unwrap_or_else(NextCardanoPosition::<Test>::get);
	apply_with_cursor(events, next)
}

fn apply_with_cursor(
	events: Vec<ObservedRegistration>,
	next: CardanoTxPosition,
) -> DispatchResultWithPostInfo {
	start_block();
	submit(events, next)
}

/// Submits a batch without pretending a new block started, so a rejection can
/// be checked against untouched storage.
///
/// Every batch in the suite goes through here, so this is where the cached
/// length is held to the map it caches.
fn submit(
	events: Vec<ObservedRegistration>,
	next: CardanoTxPosition,
) -> DispatchResultWithPostInfo {
	let result = pallet::Pallet::<Test>::apply_registrations(
		frame_system::RawOrigin::None.into(),
		RegistrationBatch::<Test>::try_from(events).expect("batch is within the per-block limit"),
		next,
	);
	assert_count_matches_the_map();
	result
}

/// `RegistrationCount` is a cached `Registrations::iter().count()`, and the
/// only thing keeping it honest is that every insert goes through `register`
/// or `inactivate`. A new write path — a migration, a governance setter, a
/// cleanup — is where it would drift, so the check lives where every test
/// passes through rather than in a test of its own.
///
/// Drift high halts observation on a map that is not full; drift low lets the
/// map grow past the bound it is supposed to have.
fn assert_count_matches_the_map() {
	assert_eq!(
		RegistrationCount::<Test>::get() as usize,
		Registrations::<Test>::iter().count(),
		"RegistrationCount drifted from the map"
	);
}

/// Rolls a real block between batches.
///
/// The pallet's own hooks do the work rather than the harness reaching into
/// storage: `on_finalize` is what clears the once-per-block flag, so a test
/// that killed the flag itself would pass with the hook deleted. Events are
/// reset with the block, so an `assert_has_event` can only be satisfied by
/// the batch under test.
fn start_block() {
	CandidateRegistrations::on_finalize(System::block_number());
	System::set_block_number(System::block_number() + 1);
	System::reset_events();
	CandidateRegistrations::on_initialize(System::block_number());
}

/// The account a session key is bound to, as the session index sees it.
fn owner_of(id: [u8; 4], bytes: &[u8]) -> Option<AccountId> {
	<MockSessionKeyAuthority as SessionKeyAuthority<AccountId>>::key_owner(id, bytes)
}

fn state_of(pool_id: MainchainKeyHash) -> RegistrationState {
	Registrations::<Test>::get(pool_id).expect("no record for pool").state()
}

#[test]
fn genesis_seeds_the_observation_parameters() {
	new_test_ext().execute_with(|| {
		assert_eq!(
			pallet::ValidatorAddress::<Test>::get().to_vec(),
			genesis_config().validator_address.into_bytes()
		);
		assert_eq!(pallet::RegistrationPolicyId::<Test>::get().to_vec(), vec![7u8; 28]);
		assert_eq!(RegistrationCount::<Test>::get(), 0);
	});
}

#[test]
fn a_fully_proven_registration_is_admitted() {
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(1);

		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(10, 0))]));

		assert_eq!(state_of(operator.pool_id()), RegistrationState::Active);
		assert_eq!(RegistrationCount::<Test>::get(), 1);
		assert_eq!(NextCardanoPosition::<Test>::get(), CardanoTxPosition::new(10, 1));

		// The keys are bound to the account the cross-chain key derives, which
		// is what keeps the committee hand-off from having to bind them later.
		let aura = operator.aura.public().to_raw_vec();
		assert_eq!(owner_of(AURA.0, &aura), Some(operator.account_id()));

		let candidates = pallet::Pallet::<Test>::active_candidates();
		assert_eq!(candidates.len(), 1);
		assert_eq!(candidates[0].0, operator.pool_id());
		assert_eq!(candidates[0].1, operator.account_id());
	});
}

#[test]
fn non_canonical_keys_are_rejected_before_any_crypto() {
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(2);
		let mut datum = operator.register();
		let mut keys = datum.keys.to_vec();
		keys.reverse();
		datum.keys = CandidateKeys::try_from(keys).unwrap();

		assert_ok!(apply(vec![observed(datum, CardanoTxPosition::new(10, 0))]));

		assert_eq!(
			state_of(operator.pool_id()),
			RegistrationState::Invalid(RegistrationInvalidReason::NonCanonicalKeys)
		);
	});
}

#[test]
fn a_duplicated_key_type_is_not_canonical() {
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(3);
		let mut datum = operator.register();
		let mut keys = datum.keys.to_vec();
		let first = keys[0].clone();
		keys.insert(1, first);
		datum.keys = CandidateKeys::try_from(keys).unwrap();

		assert_ok!(apply(vec![observed(datum, CardanoTxPosition::new(10, 0))]));

		assert_eq!(
			state_of(operator.pool_id()),
			RegistrationState::Invalid(RegistrationInvalidReason::NonCanonicalKeys)
		);
	});
}

#[test]
fn a_missing_required_key_proof_is_named() {
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(4);
		let keys: Vec<_> =
			operator.candidate_keys().into_iter().filter(|(_, id)| *id != BEEFY.0).collect();

		assert_ok!(apply(vec![observed(
			operator.register_with_keys(keys),
			CardanoTxPosition::new(10, 0)
		)]));

		assert_eq!(
			state_of(operator.pool_id()),
			RegistrationState::Invalid(RegistrationInvalidReason::MissingKeyProof(BEEFY.0))
		);
	});
}

#[test]
fn a_key_type_the_runtime_cannot_verify_is_named() {
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(5);
		let mut keys = operator.candidate_keys();
		// `zzzz` sorts last, so the list stays canonical.
		keys.push((KeyMaterial::Sr25519(operator.aura.clone()), *b"zzzz"));

		assert_ok!(apply(vec![observed(
			operator.register_with_keys(keys),
			CardanoTxPosition::new(10, 0)
		)]));

		assert_eq!(
			state_of(operator.pool_id()),
			RegistrationState::Invalid(RegistrationInvalidReason::UnknownKeyType(*b"zzzz"))
		);
	});
}

#[test]
fn a_proof_that_does_not_cover_the_key_is_rejected() {
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(6);
		let mut datum = operator.register();
		let mut keys = datum.keys.to_vec();
		keys[0].proof = vec![0xaa; 64].try_into().unwrap();
		datum.keys = CandidateKeys::try_from(keys).unwrap();

		assert_ok!(apply(vec![observed(datum, CardanoTxPosition::new(10, 0))]));

		assert_eq!(
			state_of(operator.pool_id()),
			RegistrationState::Invalid(RegistrationInvalidReason::InvalidKeyProof(AURA.0))
		);
	});
}

#[test]
fn a_proof_lifted_from_another_registration_is_rejected() {
	// R2: the message names the pool and the registrant, so a proof is only
	// good for the registration that carries it.
	new_test_ext().execute_with(|| {
		let victim = Operator::from_seed(7);
		let thief = Operator::from_seed(8);

		let stolen = victim.register().keys.to_vec();
		let mut datum = thief.register();
		datum.keys = CandidateKeys::try_from(stolen).unwrap();

		assert_ok!(apply(vec![observed(datum, CardanoTxPosition::new(10, 0))]));

		assert_eq!(
			state_of(thief.pool_id()),
			RegistrationState::Invalid(RegistrationInvalidReason::InvalidKeyProof(AURA.0))
		);
	});
}

#[test]
fn a_bad_cross_chain_signature_is_rejected() {
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(9);
		let mut datum = operator.register();
		datum.cross_chain_signature = CrossChainSignatureBytes([0u8; 65]);

		assert_ok!(apply(vec![observed(datum, CardanoTxPosition::new(10, 0))]));

		assert_eq!(
			state_of(operator.pool_id()),
			RegistrationState::Invalid(RegistrationInvalidReason::InvalidCrossChainSignature)
		);
	});
}

#[test]
fn reusing_another_pools_session_key_invalidates_the_registration() {
	new_test_ext().execute_with(|| {
		let incumbent = Operator::from_seed(10);
		let mut squatter = Operator::from_seed(11);
		squatter.grandpa = incumbent.grandpa;

		assert_ok!(apply(vec![observed(incumbent.register(), CardanoTxPosition::new(10, 0))]));
		assert_ok!(apply(vec![observed(squatter.register(), CardanoTxPosition::new(10, 1))]));

		assert_eq!(state_of(incumbent.pool_id()), RegistrationState::Active);
		assert_eq!(
			state_of(squatter.pool_id()),
			RegistrationState::Invalid(RegistrationInvalidReason::SessionKeyReuse)
		);

		// The incumbent keeps the contested key, and the squatter's other keys
		// were rolled back rather than left half-bound.
		let grandpa = incumbent.grandpa.public().to_raw_vec();
		assert_eq!(owner_of(GRANDPA.0, &grandpa), Some(incumbent.account_id()));
		let squatter_aura = squatter.aura.public().to_raw_vec();
		assert_eq!(owner_of(AURA.0, &squatter_aura), None);
	});
}

#[test]
fn a_pool_may_re_register_after_going_inactive() {
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(12);

		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(10, 0))]));
		assert_ok!(apply(vec![observed(
			inactive_deposit_of(&operator),
			CardanoTxPosition::new(11, 0)
		)]));
		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(12, 0))]));

		assert_eq!(state_of(operator.pool_id()), RegistrationState::Active);
		assert_eq!(RegistrationCount::<Test>::get(), 1);
	});
}

#[test]
fn an_invalid_registration_supersedes_an_inactive_one() {
	// The newest attributed registration wins, and the state machine lets an
	// Inactive record go either way — so a bad re-registration replaces a
	// record that once worked.
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(13);
		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(10, 0))]));
		assert_eq!(state_of(operator.pool_id()), RegistrationState::Active);

		assert_ok!(apply(vec![observed(
			inactive_deposit_of(&operator),
			CardanoTxPosition::new(11, 0)
		)]));

		let mut broken = operator.register();
		broken.cross_chain_signature = CrossChainSignatureBytes([0u8; 65]);
		assert_ok!(apply(vec![observed(broken, CardanoTxPosition::new(12, 0))]));

		assert_eq!(
			state_of(operator.pool_id()),
			RegistrationState::Invalid(RegistrationInvalidReason::InvalidCrossChainSignature)
		);
		assert_eq!(RegistrationCount::<Test>::get(), 1);
		assert!(pallet::Pallet::<Test>::active_candidates().is_empty());
	});
}

#[test]
fn an_inactivation_keeps_the_record_and_marks_it_inactive() {
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(14);
		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(10, 0))]));

		let inactive = observed(inactive_deposit_of(&operator), CardanoTxPosition::new(11, 0));
		assert_ok!(apply(vec![inactive.clone()]));

		// The map never shrinks: the record stays, the state moves.
		assert_eq!(state_of(operator.pool_id()), RegistrationState::Inactive);
		assert_eq!(RegistrationCount::<Test>::get(), 1);
		assert!(pallet::Pallet::<Test>::active_candidates().is_empty());
		assert!(!pallet::Pallet::<Test>::is_active_candidate(&operator.pool_id()));
		System::assert_has_event(
			Event::<Test>::Inactivated { pool_id: operator.pool_id(), utxo: inactive.utxo }.into(),
		);
	});
}

#[test]
fn an_inactive_registration_can_be_reactivated() {
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(16);
		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(10, 0))]));

		assert_ok!(apply(vec![observed(
			inactive_deposit_of(&operator),
			CardanoTxPosition::new(11, 0)
		)]));
		assert_eq!(state_of(operator.pool_id()), RegistrationState::Inactive);

		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(12, 0))]));
		assert_eq!(state_of(operator.pool_id()), RegistrationState::Active);
		assert_eq!(RegistrationCount::<Test>::get(), 1);
	});
}

#[test]
fn an_inactivation_need_not_resemble_what_it_inactivates() {
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(210);
		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(10, 0))]));

		// Inactivate with wholly different key material.
		let mut other = Operator::from_seed(211);
		other.spo_cold = operator.spo_cold;
		let inactivate = observed(inactive_deposit_of(&other), CardanoTxPosition::new(11, 0));
		assert_ok!(apply(vec![inactivate.clone()]));

		// The pool is down, and the account kept is the one its session keys
		// are bound under — from the registration, not from this deposit.
		assert_eq!(
			Registrations::<Test>::get(operator.pool_id()).unwrap(),
			RegistrationRecord::Inactive {
				cold_pub_key: StakePoolPublicKey(operator.spo_cold.public().0),
				observed_at: CardanoTxPosition::new(11, 0),
				utxo: inactivate.utxo,
			}
		);
		assert!(pallet::Pallet::<Test>::active_candidates().is_empty());
	});
}

#[test]
fn an_inactive_deposit_for_an_unknown_pool_is_ignored() {
	// Observation can be configured to start after a registration was
	// deposited, so its inactivation may be the first the pallet hears of
	// that pool. Nothing to inactivate, nothing broken, carry on.
	new_test_ext().execute_with(|| {
		let stranger = Operator::from_seed(15);
		let inactive = observed(inactive_deposit_of(&stranger), CardanoTxPosition::new(10, 0));

		let follower = Operator::from_seed(18);
		let follower_event = observed(follower.register(), CardanoTxPosition::new(10, 1));

		assert_ok!(apply(vec![inactive.clone(), follower_event]));

		assert!(Registrations::<Test>::get(stranger.pool_id()).is_none());
		assert_eq!(state_of(follower.pool_id()), RegistrationState::Active);
		assert_eq!(RegistrationCount::<Test>::get(), 1);
		assert_eq!(NextCardanoPosition::<Test>::get(), CardanoTxPosition::new(10, 2));
	});
}

#[test]
fn a_halt_keeps_everything_applied_before_it() {
	new_test_ext().execute_with(|| {
		let good = Operator::from_seed(19);
		let stale = Operator::from_seed(21);

		// Inactivate `stale`, so a second inactive deposit for it halts.
		assert_ok!(apply(vec![observed(stale.register(), CardanoTxPosition::new(9, 0))]));
		assert_ok!(apply(vec![observed(
			inactive_deposit_of(&stale),
			CardanoTxPosition::new(9, 1)
		)]));

		assert_ok!(apply(vec![
			observed(good.register(), CardanoTxPosition::new(10, 0)),
			observed(inactive_deposit_of(&stale), CardanoTxPosition::new(10, 1)),
		]));

		assert_eq!(state_of(good.pool_id()), RegistrationState::Active);
		// The cursor sits past the applied deposit and on the offending one.
		assert_eq!(NextCardanoPosition::<Test>::get(), CardanoTxPosition::new(10, 1));
	});
}

#[test]
fn a_second_active_registration_replaces_the_first() {
	// Cardano now permits an active registration to be replaced by another
	// active one, so the pallet admits it as any other registration: fresh
	// verdict, fresh record, same pool id.
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(22);
		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(10, 0))]));

		let second = observed(operator.register(), CardanoTxPosition::new(11, 0));
		assert_ok!(apply(vec![second.clone()]));

		assert_eq!(state_of(operator.pool_id()), RegistrationState::Active);
		assert_eq!(Registrations::<Test>::get(operator.pool_id()).unwrap().utxo(), second.utxo);
		// Replacing is not registering: the pool is still one entry.
		assert_eq!(RegistrationCount::<Test>::get(), 1);
		assert_eq!(NextCardanoPosition::<Test>::get(), CardanoTxPosition::new(11, 1));
	});
}

#[test]
fn an_invalid_registration_supersedes_an_active_one() {
	// The newest attributed registration wins even when it is the worse one.
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(25);
		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(10, 0))]));
		assert_eq!(state_of(operator.pool_id()), RegistrationState::Active);

		let mut broken = operator.register();
		broken.cross_chain_signature = CrossChainSignatureBytes([0u8; 65]);
		assert_ok!(apply(vec![observed(broken, CardanoTxPosition::new(11, 0))]));

		assert_eq!(
			state_of(operator.pool_id()),
			RegistrationState::Invalid(RegistrationInvalidReason::InvalidCrossChainSignature)
		);
		assert!(pallet::Pallet::<Test>::active_candidates().is_empty());
	});
}

#[test]
fn a_pool_may_repair_an_invalid_registration_in_place() {
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(23);
		let mut broken = operator.register();
		broken.cross_chain_signature = CrossChainSignatureBytes([0u8; 65]);
		assert_ok!(apply(vec![observed(broken, CardanoTxPosition::new(10, 0))]));
		assert!(matches!(state_of(operator.pool_id()), RegistrationState::Invalid(_)));

		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(11, 0))]));

		assert_eq!(state_of(operator.pool_id()), RegistrationState::Active);
		assert_eq!(RegistrationCount::<Test>::get(), 1);
	});
}

#[test]
fn a_second_inactive_deposit_halts_processing() {
	// A7: an Inactive output cannot be spent, so the transaction that would
	// deposit a second inactive datum cannot be built.
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(24);
		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(10, 0))]));
		assert_ok!(apply(vec![observed(
			inactive_deposit_of(&operator),
			CardanoTxPosition::new(11, 0)
		)]));

		let again = observed(inactive_deposit_of(&operator), CardanoTxPosition::new(12, 0));
		assert_ok!(apply(vec![again.clone()]));

		System::assert_has_event(
			Event::<Test>::IllegalStateTransition {
				pool_id: operator.pool_id(),
				utxo: again.utxo,
				from: Some(RegistrationState::Inactive),
				deposited: RegistrationStatus::Inactive,
			}
			.into(),
		);
		assert_eq!(state_of(operator.pool_id()), RegistrationState::Inactive);
	});
}

#[test]
fn a_full_map_halts_on_a_newcomer_but_still_accepts_replacements() {
	new_test_ext().execute_with(|| {
		// `MaxRegistrations` is 3 in the mock.
		for seed in 20..23u8 {
			let operator = Operator::from_seed(seed);
			assert_ok!(apply(vec![observed(
				operator.register(),
				CardanoTxPosition::new(10, seed as u32)
			)]));
		}
		assert_eq!(RegistrationCount::<Test>::get(), 3);

		// An incumbent re-registering is not a newcomer, so the bound does not
		// lock it out of rotating its keys.
		let incumbent = Operator::from_seed(20);
		assert_ok!(apply(vec![observed(
			inactive_deposit_of(&incumbent),
			CardanoTxPosition::new(11, 0)
		)]));
		assert_ok!(apply(vec![observed(incumbent.register(), CardanoTxPosition::new(11, 1))]));
		assert_eq!(state_of(incumbent.pool_id()), RegistrationState::Active);
		assert_eq!(RegistrationCount::<Test>::get(), 3);

		// A newcomer means the bound was wrong about Cardano's pool count.
		let newcomer = Operator::from_seed(30);
		let newcomer_event = observed(newcomer.register(), CardanoTxPosition::new(12, 0));
		assert_ok!(apply(vec![newcomer_event.clone()]));

		assert!(Registrations::<Test>::get(newcomer.pool_id()).is_none());
		System::assert_has_event(
			Event::<Test>::RegistrationRejectedMapFull {
				pool_id: newcomer.pool_id(),
				utxo: newcomer_event.utxo,
			}
			.into(),
		);
		assert_eq!(NextCardanoPosition::<Test>::get(), CardanoTxPosition::new(12, 0));
	});
}

#[test]
fn the_cursor_never_moves_backwards() {
	new_test_ext().execute_with(|| {
		assert_ok!(apply_with_cursor(vec![], CardanoTxPosition::new(50, 0)));

		start_block();
		assert_noop!(
			submit(vec![], CardanoTxPosition::new(49, 9)),
			Error::<Test>::CursorRegression
		);
	});
}

#[test]
fn a_batch_above_the_limit_is_not_a_call_that_can_be_built() {
	new_test_ext().execute_with(|| {
		let events: Vec<_> = (0..=limit() as u8)
			.map(|seed| {
				observed(
					Operator::from_seed(40 + seed).register(),
					CardanoTxPosition::new(10, seed as u32),
				)
			})
			.collect();

		assert_eq!(events.len() as u32, limit() + 1);
		assert!(RegistrationBatch::<Test>::try_from(events).is_err());
	});
}

#[test]
fn only_one_inherent_runs_per_block() {
	new_test_ext().execute_with(|| {
		assert_ok!(pallet::Pallet::<Test>::apply_registrations(
			frame_system::RawOrigin::None.into(),
			RegistrationBatch::<Test>::default(),
			CardanoTxPosition::new(10, 0),
		));
		assert_noop!(
			pallet::Pallet::<Test>::apply_registrations(
				frame_system::RawOrigin::None.into(),
				RegistrationBatch::<Test>::default(),
				CardanoTxPosition::new(10, 1),
			),
			Error::<Test>::InherentAlreadyExecuted
		);
	});
}

#[test]
fn validate_registration_answers_without_changing_anything() {
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(50);
		let datum = operator.register();

		assert_eq!(pallet::Pallet::<Test>::validate_registration(&datum), None);

		// Nothing was stored, and no key was left bound.
		assert!(Registrations::<Test>::get(operator.pool_id()).is_none());
		let aura = operator.aura.public().to_raw_vec();
		assert_eq!(owner_of(AURA.0, &aura), None);

		let mut broken = datum.clone();
		broken.cross_chain_signature = CrossChainSignatureBytes([0u8; 65]);
		assert_eq!(
			pallet::Pallet::<Test>::validate_registration(&broken),
			Some(RegistrationInvalidReason::InvalidCrossChainSignature)
		);
	});
}

#[test]
fn rotating_the_cross_chain_key_requires_rotating_every_session_key() {
	// `purge_keys` is never called, so the old account still holds the old
	// keys. A pool that rotates its cross-chain key but keeps a session key
	// collides with itself.
	new_test_ext().execute_with(|| {
		let mut operator = Operator::from_seed(90);
		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(10, 0))]));
		let first_account = operator.account_id();

		// Everything rotates except grandpa.
		operator.cross_chain = ecdsa::Pair::from_seed(&[91u8; 32]);
		operator.aura = sr25519::Pair::from_seed(&[92u8; 32]);
		operator.babe = sr25519::Pair::from_seed(&[93u8; 32]);
		operator.beefy = ecdsa::Pair::from_seed(&[94u8; 32]);
		assert_ne!(operator.account_id(), first_account);

		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(11, 0))]));

		assert_eq!(
			state_of(operator.pool_id()),
			RegistrationState::Invalid(RegistrationInvalidReason::SessionKeyReuse)
		);
		// The key is still the first account's, and the rejected registration
		// bound nothing.
		let grandpa = operator.grandpa.public().to_raw_vec();
		assert_eq!(owner_of(GRANDPA.0, &grandpa), Some(first_account));
		let aura = operator.aura.public().to_raw_vec();
		assert_eq!(owner_of(AURA.0, &aura), None);
	});
}

#[test]
fn rotating_the_cross_chain_key_with_every_session_key_succeeds() {
	new_test_ext().execute_with(|| {
		let mut operator = Operator::from_seed(95);
		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(10, 0))]));

		operator.cross_chain = ecdsa::Pair::from_seed(&[96u8; 32]);
		operator.aura = sr25519::Pair::from_seed(&[97u8; 32]);
		operator.babe = sr25519::Pair::from_seed(&[98u8; 32]);
		operator.grandpa = ed25519::Pair::from_seed(&[99u8; 32]);
		operator.beefy = ecdsa::Pair::from_seed(&[100u8; 32]);

		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(11, 0))]));

		assert_eq!(state_of(operator.pool_id()), RegistrationState::Active);
		assert_eq!(RegistrationCount::<Test>::get(), 1);
		let aura = operator.aura.public().to_raw_vec();
		assert_eq!(owner_of(AURA.0, &aura), Some(operator.account_id()));
	});
}

#[test]
fn a_session_key_is_never_released_from_the_account_that_registered_it() {
	// A session key that has been registered once is spent, for its own pool and for everyone else,
	// unless cross-chain key is the same.
	new_test_ext().execute_with(|| {
		let mut operator = Operator::from_seed(101);
		let spent_grandpa = operator.grandpa;
		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(10, 0))]));
		let first_account = operator.account_id();

		// The pool inactivates and re-registers under a new cross-chain key
		// with wholly fresh session keys, which is the supported path.
		assert_ok!(apply(vec![observed(
			inactive_deposit_of(&operator),
			CardanoTxPosition::new(11, 0)
		)]));
		operator.cross_chain = ecdsa::Pair::from_seed(&[102u8; 32]);
		operator.aura = sr25519::Pair::from_seed(&[103u8; 32]);
		operator.babe = sr25519::Pair::from_seed(&[104u8; 32]);
		operator.grandpa = ed25519::Pair::from_seed(&[105u8; 32]);
		operator.beefy = ecdsa::Pair::from_seed(&[106u8; 32]);
		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(12, 0))]));
		assert_eq!(state_of(operator.pool_id()), RegistrationState::Active);

		// The stood-down key is still owned by the account that registered it,
		// so another pool cannot pick it up.
		let mut newcomer = Operator::from_seed(107);
		newcomer.grandpa = spent_grandpa;
		assert_ok!(apply(vec![observed(newcomer.register(), CardanoTxPosition::new(13, 0))]));

		assert_eq!(
			state_of(newcomer.pool_id()),
			RegistrationState::Invalid(RegistrationInvalidReason::SessionKeyReuse)
		);
		let grandpa = spent_grandpa.public().to_raw_vec();
		assert_eq!(owner_of(GRANDPA.0, &grandpa), Some(first_account));
	});
}

#[test]
fn the_once_per_block_flag_does_not_survive_the_block_boundary() {
	// Without this, the chain would accept one registration inherent ever and
	// then refuse every later one, stalling observation for good while block
	// production carried on.
	new_test_ext().execute_with(|| {
		assert_ok!(submit(vec![], CardanoTxPosition::new(10, 0)));
		assert!(pallet::InherentExecutedThisBlock::<Test>::get());

		start_block();
		assert!(!pallet::InherentExecutedThisBlock::<Test>::get());

		// And the next block really can run one.
		assert_ok!(submit(vec![], CardanoTxPosition::new(10, 1)));
		assert_eq!(NextCardanoPosition::<Test>::get(), CardanoTxPosition::new(10, 1));
	});
}

/// A datum with nothing proven in it.
///
/// The inherent plumbing never looks inside a datum, so the oversized-batch
/// tests do not pay for 65 fully signed registrations.
fn unverified_datum(seed: u8) -> RegistrationDatum {
	RegistrationDatum {
		status: RegistrationStatus::Active,
		cold_pub_key: StakePoolPublicKey([seed; 32]),
		pool_signature: PoolSignatureBytes([0u8; 64]),
		cross_chain_pub_key: CrossChainPublicKeyBytes([seed; 33]),
		cross_chain_signature: CrossChainSignatureBytes([0u8; 65]),
		keys: CandidateKeys::default(),
	}
}

fn observation(events: Vec<ObservedRegistration>) -> RegistrationsObservation {
	let next_position = events.last().map(|event| event.position.next()).unwrap_or_default();
	RegistrationsObservation { registrations: events, next_position }
}

fn observation_with_cursor(
	events: Vec<ObservedRegistration>,
	next_position: CardanoTxPosition,
) -> RegistrationsObservation {
	RegistrationsObservation { registrations: events, next_position }
}

/// Whether the inherent functions will have anything to do with this
/// observation. They have to agree, so one answer covers all three.
fn is_proposable(observation: &RegistrationsObservation) -> bool {
	let data = inherent_data(observation);
	let proposed = pallet::Pallet::<Test>::create_inherent(&data).is_some();
	assert_eq!(
		pallet::Pallet::<Test>::is_inherent_required(&data).unwrap().is_some(),
		proposed,
		"proposed and required disagreed"
	);
	proposed
}

fn inherent_data(observation: &RegistrationsObservation) -> InherentData {
	let mut data = InherentData::new();
	data.put_data(INHERENT_IDENTIFIER, observation).unwrap();
	data
}

/// An observation of `count` deposits, one per Cardano block.
fn observation_of(count: u8) -> RegistrationsObservation {
	observation(
		(0..count)
			.map(|i| observed(unverified_datum(i), CardanoTxPosition::new(10 + i as u32, 0)))
			.collect(),
	)
}

#[test]
fn an_inherent_carries_exactly_what_was_observed() {
	new_test_ext().execute_with(|| {
		let observed_batch = observation_of(3);

		let call = pallet::Pallet::<Test>::create_inherent(&inherent_data(&observed_batch))
			.expect("an observation should be proposed");

		assert!(pallet::Pallet::<Test>::is_inherent(&call));
		match call {
			pallet::Call::apply_registrations { registrations, next_position } => {
				assert_eq!(registrations.to_vec(), observed_batch.registrations);
				assert_eq!(next_position, observed_batch.next_position);
			},
			_ => panic!("create_inherent produced the wrong call"),
		}
	});
}

#[test]
fn check_inherent_accepts_the_batch_that_was_observed() {
	new_test_ext().execute_with(|| {
		let observed_batch = observation_of(3);
		let data = inherent_data(&observed_batch);
		let call = pallet::Pallet::<Test>::create_inherent(&data).expect("proposed");

		assert_eq!(pallet::Pallet::<Test>::check_inherent(&call, &data), Ok(()));
		assert!(pallet::Pallet::<Test>::is_inherent(&call));
	});
}

#[test]
fn check_inherent_rejects_a_batch_that_is_not_what_was_observed() {
	// The importer's own observation is the only thing standing between the
	// proposer and the registration set, and `InherentError` is fatal.
	new_test_ext().execute_with(|| {
		let observed_batch = observation_of(3);
		let call = pallet::Pallet::<Test>::create_inherent(&inherent_data(&observed_batch))
			.expect("proposed");

		// One byte different inside a datum.
		let mut tampered = observed_batch.clone();
		tampered.registrations[1].datum.cross_chain_pub_key = CrossChainPublicKeyBytes([0xff; 33]);
		assert_eq!(
			pallet::Pallet::<Test>::check_inherent(&call, &inherent_data(&tampered)),
			Err(InherentError::Mismatch)
		);

		// A deposit dropped from the middle.
		let mut shortened = observed_batch.clone();
		shortened.registrations.remove(1);
		assert_eq!(
			pallet::Pallet::<Test>::check_inherent(&call, &inherent_data(&shortened)),
			Err(InherentError::Mismatch)
		);

		// The same deposits, reordered. Not `Mismatch` but `Unexpected`: a
		// batch out of Cardano order is one this node would not propose at
		// all, so it does not get as far as being compared.
		let mut reordered = observed_batch.clone();
		reordered.registrations.reverse();
		assert_eq!(
			pallet::Pallet::<Test>::check_inherent(&call, &inherent_data(&reordered)),
			Err(InherentError::Unexpected)
		);

		// The cursor moved past deposits nobody observed.
		let mut moved = observed_batch.clone();
		moved.next_position = CardanoTxPosition::new(99, 0);
		assert_eq!(
			pallet::Pallet::<Test>::check_inherent(&call, &inherent_data(&moved)),
			Err(InherentError::Mismatch)
		);
	});
}

#[test]
fn check_inherent_rejects_a_call_this_node_observed_nothing_for() {
	new_test_ext().execute_with(|| {
		let call = pallet::Pallet::<Test>::create_inherent(&inherent_data(&observation_of(1)))
			.expect("proposed");

		assert_eq!(
			pallet::Pallet::<Test>::check_inherent(&call, &InherentData::new()),
			Err(InherentError::Unexpected)
		);
	});
}

#[test]
fn an_inherent_is_required_exactly_when_one_is_proposed() {
	new_test_ext().execute_with(|| {
		// Nothing observed: nothing proposed, nothing required.
		let empty = InherentData::new();
		assert!(pallet::Pallet::<Test>::create_inherent(&empty).is_none());
		assert_eq!(pallet::Pallet::<Test>::is_inherent_required(&empty), Ok(None));

		// An observation carrying no deposits still moves the cursor, so it is
		// proposed and required like any other.
		let idle = inherent_data(&observation(vec![]));
		assert!(pallet::Pallet::<Test>::create_inherent(&idle).is_some());
		assert_eq!(
			pallet::Pallet::<Test>::is_inherent_required(&idle),
			Ok(Some(InherentError::Missing))
		);

		let batch = inherent_data(&observation_of(3));
		assert!(pallet::Pallet::<Test>::create_inherent(&batch).is_some());
		assert_eq!(
			pallet::Pallet::<Test>::is_inherent_required(&batch),
			Ok(Some(InherentError::Missing))
		);
	});
}

#[test]
fn an_observation_the_node_did_not_bound_is_refused_whole() {
	// Bounding the query by the published limit is the node's side of the
	// contract. A node that broke it is a node that does not support its
	// runtime, so the runtime refuses the observation rather than trimming it
	// to fit — it cannot trust the rest of what such a node reports either.
	new_test_ext().execute_with(|| {
		let data = inherent_data(&observation_of(limit() as u8 + 1));

		assert!(pallet::Pallet::<Test>::create_inherent(&data).is_none());

		// And not required, or the author that just dropped it could not build
		// a block at all.
		assert_eq!(pallet::Pallet::<Test>::is_inherent_required(&data), Ok(None));

		// And not accepted from a node that did propose it.
		let call =
			pallet::Pallet::<Test>::create_inherent(&inherent_data(&observation_of(limit as u8)))
				.expect("the limit itself is proposable");
		assert_eq!(
			pallet::Pallet::<Test>::check_inherent(&call, &data),
			Err(InherentError::Unexpected)
		);
	});
}

#[test]
fn max_registration_events_per_block_is_where_an_observation_stops_being_proposable() {
	new_test_ext().execute_with(|| {
		assert!(
			pallet::Pallet::<Test>::create_inherent(&inherent_data(&observation_of(limit() as u8)))
				.is_some()
		);
		assert!(
			pallet::Pallet::<Test>::create_inherent(&inherent_data(&observation_of(
				limit() as u8 + 1
			)))
			.is_none()
		);
	});
}

#[test]
fn an_unbounded_observation_never_reaches_the_mandatory_call() {
	// A failing `Mandatory` dispatch stops the author producing a block, so an
	// over-sized observation has to be stopped before it becomes a call. Two
	// things stop it, and this pins both: `create_inherent` refuses it, and
	// the batch it would have to be carried in cannot hold it anyway.
	new_test_ext().execute_with(|| {
		let limit = limit();
		let data = inherent_data(&observation_of(limit as u8 + 1));

		assert!(pallet::Pallet::<Test>::create_inherent(&data).is_none());

		let events: Vec<_> = (0..=limit as u8)
			.map(|i| observed(unverified_datum(i), CardanoTxPosition::new(10 + i as u32, 0)))
			.collect();
		assert!(RegistrationBatch::<Test>::try_from(events).is_err());
	});
}

#[test]
fn a_batch_must_start_at_the_cursor_and_ascend_from_there() {
	new_test_ext().execute_with(|| {
		NextCardanoPosition::<Test>::set(CardanoTxPosition::new(20, 0));
		let at = |block, index| observed(unverified_datum(0), CardanoTxPosition::new(block, index));

		// Starts at the cursor, ascends: fine.
		assert!(is_proposable(&observation(vec![at(20, 0), at(20, 1), at(21, 0)])));
		// Starts after the cursor: also fine, nothing registered in between.
		assert!(is_proposable(&observation(vec![at(25, 0)])));

		// Starts before the cursor — already applied.
		assert!(!is_proposable(&observation_with_cursor(
			vec![at(19, 9)],
			CardanoTxPosition::new(26, 0)
		)));
		// Out of order within the batch.
		assert!(!is_proposable(&observation(vec![at(21, 0), at(20, 0)])));
		// The same transaction twice.
		assert!(!is_proposable(&observation(vec![at(20, 0), at(20, 0)])));
		// Ascending, but one deposit sits before the cursor.
		assert!(!is_proposable(&observation(vec![at(19, 0), at(21, 0)])));
	});
}

#[test]
fn a_cursor_must_clear_the_batch_it_comes_with() {
	new_test_ext().execute_with(|| {
		NextCardanoPosition::<Test>::set(CardanoTxPosition::new(20, 0));
		let at = |block, index| observed(unverified_datum(0), CardanoTxPosition::new(block, index));

		// Exactly past the last deposit.
		assert!(is_proposable(&observation_with_cursor(
			vec![at(20, 0)],
			CardanoTxPosition::new(20, 1)
		)));
		// Further on: the node scanned past transactions that carried no
		// registration, which is the normal case.
		assert!(is_proposable(&observation_with_cursor(
			vec![at(20, 0)],
			CardanoTxPosition::new(99, 0)
		)));
		// Not past its own last deposit: that deposit would be re-presented.
		assert!(!is_proposable(&observation_with_cursor(
			vec![at(20, 0)],
			CardanoTxPosition::new(20, 0)
		)));

		// An empty batch may hold the cursor still or move it on, but not back.
		assert!(is_proposable(&observation_with_cursor(vec![], CardanoTxPosition::new(20, 0))));
		assert!(is_proposable(&observation_with_cursor(vec![], CardanoTxPosition::new(40, 0))));
		assert!(!is_proposable(&observation_with_cursor(vec![], CardanoTxPosition::new(19, 0))));
	});
}

#[test]
fn replaying_an_applied_deposit_cannot_reactivate_a_pool_that_stood_down() {
	// `apply_one` matches `(_, Active)` whatever the record currently says, so
	// a replayed Active deposit used to overwrite an Inactive record and make
	// the pool a candidate again. The guard is at the inherent boundary: the
	// observation never becomes a call.
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(120);
		let first = observed(operator.register(), CardanoTxPosition::new(10, 0));
		assert_ok!(apply(vec![first.clone()]));
		assert_ok!(apply(vec![observed(
			inactive_deposit_of(&operator),
			CardanoTxPosition::new(11, 0)
		)]));
		assert_eq!(state_of(operator.pool_id()), RegistrationState::Inactive);

		// The applied deposit, re-sent with a cursor that moves forward.
		assert!(!is_proposable(&observation_with_cursor(
			vec![first],
			CardanoTxPosition::new(12, 0)
		)));

		assert_eq!(state_of(operator.pool_id()), RegistrationState::Inactive);
		assert!(pallet::Pallet::<Test>::active_candidates().is_empty());
	});
}

#[test]
fn replaying_a_superseded_deposit_cannot_rewind_a_key_rotation() {
	// Newest-wins is positional within a batch. Without the cursor check a
	// replayed deposit is "newest" again, and the record goes back to the keys
	// the pool rotated away from.
	new_test_ext().execute_with(|| {
		let mut operator = Operator::from_seed(130);
		let first = observed(operator.register(), CardanoTxPosition::new(10, 0));
		assert_ok!(apply(vec![first.clone()]));

		operator.aura = sr25519::Pair::from_seed(&[131u8; 32]);
		operator.babe = sr25519::Pair::from_seed(&[132u8; 32]);
		operator.grandpa = ed25519::Pair::from_seed(&[133u8; 32]);
		operator.beefy = ecdsa::Pair::from_seed(&[134u8; 32]);
		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(11, 0))]));

		let rotated = Registrations::<Test>::get(operator.pool_id()).unwrap();
		assert_eq!(rotated.observed_at(), CardanoTxPosition::new(11, 0));

		assert!(!is_proposable(&observation_with_cursor(
			vec![first],
			CardanoTxPosition::new(12, 0)
		)));

		let after = Registrations::<Test>::get(operator.pool_id()).unwrap();
		assert_eq!(after.observed_at(), CardanoTxPosition::new(11, 0));
		assert_eq!(after, rotated);
	});
}

/// The session key slots, in the order a canonical datum carries them.
const KEY_SLOTS: [(usize, [u8; 4]); 4] = [(0, AURA.0), (1, BABE.0), (2, BEEFY.0), (3, GRANDPA.0)];

#[test]
fn a_key_of_the_wrong_length_for_its_slot_is_an_invalid_proof() {
	for (slot, id) in KEY_SLOTS {
		for len in [0usize, 1, 31, 32, 33, 64] {
			new_test_ext().execute_with(|| {
				let operator = Operator::from_seed(140 + slot as u8);
				let mut keys = operator.candidate_keys();
				keys[slot] = (KeyMaterial::Raw(vec![0xcd; len]), id);

				assert_ok!(apply(vec![observed(
					operator.register_with_keys(keys),
					CardanoTxPosition::new(10, 0)
				)]));

				assert_eq!(
					state_of(operator.pool_id()),
					RegistrationState::Invalid(RegistrationInvalidReason::InvalidKeyProof(id)),
					"slot {slot}, key length {len}"
				);
			});
		}
	}
}

#[test]
fn a_proof_of_the_wrong_length_for_its_slot_is_an_invalid_proof() {
	// Proofs are projected out of the message, so these can be swapped in
	// after signing without disturbing anything else — which is also why the
	// failure is attributable to exactly one slot.
	for (slot, id) in KEY_SLOTS {
		for len in [0usize, 1, 63, 64, 65, 96] {
			new_test_ext().execute_with(|| {
				let operator = Operator::from_seed(160 + slot as u8);
				let mut datum = operator.register();
				let mut keys = datum.keys.to_vec();
				keys[slot].proof = vec![0xcd; len].try_into().unwrap();
				datum.keys = CandidateKeys::try_from(keys).unwrap();

				assert_ok!(apply(vec![observed(datum, CardanoTxPosition::new(10, 0))]));

				assert_eq!(
					state_of(operator.pool_id()),
					RegistrationState::Invalid(RegistrationInvalidReason::InvalidKeyProof(id)),
					"slot {slot}, proof length {len}"
				);
			});
		}
	}
}

#[test]
fn an_empty_key_list_is_a_missing_proof_not_a_crash() {
	// The far end of the same boundary: `CandidateKeys` admits an empty list,
	// and nothing about a datum obliges it to carry keys at all.
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(180);
		let datum = operator.register_with_keys(vec![]);

		assert_ok!(apply(vec![observed(datum, CardanoTxPosition::new(10, 0))]));

		assert_eq!(
			state_of(operator.pool_id()),
			RegistrationState::Invalid(RegistrationInvalidReason::MissingKeyProof(AURA.0))
		);
	});
}

#[test]
fn an_admitted_registration_keeps_the_keys_it_bound() {
	// The record carries the assembled `SessionKeys`, not just the datum bytes
	// they came from. Rebuilding them later would be a second assembly that
	// could disagree with what `pallet_session` holds, and the committee
	// hand-off binds whatever it is handed.
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(190);
		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(10, 0))]));

		let RegistrationRecord::Active { session_keys: stored, .. } =
			Registrations::<Test>::get(operator.pool_id()).unwrap()
		else {
			panic!("an admitted registration is Active and carries its keys");
		};

		// Every slot of the stored keys is the one the session index already
		// holds for this account, so a hand-off binding these is a no-op.
		for id in MockSessionKeys::key_ids() {
			assert_eq!(
				owner_of(id.0, stored.get_raw(*id)),
				Some(operator.account_id()),
				"key type {id:?}"
			);
		}

		// And they are what selection is handed.
		let candidates = pallet::Pallet::<Test>::active_candidates();
		assert_eq!(candidates.len(), 1);
		assert_eq!(candidates[0].2, stored);
	});
}

#[test]
fn a_refused_registration_carries_no_session_keys() {
	// `Some` exactly when the verdict is Active: there is no half-assembled
	// key set for a registration that never got past verification.
	new_test_ext().execute_with(|| {
		let operator = Operator::from_seed(191);
		let mut broken = operator.register();
		broken.cross_chain_signature = CrossChainSignatureBytes([0u8; 65]);
		assert_ok!(apply(vec![observed(broken, CardanoTxPosition::new(10, 0))]));

		let RegistrationRecord::Invalid { reason, .. } =
			Registrations::<Test>::get(operator.pool_id()).unwrap()
		else {
			panic!("a refused registration is Invalid");
		};
		assert_eq!(reason, RegistrationInvalidReason::InvalidCrossChainSignature);
		assert!(pallet::Pallet::<Test>::active_candidates().is_empty());
	});
}

#[test]
fn a_pool_that_stood_down_still_collides_with_its_own_old_keys() {
	// Inactivating releases nothing: the keys stay bound to the account the
	// registration derived. A pool that comes back under a new cross-chain key
	// while keeping one of them collides with itself.
	new_test_ext().execute_with(|| {
		let mut operator = Operator::from_seed(200);
		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(10, 0))]));
		assert_ok!(apply(vec![observed(
			inactive_deposit_of(&operator),
			CardanoTxPosition::new(11, 0)
		)]));
		assert_eq!(state_of(operator.pool_id()), RegistrationState::Inactive);

		// Back with a new cross-chain key, but grandpa was not rotated.
		operator.cross_chain = ecdsa::Pair::from_seed(&[201u8; 32]);
		operator.aura = sr25519::Pair::from_seed(&[202u8; 32]);
		operator.babe = sr25519::Pair::from_seed(&[203u8; 32]);
		operator.beefy = ecdsa::Pair::from_seed(&[204u8; 32]);
		assert_ok!(apply(vec![observed(operator.register(), CardanoTxPosition::new(12, 0))]));

		assert_eq!(
			state_of(operator.pool_id()),
			RegistrationState::Invalid(RegistrationInvalidReason::SessionKeyReuse)
		);
	});
}
