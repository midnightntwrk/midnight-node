// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Tests of every row source method against an in-process fake mc-follower server

use crate::proto::{
	self,
	mc_follower_server::{McFollower, McFollowerServer},
};
use crate::{McFollowerError, McFollowerRowSource};
use cardano_serialization_lib::{BigInt, PlutusData};
use chrono::{DateTime, NaiveDateTime};
use db_sync_sqlx::{
	Address, Asset, AssetName, BlockNumber, EpochNumber, PolicyId as DbPolicyId, SlotNumber,
	StakeDelegation, TxHash, TxIndex, TxIndexInBlock,
};
use midnight_primitives_cnight_observation::{CardanoPosition, TimestampUnixMillis};
use midnight_primitives_mainchain_follower::data_source::candidates_data_source::row_source::{
	self as cds, CandidatesRowSource,
};
use midnight_primitives_mainchain_follower::db::{
	self as mf, CNightObservationRowSource, FederatedAuthorityRowSource, McBlockByHashRowSource,
};
use partner_chains_db_sync_data_sources::row_source::{
	self as pc, BlockRowSource, BridgeRowSource,
};
use sidechain_domain::{McBlockHash, McTxHash, PolicyId, UtxoId, UtxoIndex};
use std::any::Any;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tokio_stream::Stream;
use tonic::{Code, Request, Response, Status};

type RowStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

/// Fake mc-follower: answers every RPC with the canned data, or with `error`, and records the
/// requests
#[derive(Clone, Default)]
struct Fake {
	error: Option<Code>,
	/// Status ending every stream after its rows
	stream_error: Option<Code>,
	block: Option<proto::Block>,
	blocks: Vec<proto::Block>,
	tx_position: Option<proto::Position>,
	bridge_txs: Vec<proto::BridgeTxRow>,
	stable_epoch: Option<i64>,
	stake: Vec<proto::StakePoolEntry>,
	token_output: Option<proto::TokenTxOutput>,
	nonce: Option<Vec<u8>>,
	address_outputs: Vec<proto::AddressOutputRow>,
	registrations: Vec<proto::RegistrationRow>,
	deregistrations: Vec<proto::DeregistrationRow>,
	creates: Vec<proto::AssetCreateRow>,
	spends: Vec<proto::AssetSpendRow>,
	max_tx_size: Option<u64>,
	governance: Option<proto::GovernanceOutput>,
	requests: Arc<Mutex<Vec<Box<dyn Any + Send>>>>,
}

impl Fake {
	#[allow(clippy::result_large_err)]
	fn answer<T: Send + 'static>(&self, request: Request<T>) -> Result<(), Status> {
		self.requests.lock().unwrap().push(Box::new(request.into_inner()));
		match self.error {
			Some(code) => Err(Status::new(code, "fake error")),
			None => Ok(()),
		}
	}

	#[allow(clippy::result_large_err)]
	fn stream<T: Send + 'static>(&self, rows: Vec<T>) -> Response<RowStream<T>> {
		let end = self.stream_error.map(|code| Err(Status::new(code, "fake stream error")));
		Response::new(Box::pin(tokio_stream::iter(rows.into_iter().map(Ok).chain(end))))
	}

	fn last_request<T: Clone + 'static>(&self) -> T {
		let requests = self.requests.lock().unwrap();
		requests
			.iter()
			.rev()
			.find_map(|r| r.downcast_ref::<T>())
			.cloned()
			.expect("request sent")
	}
}

#[tonic::async_trait]
impl McFollower for Fake {
	type GetBlocksByNumberRangeStream = RowStream<proto::Block>;
	type GetBridgeTxsStream = RowStream<proto::BridgeTxRow>;
	type GetStakeDistributionStream = RowStream<proto::StakePoolEntry>;
	type GetUnspentOutputsAtAddressStream = RowStream<proto::AddressOutputRow>;
	type GetMappingRegistrationsStream = RowStream<proto::RegistrationRow>;
	type GetMappingDeregistrationsStream = RowStream<proto::DeregistrationRow>;
	type GetAssetCreatesStream = RowStream<proto::AssetCreateRow>;
	type GetAssetSpendsStream = RowStream<proto::AssetSpendRow>;

	async fn get_status(
		&self,
		request: Request<proto::GetStatusRequest>,
	) -> Result<Response<proto::GetStatusResponse>, Status> {
		self.answer(request)?;
		Ok(Response::new(proto::GetStatusResponse::default()))
	}

	async fn get_latest_block(
		&self,
		request: Request<proto::GetLatestBlockRequest>,
	) -> Result<Response<proto::OptionalBlock>, Status> {
		self.answer(request)?;
		Ok(Response::new(proto::OptionalBlock { block: self.block.clone() }))
	}

	async fn get_blocks_by_number_range(
		&self,
		request: Request<proto::GetBlocksByNumberRangeRequest>,
	) -> Result<Response<Self::GetBlocksByNumberRangeStream>, Status> {
		self.answer(request)?;
		Ok(self.stream(self.blocks.clone()))
	}

	async fn get_highest_block_in_window(
		&self,
		request: Request<proto::GetHighestBlockInWindowRequest>,
	) -> Result<Response<proto::OptionalBlock>, Status> {
		self.answer(request)?;
		Ok(Response::new(proto::OptionalBlock { block: self.block.clone() }))
	}

	async fn get_block_by_hash(
		&self,
		request: Request<proto::GetBlockByHashRequest>,
	) -> Result<Response<proto::OptionalBlock>, Status> {
		self.answer(request)?;
		Ok(Response::new(proto::OptionalBlock { block: self.block.clone() }))
	}

