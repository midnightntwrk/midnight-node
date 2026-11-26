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

//! What snapshot mode needs from the chain.

use crate::service::{FullBackend, FullClient};
use midnight_node_runtime::opaque::Block;
use sc_client_api::{Backend, BlockBackend};
use sp_api::{ApiExt, Core, ProvideRuntimeApi};
use sp_blockchain::{Backend as _, HeaderBackend};
use sp_core::{H256, traits::CallContext};
use sp_runtime::traits::{Block as BlockT, Header as _};
use std::sync::Arc;

pub(crate) trait Chain {
	fn genesis(&self) -> H256;
	fn finalized(&self) -> (u64, H256);
	/// A block's number and parent; `None` if the block is unknown.
	fn header(&self, hash: H256) -> Result<Option<(u64, H256)>, String>;
	fn leaves(&self) -> Result<Vec<H256>, String>;
	/// `pallet_midnight::StateKey` at a block; `None` if the block has no state.
	fn state_key(&self, hash: H256) -> Result<Option<Vec<u8>>, String>;
	/// Execute a block on its parent as block import does, discarding the Substrate result.
	fn execute(&self, hash: H256) -> Result<(), String>;
}

pub(crate) struct ClientChain {
	pub(crate) client: Arc<FullClient>,
	pub(crate) backend: Arc<FullBackend>,
}

impl Chain for ClientChain {
	fn genesis(&self) -> H256 {
		self.client.info().genesis_hash
	}

	fn finalized(&self) -> (u64, H256) {
		let info = self.client.info();
		(info.finalized_number.into(), info.finalized_hash)
	}

	fn header(&self, hash: H256) -> Result<Option<(u64, H256)>, String> {
		let header = self.client.header(hash).map_err(|e| e.to_string())?;
		Ok(header.map(|h| ((*h.number()).into(), *h.parent_hash())))
	}

	fn leaves(&self) -> Result<Vec<H256>, String> {
		self.backend.blockchain().leaves().map_err(|e| e.to_string())
	}

	fn state_key(&self, hash: H256) -> Result<Option<Vec<u8>>, String> {
		if !self.backend.have_state_at(hash, self.header(hash)?.map_or(0, |h| h.0 as u32)) {
			return Ok(None);
		}
		crate::warp_ledger_sync::read_state_key::<Block, FullClient, FullBackend>(
			&self.client,
			hash,
		)
		.map_err(|e| e.to_string())
	}

	fn execute(&self, hash: H256) -> Result<(), String> {
		let block = self
			.client
			.block(hash)
			.map_err(|e| e.to_string())?
			.ok_or_else(|| format!("no body for {hash:?}"))?
			.block;
		let (mut header, extrinsics) = block.deconstruct();
		// As block import does: the consensus engine strips the seal before execution.
		header.digest_mut().logs.retain(|item| item.as_seal().is_none());
		let parent = *header.parent_hash();
		let mut api = self.client.runtime_api();
		api.set_call_context(CallContext::Onchain { import: true });
		api.execute_block(parent, Block::new(header, extrinsics).into())
			.map_err(|e| format!("executing {hash:?}: {e}"))
	}
}
