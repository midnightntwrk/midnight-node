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

#![cfg(feature = "can-panic")]

use super::{
	CostModel, DB, KeyLocation, LocalProvingProvider, PUBLIC_PARAMS, PedersenRandomness,
	ProofMarker, ProofPreimageMarker, Resolver, ResolverTrait, Signature, StdRng, Transaction,
	ZswapResolver,
};
use async_trait::async_trait;

// Proving a transaction is mostly CPU-bound, however the `prove` method also
// resolves keys for use during proving. These may come from remote sources, hence
// the need for this method to be async.
#[async_trait]
pub trait ProofProvider<D: DB + Clone>: Send + Sync {
	async fn prove(
		&self,
		tx: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, D>,
		rng: StdRng,
		resolver: &'static Resolver,
		cost_model: CostModel,
	) -> Transaction<Signature, ProofMarker, PedersenRandomness, D>;
}

pub struct LocalProofServer {
	pub params_prover: &'static ZswapResolver,
}

impl LocalProofServer {
	pub fn new() -> Self {
		Self { params_prover: &PUBLIC_PARAMS }
	}
}

impl Default for LocalProofServer {
	fn default() -> Self {
		Self::new()
	}
}

#[async_trait]
impl<D: DB + Clone> ProofProvider<D> for LocalProofServer {
	async fn prove(
		&self,
		tx: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, D>,
		rng: StdRng,
		resolver: &'static Resolver,
		cost_model: CostModel,
	) -> Transaction<Signature, ProofMarker, PedersenRandomness, D> {
		// Local proving is CPU-bound (and, for L9, also `!Send` — this body is shared across
		// L7/L8/L9). Run it on a blocking-pool thread via `spawn_blocking`: the future is built and
		// driven inside the closure (in a fresh current-thread runtime) so even the `!Send` L9 future
		// never crosses a thread boundary, and `.await`ing the handle yields the calling worker — so N
		// semaphore-bounded proofs run in real parallel even on the toolkit's single-threaded runtime.
		// The closure captures only `tx`/`rng`/`resolver` (`&'static`)/`cost_model` — not `self` (it
		// uses `&*PUBLIC_PARAMS`, a static).
		tokio::task::spawn_blocking(move || {
			let rt = tokio::runtime::Builder::new_current_thread()
				.enable_all()
				.build()
				.expect("failed to build local proving runtime");
			rt.block_on(async move {
				log::info!("Ensuring zswap key material is available...");
				{
					let ks = futures::future::join_all(
						(10..=15).map(|k| resolver.zswap_resolver.0.fetch_k(k)),
					);
					let keys = futures::future::join_all(
						["midnight/zswap/spend", "midnight/zswap/output", "midnight/zswap/sign"]
							.into_iter()
							.map(|k| resolver.zswap_resolver.resolve_key(KeyLocation(k.into()))),
					);
					let (ks, keys) = futures::future::join(ks, keys).await;
					ks.into_iter().collect::<Result<Vec<_>, _>>().expect("failed to get keys 'ks'");
					keys.into_iter()
						.collect::<Result<Vec<_>, _>>()
						.expect("failed to get keys 'keys'");
				}

				let pp = LocalProvingProvider { rng, resolver, params: &*PUBLIC_PARAMS };

				tx.prove(pp, &cost_model).await.expect("Tx should be provable")
			})
		})
		.await
		.expect("local proving task panicked")
	}
}
