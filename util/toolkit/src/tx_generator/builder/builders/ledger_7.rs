// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#[path = "common"]
#[allow(clippy::duplicate_mod)]
pub mod inner {
	pub use midnight_node_ledger_helpers::ledger_7 as ledger_helpers_local;

	mod batches;
	mod build_txs_ext;
	mod claim_rewards;
	mod contract_call;
	mod contract_custom;
	mod contract_deploy;
	mod contract_maintenance;
	mod deregister_dust_address;
	mod do_nothing;
	mod register_dust_address;
	pub mod single_tx;
	pub mod transactions;
	mod tx_serialization;
	pub mod type_convert;

	pub use batches::*;
	pub use build_txs_ext::*;
	pub use claim_rewards::*;
	pub use contract_call::*;
	pub use contract_custom::*;
	pub use contract_deploy::*;
	pub use contract_maintenance::*;
	pub use deregister_dust_address::*;
	pub use do_nothing::*;
	pub use register_dust_address::*;
}
pub use inner::*;

use midnight_node_ledger_helpers::fork::raw_block_data::SerializedTx;
use midnight_node_ledger_helpers::ledger_7::{
	DefaultDB, ProofMarker, Signature, TransactionWithContext,
};

pub fn serialize_tx(
	tx: &TransactionWithContext<Signature, ProofMarker, DefaultDB>,
) -> SerializedTx {
	let context =
		midnight_node_ledger_helpers::fork::fork_7_to_8::block_context_7_to_8(&tx.block_context);
	let raw_tx = transactions::from_serde_tx(&tx.tx);
	let tx_hash = tx.tx.transaction_hash().0.0;
	SerializedTx { tx: raw_tx, context, tx_hash }
}
