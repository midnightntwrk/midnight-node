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

//! The row source traits of the data sources, implemented against mc-follower. Responses are
//! converted into the db-sync row types, failing where decoding the db-sync row fails.

use crate::{McFollowerError, McFollowerRowSource, RESPONSE_TIMEOUT, datum, proto};
use cardano_serialization_lib::PlutusData;
use chrono::{DateTime, NaiveDateTime};
use db_sync_sqlx::{
	Address, Asset, BlockNumber, EpochNumber, SlotNumber, StakeDelegation, TxHash, TxIndex,
	TxIndexInBlock,
};
use midnight_primitives_mainchain_follower::data_source::candidates_data_source::row_source as cds;
use midnight_primitives_mainchain_follower::db as mf;
use partner_chains_db_sync_data_sources::row_source as pc;
use sidechain_domain::{McBlockHash, McTxHash, PolicyId, UtxoId, UtxoIndex};

type Result<T> = std::result::Result<T, McFollowerError>;

fn pc_err(e: impl Into<McFollowerError>) -> pc::RowSourceError {
	pc::RowSourceError::Other(Box::new(e.into()))
}

fn mf_err(e: impl Into<McFollowerError>) -> mf::RowSourceError {
	mf::RowSourceError::Other(Box::new(e.into()))
}

fn invalid(message: String) -> McFollowerError {
	McFollowerError::InvalidResponse(message)
}

fn required<T>(field: Option<T>, name: &str) -> Result<T> {
	field.ok_or_else(|| invalid(format!("missing {name}")))
}

fn bytes_array<const N: usize>(bytes: Vec<u8>, name: &str) -> Result<[u8; N]> {
	let len = bytes.len();
	bytes
		.try_into()
		.map_err(|_| invalid(format!("{name} has {len} bytes, expected {N}")))
}

fn utxo_index(index: u32) -> Result<u16> {
	index
		.try_into()
		.map_err(|_| invalid(format!("output index {index} out of range")))
}

fn i64_value(value: u64, name: &str) -> Result<i64> {
	value.try_into().map_err(|_| invalid(format!("{name} {value} out of range")))
}

/// db-sync `block.time`, a timestamp in whole seconds
fn time_from_seconds(seconds: i64) -> Result<NaiveDateTime> {
	DateTime::from_timestamp(seconds, 0)
		.map(|time| time.naive_utc())
		.ok_or_else(|| invalid(format!("block time {seconds} out of range")))
}

fn time_from_millis(millis: i64) -> Result<NaiveDateTime> {
	DateTime::from_timestamp_millis(millis)
		.map(|time| time.naive_utc())
		.ok_or_else(|| invalid(format!("block time {millis} ms out of range")))
}

fn millis(time: NaiveDateTime) -> i64 {
	time.and_utc().timestamp_millis()
}

/// A sum of token amounts, which the db-sync row decodes into a `u64`
fn token_amount(value: Option<proto::Uint128>, name: &str) -> Result<pc::NativeTokenAmount> {
	let value = required(value, name)?;
	if value.hi != 0 {
		return Err(invalid(format!("{name} exceeds u64")));
	}
	Ok(pc::NativeTokenAmount(value.lo))
}

/// A datum of a row that fails the whole query when undecodable, as db-sync's `DbDatum`
fn strict_datum(datum: proto::Datum) -> Result<mf::DbDatum> {
	datum::plutus_data_like_db_sync(&datum.cbor)
		.map(mf::DbDatum)
		.map_err(McFollowerError::UndecodableDatum)
}

/// A datum of a row that is absent when undecodable, as db-sync's `DbDatumIfValid`
fn lenient_datum(datum: Option<proto::Datum>) -> Option<PlutusData> {
	datum::plutus_data_if_valid(datum.as_ref().map(|datum| datum.cbor.as_slice()))
}

