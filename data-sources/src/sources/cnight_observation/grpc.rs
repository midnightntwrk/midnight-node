use cardano_serialization_lib::Address;
use midnight_primitives_cnight_observation::{CNightAddresses, CardanoPosition, ObservedUtxos};
use midnight_primitives_mainchain_follower::MidnightCNightObservationDataSource;
use sidechain_domain::McBlockHash;
use tonic::transport::Endpoint;

use crate::grpc::handle::IndexerHandle;
use crate::{
	grpc::{
		midnight_state::midnight_state_client::MidnightStateClient,
		requests::cnight_observation_acropolis::get_utxo_events,
	},
	sources::AcropolisDataSourceError,
};

#[derive(thiserror::Error, Debug)]
pub enum AcropolisCNightObservationDataSourceError {
	#[error("Error extracting network id from Cardano address")]
	CardanoNetworkError(String),
	#[error("Invalid value for mapping validator address")]
	MappingValidatorInvalidAddress(String),
	#[error("Error querying gRPC `{0}`")]
	GRPCQueryError(tonic::Status),
}

/// `MidnightCNightObservationDataSource` served by the Midnight indexer
/// (remote gRPC or in-process direct calls; see `IndexerHandle`).
#[derive(Clone)]
pub struct MidnightCNightObservationGrpcImpl {
	api: IndexerHandle,
}

impl MidnightCNightObservationGrpcImpl {
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
impl MidnightCNightObservationDataSource for MidnightCNightObservationGrpcImpl {
	async fn get_utxos_up_to_capacity(
		&self,
		config: &CNightAddresses,
		start_position: &CardanoPosition,
		current_tip: McBlockHash,
		tx_capacity: usize,
		// The over-fetch row bound only exists to limit the db-sync SQL
		// window pull; the indexer truncates by tx capacity from in-memory
		// indexes directly, so there is no row limit to bound here.
		_utxo_overestimate: usize,
	) -> Result<ObservedUtxos, Box<dyn std::error::Error + Send + Sync>> {
		let cardano_network = get_cardano_network(config)?;

		let response =
			get_utxo_events(&self.api, cardano_network, start_position, tx_capacity, current_tip)
				.await
				.map_err(AcropolisDataSourceError::GRPCQueryError)?;

		let start = start_position.clone();
		let end = response.next_position;

		Ok(ObservedUtxos { start, end, utxos: response.utxos })
	}
}

#[allow(clippy::result_large_err)]
fn get_cardano_network(
	config: &CNightAddresses,
) -> Result<u8, AcropolisCNightObservationDataSourceError> {
	let addr = Address::from_bech32(&config.mapping_validator_address).map_err(|e| {
		AcropolisCNightObservationDataSourceError::MappingValidatorInvalidAddress(e.to_string())
	})?;

	addr.network_id().map_err(|_| {
		AcropolisCNightObservationDataSourceError::CardanoNetworkError(
			config.mapping_validator_address.clone(),
		)
	})
}