	async fn get_tx_position(
		&self,
		request: Request<proto::GetTxPositionRequest>,
	) -> Result<Response<proto::GetTxPositionResponse>, Status> {
		self.answer(request)?;
		Ok(Response::new(proto::GetTxPositionResponse { position: self.tx_position }))
	}

	async fn get_bridge_txs(
		&self,
		request: Request<proto::GetBridgeTxsRequest>,
	) -> Result<Response<Self::GetBridgeTxsStream>, Status> {
		self.answer(request)?;
		Ok(self.stream(self.bridge_txs.clone()))
	}

	async fn get_latest_block_for_epoch(
		&self,
		request: Request<proto::GetLatestBlockForEpochRequest>,
	) -> Result<Response<proto::OptionalBlock>, Status> {
		self.answer(request)?;
		Ok(Response::new(proto::OptionalBlock { block: self.block.clone() }))
	}

	async fn get_latest_stable_epoch(
		&self,
		request: Request<proto::GetLatestStableEpochRequest>,
	) -> Result<Response<proto::GetLatestStableEpochResponse>, Status> {
		self.answer(request)?;
		Ok(Response::new(proto::GetLatestStableEpochResponse { epoch_no: self.stable_epoch }))
	}

	async fn get_stake_distribution(
		&self,
		request: Request<proto::GetStakeDistributionRequest>,
	) -> Result<Response<Self::GetStakeDistributionStream>, Status> {
		self.answer(request)?;
		Ok(self.stream(self.stake.clone()))
	}

	async fn get_latest_token_output_for_epoch(
		&self,
		request: Request<proto::GetLatestTokenOutputForEpochRequest>,
	) -> Result<Response<proto::GetLatestTokenOutputForEpochResponse>, Status> {
		self.answer(request)?;
		Ok(Response::new(proto::GetLatestTokenOutputForEpochResponse {
			output: self.token_output.clone(),
		}))
	}

	async fn get_epoch_nonce(
		&self,
		request: Request<proto::GetEpochNonceRequest>,
	) -> Result<Response<proto::GetEpochNonceResponse>, Status> {
		self.answer(request)?;
		Ok(Response::new(proto::GetEpochNonceResponse { nonce: self.nonce.clone() }))
	}

	async fn get_unspent_outputs_at_address(
		&self,
		request: Request<proto::GetUnspentOutputsAtAddressRequest>,
	) -> Result<Response<Self::GetUnspentOutputsAtAddressStream>, Status> {
		self.answer(request)?;
		Ok(self.stream(self.address_outputs.clone()))
	}

	async fn get_mapping_registrations(
		&self,
		request: Request<proto::GetMappingRegistrationsRequest>,
	) -> Result<Response<Self::GetMappingRegistrationsStream>, Status> {
		self.answer(request)?;
		Ok(self.stream(self.registrations.clone()))
	}

	async fn get_mapping_deregistrations(
		&self,
		request: Request<proto::GetMappingDeregistrationsRequest>,
	) -> Result<Response<Self::GetMappingDeregistrationsStream>, Status> {
		self.answer(request)?;
		Ok(self.stream(self.deregistrations.clone()))
	}

	async fn get_asset_creates(
		&self,
		request: Request<proto::GetAssetCreatesRequest>,
	) -> Result<Response<Self::GetAssetCreatesStream>, Status> {
		self.answer(request)?;
		Ok(self.stream(self.creates.clone()))
	}

	async fn get_asset_spends(
		&self,
		request: Request<proto::GetAssetSpendsRequest>,
	) -> Result<Response<Self::GetAssetSpendsStream>, Status> {
		self.answer(request)?;
		Ok(self.stream(self.spends.clone()))
	}

	async fn get_max_tx_size(
		&self,
		request: Request<proto::GetMaxTxSizeRequest>,
	) -> Result<Response<proto::GetMaxTxSizeResponse>, Status> {
		self.answer(request)?;
		Ok(Response::new(proto::GetMaxTxSizeResponse { max_tx_size: self.max_tx_size }))
	}

	async fn get_latest_governance_output(
		&self,
		request: Request<proto::GetLatestGovernanceOutputRequest>,
	) -> Result<Response<proto::GetLatestGovernanceOutputResponse>, Status> {
		self.answer(request)?;
		Ok(Response::new(proto::GetLatestGovernanceOutputResponse {
			output: self.governance.clone(),
		}))
	}
}

/// Serves `fake` on a local port and connects a row source to it
async fn serve(fake: &Fake) -> McFollowerRowSource {
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
	let address = listener.local_addr().unwrap();
	let server = tonic::transport::Server::builder()
		.add_service(McFollowerServer::new(fake.clone()))
		.serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener));
	tokio::spawn(server);
	McFollowerRowSource::connect(format!("http://{address}")).await.unwrap()
}

fn pc_error(error: pc::RowSourceError) -> McFollowerError {
	match error {
		pc::RowSourceError::Other(error) => *error.downcast::<McFollowerError>().unwrap(),
		other => panic!("unexpected error {other:?}"),
	}
}

fn mf_error(error: mf::RowSourceError) -> McFollowerError {
	match error {
		mf::RowSourceError::Other(error) => *error.downcast::<McFollowerError>().unwrap(),
		other => panic!("unexpected error {other:?}"),
	}
}

const BLOCK_TIME: i64 = 1_700_000_000;

fn time(seconds: i64) -> NaiveDateTime {
	DateTime::from_timestamp(seconds, 0).unwrap().naive_utc()
}

fn block(block_no: u32) -> proto::Block {
	proto::Block {
		block_no,
		hash: vec![block_no as u8; 32],
		epoch_no: 7,
		slot_no: 1000 + block_no as u64,
		time_unix_seconds: BLOCK_TIME + block_no as i64,
		tx_count: 3,
	}
}