fn pc_block(block: proto::Block) -> Result<pc::Block> {
	Ok(pc::Block {
		block_no: BlockNumber(block.block_no),
		hash: bytes_array(block.hash, "block hash")?,
		epoch_no: EpochNumber(block.epoch_no),
		slot_no: SlotNumber(block.slot_no),
		time: time_from_seconds(block.time_unix_seconds)?,
	})
}

fn mf_block(block: proto::Block) -> Result<mf::Block> {
	Ok(mf::Block {
		block_number: BlockNumber(block.block_no),
		hash: bytes_array(block.hash, "block hash")?,
		epoch_number: EpochNumber(block.epoch_no),
		slot_number: SlotNumber(block.slot_no),
		time: time_from_seconds(block.time_unix_seconds)?,
		tx_count: i64_value(block.tx_count, "tx count")?,
	})
}

fn cds_block(block: proto::Block) -> Result<cds::Block> {
	Ok(cds::Block {
		block_no: BlockNumber(block.block_no),
		hash: bytes_array(block.hash, "block hash")?,
		epoch_no: EpochNumber(block.epoch_no),
		slot_no: SlotNumber(block.slot_no),
	})
}

fn position(block_number: u32, tx_index: u32) -> proto::Position {
	proto::Position { block_no: block_number, tx_index }
}

fn asset_id(policy: &[u8], name: &[u8]) -> proto::AssetId {
	proto::AssetId { policy_id: policy.to_vec(), asset_name: name.to_vec() }
}

/// The window of a paged cNIGHT query, with the limits the db-sync queries assert
fn window(query: &mf::PagedQuery<'_, ()>) -> (Option<proto::PositionWindow>, u32, u32) {
	assert!(query.limit < i32::MAX as usize);
	assert!(query.offset < i32::MAX as usize);
	let window = proto::PositionWindow {
		from_inclusive: Some(position(query.start.block_number, query.start.tx_index_in_block)),
		to_exclusive: Some(position(query.end.block_number, query.end.tx_index_in_block)),
	};
	(Some(window), query.limit as u32, query.offset as u32)
}

/// Collects a streamed response, failing when no message arrives within [RESPONSE_TIMEOUT]
async fn collect<T, R>(
	mut stream: tonic::Streaming<T>,
	convert: impl Fn(T) -> Result<R>,
) -> Result<Vec<R>> {
	let mut rows = Vec::new();
	while let Some(message) = tokio::time::timeout(RESPONSE_TIMEOUT, stream.message())
		.await
		.map_err(|_| McFollowerError::Timeout)??
	{
		rows.push(convert(message)?);
	}
	Ok(rows)
}

impl McFollowerRowSource {
	async fn latest_block(&self) -> Result<Option<proto::Block>> {
		Ok(self
			.client()
			.get_latest_block(proto::GetLatestBlockRequest {})
			.await?
			.into_inner()
			.block)
	}

	async fn block_by_hash(&self, hash: McBlockHash) -> Result<Option<proto::Block>> {
		let request = proto::GetBlockByHashRequest { hash: hash.0.to_vec() };
		Ok(self.client().get_block_by_hash(request).await?.into_inner().block)
	}

	async fn blocks_by_numbers(
		&self,
		from: BlockNumber,
		to: BlockNumber,
	) -> Result<Vec<pc::Block>> {
		let request =
			proto::GetBlocksByNumberRangeRequest { from_block_no: from.0, to_block_no: to.0 };
		collect(self.client().get_blocks_by_number_range(request).await?.into_inner(), pc_block)
			.await
	}

	async fn highest_block(
		&self,
		max_block_number: BlockNumber,
		min_time: NaiveDateTime,
		min_slot: SlotNumber,
		max_time: NaiveDateTime,
		max_slot: SlotNumber,
	) -> Result<Option<pc::Block>> {
		let request = proto::GetHighestBlockInWindowRequest {
			max_block_no: max_block_number.0,
			min_slot: min_slot.0,
			max_slot: max_slot.0,
			min_time_unix_millis: millis(min_time),
			max_time_unix_millis: millis(max_time),
		};
		let block = self.client().get_highest_block_in_window(request).await?.into_inner().block;
		block.map(pc_block).transpose()
	}

