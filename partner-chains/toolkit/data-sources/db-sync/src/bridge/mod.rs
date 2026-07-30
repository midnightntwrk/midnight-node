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
use sidechain_domain::McBlockHash;
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

			let data_checkpoint = match data_checkpoint {
				BridgeDataCheckpoint::Tx(tx_hash) => {
					let TxBlockInfo { block_number, tx_ix } =
						get_block_info_for_tx_hash(&self.pool, tx_hash.into()).await?.ok_or(
							format!(
								"Could not find block info for data checkpoint: {data_checkpoint:?}"
							),
						)?;
					ResolvedBridgeDataCheckpoint::Tx { block_number, tx_ix }
				},
				BridgeDataCheckpoint::Block(number) => {
					ResolvedBridgeDataCheckpoint::Block { number: number.into() }
				},
			};

			let txs = get_bridge_txs(
				self.db_sync_config.get_tx_in_config().await?,
				&self.pool,
				&main_chain_scripts.illiquid_circulation_supply_validator_address.into(),
				&main_chain_scripts.reserve_validator_address.into(),
				asset,
				data_checkpoint,
				current_mc_block.block_no,
				Some(max_transfers),
			)
			.await?;

			Ok(txs_to_transfers(txs, max_transfers, current_mc_block.block_no))
		}
	}
);

fn txs_to_transfers<RecipientAddress>(
	txs: Vec<BridgeTx>,
	max_transfers: u32,
	block_bound: BlockNumber,
) -> (Vec<BridgeTransferV1<RecipientAddress>>, BridgeDataCheckpoint)
where
	RecipientAddress: for<'a> TryFrom<&'a [u8]>,
{
	let mut transfers: Vec<BridgeTransferV1<RecipientAddress>> = vec![];
	let mut checkpoint = BridgeDataCheckpoint::Block(block_bound.into());
	// Add Cardano transaction transfers only if all of them fit into max_transfers
	for tx in &txs {
		let tx_transfers = tx_to_transfers::<RecipientAddress>(tx.clone());
		// Would go over limit, return accumulated state from previous iteration
		if transfers.len() + tx_transfers.len() > max_transfers as usize {
			return (transfers, checkpoint);
		}
		transfers.extend(tx_transfers);
		checkpoint = BridgeDataCheckpoint::Tx(tx.tx_id())
	}
	let checkpoint = if transfers.len() == max_transfers as usize {
		checkpoint
	} else {
		BridgeDataCheckpoint::Block(block_bound.into())
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
