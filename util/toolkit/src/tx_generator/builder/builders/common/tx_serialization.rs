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

//! Version-local transaction serialization.
//!
//! Provides helpers to serialize version-local `TransactionWithContext` into
//! the version-agnostic `BuiltTransactions` output format.

use super::ledger_helpers_local::{DefaultDB, ProofMarker, SerdeTransaction, Signature};
use crate::serde_def::{BuiltTransactions, SerializedTx};

use super::ledger_helpers_local::TransactionWithContext;

/// Serialize a single SerdeTransaction into a SerializedTx.
fn serialize_tx(tx: &SerdeTransaction<Signature, ProofMarker, DefaultDB>) -> SerializedTx {
	let bytes = tx.serialize_inner().expect("failed to serialize transaction");
	let tx_hash = tx.transaction_hash().0.0;
	SerializedTx { bytes, tx_hash }
}

/// Build BuiltTransactions from a single initial transaction (no batches).
pub fn build_single(
	tx_with_context: TransactionWithContext<Signature, ProofMarker, DefaultDB>,
) -> BuiltTransactions {
	let initial_tx = serialize_tx(&tx_with_context.tx);
	BuiltTransactions { initial_tx, batches: vec![] }
}

/// Build BuiltTransactions from an initial transaction and batched transactions.
pub fn build_batched(
	initial: TransactionWithContext<Signature, ProofMarker, DefaultDB>,
	batches: Vec<Vec<TransactionWithContext<Signature, ProofMarker, DefaultDB>>>,
) -> BuiltTransactions {
	let initial_tx = serialize_tx(&initial.tx);
	let batches = batches
		.iter()
		.map(|batch| batch.iter().map(|twc| serialize_tx(&twc.tx)).collect())
		.collect();
	BuiltTransactions { initial_tx, batches }
}
