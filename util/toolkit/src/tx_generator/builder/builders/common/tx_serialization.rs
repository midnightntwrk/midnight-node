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

//! Version-local transaction serialization.
//!
//! Provides helpers to serialize version-local `TransactionWithContext` into
//! the version-agnostic `SerializedTxBatches` output format.

use super::super::serialize_tx;
use super::ledger_helpers_local::{DefaultDB, ProofMarker, Signature};
use midnight_node_ledger_helpers::fork::raw_block_data::SerializedTxBatches;

use super::ledger_helpers_local::TransactionWithContext;

/// Build SerializedTxBatches from a single initial transaction (no batches).
pub fn build_single(
	tx_with_context: TransactionWithContext<Signature, ProofMarker, DefaultDB>,
) -> SerializedTxBatches {
	let initial_tx = serialize_tx(&tx_with_context);
	SerializedTxBatches { batches: vec![vec![initial_tx]] }
}

/// Build SerializedTxBatches from an initial transaction and batched transactions.
pub fn build_batched(
	batches: Vec<Vec<TransactionWithContext<Signature, ProofMarker, DefaultDB>>>,
) -> SerializedTxBatches {
	let batches = batches
		.iter()
		.map(|batch| batch.iter().map(|twc| serialize_tx(&twc)).collect())
		.collect();
	SerializedTxBatches { batches }
}
