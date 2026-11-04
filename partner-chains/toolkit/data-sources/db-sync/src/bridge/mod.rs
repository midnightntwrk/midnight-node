//! Db-Sync data source used by the Partner Chain token bridge observability
//!
//! # Assumptions
//!
//! The data source implementation assumes that the utxos found at the illiquid circulating
//! supply address conform to rules that are enforced by the Partner Chains smart contracts.
//!
//! Most importantly, transactions that spend any UTXOs from the ICS can only create at most
//! one new UTXO at the ICS address. Conversely, transactions that create more than one UTXO
//! at the illiquid supply address can only spend UTXOs from outside of it. This guarantees
//! that the observability layer can always correctly identify the number of tokens transfered
//! by calculating the delta of `tokens in the new UTXO` - `tokens in the old ICS UTXOs`.
//!
//! # Usage
//!
//! ```rust
//! use partner_chains_db_sync_data_sources::*;
//! use sqlx::PgPool;
//! use std::{ error::Error, sync::Arc };
//!
//! // Number of stable blocks ahead the bridge data source should try to cache.
//! // This is only possible when the node is catching up and speeds up syncing.
//! const BRIDGE_TRANSFER_CACHE_LOOKAHEAD: u32 = 128;
//!
//! pub async fn create_data_sources(
//!     pool: PgPool,
//!     metrics_opt: Option<McFollowerMetrics>,
//! ) -> Result<(/* other data sources */ CachedTokenBridgeDataSourceImpl), Box<dyn Error + Send + Sync>> {
//!     // block data source is reused between various other data sources
//!     let block = Arc::new(BlockDataSourceImpl::new_from_env(pool.clone()).await?);
//!
//!     // create other data sources
//!
//!     let bridge = CachedTokenBridgeDataSourceImpl::new(
//!         pool,
//!         metrics_opt,
//!         block,
//!         BRIDGE_TRANSFER_CACHE_LOOKAHEAD,
//!	    );
//!
//!     Ok((/* other data sources */ bridge))
//! }
//! ```

use crate::McFollowerMetrics;
use crate::db_model::*;
use crate::metrics::observed_async_trait;
use sidechain_domain::{McBlockHash, McTxHash};
use sp_partner_chains_bridge::*;
use sqlx::PgPool;
use sqlx::types::JsonValue;
use std::fmt::Debug;

#[cfg(test)]
mod tests;

pub(crate) mod cache;

/// Db-Sync data source serving data for Partner Chains token bridge
pub struct TokenBridgeDataSourceImpl {
	/// Postgres connection pool
	pool: PgPool,
	/// Prometheus metrics client
	metrics_opt: Option<McFollowerMetrics>,
	/// Configuration used by Db-Sync
	db_sync_config: DbSyncConfigurationProvider,
}

impl TokenBridgeDataSourceImpl {
	/// Crates a new token bridge data source
	pub fn new(pool: PgPool, metrics_opt: Option<McFollowerMetrics>) -> Self {
		Self { db_sync_config: DbSyncConfigurationProvider::new(pool.clone()), pool, metrics_opt }
	}
}

observed_async_trait!(
	impl<RecipientAddress> TokenBridgeDataSource<RecipientAddress> for TokenBridgeDataSourceImpl
	where
		RecipientAddress: Debug,
		RecipientAddress: (for<'a> TryFrom<&'a [u8]>),
	{
		async fn get_transfers(
			&self,
			main_chain_scripts: MainChainScripts,
			data_checkpoint: BridgeDataCheckpoint,
			max_transfers: u32,
			current_mc_block_hash: McBlockHash,
		) -> Result<
			(Vec<BridgeTransferV1<RecipientAddress>>, BridgeDataCheckpoint),
			Box<dyn std::error::Error + Send + Sync>,
		> {
			let asset = Asset {
				policy_id: main_chain_scripts.token_policy_id.into(),
				asset_name: main_chain_scripts.token_asset_name.into(),
			};

			let current_mc_block = get_block_by_hash(&self.pool, current_mc_block_hash.clone())
				.await?
				.ok_or(format!("Could not find block for hash {current_mc_block_hash:?}"))?;

			let block_bound = current_mc_block.block_no;

			let tx_position = match &data_checkpoint {
				BridgeDataCheckpoint::Tx(tx_hash)
				| BridgeDataCheckpoint::TxReserveTransfer(tx_hash) => {
					get_block_info_for_tx_hash(&self.pool, (*tx_hash).into())
						.await?
						.map(|TxBlockInfo { block_number, tx_ix }| (block_number, tx_ix))
				},
				BridgeDataCheckpoint::Block(_) => None,
			};

			let Some(resolved_checkpoint) =
				resolve_checkpoint(&data_checkpoint, tx_position, block_bound)
			else {
				return Ok((vec![], data_checkpoint));
			};

			let txs = get_bridge_txs(
				self.db_sync_config.get_tx_in_config().await?,
				&self.pool,
				&main_chain_scripts.illiquid_circulation_supply_validator_address.into(),
				&main_chain_scripts.reserve_validator_address.into(),
				asset,
				resolved_checkpoint,
				block_bound,
				Some(max_txs_to_read(max_transfers)),
			)
			.await?;

			Ok(txs_to_transfers(txs, &data_checkpoint, max_transfers, block_bound))
		}
	}
);

