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

use midnight_node_ledger_helpers::fork::raw_block_data::RawTransaction;
use serde::{Deserialize, Serialize};

use super::ledger_helpers_local::{
	self, DefaultDB, PureGeneratorPedersen, SystemTransaction, deserialize,
};

type Signature = ledger_helpers_local::Signature;
type ProofMarker = ledger_helpers_local::ProofMarker;
type Transaction =
	ledger_helpers_local::Transaction<Signature, ProofMarker, PureGeneratorPedersen, DefaultDB>;

#[derive(Debug, Serialize, Deserialize)]
pub struct ShowTransaction {
	pub tx_type: String,
	pub size_bytes: usize,
	#[serde(with = "hex")]
	pub hash: [u8; 32],
	pub debug_str: String,
}

impl TryFrom<&RawTransaction> for ShowTransaction {
	type Error = std::io::Error;

	fn try_from(value: &RawTransaction) -> Result<Self, Self::Error> {
		let size_bytes = value.as_bytes().len();
		match value {
			RawTransaction::Midnight(tx_bytes) => {
				let tx: Transaction = deserialize(tx_bytes.as_slice())?;
				let hash = tx.transaction_hash().0.0;
				Ok(ShowTransaction {
					tx_type: "Midnight".to_string(),
					size_bytes,
					hash,
					debug_str: format!("{tx:#?}"),
				})
			},
			RawTransaction::System(tx_bytes) => {
				let tx: SystemTransaction = deserialize(tx_bytes.as_slice())?;
				let hash = tx.transaction_hash().0.0;
				Ok(ShowTransaction {
					tx_type: "Midnight".to_string(),
					size_bytes,
					hash,
					debug_str: format!("{tx:#?}"),
				})
			},
		}
	}
}
