use crate::Result;
use crate::block::BlockDataSourceMock;
use async_trait::async_trait;
use sidechain_domain::*;
use sidechain_mc_hash::{BlockByHash, LatestStableBlockForTimestamp, StableBlockForHash};
use sp_timestamp::Timestamp;
use std::sync::Arc;

/// Mock MC reference hash data source
///
/// This source serves synthetic data generated based on inputs
pub struct McHashDataSourceMock {
	block_source: Arc<BlockDataSourceMock>,
}

impl McHashDataSourceMock {
	/// Creates a new mock MC reference hash data source
	pub fn new(inner: Arc<BlockDataSourceMock>) -> Self {
		Self { block_source: inner }
	}
}

#[async_trait]
impl sidechain_mc_hash::McHashDataSource for McHashDataSourceMock {
	async fn get_latest_stable_block_for(
		&self,
		reference_timestamp: sp_timestamp::Timestamp,
	) -> Result<LatestStableBlockForTimestamp> {
		Ok(self
			.block_source
			.get_latest_stable_block_for(Timestamp::new(reference_timestamp.as_millis()))
			.await?
			.map(LatestStableBlockForTimestamp::Found)
			.unwrap_or_else(|| LatestStableBlockForTimestamp::NoStableBlockInRange {
				max_stable_block_number: McBlockNumber(0),
				min_allowed_timestamp: Timestamp::new(0),
				max_allowed_timestamp: Timestamp::new(0),
				reference_timestamp: Timestamp::new(reference_timestamp.as_millis()),
			}))
	}

	async fn get_stable_block_for(
		&self,
		hash: McBlockHash,
		reference_timestamp: sp_timestamp::Timestamp,
	) -> Result<StableBlockForHash> {
		Ok(self
			.block_source
			.get_stable_block_for(hash.clone(), Timestamp::new(reference_timestamp.as_millis()))
			.await?
			.map(StableBlockForHash::Found)
			.unwrap_or(StableBlockForHash::BlockNotFound { hash }))
	}

	async fn get_block_by_hash(&self, hash: McBlockHash) -> Result<BlockByHash> {
		Ok(self
			.block_source
			.get_block_by_hash(hash.clone())
			.await?
			.map(BlockByHash::Found)
			.unwrap_or(BlockByHash::NotFound { hash }))
	}

	async fn is_cardano_tip_fresh(&self) -> Result<bool> {
		Ok(true)
	}

	async fn is_cardano_ok(&self) -> Result<bool> {
		Ok(true)
	}
}
