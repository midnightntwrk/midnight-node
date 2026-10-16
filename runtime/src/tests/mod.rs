// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::mock::*;
use crate::{SystemParameters, select_authorities_optionally_overriding};
use authority_selection_inherents::{AuthoritySelectionInputs, RegisterValidatorSignedMessage};
use frame_support::{
	assert_ok,
	dispatch::PostDispatchInfo,
	inherent::ProvideInherent,
	traits::{FindAuthor, UnfilteredDispatchable, WhitelistedStorageKeys},
};
use frame_system::RawOrigin;
use midnight_primitives_consensus_engine::ActiveEngine;
use parity_scale_codec::Decode;
use sidechain_domain::{
	CandidateRegistrations, CrossChainPublicKey, CrossChainSignature, DParameter, EpochNonce,
	MainchainSignature, PermissionedCandidateData, RegistrationData, ScEpochNumber,
	SidechainSignature, StakeDelegation, StakePoolPublicKey, UtxoId, UtxoInfo,
};
use sp_consensus_aura::AURA_ENGINE_ID;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_babe::{BABE_ENGINE_ID, BabeAuthorityWeight, ConsensusLog};
use sp_core::crypto::AccountId32;
use sp_core::{Pair, ed25519, hexdisplay::HexDisplay};
use sp_inherents::InherentData;
use sp_runtime::traits::Zero;
use sp_session_validator_management::CommitteeMember as _;
use std::collections::HashSet;

#[test]
fn check_whitelist() {
	let whitelist: HashSet<String> = super::AllPalletsWithSystem::whitelisted_storage_keys()
		.iter()
		.map(|e| HexDisplay::from(&e.key).to_string())
		.collect();

	// Block Number
	assert!(whitelist.contains("26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac"));
	// Execution Phase
	assert!(whitelist.contains("26aa394eea5630e07c48ae0c9558cef7ff553b5a9862a516939d82b3d3d8661a"));
	// Event Count
	assert!(whitelist.contains("26aa394eea5630e07c48ae0c9558cef70a98fdbe9ce6c55837576c60c7af3850"));
	// System Events
	assert!(whitelist.contains("26aa394eea5630e07c48ae0c9558cef780d41e5e16056765bc8461851072c9d7"));
}

// The set committee takes effect next session. Committee can be set for 1 session in advance.
#[test]
fn check_grandpa_authorities_rotation() {
	new_test_ext().execute_with(|| {
		// Needs to be run to initialize first slot and epoch numbers;
		advance_block();

		// Scheduled committee goes into effect after a 2-epoch delay
		set_committee_through_inherent_data(&[alice()]);
		until_epoch_after_finalizing(1, &|| {
			assert_current_epoch!(0);
			assert_grandpa_weights();
			assert_grandpa_authorities!([alice(), bob()]);
		});

		set_committee_through_inherent_data(&[bob()]);
		for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
			assert_current_epoch!(1);
			assert_grandpa_weights();
			assert_grandpa_authorities!([alice(), bob()]);
		});
		set_committee_through_inherent_data(&[alice()]);
		for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
			assert_current_epoch!(2);
			assert_grandpa_weights();
			assert_grandpa_authorities!([alice()]);
		});
		set_committee_through_inherent_data(&[alice(), bob()]);
		for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
			assert_current_epoch!(3);
			assert_grandpa_weights();
			assert_grandpa_authorities!([bob()]);
		});
		set_committee_through_inherent_data(&[bob(), alice()]);
		for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
			assert_current_epoch!(4);
			assert_grandpa_weights();
			assert_grandpa_authorities!([alice()]);
		});
		set_committee_through_inherent_data(&[alice()]);
		for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
			assert_current_epoch!(5);
			assert_grandpa_weights();
			assert_grandpa_authorities!([alice(), bob()]);
		});

		// When there's no new committees being scheduled, the last committee stays in power
		for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH * 3, &|| {
			assert_grandpa_weights();
			assert_grandpa_authorities!([bob(), alice()]);
		});
	});

	fn assert_grandpa_weights() {
		Grandpa::grandpa_authorities()
			.into_iter()
			.for_each(|(_, weight)| assert_eq!(weight, 1))
	}
}

// The set committee takes effect next session. Committee can be set for 1 session in advance.
#[test]
fn check_aura_authorities_rotation() {
	new_test_ext().execute_with(|| {
		// Needs to be run to initialize first slot and epoch numbers;
		advance_block();
		// Scheduled committee goes into effect after a 2-epoch delay
		set_committee_through_inherent_data(&[alice()]);
		until_epoch_after_finalizing(1, &|| {
			assert_current_epoch!(0);
			assert_aura_authorities!([alice(), bob()]);
		});

		set_committee_through_inherent_data(&[bob()]);
		for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
			assert_current_epoch!(1);
			assert_aura_authorities!([alice(), bob()]);
		});
		set_committee_through_inherent_data(&[alice()]);
		for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
			assert_current_epoch!(2);
			assert_aura_authorities!([alice()]);
		});
		set_committee_through_inherent_data(&[alice(), bob()]);
		for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
			assert_current_epoch!(3);
			assert_aura_authorities!([bob()]);
		});
		set_committee_through_inherent_data(&[bob(), alice()]);
		for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
			assert_current_epoch!(4);
			assert_aura_authorities!([alice()]);
		});
		set_committee_through_inherent_data(&[alice()]);
		for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH, &|| {
			assert_current_epoch!(5);
			assert_aura_authorities!([alice(), bob()]);
		});

		// When there's no new committees being scheduled, the last committee stays in power
		for_next_n_blocks_after_finalizing(SLOTS_PER_EPOCH * 3, &|| {
			assert_aura_authorities!([bob(), alice()]);
		});
	});
}

