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

//! What an observer of the chain sees across the flip must agree, block by block: the engine
//! the header's pre-runtime digests name, the signer those digests resolve to in every
//! authority view readable at the parent state (`AuraApi::authorities`, `BabeApi::current_epoch`
//! and `next_epoch`, the committee pallet's current and queued committees, `pallet_session`'s
//! validators), the author the runtime credits, and the epoch BABE announces for the session to
//! come. Each test runs one committee plan through the flip; its own assertions pin the
//! committee that signs the last AURA epoch and the one that BABE epoch 0 names.
//!
//! Post-flip, a block that opens an epoch is signed under the epoch BABE announced one epoch
//! earlier, i.e. `BabeApi::next_epoch` at its parent; every other block under
//! `BabeApi::current_epoch` at its parent. At the flip block both name the first BABE epoch.

use super::*;
use sp_consensus_babe::digests::PreDigest as BabePreDigest;

#[test]
fn observers_see_coherent_signers_and_epochs_across_the_flip() {
	// The flip commits at the end of epoch 1, so `[bob]` serves the first BABE epoch
	// while the genesis `[alice, bob]` still signs epoch 1.
	let flip = run(
		&[
			select(&[bob()]),
			select(&[alice()]),
			select(&[alice(), bob()]),
			select(&[bob(), alice()]),
			select(&[alice()]),
		],
		1,
	);
	assert_eq!(flip.outgoing, babe_keys(&[alice(), bob()]));
	assert_eq!(flip.epoch_zero, babe_keys(&[bob()]));
}

/// Ariadne reorders an unchanged membership every epoch: the case seen on a live flip.
#[test]
fn epoch_zero_follows_a_reordered_committee() {
	let flip = run(
		&[
			select(&[alice(), bob(), charlie()]),
			select(&[charlie(), alice(), bob()]),
			select(&[bob(), charlie(), alice()]),
			select(&[alice(), bob(), charlie()]),
		],
		2,
	);
	assert_eq!(flip.outgoing, babe_keys(&[alice(), bob(), charlie()]));
	assert_eq!(flip.epoch_zero, babe_keys(&[charlie(), alice(), bob()]));
}

/// More signers in epoch 0 than in the epoch before: indices past the outgoing set.
#[test]
fn epoch_zero_follows_a_growing_committee() {
	let flip = run(
		&[
			select(&[alice()]),
			select(&[alice(), bob(), charlie()]),
			select(&[charlie(), bob()]),
			select(&[alice()]),
		],
		2,
	);
	assert_eq!(flip.outgoing, babe_keys(&[alice()]));
	assert_eq!(flip.epoch_zero, babe_keys(&[alice(), bob(), charlie()]));
}

/// Candidates removed and a new one added on Cardano: no member in common.
#[test]
fn epoch_zero_follows_a_replaced_committee() {
	let flip = run(
		&[
			select(&[alice(), bob()]),
			select(&[charlie()]),
			select(&[bob(), alice()]),
			select(&[charlie(), alice()]),
		],
		2,
	);
	assert_eq!(flip.outgoing, babe_keys(&[alice(), bob()]));
	assert_eq!(flip.epoch_zero, babe_keys(&[charlie()]));
}

/// A D-parameter change: more seats than candidates, so the draw with replacement gives
/// members several seats and `babe.authorities` repeats their keys.
#[test]
fn epoch_zero_follows_a_change_in_seats() {
	let flip = run(
		&[
			select(&[alice(), bob(), charlie()]),
			select(&[bob(), bob(), alice(), charlie(), bob()]),
			select(&[charlie(), alice()]),
			select(&[alice(), alice(), bob()]),
		],
		2,
	);
	assert_eq!(flip.outgoing, babe_keys(&[alice(), bob(), charlie()]));
	assert_eq!(flip.epoch_zero, babe_keys(&[bob(), bob(), alice(), charlie(), bob()]));
}

/// No valid candidates for the epoch after the flip: the queued committee carries over.
#[test]
fn epoch_zero_keeps_the_committee_when_selection_fails() {
	let flip = run(
		&[
			select(&[alice(), bob(), charlie()]),
			select(&[]),
			select(&[bob(), alice()]),
			select(&[charlie()]),
		],
		2,
	);
	assert_eq!(flip.outgoing, babe_keys(&[alice(), bob(), charlie()]));
	assert_eq!(flip.epoch_zero, babe_keys(&[alice(), bob(), charlie()]));
}

