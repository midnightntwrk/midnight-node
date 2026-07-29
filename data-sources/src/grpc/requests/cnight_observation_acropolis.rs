use crate::grpc::conversions::observed_utxo_from_event;
use crate::grpc::handle::IndexerHandle;
use crate::grpc::midnight_state::{CardanoPosition as GrpcCardanoPosition, UtxoEventsRequest};
use midnight_primitives_cnight_observation::{CardanoPosition, ObservedUtxo, TimestampUnixMillis};
use sidechain_domain::*;
use tonic::Status;

pub struct ObservedUtxoEvents {
	pub utxos: Vec<ObservedUtxo>,
	pub next_position: CardanoPosition,
}

pub async fn get_utxo_events(
	api: &IndexerHandle,
	cardano_network: u8,
	start_position: &CardanoPosition,
	tx_capacity: usize,
	end_block_hash: McBlockHash,
) -> Result<ObservedUtxoEvents, Status> {
	let tx_capacity = u32::try_from(tx_capacity)
		.map_err(|_| tonic::Status::invalid_argument("utxo_capacity too large"))?;

	let response = api
		.get_utxo_events(UtxoEventsRequest {
			tx_capacity,
			end_block_hash: end_block_hash.0.to_vec(),
			start_position: Some(GrpcCardanoPosition {
				block_hash: start_position.block_hash.0.to_vec(),
				block_number: start_position.block_number,
				tx_index: start_position.tx_index_in_block,
				block_timestamp_unix_millis: start_position.block_timestamp.0,
			}),
		})
		.await?;

	let utxos = response
		.events
		.into_iter()
		.map(|e| observed_utxo_from_event(e, cardano_network))
		.collect::<Result<Vec<_>, _>>()?;

	let next_position = response
		.next_position
		.ok_or_else(|| Status::internal("missing next_position in UtxoEventsResponse"))
		.and_then(cardano_position_from_response)?;

	Ok(ObservedUtxoEvents { utxos, next_position })
}

#[allow(clippy::result_large_err)]
fn cardano_position_from_response(
	position: GrpcCardanoPosition,
) -> Result<CardanoPosition, Status> {
	Ok(CardanoPosition {
		block_hash: McBlockHash(
			position
				.block_hash
				.try_into()
				.map_err(|_| Status::internal("invalid block hash length in next_position"))?,
		),
		block_number: position.block_number,
		block_timestamp: TimestampUnixMillis(position.block_timestamp_unix_millis),
		tx_index_in_block: position.tx_index,
	})
}