	async fn tx_position(&self, tx_hash: TxHash) -> Result<Option<pc::TxBlockInfo>> {
		let request = proto::GetTxPositionRequest { tx_hash: tx_hash.0.to_vec() };
		let position = self.client().get_tx_position(request).await?.into_inner().position;
		Ok(position.map(|position| pc::TxBlockInfo {
			block_number: BlockNumber(position.block_no),
			tx_ix: TxIndexInBlock(position.tx_index),
		}))
	}

	async fn bridge_txs(
		&self,
		ics_address: &Address,
		reserve_address: &Address,
		native_token: Asset,
		checkpoint: pc::ResolvedBridgeDataCheckpoint,
		to_block: BlockNumber,
		max_txs: Option<u32>,
	) -> Result<Vec<pc::BridgeTx>> {
		use proto::get_bridge_txs_request::Lower;
		let lower = match checkpoint {
			pc::ResolvedBridgeDataCheckpoint::Block { number } => Lower::AfterBlockNo(number.0),
			pc::ResolvedBridgeDataCheckpoint::Tx { block_number, tx_ix } => {
				Lower::AfterPosition(position(block_number.0, tx_ix.0))
			},
			pc::ResolvedBridgeDataCheckpoint::TxReserveTransfer { block_number, tx_ix } => {
				Lower::FromPosition(position(block_number.0, tx_ix.0))
			},
		};
		let request = proto::GetBridgeTxsRequest {
			ics_address: ics_address.0.clone(),
			reserve_address: reserve_address.0.clone(),
			token: Some(asset_id(&native_token.policy_id.0, &native_token.asset_name.0)),
			lower: Some(lower),
			to_block_no: to_block.0,
			max_txs,
		};
		collect(self.client().get_bridge_txs(request).await?.into_inner(), |row| {
			Ok(pc::BridgeTx {
				block_number: BlockNumber(row.block_no),
				tx_ix: TxIndexInBlock(row.tx_index),
				tx_hash: TxHash(bytes_array(row.tx_hash, "tx hash")?),
				c2m_metadata: row
					.c2m_metadatum_cbor
					.and_then(|cbor| datum::c2m_metadata_like_db_sync(&cbor)),
				bridge_in: token_amount(row.bridge_in, "bridge_in")?,
				bridge_out: token_amount(row.bridge_out, "bridge_out")?,
				reserve_in: token_amount(row.reserve_in, "reserve_in")?,
				reserve_out: token_amount(row.reserve_out, "reserve_out")?,
			})
		})
		.await
	}

	async fn latest_block_for_epoch(&self, epoch: EpochNumber) -> Result<Option<cds::Block>> {
		let request = proto::GetLatestBlockForEpochRequest { epoch_no: epoch.0 };
		let block = self.client().get_latest_block_for_epoch(request).await?.into_inner().block;
		block.map(cds_block).transpose()
	}

	async fn latest_stable_epoch(&self, security_parameter: u32) -> Result<Option<EpochNumber>> {
		let request = proto::GetLatestStableEpochRequest { security_parameter };
		let epoch = self.client().get_latest_stable_epoch(request).await?.into_inner().epoch_no;
		epoch
			.map(|epoch| {
				u32::try_from(epoch)
					.map(EpochNumber)
					.map_err(|_| invalid(format!("stable epoch {epoch} out of range")))
			})
			.transpose()
	}

	async fn stake_distribution(&self, epoch: EpochNumber) -> Result<Vec<cds::StakePoolEntry>> {
		let request = proto::GetStakeDistributionRequest { epoch_no: epoch.0 };
		collect(self.client().get_stake_distribution(request).await?.into_inner(), |entry| {
			Ok(cds::StakePoolEntry {
				pool_hash: bytes_array(entry.pool_hash, "pool hash")?,
				stake: StakeDelegation(entry.stake),
			})
		})
		.await
	}