/// A member of the epoch-0 committee claims a BABE key another account holds:
/// `pallet_session` rejects it, the hand-off is aborted and the outgoing set stays on.
#[test]
fn epoch_zero_keeps_the_committee_when_the_hand_off_is_aborted() {
	let bobs_babe_key = TestKeys { babe: bob().babe, ..charlie() };
	let flip = run(
		&[
			select(&[alice(), bob()]),
			select(&[bobs_babe_key, alice()]),
			select(&[bob(), alice()]),
			select(&[charlie()]),
		],
		2,
	);
	assert_eq!(flip.outgoing, babe_keys(&[alice(), bob()]));
	assert_eq!(flip.epoch_zero, babe_keys(&[alice(), bob()]));
}

/// The last slot of the scheduled epoch stays empty: the flip moves to the next epoch's
/// last slot and epoch 0 names the committee that serves the epoch after that.
#[test]
fn epoch_zero_follows_the_committee_after_a_postponed_flip() {
	let flip = run(
		&[
			select(&[alice(), bob()]),
			select(&[charlie()]),
			select(&[bob(), charlie(), alice()]).skip_last_slot(),
			select(&[alice()]),
			select(&[charlie(), bob()]),
		],
		2,
	);
	assert_eq!(flip.at_epoch, 3);
	assert_eq!(flip.outgoing, babe_keys(&[charlie()]));
	assert_eq!(flip.epoch_zero, babe_keys(&[bob(), charlie(), alice()]));
}

/// The chain halts with the flip scheduled, past one whole epoch: the first block back
/// catches up in one rotation, and the flip names the committee queued after it.
#[test]
fn epoch_zero_follows_the_committee_after_a_halt_with_the_flip_scheduled() {
	let flip = run(
		&[
			select(&[alice(), bob()]),
			select(&[charlie()]),
			select(&[bob(), charlie(), alice()]).halt(2 * SLOTS_PER_EPOCH as u64),
			select(&[alice()]),
			select(&[charlie(), bob()]),
			select(&[bob()]),
		],
		2,
	);
	assert_eq!(flip.at_epoch, 4);
	// The committee due in the skipped epoch 3 serves epoch 4; the one selected in
	// epoch 2 is queued for epoch 5.
	assert_eq!(flip.outgoing, babe_keys(&[charlie()]));
	assert_eq!(flip.epoch_zero, babe_keys(&[bob(), charlie(), alice()]));
}

/// No block is produced in the epoch after the flip is scheduled until its last slot, so the
/// rotation due for that epoch lands in the block that would commit the flip. The flip waits for
/// the next epoch's last slot, and epoch 0 names the committee that serves the epoch after that.
#[test]
fn flip_is_deferred_when_a_rotation_is_due_in_the_flip_block() {
	let flip = run(
		&[
			select(&[alice(), bob()]),
			select(&[charlie()]),
			select(&[bob(), charlie(), alice()]).halt(2 * SLOTS_PER_EPOCH as u64 - 2),
			select(&[alice()]),
			select(&[charlie(), bob()]),
			select(&[bob()]),
		],
		2,
	);
	assert_eq!(flip.at_epoch, 4);
	assert_eq!(flip.outgoing, babe_keys(&[bob(), charlie(), alice()]));
	assert_eq!(flip.epoch_zero, babe_keys(&[alice()]));
}

/// One sidechain epoch of a plan.
struct Epoch {
	/// The committee the inherent selects in this epoch's first block; it serves two
	/// epochs later. Empty: no valid candidates, so selection falls back to the queue.
	select: Vec<TestKeys>,
	/// Leave the epoch's last slot without a block.
	skip_last_slot: bool,
	/// Slots left without a block after the epoch's first block.
	halt: u64,
}

fn select(committee: &[TestKeys]) -> Epoch {
	Epoch { select: committee.to_vec(), skip_last_slot: false, halt: 0 }
}

impl Epoch {
	fn skip_last_slot(self) -> Self {
		Epoch { skip_last_slot: true, ..self }
	}

	fn halt(self, slots: u64) -> Self {
		Epoch { halt: slots, ..self }
	}
}

