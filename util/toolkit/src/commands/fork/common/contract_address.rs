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

use super::ledger_helpers_local::{self, DefaultDB, FinalizedTransaction, mn_ledger_serialize};
use crate::commands::contract_address::{ContractAddressBoth, ContractAddressError};
use hex::ToHex;

pub fn extract_contract_address(
	tx_bytes: &[u8],
) -> Result<ContractAddressBoth, ContractAddressError> {
	let mn_tx: FinalizedTransaction<DefaultDB> = mn_ledger_serialize::tagged_deserialize(tx_bytes)
		.map_err(ContractAddressError::LedgerSerializeError)?;

	let (_, deploy) = mn_tx.deploys().next().ok_or(ContractAddressError::NoContractDeployFound)?;

	let tagged = ledger_helpers_local::serialize(&deploy.address())
		.map_err(ContractAddressError::LedgerSerializeError)?
		.encode_hex();
	let untagged = ledger_helpers_local::serialize_untagged(&deploy.address())
		.map_err(ContractAddressError::LedgerSerializeError)?
		.encode_hex();

	Ok(ContractAddressBoth::new(tagged, untagged))
}
