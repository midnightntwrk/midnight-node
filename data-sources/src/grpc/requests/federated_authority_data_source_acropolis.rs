use crate::grpc::conversions::decode_governance_datum;
use crate::grpc::handle::IndexerHandle;
use cardano_serialization_lib::PlutusData;
use midnight_primitives_federated_authority_observation::{
	AuthoritiesData, FederatedAuthorityData,
};
use sidechain_domain::McBlockHash;
use tonic::{Code, Status};

use crate::grpc::midnight_state::{
	BlockByHashRequest, CouncilDatumRequest, TechnicalCommitteeDatumRequest,
};

pub(crate) async fn get_block_number_by_hash(
	api: &IndexerHandle,
	block_hash: McBlockHash,
) -> Result<u32, Status> {
	let response = api
		.get_block_by_hash(BlockByHashRequest { block_hash: block_hash.0.to_vec() })
		.await?;

	Ok(response.block_number)
}

pub(crate) async fn get_federated_authority_data(
	api: &IndexerHandle,
	block_number: u32,
	block_hash: McBlockHash,
) -> Result<FederatedAuthorityData, Box<dyn std::error::Error + Send + Sync>> {
	let council_authorities =
		load_authorities(get_council_datum(api, block_number).await, block_number, "council")?;

	let technical_committee_authorities = load_authorities(
		get_technical_committee_datum(api, block_number).await,
		block_number,
		"technical committee",
	)?;

	Ok(FederatedAuthorityData {
		council_authorities,
		technical_committee_authorities,
		mc_block_hash: block_hash,
	})
}

pub(crate) async fn get_council_datum(
	api: &IndexerHandle,
	block_number: u32,
) -> Result<Vec<u8>, Status> {
	let response = api
		.get_council_datum(CouncilDatumRequest { block_number: u64::from(block_number) })
		.await?;

	Ok(response.datum)
}

pub(crate) async fn get_technical_committee_datum(
	api: &IndexerHandle,
	block_number: u32,
) -> Result<Vec<u8>, Status> {
	let response = api
		.get_technical_committee_datum(TechnicalCommitteeDatumRequest {
			block_number: u64::from(block_number),
		})
		.await?;

	Ok(response.datum)
}

fn load_authorities(
	response: Result<Vec<u8>, tonic::Status>,
	block_number: u32,
	body_name: &str,
) -> Result<AuthoritiesData, Box<dyn std::error::Error + Send + Sync>> {
	match response {
		Ok(bytes) => {
			let authorities = PlutusData::from_bytes(bytes)
				.map_err(|e| format!("Invalid {} datum CBOR: {}", body_name, e))
				.and_then(|datum| {
					decode_governance_datum(&datum)
						.map(AuthoritiesData::from)
						.map_err(|error| error.to_string())
				});

			match authorities {
				Ok(authorities) => Ok(authorities),
				Err(error) => {
					log::warn!(
						"Failed to decode {} datum in Cardano block {}: {}. Using empty list.",
						body_name,
						block_number,
						error,
					);
					Ok(empty_authorities_data())
				},
			}
		},
		Err(status) if status.code() == Code::NotFound => {
			log::warn!(
				"No {} datum found for Cardano block {}. Using empty list.",
				body_name,
				block_number,
			);
			Ok(empty_authorities_data())
		},
		Err(status) => Err(format!(
			"Failed to fetch {} datum for Cardano block {}: {}",
			body_name, block_number, status
		)
		.into()),
	}
}

pub(crate) fn empty_authorities_data() -> AuthoritiesData {
	AuthoritiesData { authorities: vec![], round: 0 }
}
