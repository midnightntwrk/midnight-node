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

//! The one place in the node that reads AURA/BABE pre-runtime digests out of a block header to
//! answer "which engine authored this block" and "in which slot".
//!
//! Everything that needs either answer — the import-queue dispatch, the partner-chains slot
//! extractor, the parent-slot lookup for inherent data, the BABE epoch-tree seeder, the flip
//! watcher — goes through [`authoring_engine`] / [`slot_of`] so that the decision is made exactly
//! once, by the same rule, from the header alone (no state, no body: it must hold for warp and gap
//! sync too).
//!
//! # The rule
//!
//! `pallet-consensus-engine` asserts in `on_initialize`, for every block it executes, that until
//! the flip exactly one AURA and exactly one BABE pre-runtime digest are present with the AURA one
//! **first**, and that after the flip no AURA pre-runtime digest is present. So in any block
//! executed by a runtime that contains the pallet, the first AURA/BABE pre-runtime digest names the
//! authoring engine ([`engine_from_pre_runtime_digest`]).
//!
//! Nothing enforced that layout before the pallet existed. An author could have placed a BABE
//! pre-runtime digest ahead of the AURA one in a block the network accepted as a plain AURA block,
//! and reading its layout would misroute it to the BABE verifier and make it impossible to sync
//! past. Whether a block's runtime had the pallet is also in the header: `pallet-version` deposits
//! a `Consensus` digest carrying the `spec_version` of the runtime that **executed** the block,
//! which an author cannot forge or drop (the runtime rejects a block whose digest differs from the
//! one it computed). [`babe_pre_digest_is_authoritative`] is `false` exactly when that version is
//! below [`ACTIVATION_SPEC_VERSION`], the first runtime with the pallet; such a block is AURA's
//! whatever its pre-runtime digests say. `pallet-version` has been in every runtime since genesis,
//! so every historical block carries the digest, and a block without one can only be executed by a
//! future runtime that dropped `pallet-version`, long after activation; its layout is trusted, so
//! nothing here depends on `pallet-version` staying forever.

use midnight_node_runtime::VERSION_ID;
use midnight_node_runtime::opaque::Block as RuntimeBlock;
use midnight_primitives_consensus_engine::{ACTIVATION_SPEC_VERSION, ActiveEngine};
use sp_consensus_aura::AURA_ENGINE_ID;
use sp_consensus_aura::sr25519::AuthorityPair as AuraPair;
use sp_consensus_babe::BABE_ENGINE_ID;
use sp_consensus_slots::Slot;
use sp_core::Pair;
use sp_runtime::generic::OpaqueDigestItemId;
use sp_runtime::traits::{Block as BlockT, Header as _};

/// The `spec_version` of the runtime that executed the block with this `header`, from the
/// `pallet-version` consensus digest; `None` for a block whose runtime deposited none.
pub fn executing_spec_version<Block: BlockT>(header: &Block::Header) -> Option<u32> {
	header
		.digest()
		.logs()
		.iter()
		.find_map(|log| log.try_to::<u32>(OpaqueDigestItemId::Consensus(&VERSION_ID)))
}

/// Whether a BABE pre-runtime digest in `header` may be interpreted at all.
///
/// `false` only when the runtime that executed the block predates `pallet-consensus-engine` (its
/// `pallet-version` digest is below [`ACTIVATION_SPEC_VERSION`]): nothing enforced the digest
/// layout then, so the digest is meaningless and the block is a plain AURA block. From the
/// activation version on the pallet enforces the layout; a block without a version digest is
/// trusted too (see the module docs).
pub fn babe_pre_digest_is_authoritative<Block: BlockT>(header: &Block::Header) -> bool {
	!executing_spec_version::<Block>(header).is_some_and(|v| v < ACTIVATION_SPEC_VERSION)
}