/// What the flip block left behind.
struct Flip {
	/// The epoch, from 0 at the first block, that the flip committed at the end of.
	at_epoch: usize,
	/// The BABE keys of the committee that signed the flip block's epoch.
	outgoing: BabeAuthorities,
	/// The authorities of BABE epoch 0.
	epoch_zero: BabeAuthorities,
}

fn babe_keys(committee: &[TestKeys]) -> BabeAuthorities {
	committee.iter().map(|keys| (keys.babe.public(), 1)).collect()
}

/// Run `plan` from activation, scheduling the flip in the first block of `flip_epoch`, and
/// observe every block until a few BABE epochs after the flip.
fn run(plan: &[Epoch], flip_epoch: usize) -> Flip {
	new_test_ext().execute_with(|| {
		// As the upgrade migration leaves an existing network.
		ConsensusEngine::activate();
		advance_block();

		let epoch_of = |slot: u64| (slot / SLOTS_PER_EPOCH as u64 - ARBITRARY_FIRST_EPOCH) as usize;
		let mut planned = None;
		let mut empty_slots = 0;
		let mut flip = None;
		let mut seen = std::collections::BTreeMap::<Kind, u32>::new();
		let mut last = None;
		// The activation block is observed too: the parent view is the genesis state.
		let mut parent = View::capture();
		loop {
			let next_slot = *ConsensusEngine::current_slot() + 1 + empty_slots;
			let last_slot = (next_slot + 1).is_multiple_of(SLOTS_PER_EPOCH as u64);
			if last_slot && plan.get(epoch_of(next_slot)).is_some_and(|e| e.skip_last_slot) {
				empty_slots += 1;
			}
			advance_block_skipping(empty_slots);
			empty_slots = 0;
			let kind = match (last, observe(&parent)) {
				(Some(Kind::Flip), Kind::BabeOpening) => Kind::FirstBabe,
				(_, kind) => kind,
			};
			*seen.entry(kind).or_default() += 1;
			last = Some(kind);
			parent = View::capture();

			let epoch = epoch_of(*ConsensusEngine::current_slot());
			if kind == Kind::Flip {
				flip = Some(Flip {
					at_epoch: epoch,
					outgoing: committee_babe_authorities(current_committee()),
					epoch_zero: Babe::current_epoch().authorities,
				});
			}
			// The node signs the first BABE epoch under `BabeApi::current_epoch` at the flip
			// block; the set the first BABE block installs must be that one.
			if kind == Kind::FirstBabe {
				let epoch_zero = &flip.as_ref().expect("the flip committed").epoch_zero;
				assert_eq!(&pallet_babe::Authorities::<Test>::get().to_vec(), epoch_zero);
			}
			if epoch > plan.len() && flip.as_ref().is_some_and(|flip| epoch > flip.at_epoch + 3) {
				break;
			}
			if planned != Some(epoch) {
				// Past the plan, selection keeps the queued committee, so rotations go on.
				match plan.get(epoch) {
					Some(step) => {
						set_committee_through_inherent_data(&step.select);
						empty_slots = step.halt;
					},
					None => {
						set_committee_through_inherent_data(&[]);
					},
				}
				if epoch == flip_epoch {
					assert_ok!(ConsensusEngine::schedule_flip(RawOrigin::Root.into()));
				}
				planned = Some(epoch);
			}
		}

		// Every kind of block the migration produces was checked, the singular ones once.
		let count = |kind| seen.get(&kind).copied().unwrap_or_default();
		assert_eq!(count(Kind::Flip), 1, "{seen:?}");
		assert_eq!(count(Kind::FirstBabe), 1, "{seen:?}");
		assert!(count(Kind::PreFlip) > 1, "{seen:?}");
		assert!(count(Kind::PreFlipOpening) >= 1, "{seen:?}");
		assert!(count(Kind::BabeOpening) >= 3, "{seen:?}");
		assert!(count(Kind::Babe) > 1, "{seen:?}");
		assert_eq!(parent.engine, ActiveEngine::Babe);

		// `pallet_session::historical` records no session roots: the session manager is not
		// wrapped in `NoteHistoricalRoot`, so no observer can obtain a key-ownership proof, and
		// the consensus APIs answer `None` for them.
		assert!(pallet_session::historical::HistoricalSessions::<Test>::iter().next().is_none());
		flip.expect("the flip committed")
	})
}

