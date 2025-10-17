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

pub struct RemoteProofServer {
	url: String,
}

impl RemoteProofServer {
	pub fn new(url: String) -> Self {
		Self { url }
	}

	async fn serialize_request_body<D: DB>(
		&self,
		tx: &Transaction<Signature, ProofPreimageMarker, PedersenRandomness, D>,
		resolver: &Resolver,
		cost_model: &CostModel,
	) -> Vec<u8> {
		let mut buf = vec![];
		let provider = ProofServerProvider { base_url: self.url.clone().into(), resolver };
		tx.prove(provider, cost_model)
			.await
			.unwrap_or_else(|err| panic!("Remote Server Request Error: {:?}", err))
			.serialize(&mut buf)
			.unwrap_or_else(|err| panic!("Serializing tx error: {:?}", err));
		buf
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
		let url = reqwest::Url::parse(&self.url)
			.expect("failed to parse proof server URL")
			.join("prove-tx")
			.unwrap();

		println!("Proof server URL: {}", url);

		let client = reqwest::ClientBuilder::new().pool_idle_timeout(None).build().unwrap();
		let response_bytes = retry(ExponentialBackoff::default(), || async {
			let body = self.serialize_request_body(&tx, resolver, cost_model).await;

			let resp = client.post(url.clone()).body(body).send().await.map_err(|e| {
				println!("Proof Server Send Error: {:?}", e);
				backoff::Error::transient(e)
			})?;

			let resp_err = resp.error_for_status_ref().err();
			let resp_bytes = resp.bytes().await.map_err(|e| {
				println!("Proof Server to Bytes Error: {:?}", e);
				backoff::Error::transient(e)
			})?;

			if let Some(e) = resp_err {
				println!("Proof Server Response Error: {:?}. Bytes: {:?}", e, resp_bytes);
				return Err(backoff::Error::transient(e));
			}

			Ok::<Vec<u8>, backoff::Error<reqwest::Error>>(resp_bytes.to_vec())
		})
		.await
		.expect("failed to send request");

		if response_bytes.is_empty() {
			panic!("Proof server returned empty response");
		}

		deserialize(&response_bytes[..]).expect("failed to deserialize transaction")
	}
}
