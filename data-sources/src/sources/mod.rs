pub mod authority_selection;
pub mod cnight_observation;
pub mod federated_authority;
pub mod mc_hash;
pub mod sidechain_rpc;

#[derive(thiserror::Error, Debug)]
/// Errors shared by the indexer-backed data sources.
pub enum AcropolisDataSourceError {
	#[error("Error querying gRPC `{0}`")]
	GRPCQueryError(tonic::Status),
}