fn pc_block(block_no: u32) -> pc::Block {
	pc::Block {
		block_no: BlockNumber(block_no),
		hash: [block_no as u8; 32],
		epoch_no: EpochNumber(7),
		slot_no: SlotNumber(1000 + block_no as u64),
		time: time(BLOCK_TIME + block_no as i64),
	}
}

fn datum(value: i32) -> proto::Datum {
	proto::Datum { cbor: PlutusData::new_integer(&BigInt::from(value)).to_bytes() }
}

/// 64 nested lists, rejected on the db-sync route
fn undecodable_datum() -> proto::Datum {
	let mut cbor = vec![0x81; 64];
	cbor.push(0x01);
	proto::Datum { cbor }
}

fn plutus_int(value: i32) -> PlutusData {
	PlutusData::new_integer(&BigInt::from(value))
}

fn asset() -> Asset {
	Asset { policy_id: DbPolicyId(vec![1; 28]), asset_name: AssetName(b"name".to_vec()) }
}

fn asset_id() -> proto::AssetId {
	proto::AssetId { policy_id: vec![1; 28], asset_name: b"name".to_vec() }
}

// Errors

#[tokio::test]
async fn unavailable_indexer_is_an_error_not_an_absent_row() {
	let source = serve(&Fake { error: Some(Code::Unavailable), ..Default::default() }).await;
	let error = BlockRowSource::get_latest_block_info(&source).await.unwrap_err();
	assert!(matches!(pc_error(error), McFollowerError::Unavailable(_)));
	let error = source.get_epoch_nonce(EpochNumber(1)).await.unwrap_err();
	assert!(matches!(mf_error(error), McFollowerError::Unavailable(_)));
}

#[tokio::test]
async fn unindexed_address_or_policy_is_an_error() {
	let source = serve(&Fake { error: Some(Code::FailedPrecondition), ..Default::default() }).await;
	let error = source
		.get_governance_body_utxo("addr", &PolicyId([1; 28]), 5)
		.await
		.unwrap_err();
	assert!(matches!(mf_error(error), McFollowerError::NotIndexed(_)));
}

#[tokio::test]
async fn not_found_status_is_an_error_not_an_absent_row() {
	let source = serve(&Fake { error: Some(Code::NotFound), ..Default::default() }).await;
	let error = BlockRowSource::get_block_by_hash(&source, McBlockHash([1; 32]))
		.await
		.unwrap_err();
	assert!(matches!(pc_error(error), McFollowerError::Request(s) if s.code() == Code::NotFound));
}

#[tokio::test]
async fn unreachable_indexer_is_unavailable() {
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
	let address = listener.local_addr().unwrap();
	drop(listener);
	let channel = tonic::transport::Channel::from_shared(format!("http://{address}"))
		.unwrap()
		.connect_lazy();
	let source = McFollowerRowSource::new(channel);
	let error = BlockRowSource::get_latest_block_info(&source).await.unwrap_err();
	assert!(matches!(pc_error(error), McFollowerError::Unavailable(_)));
}

#[tokio::test]
async fn error_in_the_middle_of_a_stream_fails_the_query() {
	let fake = Fake {
		stream_error: Some(Code::Unavailable),
		blocks: vec![block(1)],
		..Default::default()
	};
	let source = serve(&fake).await;
	let error = source.get_blocks_by_numbers(BlockNumber(1), BlockNumber(2)).await.unwrap_err();
	assert!(matches!(pc_error(error), McFollowerError::Unavailable(_)));
}

// Block row source

#[tokio::test]
async fn get_latest_block_info() {
	let fake = Fake { block: Some(block(5)), ..Default::default() };
	let source = serve(&fake).await;
	assert_eq!(BlockRowSource::get_latest_block_info(&source).await.unwrap(), Some(pc_block(5)));

	let source = serve(&Fake::default()).await;
	assert_eq!(BlockRowSource::get_latest_block_info(&source).await.unwrap(), None);
}

#[tokio::test]
async fn get_blocks_by_numbers() {
	let fake = Fake { blocks: vec![block(3), block(4)], ..Default::default() };
	let source = serve(&fake).await;
	let blocks = source.get_blocks_by_numbers(BlockNumber(3), BlockNumber(4)).await.unwrap();
	assert_eq!(blocks, vec![pc_block(3), pc_block(4)]);
	assert_eq!(
		fake.last_request::<proto::GetBlocksByNumberRangeRequest>(),
		proto::GetBlocksByNumberRangeRequest { from_block_no: 3, to_block_no: 4 }
	);
}

#[tokio::test]
async fn get_highest_block() {
	let fake = Fake { block: Some(block(9)), ..Default::default() };
	let source = serve(&fake).await;
	let min_time = DateTime::from_timestamp_millis(1_000_500).unwrap().naive_utc();
	let max_time = DateTime::from_timestamp_millis(2_000_250).unwrap().naive_utc();
	let result = source
		.get_highest_block(BlockNumber(10), min_time, SlotNumber(100), max_time, SlotNumber(200))
		.await
		.unwrap();
	assert_eq!(result, Some(pc_block(9)));
	assert_eq!(
		fake.last_request::<proto::GetHighestBlockInWindowRequest>(),
		proto::GetHighestBlockInWindowRequest {
			max_block_no: 10,
			min_slot: 100,
			max_slot: 200,
			min_time_unix_millis: 1_000_500,
			max_time_unix_millis: 2_000_250,
		}
	);
}