// The set committee takes effect at next session. Committee can be set for 1 session in advance.
#[test]
fn check_cross_chain_committee_rotation() {
	new_test_ext().execute_with(|| {
		advance_block();
		set_committee_through_inherent_data(&[alice()]);
		until_epoch(1, &|| {
			assert_current_epoch!(0);
			assert_next_committee!([alice()]);
		});

		set_committee_through_inherent_data(&[bob()]);
		for_next_n_blocks(SLOTS_PER_EPOCH, &|| {
			assert_current_epoch!(1);
			assert_next_committee!([bob()]);
		});

		set_committee_through_inherent_data(&[]);
		for_next_n_blocks(SLOTS_PER_EPOCH, &|| {
			assert_current_epoch!(2);
			assert_next_committee!([bob()]);
		});
	});
}

mod flip_observers;

/// Observers read the chain through runtime APIs. Each one must be the pallet state the
/// [`flip_observers`] coherence tests check, with nothing in between.
mod runtime_api_views {
	use crate::mock::{TestKeys, alice, bob};
	use crate::{
		AuraId, BABE_GENESIS_EPOCH_CONFIG, BeefyId, Block, GrandpaId, Hash, Runtime, SLOT_DURATION,
		Sidechain, opaque,
	};
	use authority_selection_inherents::{AuthoritySelectionInputs, CommitteeMember};
	use frame_support::{BoundedVec, WeakBoundedVec};
	use midnight_primitives_consensus_engine::ActiveEngine;
	use pallet_consensus_engine::{EngineState, State};
	use pallet_session_validator_management::{
		CommitteeInfo, CurrentCommittee, MainChainScriptsConfiguration, NextCommittee,
		QueuedCommittee,
	};
	use parity_scale_codec::Encode;
	use sidechain_domain::{MainchainAddress, PolicyId, ScEpochNumber};
	use sp_consensus_beefy::mmr::BeefyAuthoritySet;
	use sp_consensus_slots::{Slot, SlotDuration};
	use sp_core::{ByteArray, Pair};
	use sp_runtime::key_types::{AURA, BABE, BEEFY, GRANDPA};
	use sp_session_validator_management::MainChainScripts;

	type CommitteeMemberOf =
		<Runtime as pallet_session_validator_management::Config>::CommitteeMember;

	fn ext() -> sp_io::TestExternalities {
		sp_io::TestExternalities::default()
	}

	fn babe_set(
		seed: u8,
	) -> WeakBoundedVec<
		(sp_consensus_babe::AuthorityId, sp_consensus_babe::BabeAuthorityWeight),
		<Runtime as pallet_babe::Config>::MaxAuthorities,
	> {
		let id =
			sp_consensus_babe::AuthorityId::from(sp_core::sr25519::Public::from_raw([seed; 32]));
		WeakBoundedVec::try_from(vec![(id, 1)]).expect("one authority fits")
	}

	fn beefy_id(keys: &TestKeys) -> BeefyId {
		keys.cross_chain.public().into_inner().into()
	}

	fn member(keys: &TestKeys) -> CommitteeMemberOf {
		CommitteeMember::permissioned(
			keys.cross_chain.public(),
			opaque::SessionKeys {
				aura: keys.aura.public(),
				grandpa: keys.grandpa.public(),
				babe: keys.babe.public(),
				beefy: beefy_id(keys),
			},
		)
	}

	fn committee(
		epoch: u64,
		members: &[TestKeys],
	) -> CommitteeInfo<
		ScEpochNumber,
		CommitteeMemberOf,
		<Runtime as pallet_session_validator_management::Config>::MaxValidators,
	> {
		CommitteeInfo {
			epoch: ScEpochNumber(epoch),
			committee: BoundedVec::truncate_from(members.iter().map(member).collect()),
		}
	}

	#[test]
	fn aura_api_reports_the_pallet_authorities_and_slot_duration() {
		ext().execute_with(|| {
			let authorities = vec![alice().aura.public(), bob().aura.public()];
			pallet_aura::Authorities::<Runtime>::put(BoundedVec::truncate_from(
				authorities.clone(),
			));

			use sp_consensus_aura::runtime_decl_for_aura_api::AuraApi;
			assert_eq!(<Runtime as AuraApi<Block, AuraId>>::authorities(), authorities);
			assert_eq!(
				<Runtime as AuraApi<Block, AuraId>>::slot_duration(),
				SlotDuration::from_millis(SLOT_DURATION)
			);
		});
	}