	async fn token_utxo_for_epoch(
		&self,
		asset: &Asset,
		epoch: EpochNumber,
	) -> Result<Option<cds::TokenTxOutput>> {
		let request = proto::GetLatestTokenOutputForEpochRequest {
			asset: Some(asset_id(&asset.policy_id.0, &asset.asset_name.0)),
			epoch_no: epoch.0,
		};
		let output = self
			.client()
			.get_latest_token_output_for_epoch(request)
			.await?
			.into_inner()
			.output;
		output
			.map(|output| {
				Ok(cds::TokenTxOutput {
					origin_tx_hash: bytes_array(output.origin_tx_hash, "tx hash")?,
					utxo_index: TxIndex(utxo_index(output.utxo_index)?),
					tx_epoch_no: EpochNumber(output.tx_epoch_no),
					tx_block_no: BlockNumber(output.tx_block_no),
					tx_slot_no: SlotNumber(output.tx_slot_no),
					tx_block_index: TxIndexInBlock(output.tx_block_index),
					datum: output.datum.map(strict_datum).transpose()?,
				})
			})
			.transpose()
	}

	async fn epoch_nonce(&self, epoch: EpochNumber) -> Result<Option<cds::MainchainEpochNonce>> {
		let request = proto::GetEpochNonceRequest { epoch_no: epoch.0 };
		let nonce = self.client().get_epoch_nonce(request).await?.into_inner().nonce;
		Ok(nonce.map(cds::MainchainEpochNonce))
	}

	async fn utxos_for_address(
		&self,
		address: &Address,
		block: BlockNumber,
	) -> Result<Vec<cds::MainchainTxOutput>> {
		let request = proto::GetUnspentOutputsAtAddressRequest {
			address: address.0.clone(),
			block_no: block.0,
		};
		collect(self.client().get_unspent_outputs_at_address(request).await?.into_inner(), |row| {
			let tx_inputs = row
				.tx_inputs
				.into_iter()
				.map(|input| {
					Ok(UtxoId {
						tx_hash: McTxHash(bytes_array(input.tx_hash, "input tx hash")?),
						index: UtxoIndex(utxo_index(input.index)?),
					})
				})
				.collect::<Result<_>>()?;
			Ok(cds::MainchainTxOutput {
				utxo_id: UtxoId {
					tx_hash: McTxHash(bytes_array(row.utxo_tx_hash, "tx hash")?),
					index: UtxoIndex(utxo_index(row.utxo_index)?),
				},
				tx_block_no: BlockNumber(row.tx_block_no),
				tx_slot_no: SlotNumber(row.tx_slot_no),
				tx_epoch_no: EpochNumber(row.tx_epoch_no),
				tx_index_in_block: TxIndexInBlock(row.tx_index_in_block),
				address: row.address,
				datum: lenient_datum(row.datum),
				tx_inputs,
			})
		})
		.await
	}

	async fn max_tx_size(&self, block_no: u32) -> Result<Option<i64>> {
		let request = proto::GetMaxTxSizeRequest { block_no };
		let size = self.client().get_max_tx_size(request).await?.into_inner().max_tx_size;
		size.map(|size| i64_value(size, "max tx size")).transpose()
	}

	async fn registrations(
		&self,
		address: &str,
		auth_token: &proto::AssetId,
		query: &mf::PagedQuery<'_, ()>,
	) -> Result<Vec<mf::RegistrationRow>> {
		let (window, limit, offset) = window(query);
		let request = proto::GetMappingRegistrationsRequest {
			mapping_validator_address: address.to_string(),
			auth_token: Some(auth_token.clone()),
			window,
			limit,
			offset,
		};
		collect(self.client().get_mapping_registrations(request).await?.into_inner(), |row| {
			Ok(mf::RegistrationRow {
				full_datum: lenient_datum(Some(required(row.datum, "datum")?)).map(mf::DbDatum),
				block_number: BlockNumber(row.block_no),
				block_hash: mf::DbBlockHash(bytes_array(row.block_hash, "block hash")?),
				block_timestamp: time_from_millis(row.block_time_unix_millis)?,
				tx_index_in_block: TxIndexInBlock(row.tx_index),
				tx_hash: TxHash(bytes_array(row.tx_hash, "tx hash")?),
				utxo_index: TxIndex(utxo_index(row.utxo_index)?),
			})
		})
		.await
	}

