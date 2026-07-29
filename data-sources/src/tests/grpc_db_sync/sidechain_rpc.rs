use std::{error::Error, sync::Arc};

use midnight_primitives_mainchain_follower::partner_chains_db_sync_data_sources::{
	BlockDataSourceImpl, SidechainRpcDataSourceImpl,
};
use pallet_sidechain_rpc::SidechainRpcDataSource;

use crate::{
	SidechainRpcDataSourceGrpcImpl,
	tests::{
		common::{STANDARD_POOL_CFG, get_connection},
		configuration::IntegrationTestConfig,
	},
};

pub async fn test_grpc_sidechain_rpc_against_db_sync(
	config: &IntegrationTestConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let db_sync = create_dbsync_sidechain_rpc_source(config).await?;
	let grpc = SidechainRpcDataSourceGrpcImpl::connect(&config.grpc_endpoint).await?;

	let db_sync_block_number = db_sync.get_latest_block_info().await?.number.0;
	let grpc_block_number = grpc.get_latest_block_info().await?.number.0;
	let diff = db_sync_block_number.abs_diff(grpc_block_number);

	assert!(
		diff <= 50,
		"Block numbers differ too much: db_sync={}, grpc={}, diff={}",
		db_sync_block_number,
		grpc_block_number,
		diff
	);

	Ok(())
}

async fn create_dbsync_sidechain_rpc_source(
	config: &IntegrationTestConfig,
) -> Result<SidechainRpcDataSourceImpl, Box<dyn Error + Send + Sync + 'static>> {
	Ok(SidechainRpcDataSourceImpl::new(
		Arc::new(BlockDataSourceImpl::from_config(
			get_connection(&config.postgres_uri, STANDARD_POOL_CFG, true).await?,
			config.block_source_config.clone(),
			&config.epoch_config,
		)),
		None,
	))
}
