use std::error::Error;

use midnight_primitives_cnight_observation::{CardanoPosition, TimestampUnixMillis};
use midnight_primitives_mainchain_follower::{
	MidnightCNightObservationDataSource, MidnightCNightObservationDataSourceImpl,
};
use sidechain_domain::McBlockHash;

use crate::{
	MidnightCNightObservationGrpcImpl,
	tests::{
		common::{CNIGHT_OBSERVATION_POOL_CFG, get_connection},
		configuration::IntegrationTestConfig,
	},
};

/// Generous over-fetch bound for parity tests; only affects the db-sync
/// side's SQL row limit, the indexer ignores it.
const UTXO_OVERESTIMATE: usize = 10_000;

pub async fn test_grpc_cnight_observation_against_db_sync(
	config: &IntegrationTestConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let start_position = CardanoPosition {
		block_hash: McBlockHash([0; 32]),
		block_number: 0,
		block_timestamp: TimestampUnixMillis(0),
		tx_index_in_block: 0,
	};

	let db = create_dbsync_cnight_observation_source(&config.postgres_uri).await?;
	let grpc = MidnightCNightObservationGrpcImpl::connect(config.grpc_endpoint.clone()).await?;

	let db_utxos = db
		.get_utxos_up_to_capacity(
			&config.cnight_config,
			&start_position,
			config.params_config.tip.clone(),
			config.params_config.tx_capacity,
			UTXO_OVERESTIMATE,
		)
		.await?;
	let grpc_utxos = grpc
		.get_utxos_up_to_capacity(
			&config.cnight_config,
			&start_position,
			config.params_config.tip.clone(),
			config.params_config.tx_capacity,
			UTXO_OVERESTIMATE,
		)
		.await?;

	assert_eq!(db_utxos.start, grpc_utxos.start, "start_position mismatch");
	assert_eq!(db_utxos.utxos.len(), grpc_utxos.utxos.len(), "UTxO length mismatch");
	for (i, (db, grpc)) in db_utxos.utxos.iter().zip(&grpc_utxos.utxos).enumerate() {
		if db != grpc {
			panic!("UTxO mismatch at index {i}");
		}
	}
	assert_eq!(db_utxos.end, grpc_utxos.end, "end_position mismatch");

	Ok(())
}

async fn create_dbsync_cnight_observation_source(
	connection_string: &str,
) -> Result<MidnightCNightObservationDataSourceImpl, Box<dyn Error + Send + Sync + 'static>> {
	Ok(MidnightCNightObservationDataSourceImpl::new(
		get_connection(connection_string, CNIGHT_OBSERVATION_POOL_CFG, true).await?,
		None,
		1000,
	))
}
