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

//! Import-queue dispatch for the AURA→BABE consensus migration.
//!
//! During the migration the chain switches its block-production engine at the consensus flip, so
//! both engines' import pipelines must coexist. `sc_consensus_babe` doesn't expose a composable
//! verifier (only a whole `import_queue`), so rather than dispatch at the verifier/block-import
//! layer we dispatch one level up: [`DispatchImportQueue`] wraps the AURA import queue and a full
//! BABE import queue, and routes each incoming block to the queue for the engine active at the
//! block's **parent** — read from the runtime via
//! [`ConsensusEngineApi::active_engine`](midnight_primitives_consensus_engine::ConsensusEngineApi).
//!
//! Keying on the parent lines up the flip boundary: the flip block is the last AURA block (its
//! parent is still `ScheduledFlip` ⇒ AURA); its `on_initialize` moves the state to `Babe`, so its
//! child (parent = flip block ⇒ `Babe`) is the first block routed to the BABE queue.
//!
//! Justifications are finality (GRANDPA) and engine-agnostic; they are routed to the AURA queue,
//! whose block import owns the GRANDPA justification import.

use async_trait::async_trait;
use midnight_primitives_consensus_engine::{ActiveEngine, ConsensusEngineApi};
use sc_consensus::import_queue::{
	ImportQueue, ImportQueueService, IncomingBlock, Link, RuntimeOrigin,
};
use sp_api::ProvideRuntimeApi;
use sp_consensus::BlockOrigin;
use sp_runtime::Justifications;
use sp_runtime::traits::{Block as BlockT, Header as _, NumberFor};
use std::sync::Arc;

const LOG_TARGET: &str = "consensus-engine-dispatch";

/// Resolves which consensus engine is active at a given parent block.
pub trait EngineResolver<Block: BlockT>: Send + Sync {
	/// The engine active in the state of `parent_hash` — i.e. the engine that authored (and must
	/// verify/import) the child block.
	fn engine_at_parent(&self, parent_hash: Block::Hash) -> ActiveEngine;
}

/// [`EngineResolver`] backed by the runtime's [`ConsensusEngineApi`].
///
/// Defaults to [`ActiveEngine::Aura`] when the query fails (genesis, or a runtime predating the
/// API) — the safe pre-flip default.
pub struct RuntimeEngineResolver<C> {
	client: Arc<C>,
}

impl<C> RuntimeEngineResolver<C> {
	pub fn new(client: Arc<C>) -> Self {
		Self { client }
	}
}

impl<Block, C> EngineResolver<Block> for RuntimeEngineResolver<C>
where
	Block: BlockT,
	C: ProvideRuntimeApi<Block> + Send + Sync,
	C::Api: ConsensusEngineApi<Block>,
{
	fn engine_at_parent(&self, parent_hash: Block::Hash) -> ActiveEngine {
		match self.client.runtime_api().active_engine(parent_hash) {
			Ok(engine) => engine,
			Err(err) => {
				log::debug!(
					target: LOG_TARGET,
					"active_engine query at {parent_hash:?} failed: {err}; defaulting to AURA",
				);
				ActiveEngine::Aura
			},
		}
	}
}

/// Split an ordered batch of incoming blocks by the engine active at each block's parent,
/// preserving order within each side. Blocks without a header (which can't be routed) default to
/// the AURA queue.
fn split_by_engine<Block, R>(
	resolver: &R,
	blocks: Vec<IncomingBlock<Block>>,
) -> (Vec<IncomingBlock<Block>>, Vec<IncomingBlock<Block>>)
where
	Block: BlockT,
	R: EngineResolver<Block>,
{
	let mut aura = Vec::new();
	let mut babe = Vec::new();
	for block in blocks {
		let engine = block
			.header
			.as_ref()
			.map(|h| resolver.engine_at_parent(*h.parent_hash()))
			.unwrap_or(ActiveEngine::Aura);
		match engine {
			ActiveEngine::Aura => aura.push(block),
			ActiveEngine::Babe => babe.push(block),
		}
	}
	(aura, babe)
}

/// Service handle for [`DispatchImportQueue`]: routes submitted blocks to the AURA or BABE queue.
pub struct DispatchImportQueueService<Block: BlockT, R> {
	resolver: Arc<R>,
	aura: Box<dyn ImportQueueService<Block>>,
	babe: Box<dyn ImportQueueService<Block>>,
}

impl<Block, R> ImportQueueService<Block> for DispatchImportQueueService<Block, R>
where
	Block: BlockT,
	R: EngineResolver<Block> + 'static,
{
	fn import_blocks(&mut self, origin: BlockOrigin, blocks: Vec<IncomingBlock<Block>>) {
		let (aura, babe) = split_by_engine(&*self.resolver, blocks);
		if !aura.is_empty() {
			self.aura.import_blocks(origin, aura);
		}
		if !babe.is_empty() {
			self.babe.import_blocks(origin, babe);
		}
	}

	fn import_justifications(
		&mut self,
		who: RuntimeOrigin,
		hash: Block::Hash,
		number: NumberFor<Block>,
		justifications: Justifications,
	) {
		// Finality (GRANDPA) is engine-agnostic; the GRANDPA justification import lives under the
		// AURA queue.
		self.aura.import_justifications(who, hash, number, justifications);
	}
}

