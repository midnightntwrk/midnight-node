use tonic::transport::{Channel, Endpoint};

use crate::grpc::midnight_state::midnight_state_client::MidnightStateClient;

/// Shared tonic client for the Midnight indexer gRPC API.
#[derive(Clone)]
pub struct MidnightGrpcClient {
	client: MidnightStateClient<Channel>,
}

impl MidnightGrpcClient {
	/// Connect to the indexer at `endpoint` (an `http://…` URL).
	pub async fn connect(endpoint: impl AsRef<str>) -> Result<Self, tonic::transport::Error> {
		let endpoint = Endpoint::from_shared(endpoint.as_ref().to_string())?
			.tcp_nodelay(true)
			.http2_keep_alive_interval(std::time::Duration::from_secs(30))
			.keep_alive_while_idle(true);

		let channel = endpoint.connect().await?;

		Ok(Self { client: MidnightStateClient::new(channel) })
	}

	/// Cheap clone of the underlying generated client.
	pub fn inner(&self) -> MidnightStateClient<Channel> {
		self.client.clone()
	}
}