	#[test]
	fn babe_api_reports_the_pallet_epochs_and_configuration() {
		ext().execute_with(|| {
			pallet_babe::EpochConfig::<Runtime>::put(BABE_GENESIS_EPOCH_CONFIG);
			pallet_babe::Authorities::<Runtime>::put(babe_set(1));
			pallet_babe::NextAuthorities::<Runtime>::put(babe_set(2));
			pallet_babe::Randomness::<Runtime>::put([7; 32]);
			pallet_babe::GenesisSlot::<Runtime>::put(Slot::from(700u64));
			pallet_babe::EpochIndex::<Runtime>::put(2);
			let epoch_length = u64::from(Sidechain::slots_per_epoch().0);

			use sp_consensus_babe::runtime_decl_for_babe_api::BabeApi;
			let current = <Runtime as BabeApi<Block>>::current_epoch();
			assert_eq!(current.authorities, babe_set(1).to_vec());
			assert_eq!(current.epoch_index, 2);
			assert_eq!(current.start_slot, Slot::from(700 + 2 * epoch_length));
			assert_eq!(current.duration, epoch_length);
			assert_eq!(current.config, BABE_GENESIS_EPOCH_CONFIG);
			let next = <Runtime as BabeApi<Block>>::next_epoch();
			assert_eq!(next.authorities, babe_set(2).to_vec());
			assert_eq!(next.epoch_index, 3);
			assert_eq!(next.start_slot, Slot::from(700 + 3 * epoch_length));
			assert_eq!(
				<Runtime as BabeApi<Block>>::current_epoch_start(),
				Slot::from(700 + 2 * epoch_length)
			);
			let configuration = <Runtime as BabeApi<Block>>::configuration();
			assert_eq!(configuration.authorities, babe_set(1).to_vec());
			assert_eq!(configuration.randomness, [7; 32]);
			assert_eq!(configuration.epoch_length, epoch_length);
			assert_eq!(configuration.slot_duration, SLOT_DURATION);
			assert_eq!(configuration.c, BABE_GENESIS_EPOCH_CONFIG.c);
			assert_eq!(configuration.allowed_slots, BABE_GENESIS_EPOCH_CONFIG.allowed_slots);
			assert!(
				<Runtime as BabeApi<Block>>::generate_key_ownership_proof(
					Slot::from(700u64),
					babe_set(1)[0].0.clone()
				)
				.is_none()
			);
		});
	}

	#[test]
	fn grandpa_api_reports_the_pallet_authorities_and_set_id() {
		ext().execute_with(|| {
			let authorities = vec![(alice().grandpa.public(), 1u64), (bob().grandpa.public(), 1)];
			pallet_grandpa::Authorities::<Runtime>::put(
				WeakBoundedVec::try_from(authorities.clone()).expect("two authorities fit"),
			);
			pallet_grandpa::CurrentSetId::<Runtime>::put(3);

			use sp_consensus_grandpa::runtime_decl_for_grandpa_api::GrandpaApi;
			assert_eq!(<Runtime as GrandpaApi<Block>>::grandpa_authorities(), authorities);
			assert_eq!(<Runtime as GrandpaApi<Block>>::current_set_id(), 3);
			assert!(
				<Runtime as GrandpaApi<Block>>::generate_key_ownership_proof(
					3,
					GrandpaId::from(alice().grandpa.public())
				)
				.is_none()
			);
		});
	}

	#[test]
	fn beefy_api_reports_the_pallet_validator_set_and_genesis() {
		ext().execute_with(|| {
			let validators = vec![beefy_id(&alice()), beefy_id(&bob())];
			pallet_beefy::Authorities::<Runtime>::put(BoundedVec::truncate_from(
				validators.clone(),
			));
			pallet_beefy::ValidatorSetId::<Runtime>::put(2);
			pallet_beefy::GenesisBlock::<Runtime>::put(Some(5));

			use sp_consensus_beefy::runtime_decl_for_beefy_api::BeefyApi;
			let set = <Runtime as BeefyApi<Block, BeefyId>>::validator_set().expect("a set");
			assert_eq!(set.validators(), &validators[..]);
			assert_eq!(set.id(), 2);
			assert_eq!(<Runtime as BeefyApi<Block, BeefyId>>::beefy_genesis(), Some(5));
			assert!(
				<Runtime as BeefyApi<Block, BeefyId>>::generate_key_ownership_proof(
					2,
					beefy_id(&alice())
				)
				.is_none()
			);
		});
	}

	#[test]
	fn beefy_stakes_api_reports_the_current_and_queued_sets() {
		ext().execute_with(|| {
			pallet_beefy::Authorities::<Runtime>::put(BoundedVec::truncate_from(vec![beefy_id(
				&alice(),
			)]));
			pallet_beefy::NextAuthorities::<Runtime>::put(BoundedVec::truncate_from(vec![
				beefy_id(&bob()),
			]));
			CurrentCommittee::<Runtime>::put(committee(7, &[alice()]));
			QueuedCommittee::<Runtime>::put(committee(7, &[bob()]));

			use midnight_primitives_beefy::runtime_decl_for_beefy_stakes_api::BeefyStakesApi;
			assert_eq!(
				<Runtime as BeefyStakesApi<Block, Hash, BeefyId>>::current_beefy_stakes(),
				vec![(beefy_id(&alice()), 1)]
			);
			assert_eq!(
				<Runtime as BeefyStakesApi<Block, Hash, BeefyId>>::next_beefy_stakes(),
				Some(vec![(beefy_id(&bob()), 1)])
			);
		});
	}

	#[test]
	fn beefy_mmr_api_reports_the_authority_set_proofs() {
		ext().execute_with(|| {
			let current =
				BeefyAuthoritySet { id: 2, len: 1, keyset_commitment: Hash::repeat_byte(2) };
			let next = BeefyAuthoritySet { id: 3, len: 2, keyset_commitment: Hash::repeat_byte(3) };
			pallet_beefy_mmr::BeefyAuthorities::<Runtime>::put(current.clone());
			pallet_beefy_mmr::BeefyNextAuthorities::<Runtime>::put(next.clone());

			use pallet_beefy_mmr::runtime_decl_for_beefy_mmr_api::BeefyMmrApi;
			assert_eq!(<Runtime as BeefyMmrApi<Block, Hash>>::authority_set_proof(), current);
			assert_eq!(<Runtime as BeefyMmrApi<Block, Hash>>::next_authority_set_proof(), next);
		});
	}

