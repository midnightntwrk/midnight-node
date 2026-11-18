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

//! Proofs that cost nothing to make or check, on production's wire format.
//!
//! [`UnverifiedProofs`] shares [`ProofMarker`]'s tag and proof types, so a `ProofMarker`
//! transaction carrying [`DummyProver`]'s filler deserializes as an `UnverifiedProofs` one,
//! byte for byte. Only proof verification differs: every structural check, signature, binding
//! and fee still runs.

use crate::ledger_9::{
	ledger_storage_local::{Storable, arena::ArenaKey, db::DB, storable::Loader},
	midnight_serialize_local::{Deserializable, Serializable, Tagged},
	mn_ledger_local::{
		dust::DUST_SPEND_PROOF_SIZE,
		error::MalformedTransaction,
		structure::{
			ContractCall, ContractOperationVersion, PedersenDowngradeable, ProofKind,
			ProofVersioned, SignatureKind, Transaction,
		},
		verify::ProofVerificationMode,
	},
	onchain_runtime_local::state::ContractOperation,
	transient_crypto_local::{
		commitment::{Pedersen, PureGeneratorPedersen},
		curve::Fr,
		proofs::{
			KeyLocation, Proof, ProofPreimage, ProvingKeyMaterial, ProvingProvider, Resolver,
		},
	},
	zswap_local::{self as zswap, AuthorizedClaim, Offer, error::MalformedOffer},
};
use midnight_ledger_unsafe_helpers::ledger_9::{
	CostModel, DefaultDB, PedersenRandomness, ProofMarker, ProofPreimageMarker, ProofProvider,
	Resolver as HelpersResolver, Signature, StdRng, Transaction as HelpersTransaction,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serializable, Storable)]
#[storable(base)]
#[tag = "proof"]
pub struct UnverifiedProofs;

impl<D: DB> ProofKind<D> for UnverifiedProofs {
	type Pedersen = PureGeneratorPedersen;
	type Proof = ProofVersioned;
	type LatestProof = Proof;

	fn zswap_well_formed(
		offer: &Offer<Self::LatestProof, D>,
		segment: u16,
	) -> Result<Pedersen, MalformedOffer> {
		offer.erase_proofs().well_formed(segment)
	}

	fn zswap_claim_well_formed(
		_: &AuthorizedClaim<Self::LatestProof>,
	) -> Result<(), MalformedOffer> {
		Ok(())
	}

	fn proof_verify(
		op: &ContractOperation,
		proof: &Self::Proof,
		_: Vec<Fr>,
		call: &ContractCall<Self, D>,
		_: ProofVerificationMode,
	) -> Result<(), MalformedTransaction<D>> {
		let vk_present = match proof {
			ProofVersioned::V2(_) => op.v2_vk().is_some(),
			ProofVersioned::V3(_) => op.v3_vk().is_some(),
			_ => false,
		};
		if !vk_present {
			return Err(MalformedTransaction::VerifierKeyNotPresent {
				address: call.address,
				operation: call.entry_point.clone(),
			});
		}
		Ok(())
	}

	fn estimated_tx_size<
		S: SignatureKind<D>,
		B: Storable<D> + PedersenDowngradeable<D> + Serializable,
	>(
		tx: &Transaction<S, Self, B, D>,
	) -> usize {
		tx.serialized_size()
	}

	fn proof_version_to_operation_version(proof: &Self::Proof) -> Option<ContractOperationVersion> {
		match proof {
			ProofVersioned::V2(_) => Some(ContractOperationVersion::V3),
			ProofVersioned::V3(_) => Some(ContractOperationVersion::V4),
			_ => None,
		}
	}
}

/// Emits filler of each builtin circuit's real proof size, so fees match production.
#[derive(Clone)]
pub struct DummyProver;

impl ProvingProvider for DummyProver {
	async fn check(&self, _: &ProofPreimage) -> anyhow::Result<Vec<Option<usize>>> {
		Ok(vec![])
	}

	async fn prove(self, preimage: &ProofPreimage, _: Option<Fr>) -> anyhow::Result<Proof> {
		let size = match preimage.key_location.0.as_ref() {
			"midnight/zswap/spend" => zswap::INPUT_PROOF_SIZE,
			"midnight/zswap/output" => zswap::OUTPUT_PROOF_SIZE,
			"midnight/dust/spend" => DUST_SPEND_PROOF_SIZE,
			other => anyhow::bail!("no dummy proof for circuit {other}"),
		};
		Ok(Proof([0xde, 0xad, 0xc0, 0xde].into_iter().cycle().take(size).collect()))
	}

	fn split(&mut self) -> Self {
		Self
	}

	fn resolver(&self) -> &impl Resolver {
		self
	}
}

impl Resolver for DummyProver {
	async fn resolve_key(&self, _: KeyLocation) -> std::io::Result<Option<ProvingKeyMaterial>> {
		Ok(None)
	}
}

pub struct DummyProofProvider;

#[async_trait::async_trait]
impl ProofProvider<DefaultDB> for DummyProofProvider {
	async fn prove(
		&self,
		tx: HelpersTransaction<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>,
		_: StdRng,
		_: &'static HelpersResolver,
		cost_model: CostModel,
	) -> HelpersTransaction<Signature, ProofMarker, PedersenRandomness, DefaultDB> {
		// The proving future is `!Send` and this one must be `Send`: drive it on its own thread.
		std::thread::scope(|s| {
			s.spawn(|| futures::executor::block_on(tx.prove(DummyProver, &cost_model)))
				.join()
				.expect("dummy proving panicked")
				.expect("dummy proving failed")
		})
	}
}
