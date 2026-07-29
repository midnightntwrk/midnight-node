use midnight_primitives_federated_authority_observation::{
	FederatedAuthorityData, FederatedAuthorityObservationConfig,
};
use sidechain_domain::McBlockHash;
use tonic::transport::Endpoint;

use crate::grpc::handle::IndexerHandle;
use crate::grpc::midnight_state::midnight_state_client::MidnightStateClient;
use crate::grpc::requests::federated_authority_data_source_acropolis::get_block_number_by_hash;
use crate::grpc::requests::federated_authority_data_source_acropolis::get_federated_authority_data;
use midnight_primitives_mainchain_follower::FederatedAuthorityObservationDataSource;

/// `FederatedAuthorityObservationDataSource` served by the Midnight indexer
/// (remote gRPC or in-process direct calls; see `IndexerHandle`).
pub struct FederatedAuthorityObservationGrpcImpl {
	api: IndexerHandle,
}

impl FederatedAuthorityObservationGrpcImpl {
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
impl FederatedAuthorityObservationDataSource for FederatedAuthorityObservationGrpcImpl {
	async fn get_federated_authority_data(
		&self,
		_config: &FederatedAuthorityObservationConfig,
		mc_block_hash: &McBlockHash,
	) -> Result<FederatedAuthorityData, Box<dyn std::error::Error + Send + Sync>> {
		let block_number = get_block_number_by_hash(&self.api, mc_block_hash.clone()).await?;

		get_federated_authority_data(&self.api, block_number, mc_block_hash.clone()).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		grpc::midnight_state::{
			AriadneParametersRequest, AriadneParametersResponse, AssetCreatesRequest,
			AssetCreatesResponse, AssetSpendsRequest, AssetSpendsResponse, BlockByHashRequest,
			BlockByHashResponse, CouncilDatumRequest, CouncilDatumResponse, DeregistrationsRequest,
			DeregistrationsResponse, EpochCandidatesRequest, EpochCandidatesResponse,
			EpochNonceRequest, EpochNonceResponse, LatestBlockRequest, LatestBlockResponse,
			LatestStableBlockRequest, LatestStableBlockResponse, RegistrationsRequest,
			RegistrationsResponse, StableBlockRequest, StableBlockResponse,
			TechnicalCommitteeDatumRequest, TechnicalCommitteeDatumResponse, UtxoEventsRequest,
			UtxoEventsResponse,
			midnight_state_server::{MidnightState, MidnightStateServer},
		},
		grpc::requests::federated_authority_data_source_acropolis::empty_authorities_data,
	};
	use cardano_serialization_lib::{BigInt, PlutusData, PlutusList, PlutusMap, PlutusMapValues};
	use midnight_primitives_federated_authority_observation::{
		AuthBodyConfig, AuthoritiesData, AuthorityMemberPublicKey,
	};
	use sidechain_domain::PolicyId;
	use tokio::net::TcpListener;
	use tokio::sync::oneshot;
	use tokio_stream::wrappers::TcpListenerStream;
	use tonic::{Request, Response, Status, transport::Server};

	#[derive(Clone)]
	struct TestMidnightStateService {
		block_by_hash: Result<BlockByHashResponse, Status>,
		council_datum: Result<Option<CouncilDatumResponse>, Status>,
		technical_committee_datum: Result<Option<TechnicalCommitteeDatumResponse>, Status>,
	}

	#[tonic::async_trait]
	impl MidnightState for TestMidnightStateService {
		async fn get_asset_creates(
			&self,
			_request: Request<AssetCreatesRequest>,
		) -> Result<Response<AssetCreatesResponse>, Status> {
			Err(Status::unimplemented("not used in tests"))
		}

		async fn get_asset_spends(
			&self,
			_request: Request<AssetSpendsRequest>,
		) -> Result<Response<AssetSpendsResponse>, Status> {
			Err(Status::unimplemented("not used in tests"))
		}

		async fn get_registrations(
			&self,
			_request: Request<RegistrationsRequest>,
		) -> Result<Response<RegistrationsResponse>, Status> {
			Err(Status::unimplemented("not used in tests"))
		}

		async fn get_deregistrations(
			&self,
			_request: Request<DeregistrationsRequest>,
		) -> Result<Response<DeregistrationsResponse>, Status> {
			Err(Status::unimplemented("not used in tests"))
		}

		async fn get_technical_committee_datum(
			&self,
			_request: Request<TechnicalCommitteeDatumRequest>,
		) -> Result<Response<TechnicalCommitteeDatumResponse>, Status> {
			match &self.technical_committee_datum {
				Ok(Some(response)) => Ok(Response::new(response.clone())),
				Ok(None) => Err(Status::not_found("technical committee datum not found")),
				Err(status) => Err(status.clone()),
			}
		}

