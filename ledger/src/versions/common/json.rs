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

use super::{
	LOG_TARGET,
	api::{ContractState, ContractStateValue},
	ledger_storage_local,
	types::{LedgerApiError, SerializationError},
};
use crate::json::transform;
use ledger_storage_local::db::DB;
use serde_json::{Value, json};

pub fn contract_state_json<D: DB>(
	contract_state: ContractState<D>,
) -> Result<Vec<u8>, LedgerApiError> {
	let handle_error = |e| {
		log::error!(target: LOG_TARGET, "Error serializing: the Contract State to JSON: {:?}", e);
		LedgerApiError::Serialization(SerializationError::ContractStateToJson)
	};
	let data = match contract_state.data {
		ContractStateValue::Null => Value::Null,
		ContractStateValue::Cell(_) => json!("cell"),
		ContractStateValue::Map(_) => json!("map"),
		ContractStateValue::Array(_) => json!("array"),
		ContractStateValue::BoundedMerkleTree(_) => json!("merkle tree"),
		_ => json!("unknown"),
	};
	let operations: Value =
		serde_json::to_value(contract_state.operations).map_err(handle_error)?;
	let json = json!({
		"data": data,
		"operations": operations,
	});
	let transformed_json = transform(json);

	Ok(serde_json::to_string(&transformed_json).map_err(handle_error)?.into_bytes())
}