	#[test]
	fn session_keys_api_decodes_the_opaque_keys_in_declaration_order() {
		ext().execute_with(|| {
			let keys = alice();
			let encoded = opaque::SessionKeys {
				aura: keys.aura.public(),
				grandpa: keys.grandpa.public(),
				babe: keys.babe.public(),
				beefy: beefy_id(&keys),
			}
			.encode();

			use sp_session::runtime_decl_for_session_keys::SessionKeys;
			assert_eq!(
				<Runtime as SessionKeys<Block>>::decode_session_keys(encoded),
				Some(vec![
					(keys.aura.public().to_raw_vec(), AURA),
					(keys.grandpa.public().to_raw_vec(), GRANDPA),
					(keys.babe.public().to_raw_vec(), BABE),
					(beefy_id(&keys).to_raw_vec(), BEEFY),
				])
			);
		});
	}

	#[test]
	fn session_info_api_reports_the_session_index() {
		ext().execute_with(|| {
			pallet_session::CurrentIndex::<Runtime>::put(5);

			use midnight_primitives_session_info::runtime_decl_for_session_info_api::SessionInfoApi;
			assert_eq!(<Runtime as SessionInfoApi<Block>>::current_session_index(), 5);
		});
	}

	#[test]
	fn consensus_engine_api_reports_the_engine_state() {
		ext().execute_with(|| {
			use midnight_primitives_consensus_engine::runtime_decl_for_consensus_engine_api::ConsensusEngineApi;
			assert_eq!(
				<Runtime as ConsensusEngineApi<Block>>::active_engine(),
				ActiveEngine::Aura
			);
			EngineState::<Runtime>::put(State::ScheduledFlip);
			assert_eq!(
				<Runtime as ConsensusEngineApi<Block>>::active_engine(),
				ActiveEngine::Aura
			);
			EngineState::<Runtime>::put(State::Babe);
			assert_eq!(
				<Runtime as ConsensusEngineApi<Block>>::active_engine(),
				ActiveEngine::Babe
			);
		});
	}

	#[test]
	fn committee_api_reports_the_committees_epochs_and_scripts() {
		ext().execute_with(|| {
			let current = committee(7, &[alice()]);
			let next = committee(8, &[alice(), bob()]);
			CurrentCommittee::<Runtime>::put(current.clone());
			QueuedCommittee::<Runtime>::put(committee(7, &[bob()]));
			let scripts = MainChainScripts {
				committee_candidate_address: MainchainAddress::default(),
				d_parameter_policy_id: PolicyId([1; 28]),
				permissioned_candidates_policy_id: PolicyId([2; 28]),
			};
			MainChainScriptsConfiguration::<Runtime>::put(scripts.clone());

			use sp_session_validator_management::runtime_decl_for_session_validator_management_api::SessionValidatorManagementApi;
			macro_rules! api {
				($method:ident) => {
					<Runtime as SessionValidatorManagementApi<
						Block,
						CommitteeMemberOf,
						AuthoritySelectionInputs,
						ScEpochNumber,
					>>::$method()
				};
			}
			assert_eq!(api!(get_current_committee), (ScEpochNumber(7), current.committee.to_vec()));
			assert_eq!(api!(get_next_committee), None);
			// With nothing selected yet, the next epoch to fill follows the queued one.
			assert_eq!(api!(get_next_unset_epoch_number), ScEpochNumber(8));
			NextCommittee::<Runtime>::put(next.clone());
			assert_eq!(api!(get_next_committee), Some((ScEpochNumber(8), next.committee.to_vec())));
			assert_eq!(api!(get_next_unset_epoch_number), ScEpochNumber(9));
			assert_eq!(api!(get_main_chain_scripts), scripts);
		});
	}
}

#[test]
// The effects of setting the d parameter are already well-tested, so we will not check that. We will check the selection to ensure that it simply respects d-parameter overriding
fn check_overridden_d_param_committee_rotation() {
	new_test_ext().execute_with(|| {
		let permissioned_validators = vec![alice_mock_validator(), bob_mock_validator()];
		let registered_validators = vec![charlie_mock_validator()];

		let d_parameter =
			DParameter { num_permissioned_candidates: 1, num_registered_candidates: 0 };

		// Set initial D-parameter in SystemParameters pallet
		assert_ok!(SystemParameters::update_d_parameter(RawOrigin::Root.into(), 1, 0));

		let authority_selection_inputs = create_authority_selection_inputs(
			&permissioned_validators,
			&registered_validators,
			d_parameter,
		);

		let initially_selected_authorities = select_authorities_optionally_overriding(
			authority_selection_inputs.clone(),
			ScEpochNumber::zero(),
		);

		assert_eq!(initially_selected_authorities.unwrap().len(), 1);

		// Override the D-parameter via SystemParameters pallet
		assert_ok!(SystemParameters::update_d_parameter(RawOrigin::Root.into(), 20, 2));

		let selected_authorities_override = select_authorities_optionally_overriding(
			authority_selection_inputs,
			ScEpochNumber::zero(),
		);

		assert_eq!(selected_authorities_override.unwrap().len(), 22);
	})
}