		async fn get_council_datum(
			&self,
			_request: Request<CouncilDatumRequest>,
		) -> Result<Response<CouncilDatumResponse>, Status> {
			match &self.council_datum {
				Ok(Some(response)) => Ok(Response::new(response.clone())),
				Ok(None) => Err(Status::not_found("council datum not found")),
				Err(status) => Err(status.clone()),
			}
		}

		async fn get_ariadne_parameters(
			&self,
			_request: Request<AriadneParametersRequest>,
		) -> Result<Response<AriadneParametersResponse>, Status> {
			Err(Status::unimplemented("not used in tests"))
		}

		async fn get_block_by_hash(
			&self,
			_request: Request<BlockByHashRequest>,
		) -> Result<Response<BlockByHashResponse>, Status> {
			match &self.block_by_hash {
				Ok(response) => Ok(Response::new(*response)),
				Err(status) => Err(status.clone()),
			}
		}

		async fn get_utxo_events(
			&self,
			_request: Request<UtxoEventsRequest>,
		) -> Result<Response<UtxoEventsResponse>, Status> {
			Err(Status::unimplemented("not used in tests"))
		}

		async fn get_epoch_nonce(
			&self,
			_request: Request<EpochNonceRequest>,
		) -> Result<Response<EpochNonceResponse>, Status> {
			Err(Status::unimplemented("not used in tests"))
		}

		async fn get_epoch_candidates(
			&self,
			_request: Request<EpochCandidatesRequest>,
		) -> Result<Response<EpochCandidatesResponse>, Status> {
			Err(Status::unimplemented("not used in tests"))
		}

		async fn get_stable_block(
			&self,
			_request: Request<StableBlockRequest>,
		) -> Result<Response<StableBlockResponse>, Status> {
			Err(Status::unimplemented("not used in tests"))
		}

		async fn get_latest_stable_block(
			&self,
			_request: Request<LatestStableBlockRequest>,
		) -> Result<Response<LatestStableBlockResponse>, Status> {
			Err(Status::unimplemented("not used in tests"))
		}

		async fn get_latest_block(
			&self,
			_request: Request<LatestBlockRequest>,
		) -> Result<Response<LatestBlockResponse>, Status> {
			Err(Status::unimplemented("not used in tests"))
		}
	}

	struct TestServer {
		endpoint: String,
		shutdown_tx: Option<oneshot::Sender<()>>,
	}

	impl TestServer {
		async fn spawn(service: TestMidnightStateService) -> Self {
			let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
			let addr = listener.local_addr().unwrap();
			let (shutdown_tx, shutdown_rx) = oneshot::channel();

			tokio::spawn(async move {
				Server::builder()
					.add_service(MidnightStateServer::new(service))
					.serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
						let _ = shutdown_rx.await;
					})
					.await
					.unwrap();
			});

