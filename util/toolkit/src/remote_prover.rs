// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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

use async_trait::async_trait;
use backoff::{ExponentialBackoff, future::retry};

use midnight_node_ledger_helpers::*;

const PROOF_SERVER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub struct RemoteProofServer {
	url: String,
}

impl RemoteProofServer {
	pub fn new(url: String) -> Self {
		Self { url }
	}
}

#[async_trait]
impl<D: DB + Clone> ProofProvider<D> for RemoteProofServer {
	async fn prove(
		&self,
		tx: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, D>,
		_rng: StdRng,
		resolver: &Resolver,
		cost_model: &CostModel,
	) -> Transaction<Signature, ProofMarker, PedersenRandomness, D> {
		log::info!("Proof server URL: {}", self.url);

		let backoff = ExponentialBackoff {
			max_elapsed_time: Some(PROOF_SERVER_TIMEOUT),
			..ExponentialBackoff::default()
		};

		retry(backoff, || async {
			let provider = ProofServerProvider { base_url: self.url.clone().into(), resolver };
			tx.prove(provider, cost_model).await.map_err(|e| {
				log::warn!("proof server proving failed, retrying: {e}");
				backoff::Error::transient(e)
			})
		})
		.await
		.unwrap_or_else(|err| panic!("Failed to prove via remote proof server: {:?}", err))
	}
}
