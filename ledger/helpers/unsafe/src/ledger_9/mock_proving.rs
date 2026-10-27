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

//! Mock proving that also covers contract calls, plus parallel local proving.
//!
//! The ledger's `Transaction::mock_prove` refuses contract calls because a contract proof's
//! size depends on the circuit. For fee estimation an upper bound is enough: over-estimating
//! only overpays by the byte-price of the slack, which the ledger accepts.

use crate::ledger_9::{
	PUBLIC_PARAMS,
	base_crypto::rng::SplittableRng,
	ledger_storage::db::DB,
	mn_ledger::{
		dust::DUST_SPEND_PROOF_SIZE,
		error::TransactionProvingError,
		prove::Resolver,
		structure::{ProofMarker, ProofPreimageMarker, SignatureKind, Transaction},
	},
	onchain_runtime::cost_model::CostModel,
	transient_crypto::{
		commitment::{PedersenRandomness, PureGeneratorPedersen},
		curve::Fr,
		proofs::{Proof, ProofPreimage, ProvingProvider, Resolver as _, Zkir},
	},
	zkir::{IrSource, LocalProvingProvider},
	zswap::{INPUT_PROOF_SIZE, OUTPUT_PROOF_SIZE, prove::ZswapResolver},
};
use rand::{SeedableRng, rngs::StdRng};
use std::future::Future;

/// Real ledger-9 contract proofs observed so far are 3–6 KiB.
const DEFAULT_CONTRACT_PROOF_SIZE: usize = 8 * 1024;

const ZSWAP_KEYS: [&str; 3] =
	["midnight/zswap/spend", "midnight/zswap/output", "midnight/zswap/sign"];

fn contract_proof_size() -> usize {
	std::env::var("MN_MOCK_CONTRACT_PROOF_SIZE")
		.ok()
		.and_then(|v| v.parse().ok())
		.unwrap_or(DEFAULT_CONTRACT_PROOF_SIZE)
}

/// Runs the real transcript check, so padding and gas match real proving, but emits filler
/// bytes instead of proofs. With `real_zswap`, zswap proofs are proven for real: the ledger
/// verifies them unconditionally, while DUST and contract proof checks can be relaxed.
struct SizeHintMockProver {
	resolver: &'static Resolver,
	real_zswap: Option<ParallelLocalProver>,
}

impl ProvingProvider for SizeHintMockProver {
	async fn check(&self, preimage: &ProofPreimage) -> Result<Vec<Option<usize>>, anyhow::Error> {
		let proving_data =
			self.resolver.resolve_key(preimage.key_location.clone()).await?.ok_or_else(|| {
				anyhow::anyhow!("no circuit data for '{}'", preimage.key_location.0)
			})?;
		IrSource::load_from_tagged(std::io::Cursor::new(&proving_data.ir_source[..]))?
			.check(preimage)
	}

	async fn prove(
		self,
		preimage: &ProofPreimage,
		overwrite_binding_input: Option<Fr>,
	) -> Result<Proof, anyhow::Error> {
		let key = preimage.key_location.0.as_ref();
		if let Some(real) = self.real_zswap.filter(|_| ZSWAP_KEYS.contains(&key)) {
			return real.prove(preimage, overwrite_binding_input).await;
		}
		let size = match key {
			"midnight/zswap/spend" => INPUT_PROOF_SIZE,
			"midnight/zswap/output" => OUTPUT_PROOF_SIZE,
			"midnight/dust/spend" => DUST_SPEND_PROOF_SIZE,
			_ => contract_proof_size(),
		};
		Ok(Proof(vec![0; size]))
	}

	fn split(&mut self) -> Self {
		Self { resolver: self.resolver, real_zswap: self.real_zswap.as_mut().map(|p| p.split()) }
	}

	fn resolver(&self) -> &impl crate::ledger_9::transient_crypto::proofs::Resolver {
		self.resolver
	}
}