			Self { endpoint: format!("http://{}", addr), shutdown_tx: Some(shutdown_tx) }
		}
	}

	impl Drop for TestServer {
		fn drop(&mut self) {
			if let Some(shutdown_tx) = self.shutdown_tx.take() {
				let _ = shutdown_tx.send(());
			}
		}
	}

	#[tokio::test]
	async fn returns_authorities_from_grpc_datums() {
		let council =
			governance_datum_bytes(2, &[([0xAA; 32], [0x11; 32]), ([0xBB; 32], [0x22; 32])]);
		let technical_committee = governance_datum_bytes(3, &[([0xCC; 32], [0x33; 32])]);
		let server = TestServer::spawn(TestMidnightStateService {
			block_by_hash: Ok(BlockByHashResponse {
				block_number: 42,
				tx_count: 7,
				block_timestamp_unix: 1_700_000_000,
				epoch_number: 0,
				slot_number: 0,
			}),
			council_datum: Ok(Some(CouncilDatumResponse {
				source_block_number: 42,
				datum: council,
			})),
			technical_committee_datum: Ok(Some(TechnicalCommitteeDatumResponse {
				source_block_number: 42,
				datum: technical_committee,
			})),
		})
		.await;
		let data_source =
			FederatedAuthorityObservationGrpcImpl::connect(&server.endpoint).await.unwrap();
		let mc_block_hash = McBlockHash([0x44; 32]);

		let actual = data_source
			.get_federated_authority_data(&test_config(), &mc_block_hash)
			.await
			.unwrap();

		assert_eq!(actual.mc_block_hash, mc_block_hash);
		assert_eq!(
			sort_authorities(actual.council_authorities),
			sort_authorities(AuthoritiesData {
				authorities: vec![
					(AuthorityMemberPublicKey(vec![0x11; 32]), PolicyId([0xAA; 28])),
					(AuthorityMemberPublicKey(vec![0x22; 32]), PolicyId([0xBB; 28])),
				],
				round: 2,
			})
		);
		assert_eq!(
			actual.technical_committee_authorities,
			AuthoritiesData {
				authorities: vec![(AuthorityMemberPublicKey(vec![0x33; 32]), PolicyId([0xCC; 28]))],
				round: 3,
			}
		);
	}

	#[tokio::test]
	async fn missing_single_datum_returns_empty_authorities_for_that_body() {
		let technical_committee = governance_datum_bytes(4, &[([0xDD; 32], [0x55; 32])]);
		let server = TestServer::spawn(TestMidnightStateService {
			block_by_hash: Ok(BlockByHashResponse {
				block_number: 99,
				tx_count: 1,
				block_timestamp_unix: 1_700_000_001,
				epoch_number: 0,
				slot_number: 0,
			}),
			council_datum: Ok(None),
			technical_committee_datum: Ok(Some(TechnicalCommitteeDatumResponse {
				source_block_number: 99,
				datum: technical_committee,
			})),
		})
		.await;
		let data_source =
			FederatedAuthorityObservationGrpcImpl::connect(&server.endpoint).await.unwrap();

		let actual = data_source
			.get_federated_authority_data(&test_config(), &McBlockHash([0x12; 32]))
			.await
			.unwrap();

		assert_eq!(actual.council_authorities, empty_authorities_data());
		assert_eq!(
			actual.technical_committee_authorities,
			AuthoritiesData {
				authorities: vec![(AuthorityMemberPublicKey(vec![0x55; 32]), PolicyId([0xDD; 28]))],
				round: 4,
			}
		);
	}

	#[tokio::test]
	async fn invalid_datum_bytes_degrade_to_empty_authorities() {
		let server = TestServer::spawn(TestMidnightStateService {
			block_by_hash: Ok(BlockByHashResponse {
				block_number: 7,
				tx_count: 2,
				block_timestamp_unix: 1_700_000_002,
				epoch_number: 0,
				slot_number: 0,
			}),
			council_datum: Ok(Some(CouncilDatumResponse {
				source_block_number: 7,
				datum: vec![0xFF, 0x00, 0xAA],
			})),
			technical_committee_datum: Ok(Some(TechnicalCommitteeDatumResponse {
				source_block_number: 7,
				datum: vec![0x01, 0x02, 0x03],
			})),
		})
		.await;
		let data_source =
			FederatedAuthorityObservationGrpcImpl::connect(&server.endpoint).await.unwrap();

		let actual = data_source
			.get_federated_authority_data(&test_config(), &McBlockHash([0x77; 32]))
			.await
			.unwrap();

		assert_eq!(actual.council_authorities, empty_authorities_data());
		assert_eq!(actual.technical_committee_authorities, empty_authorities_data());
	}

	#[tokio::test]
	async fn unknown_block_hash_returns_error() {
		let server = TestServer::spawn(TestMidnightStateService {
			block_by_hash: Err(Status::not_found("block not found")),
			council_datum: Ok(None),
			technical_committee_datum: Ok(None),
		})
		.await;
		let data_source =
			FederatedAuthorityObservationGrpcImpl::connect(&server.endpoint).await.unwrap();

		let error = data_source
			.get_federated_authority_data(&test_config(), &McBlockHash([0x99; 32]))
			.await
			.unwrap_err();

		assert!(error.to_string().contains("block not found"));
	}

	fn governance_datum_bytes(round: u8, authorities: &[([u8; 32], [u8; 32])]) -> Vec<u8> {
		let mut members_map = PlutusMap::new();
		for (mainchain_key, authority_key) in authorities {
			let mut values = PlutusMapValues::new();
			values.add(&PlutusData::new_bytes(authority_key.to_vec()));
			members_map.insert(&PlutusData::new_bytes(mainchain_key.to_vec()), &values);
		}

		let mut multisig = PlutusList::new();
		multisig.add(&PlutusData::new_integer(&BigInt::from(authorities.len() as u64)));
		multisig.add(&PlutusData::new_map(&members_map));

		let mut versioned = PlutusList::new();
		versioned.add(&PlutusData::new_list(&multisig));
		versioned.add(&PlutusData::new_integer(&BigInt::from(round)));

		PlutusData::new_list(&versioned).to_bytes()
	}

	fn test_config() -> FederatedAuthorityObservationConfig {
		FederatedAuthorityObservationConfig {
			council: AuthBodyConfig {
				address: "addr_test1council".to_string(),
				policy_id: PolicyId([0x01; 28]),
				members: vec![],
				members_mainchain: vec![],
			},
			technical_committee: AuthBodyConfig {
				address: "addr_test1tech".to_string(),
				policy_id: PolicyId([0x02; 28]),
				members: vec![],
				members_mainchain: vec![],
			},
		}
	}

	fn sort_authorities(mut data: AuthoritiesData) -> AuthoritiesData {
		data.authorities.sort_by(|(left_key, left_member), (right_key, right_member)| {
			left_key.0.cmp(&right_key.0).then(left_member.0.cmp(&right_member.0))
		});
		data
	}
}
