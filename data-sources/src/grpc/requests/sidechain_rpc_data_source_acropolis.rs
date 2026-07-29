use sidechain_domain::{MainchainBlock, McBlockHash, McBlockNumber, McEpochNumber, McSlotNumber};
use tonic::Status;

use crate::{
	grpc::conversions::hash32, grpc::handle::IndexerHandle,
	grpc::midnight_state::LatestBlockRequest,
};

pub(crate) async fn get_latest_block(api: &IndexerHandle) -> Result<MainchainBlock, Status> {
	let response = api.get_latest_block(LatestBlockRequest {}).await?;

	let block = response
		.block
		.ok_or_else(|| Status::internal("LatestBlockResponse missing block"))?;

	Ok(MainchainBlock {
		number: McBlockNumber(block.block_number),
		hash: McBlockHash(hash32(block.block_hash)?),
		epoch: McEpochNumber(block.epoch_number),
		slot: McSlotNumber(block.slot_number),
		timestamp: block.block_timestamp_unix,
	})
}