#[tokio::test]
async fn get_block_by_hash() {
	let fake = Fake { block: Some(block(2)), ..Default::default() };
	let source = serve(&fake).await;
	let result = BlockRowSource::get_block_by_hash(&source, McBlockHash([2; 32])).await.unwrap();
	assert_eq!(result, Some(pc_block(2)));
	assert_eq!(fake.last_request::<proto::GetBlockByHashRequest>().hash, vec![2; 32]);
}

#[tokio::test]
async fn get_block_by_hash_with_tx_count() {
	let fake = Fake { block: Some(block(2)), ..Default::default() };
	let source = serve(&fake).await;
	let result = McBlockByHashRowSource::get_block_by_hash(&source, McBlockHash([2; 32]))
		.await
		.unwrap();
	assert_eq!(
		result,
		Some(mf::Block {
			block_number: BlockNumber(2),
			hash: [2; 32],
			epoch_number: EpochNumber(7),
			slot_number: SlotNumber(1002),
			time: time(BLOCK_TIME + 2),
			tx_count: 3,
		})
	);

	let source = serve(&Fake::default()).await;
	let result = McBlockByHashRowSource::get_block_by_hash(&source, McBlockHash([2; 32]))
		.await
		.unwrap();
	assert_eq!(result, None);
}

#[tokio::test]
async fn block_with_wrong_hash_length_is_invalid() {
	let fake =
		Fake { block: Some(proto::Block { hash: vec![1; 31], ..block(1) }), ..Default::default() };
	let source = serve(&fake).await;
	let error = BlockRowSource::get_latest_block_info(&source).await.unwrap_err();
	assert!(matches!(pc_error(error), McFollowerError::InvalidResponse(_)));
}

#[tokio::test]
async fn tx_count_beyond_i64_is_invalid() {
	let fake =
		Fake { block: Some(proto::Block { tx_count: u64::MAX, ..block(1) }), ..Default::default() };
	let source = serve(&fake).await;
	let error = McBlockByHashRowSource::get_block_by_hash(&source, McBlockHash([1; 32]))
		.await
		.unwrap_err();
	assert!(matches!(mf_error(error), McFollowerError::InvalidResponse(_)));
}

// Bridge row source

#[tokio::test]
async fn get_block_info_for_tx_hash() {
	let fake = Fake {
		tx_position: Some(proto::Position { block_no: 12, tx_index: 4 }),
		..Default::default()
	};
	let source = serve(&fake).await;
	let result = source.get_block_info_for_tx_hash(TxHash([5; 32])).await.unwrap();
	assert_eq!(
		result,
		Some(pc::TxBlockInfo { block_number: BlockNumber(12), tx_ix: TxIndexInBlock(4) })
	);
	assert_eq!(fake.last_request::<proto::GetTxPositionRequest>().tx_hash, vec![5; 32]);

	let source = serve(&Fake::default()).await;
	assert_eq!(source.get_block_info_for_tx_hash(TxHash([5; 32])).await.unwrap(), None);
}

fn amount(value: u64) -> Option<proto::Uint128> {
	Some(proto::Uint128 { hi: 0, lo: value })
}

fn metadatum(element: cardano_serialization_lib::TransactionMetadatum) -> Vec<u8> {
	let mut list = cardano_serialization_lib::MetadataList::new();
	list.add(&element);
	cardano_serialization_lib::TransactionMetadatum::new_list(&list).to_bytes()
}

#[tokio::test]
async fn get_bridge_txs() {
	let recipient = cardano_serialization_lib::TransactionMetadatum::new_bytes(vec![0xab; 4]);
	let fake = Fake {
		bridge_txs: vec![
			proto::BridgeTxRow {
				block_no: 20,
				tx_index: 1,
				tx_hash: vec![7; 32],
				c2m_metadatum_cbor: Some(metadatum(recipient.unwrap())),
				bridge_in: amount(1),
				bridge_out: amount(10),
				reserve_in: amount(3),
				reserve_out: amount(0),
			},
			proto::BridgeTxRow {
				block_no: 21,
				tx_index: 0,
				tx_hash: vec![8; 32],
				c2m_metadatum_cbor: None,
				bridge_in: amount(0),
				bridge_out: amount(u64::MAX),
				reserve_in: amount(0),
				reserve_out: amount(0),
			},
		],
		..Default::default()
	};
	let source = serve(&fake).await;
	let rows = source
		.get_bridge_txs(
			&Address("ics".into()),
			&Address("reserve".into()),
			asset(),
			pc::ResolvedBridgeDataCheckpoint::Block { number: BlockNumber(19) },
			BlockNumber(30),
			Some(5),
		)
		.await
		.unwrap();
	assert_eq!(
		rows,
		vec![
			pc::BridgeTx {
				block_number: BlockNumber(20),
				tx_ix: TxIndexInBlock(1),
				tx_hash: TxHash([7; 32]),
				c2m_metadata: Some(serde_json::json!(["0xabababab"])),
				bridge_in: pc::NativeTokenAmount(1),
				bridge_out: pc::NativeTokenAmount(10),
				reserve_in: pc::NativeTokenAmount(3),
				reserve_out: pc::NativeTokenAmount(0),
			},
			pc::BridgeTx {
				block_number: BlockNumber(21),
				tx_ix: TxIndexInBlock(0),
				tx_hash: TxHash([8; 32]),
				c2m_metadata: None,
				bridge_in: pc::NativeTokenAmount(0),
				bridge_out: pc::NativeTokenAmount(u64::MAX),
				reserve_in: pc::NativeTokenAmount(0),
				reserve_out: pc::NativeTokenAmount(0),
			},
		]
	);
	assert_eq!(
		fake.last_request::<proto::GetBridgeTxsRequest>(),
		proto::GetBridgeTxsRequest {
			ics_address: "ics".into(),
			reserve_address: "reserve".into(),
			token: Some(asset_id()),
			lower: Some(proto::get_bridge_txs_request::Lower::AfterBlockNo(19)),
			to_block_no: 30,
			max_txs: Some(5),
		}
	);
}

