use crate::Result;
use crate::block::BlockDataSourceMock;
use async_trait::async_trait;
use sidechain_domain::*;
use sidechain_mc_hash::StableBlockByHashResult;
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
	) -> Result<Option<MainchainBlock>> {
		Ok(self
			.block_source
			.get_latest_stable_block_for(Timestamp::new(reference_timestamp.as_millis()))
			.await?)
	}

	async fn get_stable_block_for(
		&self,
		hash: McBlockHash,
		reference_timestamp: sp_timestamp::Timestamp,
	) -> Result<StableBlockByHashResult> {
		match self
			.block_source
			.get_stable_block_for(hash, Timestamp::new(reference_timestamp.as_millis()))
			.await?
		{
			Some(info) => Ok(StableBlockByHashResult::BlockStable { info }),
			None => Ok(StableBlockByHashResult::BlockNotFound),
		}
	}

	async fn get_block_by_hash(&self, hash: McBlockHash) -> Result<Option<MainchainBlock>> {
		Ok(self.block_source.get_block_by_hash(hash).await?)
	}

	async fn is_cardano_tip_fresh(&self) -> Result<bool> {
		Ok(true)
	}

	async fn is_cardano_ok(&self) -> Result<bool> {
		Ok(true)
	}
}