	async fn deregistrations(
		&self,
		address: &str,
		query: &mf::PagedQuery<'_, ()>,
	) -> Result<Vec<mf::DeregistrationRow>> {
		let (window, limit, offset) = window(query);
		let request = proto::GetMappingDeregistrationsRequest {
			mapping_validator_address: address.to_string(),
			window,
			limit,
			offset,
		};
		collect(self.client().get_mapping_deregistrations(request).await?.into_inner(), |row| {
			Ok(mf::DeregistrationRow {
				full_datum: lenient_datum(Some(required(row.datum, "datum")?)).map(mf::DbDatum),
				block_number: BlockNumber(row.block_no),
				block_hash: mf::DbBlockHash(bytes_array(row.block_hash, "block hash")?),
				block_timestamp: time_from_millis(row.block_time_unix_millis)?,
				tx_index_in_block: TxIndexInBlock(row.tx_index),
				tx_hash: TxHash(bytes_array(row.tx_hash, "tx hash")?),
				utxo_tx_hash: TxHash(bytes_array(row.utxo_tx_hash, "spent tx hash")?),
				utxo_index: TxIndex(utxo_index(row.utxo_index)?),
			})
		})
		.await
	}

	async fn asset_creates(
		&self,
		asset: &proto::AssetId,
		query: &mf::PagedQuery<'_, ()>,
	) -> Result<Vec<mf::AssetCreateRow>> {
		let (window, limit, offset) = window(query);
		let request =
			proto::GetAssetCreatesRequest { asset: Some(asset.clone()), window, limit, offset };
		collect(self.client().get_asset_creates(request).await?.into_inner(), |row| {
			Ok(mf::AssetCreateRow {
				block_number: BlockNumber(row.block_no),
				block_hash: mf::DbBlockHash(bytes_array(row.block_hash, "block hash")?),
				block_timestamp: time_from_millis(row.block_time_unix_millis)?,
				tx_index_in_block: TxIndexInBlock(row.tx_index),
				quantity: i64_value(row.quantity, "quantity")?,
				holder_address: row.holder_address,
				tx_hash: TxHash(bytes_array(row.tx_hash, "tx hash")?),
				utxo_index: TxIndex(utxo_index(row.utxo_index)?),
			})
		})
		.await
	}

	async fn asset_spends(
		&self,
		asset: &proto::AssetId,
		query: &mf::PagedQuery<'_, ()>,
	) -> Result<Vec<mf::AssetSpendRow>> {
		let (window, limit, offset) = window(query);
		let request =
			proto::GetAssetSpendsRequest { asset: Some(asset.clone()), window, limit, offset };
		collect(self.client().get_asset_spends(request).await?.into_inner(), |row| {
			Ok(mf::AssetSpendRow {
				block_number: BlockNumber(row.block_no),
				block_hash: mf::DbBlockHash(bytes_array(row.block_hash, "block hash")?),
				block_timestamp: time_from_millis(row.block_time_unix_millis)?,
				tx_index_in_block: TxIndexInBlock(row.tx_index),
				quantity: i64_value(row.quantity, "quantity")?,
				holder_address: row.holder_address,
				utxo_tx_hash: TxHash(bytes_array(row.utxo_tx_hash, "spent tx hash")?),
				utxo_index: TxIndex(utxo_index(row.utxo_index)?),
				spending_tx_hash: TxHash(bytes_array(row.spending_tx_hash, "tx hash")?),
			})
		})
		.await
	}

