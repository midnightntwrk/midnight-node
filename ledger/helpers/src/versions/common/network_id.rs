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

use super::{DefaultDB, FinalizedTransaction, Transaction, deserialize};

/// Get NetworkId from transaction bytes
pub fn network_id_from_transaction_bytes(tx_bytes: &[u8]) -> Result<String, std::io::Error> {
	let tx: FinalizedTransaction<DefaultDB> = deserialize(tx_bytes)?;
	let network_id = match tx {
		Transaction::Standard(standard_transaction) => standard_transaction.network_id,
		Transaction::ClaimRewards(claim_rewards_transaction) => {
			claim_rewards_transaction.network_id
		},
	};
	Ok(network_id)
}
