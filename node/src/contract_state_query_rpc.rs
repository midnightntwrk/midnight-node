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

//! RPC endpoint for reading individual fields of a contract's state.
//!
//! `midnight_contractState` returns the whole state, too big to fetch for a large contract; this
//! walks the state tree along caller-supplied paths instead. Like `midnight_ledgerStats` it
//! resolves `pallet_midnight::StateKey` node-side, so it needs no runtime API.

use jsonrpsee::{
	core::{RpcResult, async_trait},
	proc_macros::rpc,
	types::error::{ErrorObject, ErrorObjectOwned, INTERNAL_ERROR_CODE, INVALID_PARAMS_CODE},
};
use midnight_node_ledger::QueryContractStateError;
use midnight_node_ledger_helpers::{AlignedValue, deserialize, serialize};
use sc_client_api::{Backend, StorageProvider};
use serde::{Deserialize, Serialize};
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use std::{marker::PhantomData, sync::Arc};

use crate::warp_ledger_sync::read_state_key;

pub const MAX_STATE_QUERIES: usize = 100;

/// Bounds the decode work a single request can cause before path navigation starts. Compact map
/// keys are small fixed-size values, so this leaves ample headroom.
const MAX_KEY_BYTES: usize = 512;

/// An `AlignedValue` as 0x-prefixed hex of its tagged serialization
/// (`midnight:fab-aligned-value[v1]:` followed by the untagged encoding).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HexAlignedValue(pub AlignedValue);

impl Serialize for HexAlignedValue {
	fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		let bytes = serialize(&self.0).map_err(serde::ser::Error::custom)?;
		serializer.serialize_str(&format!("0x{}", hex::encode(bytes)))
	}
}

impl<'de> Deserialize<'de> for HexAlignedValue {
	fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let s = String::deserialize(deserializer)?;
		let hex_str = s
			.strip_prefix("0x")
			.ok_or_else(|| serde::de::Error::custom("path key must be 0x-prefixed hex"))?;
		if hex_str.len() > MAX_KEY_BYTES * 2 {
			return Err(serde::de::Error::custom(format!(
				"path key exceeds the maximum of {MAX_KEY_BYTES} bytes"
			)));
		}
		let bytes = hex::decode(hex_str).map_err(serde::de::Error::custom)?;
		deserialize(&bytes[..]).map(HexAlignedValue).map_err(serde::de::Error::custom)
	}
}

/// Outcome of one path: `{"value": "0x…"}` for a cell, `{"value": null}` for a `Null` leaf (which
/// includes a map key that is absent), or `{"error": "…"}` if the path could not be followed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StateQueryResult {
	Value(Option<HexAlignedValue>),
	Error(String),
}

#[derive(Debug)]
pub enum ContractStateQueryError {
	TooManyQueries(usize),
	BadContractAddress,
	UnknownBlock,
	/// On a pruned node this is the expected outcome for a block outside the retained state window.
	StateKeyUnavailable(String),
	NoStateKey,
	ContractNotPresent,
	LedgerUnavailable(String),
}

impl std::fmt::Display for ContractStateQueryError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::TooManyQueries(got) => {
				write!(f, "too many queries: got {got}, maximum is {MAX_STATE_QUERIES}")
			},
			Self::BadContractAddress => write!(f, "contract address must be hex"),
			Self::UnknownBlock => write!(f, "requested block is unknown to this node"),
			Self::StateKeyUnavailable(reason) => write!(
				f,
				"unable to read ledger StateKey at the requested block \
				 (a pruned node has no state for older blocks): {reason}"
			),
			Self::NoStateKey => write!(f, "pallet StateKey not present at the requested block"),
			Self::ContractNotPresent => write!(f, "contract not present at the requested block"),
			Self::LedgerUnavailable(reason) => write!(f, "unable to read ledger state: {reason}"),
		}
	}
}

impl std::error::Error for ContractStateQueryError {}

impl From<ContractStateQueryError> for ErrorObjectOwned {
	fn from(value: ContractStateQueryError) -> Self {
		let code = match value {
			ContractStateQueryError::StateKeyUnavailable(_)
			| ContractStateQueryError::LedgerUnavailable(_) => INTERNAL_ERROR_CODE,
			_ => INVALID_PARAMS_CODE,
		};
		ErrorObject::owned(code, value.to_string(), None::<()>)
	}
}

#[rpc(server)]
pub trait ContractStateQueryApi<BlockHash> {
	/// Resolves each path in `paths` against the state of `contract_address` (hex), returning one
	/// result per path in the same order. Each path key is interpreted, as by the VM's `idx`
	/// instruction, as an array index, map key, or merkle tree position depending on the node it
	/// is applied to. If `at` is `None`, the best block is used.
	#[method(name = "midnight_queryContractState")]
	async fn query_contract_state(
		&self,
		contract_address: String,
		paths: Vec<Vec<HexAlignedValue>>,
		at: Option<BlockHash>,
	) -> RpcResult<Vec<StateQueryResult>>;
}

