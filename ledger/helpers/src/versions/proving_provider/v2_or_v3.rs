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

//! Whole-transaction proving provider for ledger 9, whose Dust spend circuit moved to the
//! ZKIR-v3 proof system (`midnight-zkir-v3`) while zswap and contract circuits are still
//! ZKIR-v2 (`midnight-zkir`).
//!
//! A single `LocalProvingProvider` can no longer prove a whole ledger-9 transaction: it
//! parses every circuit through `zkir`'s own `IrSource`, whose `IrMinorVersion` has no v3
//! case at all, so a v3-tagged circuit fails outright with
//! `expected one of 'ir-source[v2-generic]' or 'ir-source[v2]', got 'ir-source[v3-generic]'`.
//! Instead we peek each circuit's IR tag (a ≤512-byte prefix read, not a full parse) and
//! dispatch that one proof to the matching pipeline.
//!
//! Modelled on `midnight-ledger`'s own `CombinedProofProvider` (`ledger/src/test_utilities.rs`),
//! which isn't `pub` so can't be reused. Upstream's whole-tx proof-server endpoint
//! (`/prove-tx`) still has this bug and is marked deprecated, so that's not a reference either.

use rand::rngs::StdRng;

use super::{
	base_crypto::rng::SplittableRng,
	midnight_serialize::{peek_tag, tagged_deserialize},
	mn_ledger::prove::Resolver,
	transient_crypto::{
		curve::Fr,
		proofs::{Proof, ProofPreimage, ProvingProvider, Resolver as ResolverTrait},
	},
	zkir::LocalProvingProvider,
};

/// Legacy non-generic ZKIR-v2 tag, still what `compactc` emits for contract circuits.
const TAG_V2: &str = "ir-source[v2]";
/// `zkir::IrSource`'s own tag (asserted in `tests::ir_tags_match_upstream`).
const TAG_V2_GENERIC: &str = "ir-source[v2-generic]";
/// `zkir_v3::IrSource`'s own tag (asserted in `tests::ir_tags_match_upstream`).
const TAG_V3_GENERIC: &str = "ir-source[v3-generic]";

/// Routes each proof to the ZKIR generation its circuit was compiled for.
pub struct VersionedProvingProvider<'a> {
	/// Randomness for the v3 pipeline (`zkir_v2` carries its own independent stream).
	rng: StdRng,
	resolver: &'a Resolver,
	zkir_v2: LocalProvingProvider<'a, StdRng, Resolver, Resolver>,
}

impl VersionedProvingProvider<'_> {
	/// Resolves `preimage`'s key material and reads the ZKIR tag off its IR source.
	async fn resolve_tagged_ir(
		&self,
		preimage: &ProofPreimage,
	) -> Result<(String, Vec<u8>), anyhow::Error> {
		let material =
			self.resolver.resolve_key(preimage.key_location.clone()).await?.ok_or_else(|| {
				anyhow::anyhow!("could not resolve key location: {}", preimage.key_location.0)
			})?;
		let tag = peek_tag(&mut std::io::Cursor::new(&material.ir_source))?;
		Ok((tag, material.ir_source))
	}
}

impl ProvingProvider for VersionedProvingProvider<'_> {
	async fn check(&self, preimage: &ProofPreimage) -> Result<Vec<Option<usize>>, anyhow::Error> {
		let (tag, ir_source) = self.resolve_tagged_ir(preimage).await?;
		match tag.as_str() {
			TAG_V2 | TAG_V2_GENERIC => self.zkir_v2.check(preimage).await,
			TAG_V3_GENERIC => {
				let ir_v3: zkir_v3::IrSource = tagged_deserialize(&mut &ir_source[..])?;
				preimage.check(&ir_v3)
			},
			_ => Err(anyhow::anyhow!("Unknown ZKIR tag: '{tag}'")),
		}
	}

	async fn prove(
		self,
		preimage: &ProofPreimage,
		overwrite_binding_input: Option<Fr>,
	) -> Result<Proof, anyhow::Error> {
		let (tag, _) = self.resolve_tagged_ir(preimage).await?;
		match tag.as_str() {
			TAG_V2 | TAG_V2_GENERIC => self.zkir_v2.prove(preimage, overwrite_binding_input).await,
			TAG_V3_GENERIC => {
				let mut preimage = preimage.clone();
				if let Some(binding_input) = overwrite_binding_input {
					preimage.binding_input = binding_input;
				}
				// `Resolver` implements `ParamsProverProvider` (delegating to its zswap
				// resolver, which is `PUBLIC_PARAMS`), so it serves as both the public-params
				// source and the key resolver here.
				preimage
					.prove::<zkir_v3::IrSource>(self.rng, self.resolver, self.resolver)
					.await
					.map(|(proof, _)| proof)
			},
			_ => Err(anyhow::anyhow!("Unknown ZKIR tag: '{tag}'")),
		}
	}

	fn split(&mut self) -> Self {
		Self { rng: self.rng.split(), resolver: self.resolver, zkir_v2: self.zkir_v2.split() }
	}

	fn resolver(&self) -> &impl ResolverTrait {
		self.resolver
	}
}

/// Ledger 9 mixes ZKIR-v2 and ZKIR-v3 circuits in one transaction, so proving dispatches
/// per circuit on the IR tag.
pub fn make_proving_provider(mut rng: StdRng, resolver: &Resolver) -> VersionedProvingProvider<'_> {
	VersionedProvingProvider {
		rng: rng.split(),
		resolver,
		zkir_v2: LocalProvingProvider { rng: rng.split(), resolver, params: resolver },
	}
}

#[cfg(test)]
mod tests {
	use super::super::midnight_serialize::{Tagged, peek_tag};
	use super::{TAG_V2, TAG_V2_GENERIC, TAG_V3_GENERIC};

	/// The dispatch in `check`/`prove` keys off literal IR tag strings. If either crate renames
	/// its tag, every proof of that generation would silently fall through to the "Unknown ZKIR
	/// tag" arm, so pin the literals to what the crates actually serialize.
	#[test]
	fn ir_tags_match_upstream() {
		assert_eq!(<super::super::zkir::IrSource as Tagged>::tag(), TAG_V2_GENERIC);
		assert_eq!(<zkir_v3::IrSource as Tagged>::tag(), TAG_V3_GENERIC);
	}

	/// `compactc`-produced contract circuits carry the legacy non-generic `TAG_V2`, which has no
	/// `Tagged` type we can reach (`zkir::OldIrSource` is `pub(crate)`), so pin it against a
	/// committed artifact instead. This flipping to v3 means the v3 `check()` branch — dead today,
	/// since only contract calls ever call `check()` — has become load-bearing.
	#[test]
	fn committed_contract_ir_is_still_v2() {
		let ir = std::fs::read(concat!(
			env!("CARGO_MANIFEST_DIR"),
			"/../../static/contracts/simple-merkle-tree/zkir/check.bzkir"
		))
		.expect("committed simple-merkle-tree zkir should be readable");
		let tag = peek_tag(&mut std::io::Cursor::new(&ir)).expect("zkir should carry a tag");
		assert_eq!(tag, TAG_V2);
	}
}
