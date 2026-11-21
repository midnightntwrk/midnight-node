//! Db-Sync data source used by Partner Chain Json RPC
use crate::{
	block::BlockDataSourceImpl, metrics::McFollowerMetrics, metrics::observed_async_trait,
	row_source::BlockRowSource,
};
use pallet_sidechain_rpc::SidechainRpcDataSource;
use sidechain_domain::MainchainBlock;
use sqlx::PgPool;
use std::sync::Arc;

/// Db-Sync data source serving basic Cardano block data
pub struct SidechainRpcDataSourceImpl<R = PgPool> {
	/// [BlockDataSourceImpl] instance shared with other data sources for cache reuse.
	inner: Arc<BlockDataSourceImpl<R>>,
	/// Prometheus metrics client
	metrics_opt: Option<McFollowerMetrics>,
}

impl<R> SidechainRpcDataSourceImpl<R> {
	/// Creates a Sidechain new data source by wrapping the given instance of [BlockDataSourceImpl]
	pub fn new(inner: Arc<BlockDataSourceImpl<R>>, metrics_opt: Option<McFollowerMetrics>) -> Self {
		Self { inner, metrics_opt }
	}
}

observed_async_trait!(
	impl<R> SidechainRpcDataSource for SidechainRpcDataSourceImpl<R>
	where
		R: BlockRowSource,
	{
		async fn get_latest_block_info(
			&self,
		) -> Result<MainchainBlock, Box<dyn std::error::Error + Send + Sync>> {
			self.inner.get_latest_block_info().await
		}
	}
);