pub fn set_committee_through_inherent_data(expected_authorities: &[TestKeys]) -> PostDispatchInfo {
	let epoch = Sidechain::current_epoch_number();
	let slot = *ConsensusEngine::current_slot();
	println!(
		"(slot {slot}, epoch {epoch}) Setting {} authorities for next epoch",
		expected_authorities.len()
	);
	let inherent_data_struct = create_inherent_data_struct(expected_authorities);
	let mut inherent_data = InherentData::new();
	inherent_data
		.put_data(
			SessionCommitteeManagement::INHERENT_IDENTIFIER,
			&inherent_data_struct.data.unwrap(),
		)
		.expect("Setting inherent data should not fail");
	let call = <SessionCommitteeManagement as ProvideInherent>::create_inherent(&inherent_data)
		.expect("Creating test inherent should not fail");
	println!("    inherent: {call:?}");
	call.dispatch_bypass_filter(RuntimeOrigin::none())
		.expect("dispatching test call should work")
}

pub fn create_authority_selection_inputs(
	permissioned_candidates: &[MockValidator],
	validators: &[MockValidator],
	d_parameter: DParameter,
) -> AuthoritySelectionInputs {
	let epoch_candidates = create_epoch_candidates_idp(validators);

	let permissioned_candidates_data: Vec<PermissionedCandidateData> = permissioned_candidates
		.iter()
		.map(|c| PermissionedCandidateData {
			sidechain_public_key: c.sidechain_pub_key(),
			keys: c.session_keys(),
		})
		.collect();
	AuthoritySelectionInputs {
		d_parameter,
		permissioned_candidates: permissioned_candidates_data,
		registered_candidates: epoch_candidates,
		epoch_nonce: EpochNonce(DUMMY_EPOCH_NONCE.to_vec()),
	}
}

fn create_epoch_candidates_idp(validators: &[MockValidator]) -> Vec<CandidateRegistrations> {
	let mainchain_key_pair: ed25519::Pair = ed25519::Pair::from_seed_slice(&[7u8; 32]).unwrap();

	let candidates: Vec<CandidateRegistrations> = validators
		.iter()
		.map(|validator| {
			let signed_message = RegisterValidatorSignedMessage {
				genesis_utxo: UtxoId::default(),
				sidechain_pub_key: validator.sidechain_pub_key().0,
				registration_utxo: UtxoId::default(),
			};

			let mock_mainchain_signature = mainchain_key_pair.sign(&[]);

			let sidechain_signature_bytes_no_recovery = mock_mainchain_signature.0[..64].to_vec();

			let registration_data = RegistrationData {
				registration_utxo: signed_message.registration_utxo,
				sidechain_signature: SidechainSignature(
					sidechain_signature_bytes_no_recovery.clone(),
				),
				mainchain_signature: MainchainSignature(mock_mainchain_signature.0),
				cross_chain_signature: CrossChainSignature(sidechain_signature_bytes_no_recovery),
				sidechain_pub_key: validator.sidechain_pub_key(),
				keys: validator.session_keys(),
				cross_chain_pub_key: CrossChainPublicKey(validator.sidechain_pub_key().0),
				utxo_info: UtxoInfo::default(),
				tx_inputs: vec![signed_message.registration_utxo],
			};

			CandidateRegistrations {
				registrations: vec![registration_data],
				stake_delegation: Some(StakeDelegation(validator.stake)),
				stake_pool_public_key: StakePoolPublicKey(mainchain_key_pair.public().0),
			}
		})
		.collect();

	candidates
}

mod sidechain_slot_number {
	use crate::Runtime;
	use frame_support::traits::Hooks;
	use pallet_consensus_engine::{EngineState, State};
	use parity_scale_codec::Encode;
	use sidechain_domain::ScSlotNumber;
	use sp_consensus_babe::BABE_ENGINE_ID;
	use sp_consensus_babe::digests::{PreDigest, SecondaryPlainPreDigest};
	use sp_consensus_slots::Slot;
	use sp_runtime::{Digest, DigestItem};

	fn babe_pre_digest(slot: u64) -> DigestItem {
		DigestItem::PreRuntime(
			BABE_ENGINE_ID,
			PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
				authority_index: 0,
				slot: Slot::from(slot),
			})
			.encode(),
		)
	}

	fn initialize_block_with_logs(logs: Vec<DigestItem>) {
		frame_system::Pallet::<Runtime>::initialize(&1, &Default::default(), &Digest { logs });
	}

	fn current_slot_number() -> ScSlotNumber {
		<Runtime as pallet_sidechain::Config>::current_slot_number()
	}

	#[test]
	fn is_read_from_aura_storage_pre_flip() {
		sp_io::TestExternalities::default().execute_with(|| {
			pallet_aura::CurrentSlot::<Runtime>::put(Slot::from(7u64));
			pallet_babe::CurrentSlot::<Runtime>::put(Slot::from(99u64));
			assert_eq!(current_slot_number(), ScSlotNumber(7));
		});
	}

	#[test]
	fn is_read_from_babe_storage_post_flip() {
		sp_io::TestExternalities::default().execute_with(|| {
			EngineState::<Runtime>::put(State::Babe);
			pallet_aura::CurrentSlot::<Runtime>::put(Slot::from(7u64));
			pallet_babe::CurrentSlot::<Runtime>::put(Slot::from(42u64));
			assert_eq!(current_slot_number(), ScSlotNumber(42));
		});
	}

	/// Regression: with Babe before Sidechain, Babe's `on_initialize` copies the
	/// pre-digest into `CurrentSlot` before Sidechain reads it — so a stale
	/// storage value is overwritten for this block.
	#[test]
	fn babe_hook_refreshes_storage_before_sidechain_reads() {
		sp_io::TestExternalities::default().execute_with(|| {
			EngineState::<Runtime>::put(State::Babe);
			// Suppress premature genesis init so Babe only updates CurrentSlot.
			pallet_babe::GenesisSlot::<Runtime>::put(Slot::from(1u64));
			pallet_babe::CurrentSlot::<Runtime>::put(Slot::from(41u64));
			initialize_block_with_logs(vec![babe_pre_digest(42)]);

			pallet_babe::Pallet::<Runtime>::on_initialize(1);

			assert_eq!(current_slot_number(), ScSlotNumber(42));
		});
	}
}