/// Import queue that dispatches blocks to the AURA or BABE import queue based on the engine active
/// at the block's parent. Both queues must ultimately write to the same backend.
pub struct DispatchImportQueue<Block: BlockT, R, Aura, Babe> {
	resolver: Arc<R>,
	aura: Aura,
	babe: Babe,
	service: DispatchImportQueueService<Block, R>,
}

impl<Block, R, Aura, Babe> DispatchImportQueue<Block, R, Aura, Babe>
where
	Block: BlockT,
	R: EngineResolver<Block> + 'static,
	Aura: ImportQueue<Block>,
	Babe: ImportQueue<Block>,
{
	pub fn new(resolver: Arc<R>, aura: Aura, babe: Babe) -> Self {
		let service = DispatchImportQueueService {
			resolver: resolver.clone(),
			aura: aura.service(),
			babe: babe.service(),
		};
		Self { resolver, aura, babe, service }
	}
}

#[async_trait]
impl<Block, R, Aura, Babe> ImportQueue<Block> for DispatchImportQueue<Block, R, Aura, Babe>
where
	Block: BlockT,
	R: EngineResolver<Block> + 'static,
	Aura: ImportQueue<Block>,
	Babe: ImportQueue<Block>,
{
	fn service(&self) -> Box<dyn ImportQueueService<Block>> {
		Box::new(DispatchImportQueueService {
			resolver: self.resolver.clone(),
			aura: self.aura.service(),
			babe: self.babe.service(),
		})
	}

	fn service_ref(&mut self) -> &mut dyn ImportQueueService<Block> {
		&mut self.service
	}

	fn poll_actions(&mut self, cx: &mut futures::task::Context, link: &dyn Link<Block>) {
		self.aura.poll_actions(cx, link);
		self.babe.poll_actions(cx, link);
	}

	async fn run(self, link: &dyn Link<Block>) {
		// Drive both engines' queue workers; each forwards its results to the shared link.
		futures::future::join(self.aura.run(link), self.babe.run(link)).await;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use midnight_node_runtime::opaque::{Block, Header};
	use sp_core::H256;
	use sp_runtime::traits::Header as HeaderT;
	use std::sync::Mutex;

	/// Resolver: a parent hash whose first byte is 1 is BABE, everything else AURA.
	struct ByteResolver;
	impl EngineResolver<Block> for ByteResolver {
		fn engine_at_parent(&self, parent_hash: <Block as BlockT>::Hash) -> ActiveEngine {
			if parent_hash.as_ref()[0] == 1 { ActiveEngine::Babe } else { ActiveEngine::Aura }
		}
	}

	/// Records the hashes of blocks submitted to it.
	#[derive(Clone, Default)]
	struct Recorder(Arc<Mutex<Vec<u8>>>);
	impl ImportQueueService<Block> for Recorder {
		fn import_blocks(&mut self, _origin: BlockOrigin, blocks: Vec<IncomingBlock<Block>>) {
			self.0.lock().unwrap().extend(blocks.iter().map(|b| b.hash.as_ref()[0]));
		}
		fn import_justifications(
			&mut self,
			_who: RuntimeOrigin,
			_hash: <Block as BlockT>::Hash,
			_number: NumberFor<Block>,
			_justifications: Justifications,
		) {
		}
	}

	fn hash_with_first_byte(byte: u8) -> H256 {
		let mut bytes = [0u8; 32];
		bytes[0] = byte;
		H256::from(bytes)
	}

	fn incoming(parent_first_byte: u8, hash_first_byte: u8) -> IncomingBlock<Block> {
		let header = Header::new(
			1,
			Default::default(),
			Default::default(),
			hash_with_first_byte(parent_first_byte),
			Default::default(),
		);
		IncomingBlock {
			hash: hash_with_first_byte(hash_first_byte),
			header: Some(header),
			body: None,
			indexed_body: None,
			justifications: None,
			origin: None,
			allow_missing_state: false,
			skip_execution: false,
			import_existing: false,
			state: None,
		}
	}

	#[test]
	fn service_splits_batch_by_parent_engine() {
		let aura = Recorder::default();
		let babe = Recorder::default();
		let mut service = DispatchImportQueueService {
			resolver: Arc::new(ByteResolver),
			aura: Box::new(aura.clone()),
			babe: Box::new(babe.clone()),
		};

		// aura-parented block (hash 10), babe-parented block (hash 20), aura-parented (hash 30).
		service.import_blocks(
			BlockOrigin::NetworkBroadcast,
			vec![incoming(0, 10), incoming(1, 20), incoming(0, 30)],
		);

		assert_eq!(*aura.0.lock().unwrap(), vec![10, 30]);
		assert_eq!(*babe.0.lock().unwrap(), vec![20]);
	}

	#[test]
	fn service_routes_headerless_block_to_aura() {
		let aura = Recorder::default();
		let babe = Recorder::default();
		let mut service = DispatchImportQueueService {
			resolver: Arc::new(ByteResolver),
			aura: Box::new(aura.clone()),
			babe: Box::new(babe.clone()),
		};

		let mut block = incoming(1, 42); // babe-parented...
		block.header = None; // ...but no header, so it can't be routed → AURA
		service.import_blocks(BlockOrigin::NetworkBroadcast, vec![block]);

		assert_eq!(*aura.0.lock().unwrap(), vec![42]);
		assert!(babe.0.lock().unwrap().is_empty());
	}
}
