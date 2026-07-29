use std::error::Error;

use crate::{
	FederatedAuthorityObservationGrpcImpl,
	tests::{
		common::{STANDARD_POOL_CFG, get_connection},
		configuration::IntegrationTestConfig,
	},
};
use midnight_primitives_mainchain_follower::{
	FederatedAuthorityObservationDataSource, FederatedAuthorityObservationDataSourceImpl,
};

pub async fn test_grpc_federated_authority_against_db_sync(
	config: &IntegrationTestConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let db_sync = create_dbsync_federated_authority_source(&config.postgres_uri).await?;
	let grpc = FederatedAuthorityObservationGrpcImpl::connect(&config.grpc_endpoint).await?;

	assert_eq!(
		db_sync
			.get_federated_authority_data(&config.authority_config, &config.params_config.tip)
			.await?,
		grpc.get_federated_authority_data(&config.authority_config, &config.params_config.tip)
			.await?,
		"federated authority data mismatch"
	);

	Ok(())
}

async fn create_dbsync_federated_authority_source(
	connection_string: &str,
) -> Result<FederatedAuthorityObservationDataSourceImpl, Box<dyn Error + Send + Sync + 'static>> {
	Ok(FederatedAuthorityObservationDataSourceImpl::new(
		get_connection(connection_string, STANDARD_POOL_CFG, true).await?,
		None,
		1000,
	))
}