#[tokio::test]
async fn get_bridge_txs_sends_the_checkpoint_kind() {
	use proto::get_bridge_txs_request::Lower;
	let fake = Fake::default();
	let source = serve(&fake).await;
	let position = proto::Position { block_no: 4, tx_index: 2 };
	for (checkpoint, lower) in [
		(
			pc::ResolvedBridgeDataCheckpoint::Tx {
				block_number: BlockNumber(4),
				tx_ix: TxIndexInBlock(2),
			},
			Lower::AfterPosition(position),
		),
		(
			pc::ResolvedBridgeDataCheckpoint::TxReserveTransfer {
				block_number: BlockNumber(4),
				tx_ix: TxIndexInBlock(2),
			},
			Lower::FromPosition(position),
		),
	] {
		let rows = source
			.get_bridge_txs(
				&Address("ics".into()),
				&Address("reserve".into()),
				asset(),
				checkpoint,
				BlockNumber(30),
				None,
			)
			.await
			.unwrap();
		assert_eq!(rows, vec![]);
		let request = fake.last_request::<proto::GetBridgeTxsRequest>();
		assert_eq!(request.lower, Some(lower));
		assert_eq!(request.max_txs, None);
	}
}

#[tokio::test]
async fn bridge_sum_beyond_u64_fails_the_query() {
	let fake = Fake {
		bridge_txs: vec![proto::BridgeTxRow {
			block_no: 20,
			tx_index: 1,
			tx_hash: vec![7; 32],
			c2m_metadatum_cbor: None,
			bridge_in: amount(0),
			bridge_out: Some(proto::Uint128 { hi: 1, lo: 0 }),
			reserve_in: amount(0),
			reserve_out: amount(0),
		}],
		..Default::default()
	};
	let source = serve(&fake).await;
	let error = source
		.get_bridge_txs(
			&Address("ics".into()),
			&Address("reserve".into()),
			asset(),
			pc::ResolvedBridgeDataCheckpoint::Block { number: BlockNumber(19) },
			BlockNumber(30),
			None,
		)
		.await
		.unwrap_err();
	assert!(matches!(pc_error(error), McFollowerError::InvalidResponse(_)));
}

// Candidates row source

#[tokio::test]
async fn get_latest_block_for_epoch() {
	let fake = Fake { block: Some(block(6)), ..Default::default() };
	let source = serve(&fake).await;
	let result = source.get_latest_block_for_epoch(EpochNumber(7)).await.unwrap();
	assert_eq!(
		result,
		Some(cds::Block {
			block_no: BlockNumber(6),
			hash: [6; 32],
			epoch_no: EpochNumber(7),
			slot_no: SlotNumber(1006),
		})
	);
	assert_eq!(fake.last_request::<proto::GetLatestBlockForEpochRequest>().epoch_no, 7);
}

#[tokio::test]
async fn get_latest_stable_epoch() {
	let fake = Fake { stable_epoch: Some(41), ..Default::default() };
	let source = serve(&fake).await;
	assert_eq!(source.get_latest_stable_epoch(432).await.unwrap(), Some(EpochNumber(41)));
	assert_eq!(fake.last_request::<proto::GetLatestStableEpochRequest>().security_parameter, 432);

	let source = serve(&Fake::default()).await;
	assert_eq!(source.get_latest_stable_epoch(432).await.unwrap(), None);

	// Stable block in epoch 0: db-sync returns -1, which fails to decode.
	let source = serve(&Fake { stable_epoch: Some(-1), ..Default::default() }).await;
	let error = source.get_latest_stable_epoch(432).await.unwrap_err();
	assert!(matches!(mf_error(error), McFollowerError::InvalidResponse(_)));
}

#[tokio::test]
async fn get_stake_distribution() {
	let fake = Fake {
		stake: vec![
			proto::StakePoolEntry { pool_hash: vec![1; 28], stake: 100 },
			proto::StakePoolEntry { pool_hash: vec![2; 28], stake: u64::MAX },
		],
		..Default::default()
	};
	let source = serve(&fake).await;
	let result = source.get_stake_distribution(EpochNumber(5)).await.unwrap();
	assert_eq!(
		result,
		vec![
			cds::StakePoolEntry { pool_hash: [1; 28], stake: StakeDelegation(100) },
			cds::StakePoolEntry { pool_hash: [2; 28], stake: StakeDelegation(u64::MAX) },
		]
	);
	assert_eq!(fake.last_request::<proto::GetStakeDistributionRequest>().epoch_no, 5);
}

fn token_output(datum: Option<proto::Datum>) -> proto::TokenTxOutput {
	proto::TokenTxOutput {
		origin_tx_hash: vec![3; 32],
		utxo_index: 2,
		tx_epoch_no: 6,
		tx_block_no: 50,
		tx_slot_no: 5000,
		tx_block_index: 1,
		datum,
	}
}

#[tokio::test]
async fn get_token_utxo_for_epoch() {
	let fake = Fake { token_output: Some(token_output(Some(datum(9)))), ..Default::default() };
	let source = serve(&fake).await;
	let result = source.get_token_utxo_for_epoch(&asset(), EpochNumber(8)).await.unwrap();
	assert_eq!(
		result,
		Some(cds::TokenTxOutput {
			origin_tx_hash: [3; 32],
			utxo_index: TxIndex(2),
			tx_epoch_no: EpochNumber(6),
			tx_block_no: BlockNumber(50),
			tx_slot_no: SlotNumber(5000),
			tx_block_index: TxIndexInBlock(1),
			datum: Some(mf::DbDatum(plutus_int(9))),
		})
	);
	assert_eq!(
		fake.last_request::<proto::GetLatestTokenOutputForEpochRequest>(),
		proto::GetLatestTokenOutputForEpochRequest { asset: Some(asset_id()), epoch_no: 8 }
	);

	let source = serve(&Fake::default()).await;
	assert_eq!(source.get_token_utxo_for_epoch(&asset(), EpochNumber(8)).await.unwrap(), None);
}

