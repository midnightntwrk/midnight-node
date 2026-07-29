use sidechain_domain::{MainchainBlock, McBlockHash, McBlockNumber, McEpochNumber, McSlotNumber};
use tonic::Status;

use crate::grpc::conversions::hash32;
use crate::grpc::handle::IndexerHandle;
use crate::grpc::midnight_state::{
	BlockByHashRequest, LatestStableBlockRequest, StableBlockRequest,
};

pub(crate) async fn get_latest_stable_block(
	api: &IndexerHandle,
	stability_offset: u32,
	as_of_timestamp_unix_millis: u64,
) -> Result<Option<MainchainBlock>, Status> {
	let response = api
		.get_latest_stable_block(LatestStableBlockRequest {
			stability_offset,
			as_of_timestamp_unix_millis,
		})
		.await?;

	response
		.block
		.map(|block| {
			Ok(MainchainBlock {
				number: McBlockNumber(block.block_number),
				hash: McBlockHash(hash32(block.block_hash)?),
				epoch: McEpochNumber(block.epoch_number),
				slot: McSlotNumber(block.slot_number),
				timestamp: block.block_timestamp_unix,
			})
		})
		.transpose()
}
pub(crate) async fn get_stable_block(
	api: &IndexerHandle,
	hash: McBlockHash,
	stability_offset: u32,
	as_of_timestamp_unix_millis: u64,
) -> Result<Option<MainchainBlock>, Status> {
	let response = api
		.get_stable_block(StableBlockRequest {
			block_hash: hash.0.to_vec(),
			stability_offset,
			as_of_timestamp_unix_millis,
		})
		.await?;

	response
		.block
		.map(|block| {
			Ok(MainchainBlock {
				number: McBlockNumber(block.block_number),
				hash: McBlockHash(hash32(block.block_hash)?),
				epoch: McEpochNumber(block.epoch_number),
				slot: McSlotNumber(block.slot_number),
				timestamp: block.block_timestamp_unix,
			})
		})
		.transpose()
}

pub(crate) async fn get_block_by_hash(
	api: &IndexerHandle,
	block_hash: McBlockHash,
) -> Result<Option<MainchainBlock>, Status> {
	let response = match api
		.get_block_by_hash(BlockByHashRequest { block_hash: block_hash.0.to_vec() })
		.await
	{
		Ok(response) => response,
		// Unknown block is a valid answer (None), not an error — mirrors the
		// db-sync backend's `Option` semantics.
		Err(status) if status.code() == tonic::Code::NotFound => return Ok(None),
		Err(status) => return Err(status),
	};

	let timestamp = u64::try_from(response.block_timestamp_unix)
		.map_err(|_| Status::internal("negative block timestamp"))?;
	Ok(Some(MainchainBlock {
		number: McBlockNumber(response.block_number),
		hash: block_hash,
		epoch: McEpochNumber(response.epoch_number),
		slot: McSlotNumber(response.slot_number),
		timestamp,
	}))
}
