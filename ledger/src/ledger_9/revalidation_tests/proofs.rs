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
//! and fee still runs, and a contract call's proof must be the one [`DummyProver`] made for the
//! verifier key it is checked against.

use crate::ledger_9::{
	coin_structure_local::contract::ContractAddress,
	ledger_storage_local::{Storable, arena::ArenaKey, db::DB, storable::Loader},
	midnight_serialize_local::{Deserializable, Serializable, Tagged, tagged_serialize},
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
		proofs::{KeyLocation, Proof, ProofPreimage, ProvingProvider, Resolver},
	},
	zswap_local::{self as zswap, AuthorizedClaim, Offer, error::MalformedOffer},
};
use midnight_ledger_unsafe_helpers::ledger_9::{
	ContractOperationVersionedVerifierKey, CostModel, DefaultDB, PedersenRandomness, ProofMarker,
	ProofPreimageMarker, ProofProvider, Resolver as HelpersResolver, SeedableRng, Signature,
	StdRng, Transaction as HelpersTransaction, VersionedProvingProvider,
	contract_operation_versioned_verifier_key,
};
use std::cell::Cell;
use storage_sha2::{Digest, Sha256};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serializable, Storable)]
#[storable(base)]
#[tag = "proof"]
pub struct UnverifiedProofs;

/// What [`UnverifiedProofs`] was asked to verify: contract call and dust spend proofs, and zswap
/// offers.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
	pub contract: usize,
	pub dust: usize,
	pub zswap: usize,
}

thread_local! {
	static COUNTS: Cell<Counts> = Cell::default();
}

fn bump(f: impl FnOnce(&mut Counts)) {
	COUNTS.with(|c| {
		let mut counts = c.get();
		f(&mut counts);
		c.set(counts);
	});
}

/// Runs `f`, counting what this thread verified during it.
pub fn counting<T>(f: impl FnOnce() -> T) -> (T, Counts) {
	COUNTS.with(|c| c.set(Counts::default()));
	let result = f();
	(result, COUNTS.with(Cell::get))
}

impl<D: DB> ProofKind<D> for UnverifiedProofs {
	type Pedersen = PureGeneratorPedersen;
	type Proof = ProofVersioned;
	type LatestProof = Proof;

	fn zswap_well_formed(
		offer: &Offer<Self::LatestProof, D>,
		segment: u16,
	) -> Result<Pedersen, MalformedOffer> {
		bump(|c| c.zswap += 1);
		let offer = offer.erase_proofs();
		// The proof-erased offer's `well_formed` skips the per-output checks `ProofMarker` runs.
		offer.outputs.iter().try_for_each(|o| o.well_formed(segment))?;
		offer.transient.iter().try_for_each(|t| t.well_formed(segment))?;
		offer.well_formed(segment)
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
		// The ledger verifies a dust spend as a call to the default address with no entry point.
		let is_dust = call.address == ContractAddress::default() && call.entry_point.0.is_empty();
		bump(|c| if is_dust { c.dust += 1 } else { c.contract += 1 });
		let bound = match proof {
			ProofVersioned::V2(proof) => op.v2_vk().map(|vk| (proof, bound_proof(vk))),
			ProofVersioned::V3(proof) => op.v3_vk().map(|vk| (proof, bound_proof(vk))),
			_ => None,
		};
		let Some((proof, bound)) = bound else {
			return Err(MalformedTransaction::VerifierKeyNotPresent {
				address: call.address,
				operation: call.entry_point.clone(),
			});
		};
		if !is_dust && proof.0 != bound {
			return Err(MalformedTransaction::InvalidProof(anyhow::anyhow!(
				"v1 verification: proof made for another verifier key"
			)));
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

/// A contract call's dummy proof: the hash of the verifier key it was made for, at an input
/// proof's size, so it fails against any other key.
fn bound_proof(vk: &(impl Serializable + Tagged)) -> Vec<u8> {
	let mut bytes = vec![];
	tagged_serialize(vk, &mut bytes).expect("verifier keys serialize");
	Sha256::digest(&bytes)
		.into_iter()
		.chain(std::iter::repeat(0))
		.take(zswap::INPUT_PROOF_SIZE)
		.collect()
}

fn builtin_proof_size(key: &KeyLocation) -> Option<usize> {
	match key.0.as_ref() {
		"midnight/zswap/spend" => Some(zswap::INPUT_PROOF_SIZE),
		"midnight/zswap/output" => Some(zswap::OUTPUT_PROOF_SIZE),
		"midnight/dust/spend" => Some(DUST_SPEND_PROOF_SIZE),
		_ => None,
	}
}

/// Emits filler of each builtin circuit's real proof size, so fees match production, and a
/// [`bound_proof`] for a contract circuit, after checking the call against the circuit's IR.
#[derive(Clone, Copy)]
pub struct DummyProver {
	resolver: &'static HelpersResolver,
	/// Proves contract calls for a key no contract holds.
	wrong_key: bool,
}

impl ProvingProvider for DummyProver {
	async fn check(&self, preimage: &ProofPreimage) -> anyhow::Result<Vec<Option<usize>>> {
		if builtin_proof_size(&preimage.key_location).is_some() {
			return Ok(vec![]);
		}
		let rng = StdRng::seed_from_u64(0);
		VersionedProvingProvider { rng, resolver: self.resolver }.check(preimage).await
	}

	async fn prove(self, preimage: &ProofPreimage, _: Option<Fr>) -> anyhow::Result<Proof> {
		if let Some(size) = builtin_proof_size(&preimage.key_location) {
			return Ok(Proof([0xde, 0xad, 0xc0, 0xde].into_iter().cycle().take(size).collect()));
		}
		let key =
			self.resolver.resolve_key(preimage.key_location.clone()).await?.ok_or_else(|| {
				anyhow::anyhow!("no circuit data for {}", preimage.key_location.0)
			})?;
		let mut proof = match contract_operation_versioned_verifier_key(key.verifier_key)? {
			ContractOperationVersionedVerifierKey::V3(vk) => bound_proof(&vk),
			ContractOperationVersionedVerifierKey::V4(vk) => bound_proof(&vk),
			other => anyhow::bail!("no dummy proof for {other:?}"),
		};
		if self.wrong_key {
			proof[0] ^= 1;
		}
		Ok(Proof(proof))
	}

	fn split(&mut self) -> Self {
		*self
	}

	fn resolver(&self) -> &impl Resolver {
		self.resolver
	}
}

#[derive(Default)]
pub struct DummyProofProvider {
	/// Proves contract calls for a key no contract holds.
	pub wrong_key: bool,
}

#[async_trait::async_trait]
impl ProofProvider<DefaultDB> for DummyProofProvider {
	async fn prove(
		&self,
		tx: HelpersTransaction<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>,
		_: StdRng,
		resolver: &'static HelpersResolver,
		cost_model: CostModel,
	) -> HelpersTransaction<Signature, ProofMarker, PedersenRandomness, DefaultDB> {
		let prover = DummyProver { resolver, wrong_key: self.wrong_key };
		// The proving future is `!Send` and this one must be `Send`: drive it on its own thread.
		std::thread::scope(|s| {
			s.spawn(|| futures::executor::block_on(tx.prove(prover, &cost_model)))
				.join()
				.expect("dummy proving panicked")
				.expect("dummy proving failed")
		})
	}
}