#[tokio::test]
async fn token_utxo_without_datum_has_no_datum() {
	let fake = Fake { token_output: Some(token_output(None)), ..Default::default() };
	let source = serve(&fake).await;
	let result = source.get_token_utxo_for_epoch(&asset(), EpochNumber(8)).await.unwrap();
	assert_eq!(result.unwrap().datum, None);
}

#[tokio::test]
async fn token_utxo_with_undecodable_datum_fails_the_query() {
	let fake =
		Fake { token_output: Some(token_output(Some(undecodable_datum()))), ..Default::default() };
	let source = serve(&fake).await;
	let error = source.get_token_utxo_for_epoch(&asset(), EpochNumber(8)).await.unwrap_err();
	assert!(matches!(mf_error(error), McFollowerError::UndecodableDatum(_)));
}

#[tokio::test]
async fn token_utxo_with_output_index_beyond_u16_is_invalid() {
	let fake = Fake {
		token_output: Some(proto::TokenTxOutput { utxo_index: 70_000, ..token_output(None) }),
		..Default::default()
	};
	let source = serve(&fake).await;
	let error = source.get_token_utxo_for_epoch(&asset(), EpochNumber(8)).await.unwrap_err();
	assert!(matches!(mf_error(error), McFollowerError::InvalidResponse(_)));
}

#[tokio::test]
async fn get_epoch_nonce() {
	let fake = Fake { nonce: Some(vec![4; 32]), ..Default::default() };
	let source = serve(&fake).await;
	let result = source.get_epoch_nonce(EpochNumber(3)).await.unwrap();
	assert_eq!(result, Some(cds::MainchainEpochNonce(vec![4; 32])));
	assert_eq!(fake.last_request::<proto::GetEpochNonceRequest>().epoch_no, 3);

	let source = serve(&Fake::default()).await;
	assert_eq!(source.get_epoch_nonce(EpochNumber(3)).await.unwrap(), None);
}

fn address_output(utxo_index: u32, datum: Option<proto::Datum>) -> proto::AddressOutputRow {
	proto::AddressOutputRow {
		utxo_tx_hash: vec![9; 32],
		utxo_index,
		tx_block_no: 70,
		tx_slot_no: 7000,
		tx_epoch_no: 6,
		tx_index_in_block: 2,
		address: "addr_test1".into(),
		datum,
		tx_inputs: vec![
			proto::OutputRef { tx_hash: vec![1; 32], index: 0 },
			proto::OutputRef { tx_hash: vec![2; 32], index: 3 },
		],
	}
}

#[tokio::test]
async fn get_utxos_for_address() {
	let fake = Fake {
		address_outputs: vec![
			address_output(0, Some(datum(1))),
			address_output(1, Some(undecodable_datum())),
			address_output(2, None),
		],
		..Default::default()
	};
	let source = serve(&fake).await;
	let result = source
		.get_utxos_for_address(&Address("addr_test1".into()), BlockNumber(80))
		.await
		.unwrap();
	let expected = |index: u16, datum: Option<PlutusData>| cds::MainchainTxOutput {
		utxo_id: UtxoId { tx_hash: McTxHash([9; 32]), index: UtxoIndex(index) },
		tx_block_no: BlockNumber(70),
		tx_slot_no: SlotNumber(7000),
		tx_epoch_no: EpochNumber(6),
		tx_index_in_block: TxIndexInBlock(2),
		address: "addr_test1".into(),
		datum,
		tx_inputs: vec![
			UtxoId { tx_hash: McTxHash([1; 32]), index: UtxoIndex(0) },
			UtxoId { tx_hash: McTxHash([2; 32]), index: UtxoIndex(3) },
		],
	};
	// An undecodable datum is absent and the row is kept.
	assert_eq!(
		result,
		vec![expected(0, Some(plutus_int(1))), expected(1, None), expected(2, None)]
	);
	assert_eq!(
		fake.last_request::<proto::GetUnspentOutputsAtAddressRequest>(),
		proto::GetUnspentOutputsAtAddressRequest { address: "addr_test1".into(), block_no: 80 }
	);
}

// cNIGHT observation row source

fn cardano_position(block_number: u32, tx_index_in_block: u32) -> CardanoPosition {
	CardanoPosition {
		block_hash: McBlockHash([block_number as u8; 32]),
		block_number,
		block_timestamp: TimestampUnixMillis(0),
		tx_index_in_block,
	}
}

const MILLIS: i64 = BLOCK_TIME * 1000;