/// The kinds of block the migration produces, each with its own observer rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
	PreFlip,
	PreFlipOpening,
	Flip,
	FirstBabe,
	BabeOpening,
	Babe,
}

type Member = <Test as pallet_session_validator_management::Config>::CommitteeMember;
type BabeAuthorities = Vec<(sp_consensus_babe::AuthorityId, BabeAuthorityWeight)>;

/// The views an observer can read at a block's state, used for the block's child.
struct View {
	engine: ActiveEngine,
	session: u32,
	aura: Vec<AuraId>,
	babe: BabeAuthorities,
	current_epoch: Option<sp_consensus_babe::Epoch>,
	next_epoch: BabeAuthorities,
	validators: Vec<AccountId32>,
	queued_validators: Vec<AccountId32>,
	committee: Vec<Member>,
	queued_committee: Vec<Member>,
}

impl View {
	fn capture() -> Self {
		// `BabeApi::current_epoch` needs the epoch config the flip writes.
		let current_epoch = pallet_babe::EpochConfig::<Test>::get().map(|_| Babe::current_epoch());
		View {
			engine: ConsensusEngine::active_engine(),
			session: Session::current_index(),
			aura: pallet_aura::Authorities::<Test>::get().to_vec(),
			babe: pallet_babe::Authorities::<Test>::get().to_vec(),
			current_epoch,
			next_epoch: pallet_babe::NextAuthorities::<Test>::get().to_vec(),
			validators: Session::validators(),
			queued_validators: pallet_session::QueuedKeys::<Test>::get()
				.into_iter()
				.map(|(validator, _)| validator)
				.collect(),
			committee: SessionCommitteeManagement::current_committee_storage().committee.to_vec(),
			queued_committee: SessionCommitteeManagement::queued_committee_storage()
				.committee
				.to_vec(),
		}
	}
}

