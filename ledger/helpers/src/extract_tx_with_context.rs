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

#[cfg(feature = "can-panic")]
use crate::fork::raw_block_data::{RawTransaction, SerializedTx};

#[cfg(feature = "can-panic")]
pub fn extract_tx_with_context_ledger_9(bytes: &[u8]) -> (Vec<u8>, crate::ledger_9::BlockContext) {
	let serialized_tx: SerializedTx =
		serde_json::from_slice(bytes).expect("failed to deserialize as SerializedTx");
	let RawTransaction::Midnight(tx_bytes) = serialized_tx.tx else {
		panic!("expected test to run against midnight transaction");
	};
	let block_context = serialized_tx.context;

	(tx_bytes, block_context)
}

#[cfg(feature = "can-panic")]
pub fn extract_tx_with_context_ledger_8(bytes: &[u8]) -> (Vec<u8>, crate::ledger_8::BlockContext) {
	use crate::fork::raw_block_data::RawTransaction;

	let serialized_tx: SerializedTx =
		serde_json::from_slice(bytes).expect("failed to deserialize as SerializedTx");
	let RawTransaction::Midnight(tx_bytes) = serialized_tx.tx else {
		panic!("expected test to run against midnight transaction");
	};

	let block_context = crate::ledger_8::BlockContext {
		tblock: serialized_tx.context.tblock,
		tblock_err: serialized_tx.context.tblock_err,
		parent_block_hash: serialized_tx.context.parent_block_hash,
		last_block_time: serialized_tx.context.last_block_time,
	};

	(tx_bytes, block_context)
}

#[cfg(feature = "can-panic")]
pub fn extract_tx_with_context_ledger_7(bytes: &[u8]) -> (Vec<u8>, crate::ledger_7::BlockContext) {
	use crate::fork::raw_block_data::RawTransaction;
	use crate::ledger_7::base_crypto::{hash::HashOutput, time::Timestamp};

	let serialized_tx: SerializedTx =
		serde_json::from_slice(bytes).expect("failed to deserialize as SerializedTx");
	let RawTransaction::Midnight(tx_bytes) = serialized_tx.tx else {
		panic!("expected test to run against midnight transaction");
	};

	let block_context = crate::ledger_7::BlockContext {
		tblock: Timestamp::from_secs(serialized_tx.context.tblock.to_secs()),
		tblock_err: serialized_tx.context.tblock_err,
		parent_block_hash: HashOutput(serialized_tx.context.parent_block_hash.0),
	};

	(tx_bytes, block_context)
}