pub struct ContractStateQueryRpc<C, Block: BlockT, BE> {
	client: Arc<C>,
	/// Whether the ledger arena uses the unified ParityDb layout — selects the DB instantiation.
	unified: bool,
	_phantom: PhantomData<(Block, BE)>,
}

impl<C, Block: BlockT, BE> ContractStateQueryRpc<C, Block, BE> {
	pub fn new(client: Arc<C>, unified: bool) -> Self {
		Self { client, unified, _phantom: PhantomData }
	}
}

impl<C, Block, BE> ContractStateQueryRpc<C, Block, BE>
where
	Block: BlockT,
	BE: Backend<Block> + Send + Sync + 'static,
	C: HeaderBackend<Block> + StorageProvider<Block, BE> + Send + Sync + 'static,
{
	fn query(
		client: &C,
		unified: bool,
		address: &[u8],
		paths: &[Vec<AlignedValue>],
		at: Block::Hash,
	) -> Result<Vec<StateQueryResult>, ContractStateQueryError> {
		if client
			.header(at)
			.map_err(|e| ContractStateQueryError::StateKeyUnavailable(e.to_string()))?
			.is_none()
		{
			return Err(ContractStateQueryError::UnknownBlock);
		}

		let state_key = read_state_key::<Block, C, BE>(client, at)
			.map_err(|e| ContractStateQueryError::StateKeyUnavailable(e.to_string()))?
			.ok_or(ContractStateQueryError::NoStateKey)?;

		let results =
			midnight_node_ledger::query_contract_state(unified, &state_key, address, paths)
				.map_err(|e| match e {
					QueryContractStateError::ContractNotPresent => {
						ContractStateQueryError::ContractNotPresent
					},
					QueryContractStateError::Ledger(reason) => {
						ContractStateQueryError::LedgerUnavailable(reason)
					},
				})?;

		Ok(results
			.into_iter()
			.map(|r| match r {
				Ok(value) => StateQueryResult::Value(value.map(HexAlignedValue)),
				Err(e) => StateQueryResult::Error(e),
			})
			.collect())
	}
}

#[async_trait]
impl<C, Block, BE> ContractStateQueryApiServer<Block::Hash> for ContractStateQueryRpc<C, Block, BE>
where
	Block: BlockT,
	BE: Backend<Block> + Send + Sync + 'static,
	C: HeaderBackend<Block> + StorageProvider<Block, BE> + Send + Sync + 'static,
{
	async fn query_contract_state(
		&self,
		contract_address: String,
		paths: Vec<Vec<HexAlignedValue>>,
		at: Option<Block::Hash>,
	) -> RpcResult<Vec<StateQueryResult>> {
		if paths.len() > MAX_STATE_QUERIES {
			return Err(ContractStateQueryError::TooManyQueries(paths.len()).into());
		}
		let address = hex::decode(&contract_address)
			.map_err(|_| ContractStateQueryError::BadContractAddress)?;
		let paths: Vec<Vec<AlignedValue>> =
			paths.into_iter().map(|p| p.into_iter().map(|k| k.0).collect()).collect();
		let at = at.unwrap_or_else(|| self.client.info().best_hash);

		// Arena reads can block on disk and on the arena's process-global locks, so keep them off
		// the async workers.
		let client = self.client.clone();
		let unified = self.unified;
		let results = tokio::task::spawn_blocking(move || {
			Self::query(&client, unified, &address, &paths, at)
		})
		.await
		.map_err(|e| {
			ContractStateQueryError::LedgerUnavailable(format!("state query task failed: {e}"))
		})??;

		Ok(results)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn hex_aligned_value_round_trips_tagged() {
		let key = HexAlignedValue(AlignedValue::from(3u8));
		let json = serde_json::to_string(&key).unwrap();
		let hex_tag = hex::encode("midnight:fab-aligned-value[v1]:");
		assert!(json.starts_with(&format!("\"0x{hex_tag}")), "{json}");
		assert_eq!(serde_json::from_str::<HexAlignedValue>(&json).unwrap(), key);
	}

	#[test]
	fn hex_aligned_value_rejects_untagged_and_trailing_bytes() {
		let tagged = serialize(&AlignedValue::from(3u8)).unwrap();
		let untagged = &tagged["midnight:fab-aligned-value[v1]:".len()..];
		let reject = |bytes: &[u8]| {
			serde_json::from_str::<HexAlignedValue>(&format!("\"0x{}\"", hex::encode(bytes)))
				.is_err()
		};
		assert!(reject(untagged));
		assert!(reject(&[tagged.as_slice(), &[0]].concat()));
	}

	#[test]
	fn results_serialize_as_value_or_error() {
		let results = vec![
			StateQueryResult::Value(None),
			StateQueryResult::Error("array index 9 out of bounds".into()),
		];
		assert_eq!(
			serde_json::to_string(&results).unwrap(),
			r#"[{"value":null},{"error":"array index 9 out of bounds"}]"#
		);
	}
}
