//! Main-chain follower data sources backed by the acropolis Midnight
//! indexer's gRPC API — a drop-in alternative to the db-sync/postgres
//! implementations. With the `embedded` feature the indexer itself runs
//! inside the node process (the `embedded` module). Ported from Sundae Labs'
//! parity-tested `whankinsiv/midnight-node-acropolis` fork.

// The gRPC request layer passes `tonic::Status` (176 bytes) through `Result`
// closures; these are cold, once-per-block error paths, so boxing would be
// noise. Same allowance as the upstream Sundae fork.
#![allow(clippy::result_large_err)]

#[cfg(feature = "embedded")]
pub mod embedded;
mod grpc;
mod sources;
pub use grpc::client::MidnightGrpcClient;
pub use sources::{
	authority_selection::grpc::AuthoritySelectionDataSourceGrpcImpl,
	cnight_observation::grpc::MidnightCNightObservationGrpcImpl,
	federated_authority::grpc::FederatedAuthorityObservationGrpcImpl,
	mc_hash::grpc::McHashDataSourceGrpcImpl, sidechain_rpc::grpc::SidechainRpcDataSourceGrpcImpl,
};

#[cfg(test)]
mod tests;
