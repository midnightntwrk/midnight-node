use pallet_sidechain_rpc::SidechainRpcDataSource;
use sidechain_domain::MainchainBlock;
use tonic::transport::Endpoint;

use crate::grpc::handle::IndexerHandle;
use crate::grpc::{
	midnight_state::midnight_state_client::MidnightStateClient,
	requests::sidechain_rpc_data_source_acropolis::get_latest_block,
};

/// `SidechainRpcDataSource` served by the Midnight indexer
/// (remote gRPC or in-process direct calls; see `IndexerHandle`).
pub struct SidechainRpcDataSourceGrpcImpl {
	api: IndexerHandle,
}

impl SidechainRpcDataSourceGrpcImpl {
	/// Connect to the indexer at `endpoint` (an `http://…` URL).
	pub async fn connect(
		endpoint: impl AsRef<str>,
	) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
		let endpoint_str = endpoint.as_ref();

		let endpoint = Endpoint::from_shared(endpoint_str.to_string())
			.map_err(|e| format!("Invalid gRPC endpoint `{}`: {}", endpoint_str, e))?
			.tcp_nodelay(true)
			.http2_keep_alive_interval(std::time::Duration::from_secs(30))
			.keep_alive_while_idle(true);

		let channel = endpoint.connect().await.map_err(|e| {
			format!("Failed to connect to gRPC server at `{}`: {}", endpoint_str, e)
		})?;

		Ok(Self { api: IndexerHandle::Grpc(MidnightStateClient::new(channel)) })
	}

	/// Build over the in-process indexer service (no transport, no codec).
	#[cfg(feature = "embedded")]
	pub fn direct(
		service: acropolis_module_midnight_state::grpc::service::MidnightStateService,
	) -> Self {
		let api = IndexerHandle::Direct(service);
		Self { api }
	}
}

#[async_trait::async_trait]
impl SidechainRpcDataSource for SidechainRpcDataSourceGrpcImpl {
	async fn get_latest_block_info(
		&self,
	) -> Result<MainchainBlock, Box<dyn std::error::Error + Send + Sync>> {
		get_latest_block(&self.api)
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
	}
}
