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
use std::sync::Arc;

use crate::builder::{
	BuildTxs, DefaultDB, DeserializedTransactionsWithContext, ProofProvider, ProofType,
	SignatureType,
};

pub struct ReplaceInitialTxBuilder;

impl ReplaceInitialTxBuilder {
	pub fn new() -> Self {
		Self
	}
}

#[async_trait]
impl BuildTxs for ReplaceInitialTxBuilder {
	async fn build_txs_from(
		&self,
		mut received_tx: DeserializedTransactionsWithContext<SignatureType, ProofType>,
		_prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Result<
		DeserializedTransactionsWithContext<SignatureType, ProofType>,
		Box<dyn std::error::Error>,
	> {
		let new_initial_tx = received_tx
			.batches
			.first_mut()
			.ok_or("No batches available to migrate")?
			.txs
			.remove(0);

		received_tx.initial_tx = new_initial_tx;

		Ok(received_tx)
	}
}