/// Tests for the slot reported by the `GetSidechainStatus` runtime API across the
/// AURA to BABE consensus flip.
mod sidechain_status {
	use crate::Runtime;
	use pallet_consensus_engine::{EngineState, State};
	use sidechain_domain::{ScEpochNumber, ScSlotNumber};
	use sp_consensus_slots::Slot;
	use sp_sidechain::SidechainStatus;

	/// Slot the AURA storage is left frozen at, standing in for the value
	/// `pallet_aura::CurrentSlot` keeps forever once BABE takes over.
	const STALE_AURA_SLOT: u64 = 41;
	const BABE_SLOT: u64 = 4242;

	fn get_sidechain_status() -> SidechainStatus {
		<Runtime as sp_sidechain::runtime_decl_for_get_sidechain_status::GetSidechainStatus<
			crate::Block,
		>>::get_sidechain_status()
	}

	fn slots_per_epoch() -> u64 {
		u64::from(crate::Sidechain::slots_per_epoch().0)
	}

	#[test]
	fn slot_is_read_from_aura_storage_pre_flip() {
		sp_io::TestExternalities::default().execute_with(|| {
			// Default engine state is Aura.
			pallet_aura::CurrentSlot::<Runtime>::put(Slot::from(STALE_AURA_SLOT));
			pallet_babe::CurrentSlot::<Runtime>::put(Slot::from(BABE_SLOT));

			let status = get_sidechain_status();

			assert_eq!(status.slot, ScSlotNumber(STALE_AURA_SLOT));
			assert_eq!(status.epoch, ScEpochNumber(STALE_AURA_SLOT / slots_per_epoch()));
		});
	}

	// Regression test: this API used to read `pallet_aura::CurrentSlot`
	// unconditionally, which stops advancing once BABE produces blocks, so the
	// reported slot (and the epoch derived from it) froze at the flip.
	#[test]
	fn slot_is_read_from_babe_storage_post_flip() {
		sp_io::TestExternalities::default().execute_with(|| {
			EngineState::<Runtime>::put(State::Babe);
			pallet_aura::CurrentSlot::<Runtime>::put(Slot::from(STALE_AURA_SLOT));
			pallet_babe::CurrentSlot::<Runtime>::put(Slot::from(BABE_SLOT));

			let status = get_sidechain_status();

			assert_eq!(status.slot, ScSlotNumber(BABE_SLOT));
			assert_eq!(status.epoch, ScEpochNumber(BABE_SLOT / slots_per_epoch()));
		});
	}

	// The scheduled state still produces AURA blocks, so the slot must keep
	// coming from AURA until the flip actually completes.
	#[test]
	fn slot_is_read_from_aura_storage_while_the_flip_is_pending() {
		sp_io::TestExternalities::default().execute_with(|| {
			EngineState::<Runtime>::put(State::ScheduledFlip);
			pallet_aura::CurrentSlot::<Runtime>::put(Slot::from(STALE_AURA_SLOT));
			pallet_babe::CurrentSlot::<Runtime>::put(Slot::from(BABE_SLOT));

			assert_eq!(get_sidechain_status().slot, ScSlotNumber(STALE_AURA_SLOT));
		});
	}
}

/// Nodes distrust the AURA/BABE pre-runtime digest layout of blocks executed by runtimes from
/// before `pallet-consensus-engine`, identified by `spec_version` via the `pallet-version`
/// digest, so this runtime must not report a version below the activation one.
mod consensus_engine_activation {
	use crate::VERSION;
	use midnight_primitives_consensus_engine::ACTIVATION_SPEC_VERSION;

	#[test]
	fn this_runtime_is_at_or_past_the_activation_version() {
		const {
			assert!(
				VERSION.spec_version >= ACTIVATION_SPEC_VERSION,
				"pallet-consensus-engine is in this runtime, so its spec_version must not be \
				 below ACTIVATION_SPEC_VERSION",
			)
		};
	}
}

/// The slot duration reported by `SlotApi` comes from AURA's config, so it must not
/// diverge from BABE's once BABE produces the blocks.
mod slot_config {
	use crate::{Block, Runtime};

	fn slot_config() -> sidechain_slots::ScSlotConfig {
		<Runtime as sidechain_slots::runtime_decl_for_slot_api::SlotApi<Block>>::slot_config()
	}

