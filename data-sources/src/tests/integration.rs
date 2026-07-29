use crate::tests::{
	configuration::IntegrationTestConfig,
	grpc_db_sync::{
		authority_selection::test_grpc_authority_selection_against_db_sync,
		cnight_observation::test_grpc_cnight_observation_against_db_sync,
		federated_authority::test_grpc_federated_authority_against_db_sync,
		mc_hash::test_grpc_mc_hash_grpc_against_db_sync,
		sidechain_rpc::test_grpc_sidechain_rpc_against_db_sync,
	},
};

#[tokio::test]
#[ignore = "requires local db-sync postgres and Acropolis gRPC"]
async fn test_grpc_datasources_against_db_sync()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let config = IntegrationTestConfig::from_env().map_err(|e| format!("config failed: {e}"))?;

	test_grpc_cnight_observation_against_db_sync(&config)
		.await
		.map_err(|e| format!("CNight observation test failed: {e}"))?;
	println!("CNight observation passed");

	test_grpc_authority_selection_against_db_sync(&config)
		.await
		.map_err(|e| format!("authority selection test failed: {e}"))?;
	println!("Authority selection passed");

	test_grpc_federated_authority_against_db_sync(&config)
		.await
		.map_err(|e| format!("federated authority test failed: {e}"))?;
	println!("Federated authority passed");

	test_grpc_mc_hash_grpc_against_db_sync(&config)
		.await
		.map_err(|e| format!("mc hash test failed: {e}"))?;
	println!("MC hash passed");

	test_grpc_sidechain_rpc_against_db_sync(&config)
		.await
		.map_err(|e| format!("sidechain rpc test failed: {e}"))?;
	println!("Sidechain RPC passed");

	Ok(())
}