/// Check the block just initialized against the views at its parent.
fn observe(parent: &View) -> Kind {
	let block = System::block_number();
	let logs = System::digest().logs;
	let engines = logs
		.iter()
		.filter_map(|log| log.as_pre_runtime())
		.map(|(engine, _)| engine)
		.collect::<Vec<_>>();
	let expected = match parent.engine {
		ActiveEngine::Aura => vec![AURA_ENGINE_ID, BABE_ENGINE_ID],
		ActiveEngine::Babe => vec![BABE_ENGINE_ID],
	};
	assert_eq!(engines, expected, "block {block}: pre-runtime digests");

	// The signer's index as the header claims it: the BABE pre-digest names it under
	// both engines.
	let index = logs
		.iter()
		.filter_map(|log| log.as_pre_runtime())
		.find(|(engine, _)| *engine == BABE_ENGINE_ID)
		.and_then(|(_, mut data)| BabePreDigest::decode(&mut data).ok())
		.expect("a BABE pre-digest")
		.authority_index() as usize;
	let credited = <ConsensusEngine as FindAuthor<u32>>::find_author(
		logs.iter().filter_map(|log| log.as_pre_runtime()),
	)
	.expect("the digests name the author") as usize;
	// `pallet_aura` credits `slot % n` over the set the block installs, so an AURA block
	// that rotates to a committee of another size credits another index than the signer's.
	// Nothing consumes the AURA author; pinned so an upstream change is noticed.
	let installed = pallet_aura::Authorities::<Test>::get().len();
	if parent.engine == ActiveEngine::Aura && installed != parent.aura.len() {
		let slot = *ConsensusEngine::current_slot() as usize;
		assert_eq!(credited, slot % installed, "block {block}: pallet_aura's credited author");
	} else {
		assert_eq!(credited, index, "block {block}: credited author");
	}
	let opens_epoch = babe_next_epoch_data().is_some();
	let rotated = Session::current_index() != parent.session;
	assert_eq!(
		opens_epoch, rotated,
		"block {block}: BABE announces an epoch iff the session rotates"
	);

	// The signer: who the author index names under the set the block is signed in.
	let (signer_babe, member, session_view) = match (parent.engine, opens_epoch) {
		// AURA: `slot % n` in the AURA list; the BABE digest names the same member.
		(ActiveEngine::Aura, _) => {
			assert_eq!(parent.aura[index], parent.committee[index].authority_keys().aura);
			(parent.babe[index].0.clone(), &parent.committee[index], &parent.validators)
		},
		(ActiveEngine::Babe, false) => {
			let epoch = parent.current_epoch.as_ref().expect("post-flip epoch");
			(epoch.authorities[index].0.clone(), &parent.committee[index], &parent.validators)
		},
		(ActiveEngine::Babe, true) => (
			parent.next_epoch[index].0.clone(),
			&parent.queued_committee[index],
			&parent.queued_validators,
		),
	};
	let signer: AccountId32 = member.authority_id().into();
	assert_eq!(signer_babe, member.authority_keys().babe, "block {block}: committee view");
	assert_eq!(session_view[index], signer, "block {block}: pallet_session view");

	// Post-flip the signer's set is this block's current set everywhere.
	if parent.engine == ActiveEngine::Babe {
		assert_eq!(Session::validators()[index], signer, "block {block}");
		assert_eq!(pallet_babe::Authorities::<Test>::get()[index].0, signer_babe, "block {block}");
		let current: AccountId32 = current_committee()[index].authority_id().into();
		assert_eq!(current, signer, "block {block}");
	}

	// What BABE announces is the set the next rotation installs.
	if let Some(announced) = babe_next_epoch_data() {
		assert_eq!(
			announced,
			pallet_babe::NextAuthorities::<Test>::get().to_vec(),
			"block {block}"
		);
		assert_eq!(announced, session_queued_babe_authorities(), "block {block}");
		assert_eq!(announced, committee_babe_authorities(queued_committee()), "block {block}");
		if pallet_babe::EpochConfig::<Test>::get().is_some() {
			assert_eq!(announced, Babe::next_epoch().authorities, "block {block}");
		}
	}

	// The flip block: AURA-authored (its header says so) while its state already reports
	// BABE, so observers must take the engine from the header. Its state seeds epoch 0 with
	// the committee queued for the epoch it covers.
	let flipped = parent.engine == ActiveEngine::Aura
		&& ConsensusEngine::active_engine() == ActiveEngine::Babe;
	if flipped {
		assert!(!opens_epoch, "block {block}: the flip block ends an epoch");
		let epoch_zero = Babe::current_epoch();
		assert_eq!(epoch_zero.authorities, pallet_babe::NextAuthorities::<Test>::get().to_vec());
		assert_eq!(epoch_zero.authorities, session_queued_babe_authorities());
		assert_eq!(epoch_zero.authorities, committee_babe_authorities(queued_committee()));
		assert_eq!(epoch_zero.authorities, Babe::next_epoch().authorities);
	}
	match (parent.engine, opens_epoch, flipped) {
		(_, _, true) => Kind::Flip,
		(ActiveEngine::Aura, true, _) => Kind::PreFlipOpening,
		(ActiveEngine::Aura, false, _) => Kind::PreFlip,
		(ActiveEngine::Babe, true, _) => Kind::BabeOpening,
		(ActiveEngine::Babe, false, _) => Kind::Babe,
	}
}

/// The BABE keys `pallet_session` activates at its next rotation.
fn session_queued_babe_authorities() -> BabeAuthorities {
	pallet_session::QueuedKeys::<Test>::get()
		.into_iter()
		.map(|(_, keys)| (keys.babe, 1))
		.collect()
}

fn queued_committee() -> Vec<Member> {
	SessionCommitteeManagement::queued_committee_storage().committee.to_vec()
}

fn current_committee() -> Vec<Member> {
	SessionCommitteeManagement::current_committee_storage().committee.to_vec()
}

fn committee_babe_authorities(committee: Vec<Member>) -> BabeAuthorities {
	committee.into_iter().map(|member| (member.authority_keys().babe, 1)).collect()
}

/// The authorities of the `NextEpochData` BABE consensus log in the current block, if any.
fn babe_next_epoch_data() -> Option<BabeAuthorities> {
	System::digest().logs.iter().find_map(|log| {
		let (engine, mut data) = log.as_consensus()?;
		if engine != BABE_ENGINE_ID {
			return None;
		}
		match ConsensusLog::decode(&mut data).ok()? {
			ConsensusLog::NextEpochData(descriptor) => Some(descriptor.authorities),
			_ => None,
		}
	})
}