/// Resolves `data_checkpoint` to its position in Cardano history as of `block_bound`
///
/// `tx_position` is the position Db-Sync knows for the checkpoint transaction, if there is one.
/// Returns `None` when the checkpoint is not at or before `block_bound`. The checkpoint is then
/// kept with no transfers: moving it back to `block_bound` would return transfers it has already
/// passed, and failing would also fail the block carrying a governance call that replaces it.
fn resolve_checkpoint(
	data_checkpoint: &BridgeDataCheckpoint,
	tx_position: Option<TxOrderingKey>,
	block_bound: BlockNumber,
) -> Option<ResolvedBridgeDataCheckpoint> {
	let known_position = |tx_hash: &McTxHash| {
		if tx_position.is_none() {
			log::warn!(
				"Bridge data checkpoint transaction {tx_hash:?} not found in Db-Sync, no transfers are observed until it is"
			);
		}
		tx_position
	};

	let resolved = match data_checkpoint {
		BridgeDataCheckpoint::Block(number) => {
			ResolvedBridgeDataCheckpoint::Block { number: (*number).into() }
		},
		BridgeDataCheckpoint::Tx(tx_hash) => {
			let (block_number, tx_ix) = known_position(tx_hash)?;
			ResolvedBridgeDataCheckpoint::Tx { block_number, tx_ix }
		},
		// The reserve transfer of this transaction, which the runtime has already handled, is
		// left out of the returned data.
		BridgeDataCheckpoint::TxReserveTransfer(tx_hash) => {
			let (block_number, tx_ix) = known_position(tx_hash)?;
			ResolvedBridgeDataCheckpoint::TxReserveTransfer { block_number, tx_ix }
		},
	};

	if resolved.get_block_number() > block_bound {
		log::debug!(
			"Bridge data checkpoint {data_checkpoint:?} is ahead of Cardano block {}, no transfers are observed",
			block_bound.0
		);
		return None;
	}

	Some(resolved)
}

/// Number of Cardano transaction rows to read in order to return up to `max_transfers` transfers
///
/// One transaction yields at least one transfer, of which at most one is dropped as already
/// processed, so reading one transaction more than the transfer limit guarantees that the limit
/// can always be filled if there is enough data for it. In particular, progress is always
/// possible, even with a limit of a single transfer.
fn max_txs_to_read(max_transfers: u32) -> u32 {
	max_transfers.saturating_add(1)
}

/// Turns the observed Cardano transactions into the transfers to be handled next
///
/// Transfers are returned in the order they were made, cut off at `max_transfers`. The cut can
/// fall between the two transfers of one Cardano transaction, in which case the returned
/// checkpoint is a [BridgeDataCheckpoint::TxReserveTransfer].
fn txs_to_transfers<RecipientAddress>(
	txs: Vec<BridgeTx>,
	current_checkpoint: &BridgeDataCheckpoint,
	max_transfers: u32,
	block_bound: BlockNumber,
) -> (Vec<BridgeTransferV1<RecipientAddress>>, BridgeDataCheckpoint)
where
	RecipientAddress: for<'a> TryFrom<&'a [u8]>,
{
	// Hitting the transaction limit means there may be further transactions before `block_bound`
	// that were not read.
	let all_txs_read = txs.len() < max_txs_to_read(max_transfers) as usize;

	let mut remaining = txs
		.into_iter()
		.flat_map(tx_to_transfers::<RecipientAddress>)
		// The transfer that the current checkpoint points at has already been handled. This only
		// ever matches the reserve transfer of a `TxReserveTransfer` checkpoint's transaction,
		// because transactions the checkpoint has passed are not returned by the query at all.
		.filter(|transfer| transfer.checkpoint() != *current_checkpoint)
		.peekable();

	let transfers: Vec<BridgeTransferV1<RecipientAddress>> =
		remaining.by_ref().take(max_transfers as usize).collect();

	let checkpoint = if all_txs_read && remaining.peek().is_none() {
		BridgeDataCheckpoint::Block(block_bound.into())
	} else {
		match transfers.last() {
			Some(tx) => tx.checkpoint(),
			None => current_checkpoint.clone(),
		}
	};

	(transfers, checkpoint)
}

/// This function from [BridgeTx] to [Vec<BridgeTransferV1>] works under assumption that
/// Reserve can unlock only to ICS. If reserve shrinked, then delta went to ICS.
/// User transfer is computed in the second place as the rest of ICS surplus.
/// In case the second step took tokens out of ICS, it means it was ICS withdrawal.
/// ICS are not supported by this bride, so no additional [BridgeTransferV1] is emmited in such a case.
fn tx_to_transfers<RecipientAddress>(tx: BridgeTx) -> Vec<BridgeTransferV1<RecipientAddress>>
where
	RecipientAddress: for<'a> TryFrom<&'a [u8]>,
{
	let mc_tx_hash = tx.tx_id();
	let reserve_debit: u64 = tx.reserve_in.saturating_sub(tx.reserve_out).into();
	let ics_credit: u64 = tx.bridge_out.saturating_sub(tx.bridge_in).into();
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

fn metadata_to_recipient<RecipientAddress>(
	metadata: Option<JsonValue>,
) -> TransferRecipient<RecipientAddress>
where
	RecipientAddress: for<'a> TryFrom<&'a [u8]>,
{
	match metadata {
		Some(JsonValue::Array(values)) => match values.as_slice() {
			[JsonValue::String(str)] => {
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