	#[test]
	fn slot_duration_is_the_same_for_both_engines() {
		sp_io::TestExternalities::default().execute_with(|| {
			// Both are config constants — AURA's `SlotDuration` and BABE's
			// `MinimumPeriod * 2` — so `slot_config` needs no engine dispatch as
			// long as they agree. This test fails if a future config change makes
			// them diverge.
			assert_eq!(
				crate::Aura::slot_duration(),
				crate::Babe::slot_duration(),
				"AURA and BABE slot durations diverged; SlotApi::slot_config must \
				 dispatch on the active engine"
			);
			assert_eq!(slot_config().slot_duration.as_millis(), crate::SLOT_DURATION);
		});
	}
}

mod failed_mbm_recovery {
	use crate::{AccountId, BlockNumber, Executive, Runtime, RuntimeCall, VERSION};
	use frame_support::{
		assert_ok, dispatch::GetDispatchInfo, migrations::MultiStepMigrator, traits::Contains,
	};
	use parity_scale_codec::Encode;
	use sp_runtime::{
		BuildStorage, ExtrinsicInclusionMode,
		traits::{Dispatchable, Hash as _, Header as _},
	};

	fn ongoing() -> bool {
		<Runtime as frame_system::Config>::MultiBlockMigrator::ongoing()
	}

	fn can_set_code() -> frame_system::CanSetCodeResult<Runtime> {
		frame_system::Pallet::<Runtime>::can_set_code(&[], false)
	}

	/// Fail migration 0 (cnight v1 `MigrateV0ToV1`) by planting an active cursor whose
	/// 1-byte inner cursor fails to SCALE-decode as the migration's fixed-size cursor
	/// type -> `InvalidCursor` -> `FailedMigrationHandler`, then step the MBMs the way
	/// Executive does after inherent application. Leaves the chain in safe mode with
	/// the cursor unstuck.
	fn fail_mbm_and_enter_safe_mode() {
		// Mark the runtime as already upgraded so a later `initialize_block` doesn't
		// onboard the MBMs again.
		frame_system::LastRuntimeUpgrade::<Runtime>::put(
			frame_system::LastRuntimeUpgradeInfo::from(VERSION),
		);
		frame_system::Pallet::<Runtime>::set_block_number(1);
		assert!(can_set_code().into_result().is_ok());

		assert_ok!(crate::MultiBlockMigrations::force_set_active_cursor(
			crate::RuntimeOrigin::root(),
			0,
			Some(vec![0u8].try_into().unwrap()),
			Some(1),
		));
		assert!(ongoing());
		assert!(matches!(
			can_set_code(),
			frame_system::CanSetCodeResult::MultiBlockMigrationsOngoing
		));

		Executive::inherents_applied();

		// Safe mode entered indefinitely, cursor unstuck, upgrades unblocked.
		assert_eq!(pallet_safe_mode::EnteredUntil::<Runtime>::get(), Some(BlockNumber::MAX));
		assert!(!ongoing());
		assert!(can_set_code().into_result().is_ok());
	}

	/// A failed multi-block migration must enter safe mode and unstuck the cursor
	/// instead of freezing the chain (`can_set_code` blocked forever with no on-chain
	/// recovery on a standalone chain).
	#[test]
	fn failed_mbm_enters_safe_mode_and_unstucks() {
		let t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
		sp_io::TestExternalities::from(t).execute_with(|| {
			fail_mbm_and_enter_safe_mode();

			// `BaseCallFilter` now blocks user calls...
			type Filter = <Runtime as frame_system::Config>::BaseCallFilter;
			assert!(!Filter::contains(&RuntimeCall::Midnight(
				pallet_midnight::Call::send_mn_transaction { midnight_tx: vec![] }
			)));
			// ...but keeps the inherents, governance, and safe-mode recovery calls.
			assert!(Filter::contains(&RuntimeCall::Timestamp(pallet_timestamp::Call::set {
				now: 0
			})));
			assert!(Filter::contains(&RuntimeCall::Council(pallet_collective::Call::vote {
				proposal: crate::Hash::default(),
				index: 0,
				approve: true,
			})));
			assert!(Filter::contains(&RuntimeCall::SafeMode(
				pallet_safe_mode::Call::force_exit {}
			)));

			// The next block admits normal (non-inherent) extrinsics again. Like any
			// AURA block it must carry the AURA pre-digest followed by the matching BABE
			// `SecondaryPlain` one, which `pallet-consensus-engine` checks against the
			// AURA authority set (`authority_index == slot % n`).
			let authority = sp_consensus_aura::sr25519::AuthorityId::from(
				sp_core::sr25519::Public::from_raw([1u8; 32]),
			);
			pallet_aura::Authorities::<Runtime>::put(frame_support::BoundedVec::truncate_from(
				vec![authority],
			));
			let slot = sp_consensus_slots::Slot::from(1);
			let digest = sp_runtime::Digest {
				logs: vec![
					sp_runtime::DigestItem::PreRuntime(
						sp_consensus_aura::AURA_ENGINE_ID,
						slot.encode(),
					),
					sp_runtime::DigestItem::PreRuntime(
						sp_consensus_babe::BABE_ENGINE_ID,
						sp_consensus_babe::digests::PreDigest::SecondaryPlain(
							sp_consensus_babe::digests::SecondaryPlainPreDigest {
								authority_index: 0,
								slot,
							},
						)
						.encode(),
					),
				],
			};
			let header = crate::Header::new(
				2,
				Default::default(),
				Default::default(),
				frame_system::Pallet::<Runtime>::parent_hash(),
				digest,
			);
			assert_eq!(Executive::initialize_block(&header), ExtrinsicInclusionMode::AllExtrinsics);
		});
	}