#[tokio::test]
async fn cnight_asset_and_bounds_need_no_lookup() {
	let fake = Fake { error: Some(Code::Unavailable), ..Default::default() };
	let source = serve(&fake).await;
	assert_eq!(source.resolve_asset(&[1; 28], b"name").await.unwrap(), Some(asset_id()));
	assert_eq!(source.get_low_bounds(1).await.unwrap(), Some(()));
	assert_eq!(source.get_high_bounds(2).await.unwrap(), Some(()));
	assert!(fake.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn get_max_tx_size() {
	let fake = Fake { max_tx_size: Some(16384), ..Default::default() };
	let source = serve(&fake).await;
	assert_eq!(source.get_max_tx_size(44).await.unwrap(), Some(16384));
	assert_eq!(fake.last_request::<proto::GetMaxTxSizeRequest>().block_no, 44);

	let source = serve(&Fake::default()).await;
	assert_eq!(source.get_max_tx_size(44).await.unwrap(), None);

	let source = serve(&Fake { max_tx_size: Some(u64::MAX), ..Default::default() }).await;
	let error = source.get_max_tx_size(44).await.unwrap_err();
	assert!(matches!(mf_error(error), McFollowerError::InvalidResponse(_)));
}

#[tokio::test]
async fn get_registrations() {
	let row = |utxo_index: u32, datum: proto::Datum| proto::RegistrationRow {
		datum: Some(datum),
		block_no: 30,
		block_hash: vec![30; 32],
		block_time_unix_millis: MILLIS,
		tx_index: 2,
		tx_hash: vec![6; 32],
		utxo_index,
	};
	let fake = Fake {
		registrations: vec![row(0, datum(5)), row(1, undecodable_datum())],
		..Default::default()
	};
	let source = serve(&fake).await;
	let (start, end) = (cardano_position(29, 1), cardano_position(31, 4));
	let query = mf::PagedQuery {
		start: &start,
		end: &end,
		limit: 10,
		offset: 2,
		low_bound: (),
		high_bound: (),
	};
	let rows = source.get_registrations("addr_mapping", &asset_id(), &query).await.unwrap();
	// An undecodable datum is absent and the row is kept, so it counts towards the limit.
	assert_eq!(rows.len(), 2);
	assert_eq!(rows[0].full_datum, Some(mf::DbDatum(plutus_int(5))));
	assert_eq!(rows[1].full_datum, None);
	assert_eq!(rows[1].block_number, BlockNumber(30));
	assert_eq!(rows[1].block_hash.0, [30; 32]);
	assert_eq!(rows[1].block_timestamp, time(BLOCK_TIME));
	assert_eq!(rows[1].tx_index_in_block, TxIndexInBlock(2));
	assert_eq!(rows[1].tx_hash, TxHash([6; 32]));
	assert_eq!(rows[1].utxo_index, TxIndex(1));
	assert_eq!(
		fake.last_request::<proto::GetMappingRegistrationsRequest>(),
		proto::GetMappingRegistrationsRequest {
			mapping_validator_address: "addr_mapping".into(),
			auth_token: Some(asset_id()),
			window: Some(proto::PositionWindow {
				from_inclusive: Some(proto::Position { block_no: 29, tx_index: 1 }),
				to_exclusive: Some(proto::Position { block_no: 31, tx_index: 4 }),
			}),
			limit: 10,
			offset: 2,
		}
	);
}

#[tokio::test]
async fn registration_without_datum_is_invalid() {
	let fake = Fake {
		registrations: vec![proto::RegistrationRow {
			datum: None,
			block_hash: vec![1; 32],
			tx_hash: vec![1; 32],
			..Default::default()
		}],
		..Default::default()
	};
	let source = serve(&fake).await;
	let (start, end) = (cardano_position(1, 0), cardano_position(2, 0));
	let query = mf::PagedQuery {
		start: &start,
		end: &end,
		limit: 10,
		offset: 0,
		low_bound: (),
		high_bound: (),
	};
	let error = source.get_registrations("addr", &asset_id(), &query).await.unwrap_err();
	assert!(matches!(mf_error(error), McFollowerError::InvalidResponse(_)));
}

#[tokio::test]
async fn get_deregistrations() {
	let fake = Fake {
		deregistrations: vec![proto::DeregistrationRow {
			datum: Some(undecodable_datum()),
			block_no: 31,
			block_hash: vec![31; 32],
			block_time_unix_millis: MILLIS + 1000,
			tx_index: 0,
			tx_hash: vec![8; 32],
			utxo_tx_hash: vec![6; 32],
			utxo_index: 1,
		}],
		..Default::default()
	};
	let source = serve(&fake).await;
	let (start, end) = (cardano_position(30, 0), cardano_position(32, 1));
	let query = mf::PagedQuery {
		start: &start,
		end: &end,
		limit: 7,
		offset: 0,
		low_bound: (),
		high_bound: (),
	};
	let rows = source.get_deregistrations("addr_mapping", &query).await.unwrap();
	assert_eq!(rows.len(), 1);
	assert_eq!(rows[0].full_datum, None);
	assert_eq!(rows[0].block_number, BlockNumber(31));
	assert_eq!(rows[0].block_hash.0, [31; 32]);
	assert_eq!(rows[0].block_timestamp, time(BLOCK_TIME + 1));
	assert_eq!(rows[0].tx_index_in_block, TxIndexInBlock(0));
	assert_eq!(rows[0].tx_hash, TxHash([8; 32]));
	assert_eq!(rows[0].utxo_tx_hash, TxHash([6; 32]));
	assert_eq!(rows[0].utxo_index, TxIndex(1));
	let request = fake.last_request::<proto::GetMappingDeregistrationsRequest>();
	assert_eq!(request.mapping_validator_address, "addr_mapping");
	assert_eq!((request.limit, request.offset), (7, 0));
	assert_eq!(
		request.window.unwrap().to_exclusive,
		Some(proto::Position { block_no: 32, tx_index: 1 })
	);
}

#[tokio::test]
async fn get_asset_creates() {
	let fake = Fake {
		creates: vec![proto::AssetCreateRow {
			block_no: 40,
			block_hash: vec![40; 32],
			block_time_unix_millis: MILLIS,
			tx_index: 3,
			quantity: i64::MAX as u64,
			holder_address: "addr_holder".into(),
			tx_hash: vec![2; 32],
			utxo_index: 5,
		}],
		..Default::default()
	};
	let source = serve(&fake).await;
	let (start, end) = (cardano_position(39, 0), cardano_position(41, 0));
	let query = mf::PagedQuery {
		start: &start,
		end: &end,
		limit: 3,
		offset: 0,
		low_bound: (),
		high_bound: (),
	};
	let rows = source.get_asset_creates(&asset_id(), &query).await.unwrap();
	assert_eq!(rows.len(), 1);
	assert_eq!(rows[0].block_number, BlockNumber(40));
	assert_eq!(rows[0].block_hash.0, [40; 32]);
	assert_eq!(rows[0].block_timestamp, time(BLOCK_TIME));
	assert_eq!(rows[0].tx_index_in_block, TxIndexInBlock(3));
	assert_eq!(rows[0].quantity, i64::MAX);
	assert_eq!(rows[0].holder_address, "addr_holder");
	assert_eq!(rows[0].tx_hash, TxHash([2; 32]));
	assert_eq!(rows[0].utxo_index, TxIndex(5));
	let request = fake.last_request::<proto::GetAssetCreatesRequest>();
	assert_eq!(request.asset, Some(asset_id()));
	assert_eq!(request.limit, 3);
}

#[tokio::test]
async fn asset_quantity_beyond_i64_fails_the_query() {
	let fake = Fake {
		creates: vec![proto::AssetCreateRow {
			block_hash: vec![1; 32],
			tx_hash: vec![1; 32],
			quantity: i64::MAX as u64 + 1,
			..Default::default()
		}],
		..Default::default()
	};
	let source = serve(&fake).await;
	let (start, end) = (cardano_position(1, 0), cardano_position(2, 0));
	let query = mf::PagedQuery {
		start: &start,
		end: &end,
		limit: 3,
		offset: 0,
		low_bound: (),
		high_bound: (),
	};
	let error = source.get_asset_creates(&asset_id(), &query).await.unwrap_err();
	assert!(matches!(mf_error(error), McFollowerError::InvalidResponse(_)));
}

#[tokio::test]
async fn get_asset_spends() {
	let fake = Fake {
		spends: vec![proto::AssetSpendRow {
			block_no: 42,
			block_hash: vec![42; 32],
			block_time_unix_millis: MILLIS,
			tx_index: 1,
			quantity: 77,
			holder_address: "addr_holder".into(),
			utxo_tx_hash: vec![2; 32],
			utxo_index: 5,
			spending_tx_hash: vec![3; 32],
		}],
		..Default::default()
	};
	let source = serve(&fake).await;
	let (start, end) = (cardano_position(41, 0), cardano_position(43, 0));
	let query = mf::PagedQuery {
		start: &start,
		end: &end,
		limit: 3,
		offset: 1,
		low_bound: (),
		high_bound: (),
	};
	let rows = source.get_asset_spends(&asset_id(), &query).await.unwrap();
	assert_eq!(rows.len(), 1);
	assert_eq!(rows[0].block_number, BlockNumber(42));
	assert_eq!(rows[0].block_hash.0, [42; 32]);
	assert_eq!(rows[0].block_timestamp, time(BLOCK_TIME));
	assert_eq!(rows[0].tx_index_in_block, TxIndexInBlock(1));
	assert_eq!(rows[0].quantity, 77);
	assert_eq!(rows[0].holder_address, "addr_holder");
	assert_eq!(rows[0].utxo_tx_hash, TxHash([2; 32]));
	assert_eq!(rows[0].utxo_index, TxIndex(5));
	assert_eq!(rows[0].spending_tx_hash, TxHash([3; 32]));
	let request = fake.last_request::<proto::GetAssetSpendsRequest>();
	assert_eq!(request.asset, Some(asset_id()));
	assert_eq!((request.limit, request.offset), (3, 1));
}

// Federated authority row source

fn governance_output(datum: proto::Datum) -> proto::GovernanceOutput {
	proto::GovernanceOutput {
		datum: Some(datum),
		block_no: 60,
		block_hash: vec![60; 32],
		tx_index: 4,
		tx_hash: vec![5; 32],
		utxo_index: 0,
	}
}

#[tokio::test]
async fn get_governance_body_utxo() {
	let fake = Fake { governance: Some(governance_output(datum(3))), ..Default::default() };
	let source = serve(&fake).await;
	let row = source
		.get_governance_body_utxo("addr_council", &PolicyId([7; 28]), 61)
		.await
		.unwrap()
		.unwrap();
	assert_eq!(row.full_datum, mf::DbDatum(plutus_int(3)));
	assert_eq!(row.block_number, BlockNumber(60));
	assert_eq!(row.block_hash.0, [60; 32]);
	assert_eq!(row.tx_index_in_block, TxIndexInBlock(4));
	assert_eq!(row.tx_hash, TxHash([5; 32]));
	assert_eq!(row.utxo_index, TxIndex(0));
	assert_eq!(
		fake.last_request::<proto::GetLatestGovernanceOutputRequest>(),
		proto::GetLatestGovernanceOutputRequest {
			address: "addr_council".into(),
			policy_id: vec![7; 28],
			block_no: 61,
		}
	);

	let source = serve(&Fake::default()).await;
	let result = source.get_governance_body_utxo("addr_council", &PolicyId([7; 28]), 61).await;
	assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn governance_utxo_with_undecodable_datum_fails_the_query() {
	let fake =
		Fake { governance: Some(governance_output(undecodable_datum())), ..Default::default() };
	let source = serve(&fake).await;
	let error = source
		.get_governance_body_utxo("addr_council", &PolicyId([7; 28]), 61)
		.await
		.unwrap_err();
	assert!(matches!(mf_error(error), McFollowerError::UndecodableDatum(_)));
}