/// The engine named by the first AURA or BABE pre-runtime digest in `header`, or `None` if it
/// carries neither. Reads the layout only, without asking whether it may be trusted; callers want
/// [`authoring_engine`].
fn engine_from_pre_runtime_digest<Block: BlockT>(header: &Block::Header) -> Option<ActiveEngine> {
	header.digest().logs().iter().find_map(|log| match log.as_pre_runtime() {
		Some((id, _)) if id == AURA_ENGINE_ID => Some(ActiveEngine::Aura),
		Some((id, _)) if id == BABE_ENGINE_ID => Some(ActiveEngine::Babe),
		_ => None,
	})
}

/// The consensus engine that authored the block with this `header`.
///
/// A block executed by a runtime from before `pallet-consensus-engine` is AURA's whatever its
/// digests say. Otherwise the first AURA/BABE pre-runtime digest decides, and `None` means the
/// header carries neither.
pub fn authoring_engine<Block: BlockT>(header: &Block::Header) -> Option<ActiveEngine> {
	if !babe_pre_digest_is_authoritative::<Block>(header) {
		return Some(ActiveEngine::Aura);
	}
	engine_from_pre_runtime_digest::<Block>(header)
}

/// Whether `header` carries a BABE pre-runtime digest that may be interpreted: a block of the
/// pallet's era that was authored by BABE, or by AURA with the BABE marker alongside (every
/// pre-flip block of that era, including the flip block).
pub fn has_authoritative_babe_pre_digest<Block: BlockT>(header: &Block::Header) -> bool {
	babe_pre_digest_is_authoritative::<Block>(header)
		&& header
			.digest()
			.logs()
			.iter()
			.any(|log| matches!(log.as_pre_runtime(), Some((id, _)) if id == BABE_ENGINE_ID))
}

/// The slot the block with this `header` was authored in, read from the pre-runtime digest of its
/// [`authoring_engine`]. Genesis has no slot and is an error here; callers that accept genesis
/// check the number first.
pub fn slot_of<Block: BlockT>(header: &Block::Header) -> Result<Slot, String> {
	match authoring_engine::<Block>(header) {
		Some(ActiveEngine::Aura) => sc_consensus_aura::find_pre_digest::<
			Block,
			<AuraPair as Pair>::Signature,
		>(header)
		.map_err(|e| format!("AURA block #{:?} has no readable AURA slot: {e}", header.number())),
		Some(ActiveEngine::Babe) => sc_consensus_babe::find_pre_digest::<Block>(header)
			.map(|pre_digest| pre_digest.slot())
			.map_err(|e| {
				format!("BABE block #{:?} has no readable BABE slot: {e}", header.number())
			}),
		None => Err(format!(
			"block #{:?} has neither an AURA nor a BABE pre-runtime digest",
			header.number()
		)),
	}
}

/// [`sc_partner_chains_consensus::SlotExtractor`] for both import pipelines: the slot of whichever
/// engine authored the block, per [`slot_of`].
pub struct EngineSlotExtractor;

