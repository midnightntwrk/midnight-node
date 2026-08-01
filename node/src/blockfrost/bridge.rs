// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Token bridge data source: Cardano-to-Midnight transfers and their checkpoints.

use std::sync::Arc;

use blockfrost::BlockCursor;
use partner_chains_plutus_data::bridge::TOKEN_TRANSFER_METADATUM_KEY;
use sidechain_domain::*;
use sp_partner_chains_bridge::{
	BridgeDataCheckpoint, BridgeTransferV1, MainChainScripts, TokenBridgeDataSource,
	TransferRecipient,
};

use super::client::*;
use super::convert::*;
use super::support::*;

// ---------------------------------------------------------------------------
// Token bridge
// ---------------------------------------------------------------------------

/// Per-tx bridge token flow totals, the Blockfrost equivalent of the db-sync
/// `get_bridge_txs` row.
#[derive(Debug, Clone, PartialEq)]
struct BridgeTx {
	block_number: u32,
	tx_ix: u32,
	tx_hash: McTxHash,
	c2m_metadata: Option<serde_json::Value>,
	bridge_in: u64,
	bridge_out: u64,
	reserve_in: u64,
	reserve_out: u64,
}

#[derive(Debug, Clone, Copy)]
enum ResolvedCheckpoint {
	Tx { block_number: u32, tx_ix: u32 },
	Block { number: u32 },
}

impl ResolvedCheckpoint {
	fn block_number(&self) -> u32 {
		match self {
			Self::Tx { block_number, .. } => *block_number,
			Self::Block { number } => *number,
		}
	}

	/// Strict comparison, mirroring the SQL `block.block_no > n` /
	/// `(block.block_no, tx.block_index) > (b, ix)` checkpoint filters.
	fn is_before(&self, block_number: u32, tx_ix: u32) -> bool {
		match self {
			Self::Block { number } => block_number > *number,
			Self::Tx { block_number: cp_block, tx_ix: cp_ix } => {
				(block_number, tx_ix) > (*cp_block, *cp_ix)
			},
		}
	}
}

/// Mirrors `txs_to_transfers` from the db-sync bridge data source.
fn txs_to_transfers<RecipientAddress>(
	txs: Vec<BridgeTx>,
	max_transfers: u32,
	block_bound: u32,
) -> (Vec<BridgeTransferV1<RecipientAddress>>, BridgeDataCheckpoint)
where
	RecipientAddress: for<'a> TryFrom<&'a [u8]>,
{
	let mut transfers: Vec<BridgeTransferV1<RecipientAddress>> = vec![];
	let mut checkpoint = BridgeDataCheckpoint::Block(McBlockNumber(block_bound));
	// Add Cardano transaction transfers only if all of them fit into max_transfers
	for tx in &txs {
		let tx_transfers = tx_to_transfers::<RecipientAddress>(tx.clone());
		// Would go over limit, return accumulated state from previous iteration
		if transfers.len() + tx_transfers.len() > max_transfers as usize {
			return (transfers, checkpoint);
		}
		transfers.extend(tx_transfers);
		checkpoint = BridgeDataCheckpoint::Tx(tx.tx_hash)
	}
	let checkpoint = if transfers.len() == max_transfers as usize {
		checkpoint
	} else {
		BridgeDataCheckpoint::Block(McBlockNumber(block_bound))
	};
	(transfers, checkpoint)
}

/// Mirrors `tx_to_transfers`: Reserve can unlock only to ICS — if the reserve shrank,
/// the delta went to ICS; the rest of the ICS surplus is a user transfer.
fn tx_to_transfers<RecipientAddress>(tx: BridgeTx) -> Vec<BridgeTransferV1<RecipientAddress>>
where
	RecipientAddress: for<'a> TryFrom<&'a [u8]>,
{
	let mc_tx_hash = tx.tx_hash;
	let reserve_debit: u64 = tx.reserve_in.saturating_sub(tx.reserve_out);
	let ics_credit: u64 = tx.bridge_out.saturating_sub(tx.bridge_in);
	let locked_amount = ics_credit.saturating_sub(reserve_debit);

	let mut transfers = Vec::with_capacity(2);

	if reserve_debit > 0 {
		let recipient = TransferRecipient::Reserve;
		transfers.push(BridgeTransferV1 { mc_tx_hash, amount: reserve_debit, recipient })
	}

	if locked_amount > 0 {
		let recipient = metadata_to_recipient(tx.c2m_metadata);
		transfers.push(BridgeTransferV1 { mc_tx_hash, amount: locked_amount, recipient })
	}

	transfers
}

/// Mirrors `metadata_to_recipient`: the metadata value must be `["0x<hex>"]`.
fn metadata_to_recipient<RecipientAddress>(
	metadata: Option<serde_json::Value>,
) -> TransferRecipient<RecipientAddress>
where
	RecipientAddress: for<'a> TryFrom<&'a [u8]>,
{
	match metadata {
		Some(serde_json::Value::Array(values)) => match values.as_slice() {
			[serde_json::Value::String(str)] => {
				match str
					.strip_prefix("0x")
					.and_then(|str| hex::decode(str).ok())
					.and_then(|bytes| RecipientAddress::try_from(&bytes).ok())
				{
					Some(recipient) => TransferRecipient::Address { recipient },
					_ => TransferRecipient::Invalid,
				}
			},
			_ => TransferRecipient::Invalid,
		},
		_ => TransferRecipient::Invalid,
	}
}

