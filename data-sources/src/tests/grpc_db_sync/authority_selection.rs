use std::error::Error;

use authority_selection_inherents::AuthoritySelectionDataSource;
use midnight_primitives_mainchain_follower::CandidatesDataSourceImpl;

use crate::{
	AuthoritySelectionDataSourceGrpcImpl,
	tests::{
		common::{STANDARD_POOL_CFG, get_connection},
		configuration::IntegrationTestConfig,
	},
};

pub async fn test_grpc_authority_selection_against_db_sync(
	config: &IntegrationTestConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let db_sync = create_dbsync_authority_selection_source(config).await?;
	let grpc = AuthoritySelectionDataSourceGrpcImpl::connect(&config.grpc_endpoint).await?;

	test_parameters_match(&grpc, &db_sync, config).await?;
	test_epoch_candidates_match(&grpc, &db_sync, config).await?;
	test_epoch_nonce_match(&grpc, &db_sync, config).await?;
	test_data_epoch_match(&grpc, &db_sync, config).await?;

	Ok(())
}

async fn test_parameters_match(
	grpc: &AuthoritySelectionDataSourceGrpcImpl,
	db_sync: &CandidatesDataSourceImpl,
	cfg: &IntegrationTestConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	assert_eq!(
		db_sync
			.get_ariadne_parameters(
				cfg.params_config.epoch_number,
				cfg.d_parameter_policy_id.clone(),
				cfg.permissioned_candidates_policy.clone(),
			)
			.await?,
		grpc.get_ariadne_parameters(
			cfg.params_config.epoch_number,
			cfg.d_parameter_policy_id.clone(),
			cfg.permissioned_candidates_policy.clone(),
		)
		.await?,
		"ariadne parameter mismatch"
	);

	Ok(())
}

async fn test_epoch_candidates_match(
	grpc: &AuthoritySelectionDataSourceGrpcImpl,
	db_sync: &CandidatesDataSourceImpl,
	cfg: &IntegrationTestConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	assert_eq!(
		db_sync
			.get_candidates(cfg.params_config.epoch_number, cfg.committee_candidate_address.clone())
			.await?,
		grpc.get_candidates(
			cfg.params_config.epoch_number,
			cfg.committee_candidate_address.clone()
		)
		.await?,
		"epoch candidates mismatch"
	);

	Ok(())
}

async fn test_epoch_nonce_match(
	grpc: &AuthoritySelectionDataSourceGrpcImpl,
	db_sync: &CandidatesDataSourceImpl,
	cfg: &IntegrationTestConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	assert_eq!(
		db_sync.get_epoch_nonce(cfg.params_config.epoch_number).await?,
		grpc.get_epoch_nonce(cfg.params_config.epoch_number).await?,
		"epoch nonce mismatch"
	);

	Ok(())
}

async fn test_data_epoch_match(
	grpc: &AuthoritySelectionDataSourceGrpcImpl,
	db_sync: &CandidatesDataSourceImpl,
	cfg: &IntegrationTestConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	assert_eq!(
		db_sync.data_epoch(cfg.params_config.epoch_number).await?,
		grpc.data_epoch(cfg.params_config.epoch_number).await?,
		"data epoch mismatch"
	);

	Ok(())
}

async fn create_dbsync_authority_selection_source(
	config: &IntegrationTestConfig,
) -> Result<CandidatesDataSourceImpl, Box<dyn Error + Send + Sync + 'static>> {
	CandidatesDataSourceImpl::new(
		get_connection(&config.postgres_uri, STANDARD_POOL_CFG, true).await?,
		None,
	)
	.await
}
