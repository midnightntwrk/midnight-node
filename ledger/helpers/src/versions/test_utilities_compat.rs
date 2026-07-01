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

//! Vendored from `midnight-ledger` 7.0.3 `test_utilities` (byte-identical in 8.1.0).
//!
//! The upstream module is feature-gated behind `test-utilities`, whose `zkir_v2`
//! dependency (`midnight-zkir ^2.1.0`) now unifies to the 3.x-crypto-stack
//! `midnight-zkir` 2.2.0 — type-incompatible with ledgers 7/8. We therefore build
//! L7/L8 without `test-utilities` and supply these items ourselves, against the
//! renamed 2.x-stack `zkir` crate (tag `crate-zkir-2.1.0`) that zkir 2.2.0 itself
//! uses internally for backwards-compatible proving.
//!
//! Kept byte-close to upstream for easy re-diffing
//!  — don't restyle (e.g. the `println!`s are upstream's).

use super::{
	base_crypto::data_provider::{self, MidnightDataProvider},
	midnight_serialize::{tagged_deserialize, tagged_serialize},
	mn_ledger::{
		dust::{DUST_EXPECTED_FILES, DustResolver},
		prove::Resolver,
		structure::{ProofPreimageVersioned, ProofVersioned},
	},
	transient_crypto::{
		curve::Fr,
		proofs::{
			KeyLocation, ProverKey, ProvingKeyMaterial, ProvingProvider, Resolver as ResolverTrait,
			VerifierKey, WrappedIr,
		},
	},
	zkir::IrSource,
	zswap::{ZSWAP_EXPECTED_FILES, prove::ZswapResolver},
};
use lazy_static::lazy_static;
use reqwest::Client;
use std::{env, io};

pub type Pk = ProverKey<IrSource>;

lazy_static! {
	pub static ref PUBLIC_PARAMS: ZswapResolver = ZswapResolver(
		MidnightDataProvider::new(
			data_provider::FetchMode::OnDemand,
			data_provider::OutputMode::Log,
			ZSWAP_EXPECTED_FILES.to_owned(),
		)
		.unwrap()
	);
}

pub async fn verifier_key(resolver: &Resolver, name: &'static str) -> Option<VerifierKey> {
	let proof_data = resolver
		.resolve_key(KeyLocation(std::borrow::Cow::Borrowed(name)))
		.await
		.ok()??;
	tagged_deserialize(&mut &proof_data.verifier_key[..]).ok()
}

pub fn test_resolver(test_name: &'static str) -> Resolver {
	let test_dir = env::var("MIDNIGHT_LEDGER_TEST_STATIC_DIR")
		.expect("MIDNIGHT_LEDGER_TEST_STATIC_DIR should be set as env variable");

	Resolver::new(
		PUBLIC_PARAMS.clone(),
		DustResolver(
			MidnightDataProvider::new(
				data_provider::FetchMode::OnDemand,
				data_provider::OutputMode::Log,
				DUST_EXPECTED_FILES.to_owned(),
			)
			.unwrap(),
		),
		Box::new(move |KeyLocation(loc)| {
			let sync_block = || {
				let read_file = |dir, ext| {
					let path = format!("{test_dir}/{test_name}/{dir}/{loc}.{ext}");
					let res = std::fs::read(path);
					match res {
						Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
						Err(e) => Err(e),
						Ok(v) => Ok(Some(v)),
					}
				};
				let Some(prover_key) = read_file("keys", "prover")? else {
					return Ok(None);
				};
				let Some(verifier_key) = read_file("keys", "verifier")? else {
					return Ok(None);
				};
				let Some(ir_source) = read_file("zkir", "bzkir")? else {
					return Ok(None);
				};
				Ok(Some(ProvingKeyMaterial { prover_key, verifier_key, ir_source }))
			};
			let res = sync_block();
			Box::pin(std::future::ready(res))
		}),
	)
}