	async fn governance_body_utxo(
		&self,
		script_address: &str,
		policy_id: &PolicyId,
		block_number: u32,
	) -> Result<Option<mf::GovernanceBodyUtxoRow>> {
		let request = proto::GetLatestGovernanceOutputRequest {
			address: script_address.to_string(),
			policy_id: policy_id.0.to_vec(),
			block_no: block_number,
		};
		let output = self.client().get_latest_governance_output(request).await?.into_inner().output;
		output
			.map(|output| {
				Ok(mf::GovernanceBodyUtxoRow {
					full_datum: strict_datum(required(output.datum, "datum")?)?,
					block_number: BlockNumber(output.block_no),
					block_hash: mf::DbBlockHash(bytes_array(output.block_hash, "block hash")?),
					tx_index_in_block: TxIndexInBlock(output.tx_index),
					tx_hash: TxHash(bytes_array(output.tx_hash, "tx hash")?),
					utxo_index: TxIndex(utxo_index(output.utxo_index)?),
				})
			})
			.transpose()
	}
}

#[async_trait::async_trait]
impl pc::BlockRowSource for McFollowerRowSource {
	async fn get_latest_block_info(
		&self,
	) -> std::result::Result<Option<pc::Block>, pc::RowSourceError> {
		self.latest_block()
			.await
			.and_then(|b| b.map(pc_block).transpose())
			.map_err(pc_err)
	}

	async fn get_blocks_by_numbers(
		&self,
		from: BlockNumber,
		to: BlockNumber,
	) -> std::result::Result<Vec<pc::Block>, pc::RowSourceError> {
		self.blocks_by_numbers(from, to).await.map_err(pc_err)
	}

	async fn get_highest_block(
		&self,
		max_block_number: BlockNumber,
		min_time: NaiveDateTime,
		min_slot: SlotNumber,
		max_time: NaiveDateTime,
		max_slot: SlotNumber,
	) -> std::result::Result<Option<pc::Block>, pc::RowSourceError> {
		self.highest_block(max_block_number, min_time, min_slot, max_time, max_slot)
			.await
			.map_err(pc_err)
	}

	async fn get_block_by_hash(
		&self,
		hash: McBlockHash,
	) -> std::result::Result<Option<pc::Block>, pc::RowSourceError> {
		self.block_by_hash(hash)
			.await
			.and_then(|b| b.map(pc_block).transpose())
			.map_err(pc_err)
	}
}

#[async_trait::async_trait]
impl pc::BridgeRowSource for McFollowerRowSource {
	async fn get_block_info_for_tx_hash(
		&self,
		tx_hash: TxHash,
	) -> std::result::Result<Option<pc::TxBlockInfo>, pc::RowSourceError> {
		self.tx_position(tx_hash).await.map_err(pc_err)
	}

	async fn get_bridge_txs(
		&self,
		ics_address: &Address,
		reserve_address: &Address,
		native_token: Asset,
		checkpoint: pc::ResolvedBridgeDataCheckpoint,
		to_block: BlockNumber,
		max_txs: Option<u32>,
	) -> std::result::Result<Vec<pc::BridgeTx>, pc::RowSourceError> {
		self.bridge_txs(ics_address, reserve_address, native_token, checkpoint, to_block, max_txs)
			.await
			.map_err(pc_err)
	}
}

#[async_trait::async_trait]
impl cds::CandidatesRowSource for McFollowerRowSource {
	async fn get_latest_block_for_epoch(
		&self,
		epoch: EpochNumber,
	) -> std::result::Result<Option<cds::Block>, mf::RowSourceError> {
		self.latest_block_for_epoch(epoch).await.map_err(mf_err)
	}

	async fn get_latest_stable_epoch(
		&self,
		security_parameter: u32,
	) -> std::result::Result<Option<EpochNumber>, mf::RowSourceError> {
		self.latest_stable_epoch(security_parameter).await.map_err(mf_err)
	}

	async fn get_stake_distribution(
		&self,
		epoch: EpochNumber,
	) -> std::result::Result<Vec<cds::StakePoolEntry>, mf::RowSourceError> {
		self.stake_distribution(epoch).await.map_err(mf_err)
	}