/// Ledger-9 proving futures are `!Send`, so drive them on a blocking thread with their own
/// runtime.
async fn on_blocking_thread<T: Send + 'static, F: Future<Output = T>>(
	make_future: impl FnOnce() -> F + Send + 'static,
) -> T {
	tokio::task::spawn_blocking(move || {
		tokio::runtime::Builder::new_current_thread()
			.enable_all()
			.build()
			.expect("failed to build proving runtime")
			.block_on(make_future())
	})
	.await
	.expect("proving task panicked")
}

/// Mock-proves `tx` without sealing. Pass `real_zswap` to produce a transaction that
/// `mock-proofs` nodes accept.
pub async fn mock_prove_unsealed<S: SignatureKind<D> + 'static, D: DB + 'static>(
	tx: Transaction<S, ProofPreimageMarker, PedersenRandomness, D>,
	resolver: &'static Resolver,
	cost_model: CostModel,
	real_zswap: Option<StdRng>,
) -> anyhow::Result<Transaction<S, ProofMarker, PedersenRandomness, D>> {
	on_blocking_thread(move || async move {
		let prover = SizeHintMockProver {
			resolver,
			real_zswap: real_zswap.map(|rng| ParallelLocalProver { rng, resolver }),
		};
		tx.prove(prover, &cost_model)
			.await
			.map_err(|e| anyhow::anyhow!("mock proving failed: {e:?}"))
	})
	.await
}

/// Fee-estimation mock proving. Like `Transaction::mock_prove`, it seals with a fixed RNG and
/// pins every binding commitment to the largest representable value, so the size is an upper
/// bound.
pub async fn mock_prove_for_fees<S: SignatureKind<D> + 'static, D: DB + 'static>(
	tx: Transaction<S, ProofPreimageMarker, PedersenRandomness, D>,
	resolver: &'static Resolver,
	cost_model: CostModel,
) -> anyhow::Result<Transaction<S, ProofMarker, PureGeneratorPedersen, D>> {
	let mut proven = mock_prove_unsealed(tx, resolver, cost_model, None)
		.await?
		.seal(StdRng::seed_from_u64(0));
	if let Transaction::Standard(ref mut stx) = proven {
		stx.intents = stx
			.intents
			.iter()
			.map(|entry| {
				let mut intent = (*entry.1).clone();
				intent.binding_commitment = PureGeneratorPedersen::largest_representable();
				(*entry.0, intent)
			})
			.collect();
	}
	Ok(proven)
}

/// The ledger drives a transaction's proofs as futures on one thread, so they run back to
/// back. Proving each on its own blocking thread lets them overlap.
struct ParallelLocalProver {
	rng: StdRng,
	resolver: &'static Resolver,
}

impl ParallelLocalProver {
	fn local(&self) -> LocalProvingProvider<'static, StdRng, Resolver, ZswapResolver> {
		LocalProvingProvider {
			rng: self.rng.clone(),
			resolver: self.resolver,
			params: &PUBLIC_PARAMS,
		}
	}
}

impl ProvingProvider for ParallelLocalProver {
	async fn check(&self, preimage: &ProofPreimage) -> Result<Vec<Option<usize>>, anyhow::Error> {
		self.local().check(preimage).await
	}

	async fn prove(
		self,
		preimage: &ProofPreimage,
		overwrite_binding_input: Option<Fr>,
	) -> Result<Proof, anyhow::Error> {
		let preimage = preimage.clone();
		let local = self.local();
		on_blocking_thread(
			move || async move { local.prove(&preimage, overwrite_binding_input).await },
		)
		.await
	}

	fn split(&mut self) -> Self {
		Self { rng: self.rng.split(), resolver: self.resolver }
	}

	fn resolver(&self) -> &impl crate::ledger_9::transient_crypto::proofs::Resolver {
		self.resolver
	}
}

pub async fn prove_tx_in_parallel<S: SignatureKind<D>, D: DB>(
	tx: &Transaction<S, ProofPreimageMarker, PedersenRandomness, D>,
	rng: StdRng,
	resolver: &'static Resolver,
	cost_model: &CostModel,
) -> Result<Transaction<S, ProofMarker, PedersenRandomness, D>, TransactionProvingError<D>> {
	tx.prove(ParallelLocalProver { rng, resolver }, cost_model).await
}
