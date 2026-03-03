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

use async_trait::async_trait;
use std::sync::Arc;

use super::ledger_helpers_local::{
	BuildIntent, DefaultDB, FromContext, LedgerContext, ProofProvider, StandardTrasactionInfo,
	WalletSeed,
};

/// An extension to help build transactions.
pub trait BuildTxsExt {
	fn funding_seed(&self) -> WalletSeed;

	fn rng_seed(&self) -> Option<[u8; 32]>;

	/// Returns a reference to the stored context.
	fn context(&self) -> &Arc<LedgerContext<DefaultDB>>;

	/// Returns a reference to the stored prover.
	fn prover(&self) -> &Arc<dyn ProofProvider<DefaultDB>>;

	/// Returns a tuple of an Arc<LedgerContext> and the StandardTransactionInfo.
	fn context_and_tx_info(
		&self,
	) -> (Arc<LedgerContext<DefaultDB>>, StandardTrasactionInfo<DefaultDB>) {
		let context = self.context().clone();
		let prover = self.prover().clone();
		let tx_info =
			StandardTrasactionInfo::new_from_context(context.clone(), prover, self.rng_seed());

		(context, tx_info)
	}
}

/// Create Intent Info
pub trait CreateIntentInfo {
	fn create_intent_info(&self) -> Box<dyn BuildIntent<DefaultDB>>;
}

/// A trait to save a Contract (serialized`Intent` Structure) into a file.
#[async_trait]
pub trait IntentToFile: CreateIntentInfo + BuildTxsExt {
	async fn generate_intent_file(&mut self, dir: &str, partial_name: &str) {
		println!("Generate intent file...");
		let (_, mut tx_info) = self.context_and_tx_info();

		let intent_info = self.create_intent_info();

		tx_info.add_intent(1, intent_info);

		tx_info.save_intents_to_file(dir, partial_name).await;
	}
}