	fn account(b: u8) -> AccountId {
		AccountId::from([b; 32])
	}

	/// Dispatch `call` the way an applied extrinsic would: through the signed origin,
	/// which carries `BaseCallFilter`. This is what makes the test end-to-end — a call
	/// filtered in safe mode fails here with `CallFiltered`.
	fn dispatch_signed(call: RuntimeCall, who: AccountId) {
		assert_ok!(call.dispatch(crate::RuntimeOrigin::signed(who)));
	}

	/// Governance must be able to execute the full set-code recovery flow while the
	/// chain sits in post-failure safe mode: both collectives approve a
	/// `FederatedAuthority` motion, and closing the motion dispatches
	/// `System::authorize_upgrade` as Root (the fixed runtime is then applied via the
	/// whitelisted `System::apply_authorized_upgrade`). The collectives dispatch
	/// `motion_approve` internally with their (non-Root) `Members` origin, which goes
	/// through `BaseCallFilter` — the safe-mode whitelist must let it through or
	/// recovery dead-ends.
	#[test]
	fn governance_can_authorize_set_code_in_safe_mode() {
		let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
		// Seed both governance bodies (membership genesis forwards to the collectives).
		pallet_membership::GenesisConfig::<Runtime, pallet_membership::Instance1> {
			members: vec![account(1), account(2)].try_into().unwrap(),
			..Default::default()
		}
		.assimilate_storage(&mut t)
		.unwrap();
		pallet_membership::GenesisConfig::<Runtime, pallet_membership::Instance2> {
			members: vec![account(1), account(2)].try_into().unwrap(),
			..Default::default()
		}
		.assimilate_storage(&mut t)
		.unwrap();

		sp_io::TestExternalities::from(t).execute_with(|| {
			fail_mbm_and_enter_safe_mode();

			// The motion governance wants dispatched as Root: authorize the fixed
			// runtime's code hash.
			let motion = RuntimeCall::System(frame_system::Call::authorize_upgrade {
				code_hash: crate::Hash::repeat_byte(7),
			});
			let motion_hash = <Runtime as frame_system::Config>::Hashing::hash_of(&motion);
			assert!(frame_system::Pallet::<Runtime>::authorized_upgrade().is_none());

			// Council: propose motion_approve, both members vote aye, close dispatches
			// it with the Council's `Members` origin.
			let proposal =
				RuntimeCall::FederatedAuthority(pallet_federated_authority::Call::motion_approve {
					call: alloc::boxed::Box::new(motion.clone()),
				});
			let len = proposal.encoded_size() as u32;
			let weight = proposal.get_dispatch_info().call_weight;
			let hash = <Runtime as frame_system::Config>::Hashing::hash_of(&proposal);

			dispatch_signed(
				RuntimeCall::Council(pallet_collective::Call::propose {
					threshold: 2,
					proposal: alloc::boxed::Box::new(proposal.clone()),
					length_bound: len,
				}),
				account(1),
			);
			for who in [account(1), account(2)] {
				dispatch_signed(
					RuntimeCall::Council(pallet_collective::Call::vote {
						proposal: hash,
						index: 0,
						approve: true,
					}),
					who,
				);
			}
			dispatch_signed(
				RuntimeCall::Council(pallet_collective::Call::close {
					proposal_hash: hash,
					index: 0,
					proposal_weight_bound: weight,
					length_bound: len,
				}),
				account(1),
			);
			// The inner motion_approve must have landed (do_approve_proposal swallows a
			// filtered dispatch into an event, so check the motion actually exists).
			assert!(
				pallet_federated_authority::Motions::<Runtime>::get(motion_hash).is_some(),
				"Council's motion_approve was not executed — blocked by BaseCallFilter?"
			);

			// Technical Committee: same flow.
			dispatch_signed(
				RuntimeCall::TechnicalCommittee(pallet_collective::Call::propose {
					threshold: 2,
					proposal: alloc::boxed::Box::new(proposal.clone()),
					length_bound: len,
				}),
				account(1),
			);
			for who in [account(1), account(2)] {
				dispatch_signed(
					RuntimeCall::TechnicalCommittee(pallet_collective::Call::vote {
						proposal: hash,
						index: 0,
						approve: true,
					}),
					who,
				);
			}
			dispatch_signed(
				RuntimeCall::TechnicalCommittee(pallet_collective::Call::close {
					proposal_hash: hash,
					index: 0,
					proposal_weight_bound: weight,
					length_bound: len,
				}),
				account(1),
			);

			// Both bodies approved: closing the motion dispatches it as Root.
			dispatch_signed(
				RuntimeCall::FederatedAuthority(pallet_federated_authority::Call::motion_close {
					motion_hash,
					proposal_weight_bound: motion.get_dispatch_info().call_weight,
				}),
				account(1),
			);
			assert!(
				frame_system::Pallet::<Runtime>::authorized_upgrade().is_some(),
				"authorize_upgrade did not execute — motion dispatch failed"
			);

			// And governance can lift safe mode once the fixed runtime is applied.
			assert_ok!(crate::SafeMode::force_exit(crate::RuntimeOrigin::root()));
			assert_eq!(pallet_safe_mode::EnteredUntil::<Runtime>::get(), None);
		});
	}
}
