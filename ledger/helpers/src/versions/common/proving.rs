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

// `prove` must stay `Send` (the trait is a `#[async_trait]`, not `?Send`): the toolkit
// `tokio::task::spawn`s proving (tx_generator batches.rs / claim_rewards.rs), which genuinely
// needs a `Send` future. But L9 proving is non-Send: midnight-zkir 2.2.0's `LocalProvingProvider`
// routes L9 through the 2.x (`transient_crypto_old`) `ProvingProvider`, whose `Resolver::resolve_key`
// is a bare async-fn-in-trait (no `Send` bound). Because that's an RPITIT, its future is treated as
// `!Send` across the trait boundary even when fully monomorphized — so a Send-clean resolver wrapper
// can't fix it at the type level, and `tx.prove(..).await` here would taint this future.
//
// We bridge at the executor level instead (see `LocalProofServer::prove`): the `!Send` proving future
// is built and driven to completion *inside* a `tokio::task::spawn_blocking` closure, on a dedicated
// blocking-pool thread running its own current-thread tokio runtime, so the future never crosses a
// thread boundary. `.await`ing the `spawn_blocking` handle yields the calling worker, so N
// semaphore-bounded proofs run on N blocking-pool threads in real parallel even on the toolkit's
// single-threaded runtime. This `async fn` body holds no non-Send state across an await point,
// leaving the boxed future trivially `Send`.
//
// To make the closure `Send + 'static` (a `spawn_blocking` requirement), `resolver` is `&'static`
// (the resolver is already a `lazy_static`, so this is no plumbing change) and `cost_model` is owned
// (`CostModel` is `Clone`, so the few call sites just `.clone()`). `tx`/`rng`/the returned
// `Transaction` are already `Send + 'static`, since this trait's `Send` future requires that anyway.
// No upstream patch needed; this is independent of the transient-crypto-old `Send` fix.
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