#[derive(Clone)]
pub struct ProofServerProvider<'a> {
	pub base_url: String,
	pub resolver: &'a Resolver,
}

impl ProofServerProvider<'_> {
	fn is_builtin_key(loc: &KeyLocation) -> bool {
		[
			"midnight/zswap/spend",
			"midnight/zswap/output",
			"midnight/zswap/sign",
			"midnight/dust/spend",
		]
		.contains(&loc.0.as_ref())
	}
	pub async fn check_request_body(
		&self,
		preimage: &ProofPreimageVersioned,
	) -> Result<Vec<u8>, anyhow::Error> {
		let ir = if Self::is_builtin_key(preimage.key_location()) {
			None
		} else {
			let data =
				self.resolver.resolve_key(preimage.key_location().clone()).await?.ok_or_else(
					|| anyhow::anyhow!("failed to find key '{}'", &preimage.key_location().0),
				)?;
			Some(WrappedIr(data.ir_source))
		};
		let mut res = Vec::new();
		tagged_serialize(&(preimage.clone(), ir), &mut res)?;
		Ok(res)
	}

	pub async fn proving_request_body(
		&self,
		preimage: &ProofPreimageVersioned,
		overwrite_binding_input: Option<Fr>,
	) -> Result<Vec<u8>, anyhow::Error> {
		let data = if Self::is_builtin_key(preimage.key_location()) {
			None
		} else {
			self.resolver.resolve_key(preimage.key_location().clone()).await?
		};
		let mut res = Vec::new();
		tagged_serialize(&(preimage.clone(), data, overwrite_binding_input), &mut res)?;
		Ok(res)
	}
}

impl ProvingProvider for ProofServerProvider<'_> {
	async fn check(
		&self,
		preimage: &super::transient_crypto::proofs::ProofPreimage,
	) -> Result<Vec<Option<usize>>, anyhow::Error> {
		let ser = self
			.check_request_body(&ProofPreimageVersioned::V2(std::sync::Arc::new(preimage.clone())))
			.await?;
		println!("    Check request: {} bytes", ser.len());
		let resp = Client::new().post(format!("{}/check", &self.base_url)).body(ser).send().await?;
		if resp.status().is_success() {
			let bytes = resp.bytes().await?;
			println!("    Check response: {} bytes", bytes.len());
			let res: Vec<Option<u64>> = tagged_deserialize(&mut bytes.to_vec().as_slice())?;
			Ok(res.into_iter().map(|i| i.map(|i| i as usize)).collect())
		} else {
			anyhow::bail!(
				"proving server error: {}",
				resp.text().await.expect("error retrieving error")
			)
		}
	}
	async fn prove(
		self,
		preimage: &super::transient_crypto::proofs::ProofPreimage,
		overwrite_binding_input: Option<Fr>,
	) -> Result<super::transient_crypto::proofs::Proof, anyhow::Error> {
		let ser = self
			.proving_request_body(
				&ProofPreimageVersioned::V2(std::sync::Arc::new(preimage.clone())),
				overwrite_binding_input,
			)
			.await?;
		println!("    Proving request: {} bytes", ser.len());
		let resp = Client::new().post(format!("{}/prove", &self.base_url)).body(ser).send().await?;
		if resp.status().is_success() {
			let bytes = resp.bytes().await?;
			println!("    Proving response: {} bytes", bytes.len());
			let proof: ProofVersioned = tagged_deserialize(&mut bytes.to_vec().as_slice())?;
			// ProofVersioned is #[non_exhaustive] outside midnight-ledger; only V2 exists today.
			match proof {
				ProofVersioned::V2(proof) => Ok(proof),
				_ => anyhow::bail!("proof server returned an unsupported proof version"),
			}
		} else {
			anyhow::bail!(
				"proving server error: {}",
				resp.text().await.expect("error retrieving error")
			)
		}
	}
	fn split(&mut self) -> Self {
		self.clone()
	}
}