/// Mirrors `TokenBridgeDataSourceImpl` from `partner-chains-db-sync-data-sources`.
/// No lookahead cache: that is purely a sync-speed optimization on the db-sync path.
pub struct BlockfrostTokenBridgeDataSource {
	client: Arc<BlockfrostClient>,
}

impl BlockfrostTokenBridgeDataSource {
	pub fn new(client: Arc<BlockfrostClient>) -> Self {
		Self { client }
	}
}

#[async_trait::async_trait]
impl<RecipientAddress> TokenBridgeDataSource<RecipientAddress> for BlockfrostTokenBridgeDataSource
where
	RecipientAddress: std::fmt::Debug + Send + Sync,
	RecipientAddress: for<'a> TryFrom<&'a [u8]>,
{
	async fn get_transfers(
		&self,
		main_chain_scripts: MainChainScripts,
		data_checkpoint: BridgeDataCheckpoint,
		max_transfers: u32,
		current_mc_block: McBlockHash,
	) -> Result<(Vec<BridgeTransferV1<RecipientAddress>>, BridgeDataCheckpoint), BoxError> {
		let _t = Timer::new(format!("get_transfers[{current_mc_block}]"));
		let unit = format!(
			"{}{}",
			hex::encode(main_chain_scripts.token_policy_id.0),
			hex::encode(&main_chain_scripts.token_asset_name.0[..])
		);
		let ics_address =
			main_chain_scripts.illiquid_circulation_supply_validator_address.to_string();
		let reserve_address = main_chain_scripts.reserve_validator_address.to_string();

		let current_block = self
			.client
			.block_by_id(&hex::encode(current_mc_block.0))
			.await?
			.ok_or_else(|| format!("Could not find block for hash {current_mc_block:?}"))?;
		let to_block = block_height(&current_block)?;

		let checkpoint = match &data_checkpoint {
			BridgeDataCheckpoint::Tx(tx_hash) => {
				let _t = Timer::new(format!("GET txs/{}", hex::encode(tx_hash.0)));
				let tx = deadline(
					&format!("txs/{}", hex::encode(tx_hash.0)),
					self.client.api.transaction_by_hash(&hex::encode(tx_hash.0)),
				)
				.await?
				.map_err(|e| -> BoxError {
					// An over-quota 402 here is not a missing checkpoint; keep the real reason.
					if is_over_quota(&e) {
						return box_err(e);
					}
					format!(
						"Could not find block info for data checkpoint: {data_checkpoint:?} ({e})"
					)
					.into()
				})?;
				ResolvedCheckpoint::Tx {
					block_number: u32::try_from(tx.block_height)?,
					tx_ix: u32::try_from(tx.index)?,
				}
			},
			BridgeDataCheckpoint::Block(number) => ResolvedCheckpoint::Block { number: number.0 },
		};

		let rows = self
			.client
			.range_txs(
				TxSource::Address(&ics_address),
				Some(BlockCursor::block(u64::from(checkpoint.block_number()))),
				Some(BlockCursor::block(u64::from(to_block))),
			)
			.await?;

		let mut bridge_txs: Vec<BridgeTx> = Vec::new();
		for row in rows {
			if !checkpoint.is_before(row.block_height, row.tx_index) {
				continue;
			}
			let utxos = self.client.tx_utxos(&row.tx_hash).await?;
			// db-sync sums these in `NUMERIC` columns that reject a value outside the
			// token range, so an out-of-range total is an error here too rather than a
			// silently truncated cast.
			let sum_outputs = |address: &str| -> Result<u64, BoxError> {
				let mut total: u128 = 0;
				for output in utxos.outputs.iter().filter(|o| !o.collateral && o.address == address)
				{
					total = total
						.checked_add(amount_of(&output.amount, &unit)?)
						.ok_or("bridge output total overflowed u128")?;
				}
				Ok(u64::try_from(total)?)
			};
			let sum_inputs = |address: &str| -> Result<u64, BoxError> {
				let mut total: u128 = 0;
				for input in utxos
					.inputs
					.iter()
					.filter(|i| !i.collateral && !i.reference.unwrap_or(false))
					.filter(|i| i.address == address)
				{
					total = total
						.checked_add(amount_of(&input.amount, &unit)?)
						.ok_or("bridge input total overflowed u128")?;
				}
				Ok(u64::try_from(total)?)
			};
			let bridge_out = sum_outputs(&ics_address)?;
			// Only txs that create a token output at the ICS address are relevant
			// (the SQL `relevant_txs` CTE).
			if bridge_out == 0 {
				continue;
			}
			let bridge_in = sum_inputs(&ics_address)?;
			let reserve_in = sum_inputs(&reserve_address)?;
			let reserve_out = sum_outputs(&reserve_address)?;
			if !(bridge_out > bridge_in || reserve_in > reserve_out) {
				continue;
			}
			let c2m_metadata = self
				.client
				.tx_metadata_label(&row.tx_hash, &TOKEN_TRANSFER_METADATUM_KEY.to_string())
				.await?;
			bridge_txs.push(BridgeTx {
				block_number: row.block_height,
				tx_ix: row.tx_index,
				tx_hash: McTxHash(decode_hash32(&row.tx_hash)?),
				c2m_metadata,
				bridge_in,
				bridge_out,
				reserve_in,
				reserve_out,
			});
			// The SQL applies `LIMIT max_transfers` to candidate txs.
			if bridge_txs.len() == max_transfers as usize {
				break;
			}
		}

		Ok(txs_to_transfers(bridge_txs, max_transfers, to_block))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sidechain_domain::byte_string::ByteString;

	fn bridge_tx(block_number: u32, tx_ix: u32, seed: u8) -> BridgeTx {
		BridgeTx {
			block_number,
			tx_ix,
			tx_hash: McTxHash([seed; 32]),
			c2m_metadata: Some(serde_json::json!(["0xabcd"])),
			bridge_in: 0,
			bridge_out: 100,
			reserve_in: 0,
			reserve_out: 0,
		}
	}

	#[test]
	fn metadata_to_recipient_accepts_only_single_hex_string_array() {
		let ok = metadata_to_recipient::<ByteString>(Some(serde_json::json!(["0xabcd"])));
		assert_eq!(ok, TransferRecipient::Address { recipient: ByteString(vec![0xab, 0xcd]) });
		for invalid in [
			None,
			Some(serde_json::json!("0xabcd")),
			Some(serde_json::json!(["abcd"])),
			Some(serde_json::json!(["0xzz"])),
			Some(serde_json::json!(["0xabcd", "0xabcd"])),
			Some(serde_json::json!({"0": "0xabcd"})),
		] {
			assert_eq!(metadata_to_recipient::<ByteString>(invalid), TransferRecipient::Invalid);
		}
	}

	#[test]
	fn tx_to_transfers_splits_reserve_and_user_transfer() {
		// Reserve released 100, ICS grew by 165: 100 reserve transfer + 65 user transfer.
		let tx = BridgeTx {
			bridge_in: 35,
			bridge_out: 200,
			reserve_in: 150,
			reserve_out: 50,
			..bridge_tx(8, 0, 8)
		};
		let transfers = tx_to_transfers::<ByteString>(tx.clone());
		assert_eq!(
			transfers,
			vec![
				BridgeTransferV1 {
					mc_tx_hash: tx.tx_hash,
					amount: 100,
					recipient: TransferRecipient::Reserve
				},
				BridgeTransferV1 {
					mc_tx_hash: tx.tx_hash,
					amount: 65,
					recipient: TransferRecipient::Address {
						recipient: ByteString(vec![0xab, 0xcd])
					}
				},
			]
		);
	}

	#[test]
	fn txs_to_transfers_returns_block_checkpoint_when_under_limit() {
		let (transfers, checkpoint) =
			txs_to_transfers::<ByteString>(vec![bridge_tx(2, 0, 1), bridge_tx(2, 1, 2)], 10, 4);
		assert_eq!(transfers.len(), 2);
		assert_eq!(checkpoint, BridgeDataCheckpoint::Block(McBlockNumber(4)));
	}

	#[test]
	fn txs_to_transfers_returns_tx_checkpoint_when_limit_reached() {
		let (transfers, checkpoint) =
			txs_to_transfers::<ByteString>(vec![bridge_tx(2, 0, 1), bridge_tx(2, 1, 2)], 1, 4);
		assert_eq!(transfers.len(), 1);
		assert_eq!(checkpoint, BridgeDataCheckpoint::Tx(McTxHash([1; 32])));
	}

	#[test]
	fn txs_to_transfers_drops_tx_whose_transfers_would_exceed_limit() {
		// The second tx produces two transfers (reserve + user); with a limit of 2 only
		// the first tx fits, and the checkpoint points at it.
		let two_transfer_tx = BridgeTx {
			bridge_in: 0,
			bridge_out: 165,
			reserve_in: 100,
			reserve_out: 0,
			..bridge_tx(3, 0, 9)
		};
		let (transfers, checkpoint) =
			txs_to_transfers::<ByteString>(vec![bridge_tx(2, 0, 1), two_transfer_tx], 2, 4);
		assert_eq!(transfers.len(), 1);
		assert_eq!(checkpoint, BridgeDataCheckpoint::Tx(McTxHash([1; 32])));
	}

	#[test]
	fn checkpoint_comparisons_are_strict() {
		let block_cp = ResolvedCheckpoint::Block { number: 2 };
		assert!(!block_cp.is_before(2, 99));
		assert!(block_cp.is_before(3, 0));

		let tx_cp = ResolvedCheckpoint::Tx { block_number: 2, tx_ix: 5 };
		assert!(!tx_cp.is_before(2, 5));
		assert!(tx_cp.is_before(2, 6));
		assert!(tx_cp.is_before(3, 0));
		assert!(!tx_cp.is_before(1, 99));
	}
}