	async fn get_token_utxo_for_epoch(
		&self,
		asset: &Asset,
		epoch: EpochNumber,
	) -> std::result::Result<Option<cds::TokenTxOutput>, mf::RowSourceError> {
		self.token_utxo_for_epoch(asset, epoch).await.map_err(mf_err)
	}

	async fn get_epoch_nonce(
		&self,
		epoch: EpochNumber,
	) -> std::result::Result<Option<cds::MainchainEpochNonce>, mf::RowSourceError> {
		self.epoch_nonce(epoch).await.map_err(mf_err)
	}

	async fn get_utxos_for_address(
		&self,
		address: &Address,
		block: BlockNumber,
	) -> std::result::Result<Vec<cds::MainchainTxOutput>, mf::RowSourceError> {
		self.utxos_for_address(address, block).await.map_err(mf_err)
	}
}

#[async_trait::async_trait]
impl mf::McBlockByHashRowSource for McFollowerRowSource {
	async fn get_block_by_hash(
		&self,
		hash: McBlockHash,
	) -> std::result::Result<Option<mf::Block>, mf::RowSourceError> {
		self.block_by_hash(hash)
			.await
			.and_then(|b| b.map(mf_block).transpose())
			.map_err(mf_err)
	}
}

#[async_trait::async_trait]
impl mf::CNightObservationRowSource for McFollowerRowSource {
	/// The asset itself: mc-follower takes `(policy, name)` in its requests
	type AssetIdent = proto::AssetId;
	/// mc-follower needs no pre-filter
	type Bounds = ();

	async fn resolve_asset(
		&self,
		policy: &[u8],
		name: &[u8],
	) -> std::result::Result<Option<proto::AssetId>, mf::RowSourceError> {
		Ok(Some(asset_id(policy, name)))
	}

	async fn get_low_bounds(
		&self,
		_block_no: i64,
	) -> std::result::Result<Option<()>, mf::RowSourceError> {
		Ok(Some(()))
	}

	async fn get_high_bounds(
		&self,
		_block_no: i64,
	) -> std::result::Result<Option<()>, mf::RowSourceError> {
		Ok(Some(()))
	}

	async fn get_max_tx_size(
		&self,
		block_no: u32,
	) -> std::result::Result<Option<i64>, mf::RowSourceError> {
		self.max_tx_size(block_no).await.map_err(mf_err)
	}

	async fn get_registrations(
		&self,
		address: &str,
		auth_token: &proto::AssetId,
		query: &mf::PagedQuery<'_, ()>,
	) -> std::result::Result<Vec<mf::RegistrationRow>, mf::RowSourceError> {
		self.registrations(address, auth_token, query).await.map_err(mf_err)
	}

	async fn get_deregistrations(
		&self,
		address: &str,
		query: &mf::PagedQuery<'_, ()>,
	) -> std::result::Result<Vec<mf::DeregistrationRow>, mf::RowSourceError> {
		self.deregistrations(address, query).await.map_err(mf_err)
	}

	async fn get_asset_creates(
		&self,
		asset: &proto::AssetId,
		query: &mf::PagedQuery<'_, ()>,
	) -> std::result::Result<Vec<mf::AssetCreateRow>, mf::RowSourceError> {
		self.asset_creates(asset, query).await.map_err(mf_err)
	}

	async fn get_asset_spends(
		&self,
		asset: &proto::AssetId,
		query: &mf::PagedQuery<'_, ()>,
	) -> std::result::Result<Vec<mf::AssetSpendRow>, mf::RowSourceError> {
		self.asset_spends(asset, query).await.map_err(mf_err)
	}
}

#[async_trait::async_trait]
impl mf::FederatedAuthorityRowSource for McFollowerRowSource {
	async fn get_governance_body_utxo(
		&self,
		script_address: &str,
		policy_id: &PolicyId,
		block_number: u32,
	) -> std::result::Result<Option<mf::GovernanceBodyUtxoRow>, mf::RowSourceError> {
		self.governance_body_utxo(script_address, policy_id, block_number)
			.await
			.map_err(mf_err)
	}
}
