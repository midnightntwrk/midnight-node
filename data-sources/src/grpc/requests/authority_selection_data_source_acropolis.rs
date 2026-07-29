use std::collections::HashMap;

use crate::grpc::handle::IndexerHandle;
use crate::grpc::{
	conversions::{get_stake_delegation, make_stake_map, registration_from_candidate},
	midnight_state::{AriadneParametersRequest, EpochCandidatesRequest, EpochNonceRequest},
};
use cardano_serialization_lib::PlutusData;
use partner_chains_plutus_data::permissioned_candidates::PermissionedCandidateDatums;
use sidechain_domain::{
	CandidateRegistrations, EpochNonce, McEpochNumber, PermissionedCandidateData,
	StakePoolPublicKey,
};
use tonic::Status;

pub async fn get_permissioned_candidates(
	api: &IndexerHandle,
	epoch: McEpochNumber,
) -> Result<Option<Vec<PermissionedCandidateData>>, Status> {
	let response = api
		.get_ariadne_parameters(AriadneParametersRequest { epoch: epoch.0 as u64 })
		.await?;

	if response.datum.is_empty() {
		Ok(None)
	} else {
		let datums =
			PermissionedCandidateDatums::try_from(PlutusData::from_bytes(response.datum).map_err(
				|e| Status::internal(format!("failed to parse Ariadne parameters datum: {e}")),
			)?)
			.map_err(|e| Status::internal(format!("failed to decode Ariadne parameters: {e}")))?;

		Ok(Some(datums.into()))
	}
}

pub async fn get_candidates(
	api: &IndexerHandle,
	epoch: McEpochNumber,
) -> Result<Vec<CandidateRegistrations>, Status> {
	let response = api
		.get_epoch_candidates(EpochCandidatesRequest { epoch: epoch.0 as u64 })
		.await?;

	let stake_map = make_stake_map(response.stake_distribution)
		.map_err(|e| Status::internal(format!("candidate conversion failed: {e:?}")))?;

	let mut grouped: HashMap<StakePoolPublicKey, Vec<sidechain_domain::RegistrationData>> =
		HashMap::new();

	for candidate in response.candidates {
		let (pool_key, registration) = registration_from_candidate(candidate)
			.map_err(|e| Status::internal(format!("candidate conversion failed: {e:?}")))?;

		grouped.entry(pool_key).or_default().push(registration);
	}

	Ok(grouped
		.into_iter()
		.map(|(stake_pool_public_key, registrations)| {
			let stake_delegation = get_stake_delegation(&stake_map, &stake_pool_public_key);

			CandidateRegistrations { stake_pool_public_key, registrations, stake_delegation }
		})
		.collect())
}

pub async fn get_epoch_nonce(
	api: &IndexerHandle,
	epoch: McEpochNumber,
) -> Result<Option<EpochNonce>, Status> {
	let response = api.get_epoch_nonce(EpochNonceRequest { epoch: epoch.0 as u64 }).await?;

	Ok(response.nonce.map(EpochNonce))
}