impl sc_partner_chains_consensus::SlotExtractor<RuntimeBlock> for EngineSlotExtractor {
	fn extract_slot(header: &<RuntimeBlock as BlockT>::Header) -> Result<Slot, String> {
		slot_of::<RuntimeBlock>(header)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use midnight_node_runtime::opaque::Header;
	use parity_scale_codec::Encode;
	use sp_consensus_babe::digests::{PreDigest, SecondaryPlainPreDigest};
	use sp_runtime::{ConsensusEngineId, DigestItem};

	type Block = RuntimeBlock;

	/// Some other engine's pre-runtime digest, as the partner-chains main-chain hash digest is.
	const OTHER_ENGINE_ID: ConsensusEngineId = *b"mcsh";
	/// A runtime from before `pallet-consensus-engine` existed.
	const PRE_ACTIVATION_SPEC_VERSION: u32 = ACTIVATION_SPEC_VERSION - 1;

	fn pre_runtime(id: ConsensusEngineId) -> DigestItem {
		DigestItem::PreRuntime(id, vec![0])
	}

	fn aura_slot(slot: u64) -> DigestItem {
		DigestItem::PreRuntime(AURA_ENGINE_ID, Slot::from(slot).encode())
	}

	fn babe_slot(slot: u64) -> DigestItem {
		DigestItem::PreRuntime(
			BABE_ENGINE_ID,
			PreDigest::SecondaryPlain(SecondaryPlainPreDigest {
				authority_index: 0,
				slot: Slot::from(slot),
			})
			.encode(),
		)
	}

	/// The `pallet-version` consensus digest of a block executed by runtime `spec_version`.
	fn version_digest(spec_version: u32) -> DigestItem {
		DigestItem::Consensus(VERSION_ID, spec_version.encode())
	}

	/// Header of block 1 carrying `logs` verbatim (no version digest unless `logs` has one).
	fn raw_header(logs: Vec<DigestItem>) -> Header {
		let mut header = Header::new(
			1,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		);
		for log in logs {
			header.digest_mut().push(log);
		}
		header
	}

	/// Header of a block executed by the runtime that introduced the pallet.
	fn header(logs: Vec<DigestItem>) -> Header {
		let mut all = vec![version_digest(ACTIVATION_SPEC_VERSION)];
		all.extend(logs);
		raw_header(all)
	}

	/// Header of a block executed by a runtime from before the pallet existed.
	fn pre_activation_header(logs: Vec<DigestItem>) -> Header {
		let mut all = vec![version_digest(PRE_ACTIVATION_SPEC_VERSION)];
		all.extend(logs);
		raw_header(all)
	}

	#[test]
	fn executing_spec_version_is_read_from_the_version_digest() {
		assert_eq!(
			executing_spec_version::<Block>(&pre_activation_header(vec![])),
			Some(PRE_ACTIVATION_SPEC_VERSION)
		);
		assert_eq!(executing_spec_version::<Block>(&raw_header(vec![])), None);
	}

	#[test]
	fn babe_pre_digest_is_authoritative_unless_the_runtime_predates_the_pallet() {
		assert!(!babe_pre_digest_is_authoritative::<Block>(&pre_activation_header(vec![])));
		assert!(babe_pre_digest_is_authoritative::<Block>(&header(vec![])));
		assert!(babe_pre_digest_is_authoritative::<Block>(&raw_header(vec![version_digest(
			ACTIVATION_SPEC_VERSION + 1
		)])));
		// A future runtime without `pallet-version` deposits no digest; the layout it enforces is
		// still trusted, so dropping that pallet later cannot silently turn BABE blocks into AURA.
		assert!(babe_pre_digest_is_authoritative::<Block>(&raw_header(vec![])));
	}

	#[test]
	fn engine_is_read_from_the_first_aura_or_babe_pre_runtime_digest() {
		assert_eq!(
			authoring_engine::<Block>(&header(vec![pre_runtime(OTHER_ENGINE_ID), aura_slot(1)])),
			Some(ActiveEngine::Aura)
		);
		assert_eq!(
			authoring_engine::<Block>(&header(vec![pre_runtime(OTHER_ENGINE_ID), babe_slot(1)])),
			Some(ActiveEngine::Babe)
		);
		// Pre-flip blocks of the pallet's era carry both; the pallet guarantees AURA comes first.
		assert_eq!(
			authoring_engine::<Block>(&header(vec![aura_slot(1), babe_slot(1)])),
			Some(ActiveEngine::Aura)
		);
		// The layout is enforced, so a leading BABE digest really means a BABE block (a forged
		// one fails the BABE verifier it is handed to).
		assert_eq!(
			authoring_engine::<Block>(&header(vec![babe_slot(1), aura_slot(1)])),
			Some(ActiveEngine::Babe)
		);
	}

	#[test]
	fn header_without_an_aura_or_babe_pre_runtime_digest_has_no_engine() {
		let h =
			header(vec![pre_runtime(OTHER_ENGINE_ID), DigestItem::Seal(BABE_ENGINE_ID, vec![])]);
		assert_eq!(authoring_engine::<Block>(&h), None);
		assert!(slot_of::<Block>(&h).is_err());
	}

	#[test]
	fn blocks_from_before_the_pallet_are_aura_whatever_their_pre_runtime_digests() {
		// Nothing checked the layout back then, so none of these shapes may be trusted: a BABE
		// digest ahead of the AURA one, a BABE digest alone, or the honest dual layout.
		for logs in [
			vec![babe_slot(1), aura_slot(1)],
			vec![pre_runtime(OTHER_ENGINE_ID), babe_slot(1)],
			vec![aura_slot(1), babe_slot(1)],
			vec![],
		] {
			assert_eq!(
				authoring_engine::<Block>(&pre_activation_header(logs.clone())),
				Some(ActiveEngine::Aura),
				"{logs:?}"
			);
			assert!(!has_authoritative_babe_pre_digest::<Block>(&pre_activation_header(logs)));
		}
	}

	#[test]
	fn blocks_without_a_version_digest_are_routed_by_their_pre_runtime_digests() {
		assert_eq!(
			authoring_engine::<Block>(&raw_header(vec![
				pre_runtime(OTHER_ENGINE_ID),
				babe_slot(1)
			])),
			Some(ActiveEngine::Babe)
		);
		assert_eq!(
			authoring_engine::<Block>(&raw_header(vec![aura_slot(1)])),
			Some(ActiveEngine::Aura)
		);
	}

	#[test]
	fn authoritative_babe_pre_digest_is_detected_only_in_the_pallets_era() {
		assert!(has_authoritative_babe_pre_digest::<Block>(&header(vec![
			aura_slot(1),
			babe_slot(1)
		])));
		assert!(has_authoritative_babe_pre_digest::<Block>(&header(vec![babe_slot(1)])));
		assert!(!has_authoritative_babe_pre_digest::<Block>(&header(vec![aura_slot(1)])));
		assert!(!has_authoritative_babe_pre_digest::<Block>(&pre_activation_header(vec![
			aura_slot(1),
			babe_slot(1)
		])));
	}

	#[test]
	fn slot_comes_from_the_authoring_engines_pre_digest() {
		// AURA block with the BABE marker: the AURA slot, even though a (matching) BABE one exists.
		assert_eq!(slot_of::<Block>(&header(vec![aura_slot(7), babe_slot(7)])), Ok(Slot::from(7)));
		// BABE block: the BABE slot.
		assert_eq!(slot_of::<Block>(&header(vec![babe_slot(9)])), Ok(Slot::from(9)));
		// Pre-pallet block with a forged leading BABE digest: still the AURA slot.
		assert_eq!(
			slot_of::<Block>(&pre_activation_header(vec![babe_slot(99), aura_slot(3)])),
			Ok(Slot::from(3))
		);
	}

	#[test]
	fn slot_extraction_fails_when_the_authoring_engines_digest_is_malformed() {
		// Routed to AURA (first digest), but the AURA payload is not a slot.
		let h = header(vec![DigestItem::PreRuntime(AURA_ENGINE_ID, vec![1, 2, 3])]);
		assert!(slot_of::<Block>(&h).unwrap_err().contains("AURA"));
		// Pre-pallet: AURA whatever the layout, and there is no AURA digest to read.
		let h = pre_activation_header(vec![babe_slot(5)]);
		assert!(slot_of::<Block>(&h).unwrap_err().contains("AURA"));
	}

	#[test]
	fn engine_slot_extractor_delegates_to_slot_of() {
		use sc_partner_chains_consensus::SlotExtractor;
		let h = header(vec![babe_slot(11)]);
		assert_eq!(EngineSlotExtractor::extract_slot(&h), slot_of::<Block>(&h));
	}
}
