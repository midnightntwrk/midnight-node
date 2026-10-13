extern crate alloc;

use crate::bridge::cache::CachedTokenBridgeDataSourceImpl;
use crate::tests::normalize_tx_out_addresses;
use crate::{BlockDataSourceImpl, DbSyncBlockDataSourceConfig, TokenBridgeDataSourceImpl};
use db_sync_sqlx::{DbSyncAddressMode, DbSyncQueryConfig, DbSyncTxInputMode};
use hex_literal::hex;
use sidechain_domain::byte_string::ByteString;
use sidechain_domain::mainchain_epoch::{Duration, MainchainEpochConfig, Timestamp};
use sidechain_domain::{
	AssetName, MainchainAddress, McBlockHash, McBlockNumber, McTxHash, PolicyId,
};
use sp_partner_chains_bridge::{
	BridgeDataCheckpoint, BridgeTransferV1, MainChainScripts, TokenBridgeDataSource,
	TransferRecipient,
};
use sqlx::PgPool;
use std::str::FromStr;
use std::sync::Arc;

fn token_policy_id() -> PolicyId {
	PolicyId(hex!("500000000000000000000000000000000000434845434b504f494e69"))
}

fn token_asset_name() -> AssetName {
	AssetName(b"native token".to_vec().try_into().unwrap())
}

fn illiquid_circulation_supply_validator_address() -> MainchainAddress {
	MainchainAddress::from_str("ics address").unwrap()
}

fn reserve_validator_address() -> MainchainAddress {
	MainchainAddress::from_str("reserve address").unwrap()
}

fn block_2_hash() -> McBlockHash {
	McBlockHash(hex!("b000000000000000000000000000000000000000000000000000000000000002"))
}

fn block_3_hash() -> McBlockHash {
	McBlockHash(hex!("b000000000000000000000000000000000000000000000000000000000000003"))
}

fn block_4_hash() -> McBlockHash {
	McBlockHash(hex!("b000000000000000000000000000000000000000000000000000000000000004"))
}

fn block_8_hash() -> McBlockHash {
	McBlockHash(hex!("b000000000000000000000000000000000000000000000000000000000000008"))
}

fn init_ics_tx_hash() -> McTxHash {
	McTxHash(hex!("c000000000000000000000000000000000000000000000000000000000000001"))
}

fn reserve_transfer() -> BridgeTransferV1<ByteString> {
	BridgeTransferV1 {
		amount: 100,
		mc_tx_hash: reserve_transfer_tx(),
		recipient: TransferRecipient::Reserve,
	}
}

fn user_transfer_1() -> BridgeTransferV1<ByteString> {
	BridgeTransferV1 {
		// user transfer 1 consumes utxo from reserve transfer
		amount: 110 - 100,
		recipient: TransferRecipient::Address { recipient: ByteString(hex!("abcd").to_vec()) },
		mc_tx_hash: user_transfer_1_tx(),
	}
}

fn user_transfer_2() -> BridgeTransferV1<ByteString> {
	BridgeTransferV1 {
		// user transfer 2 consumes utxo from user transfer 1
		amount: 120 - 110,
		recipient: TransferRecipient::Address { recipient: ByteString(hex!("1234").to_vec()) },
		mc_tx_hash: user_transfer_2_tx(),
	}
}

// transfer with invalid datum
fn invalid_transfer_1() -> BridgeTransferV1<ByteString> {
	BridgeTransferV1 {
		// invalid transfer consumes utxo from user transfer 2
		amount: 1000 - 120,
		mc_tx_hash: invalid_transfer_1_tx(),
		recipient: TransferRecipient::Invalid,
	}
}

// transfer with no datum
fn invalid_transfer_2() -> BridgeTransferV1<ByteString> {
	BridgeTransferV1 {
		amount: 1000,
		mc_tx_hash: invalid_transfer_2_tx(),
		recipient: TransferRecipient::Invalid,
	}
}

fn complex_transfer() -> BridgeTransferV1<ByteString> {
	BridgeTransferV1 {
		amount: 50,
		mc_tx_hash: complex_transfer_tx(),
		recipient: TransferRecipient::Reserve,
	}
}

fn reserve_and_user_tx_reserve_transfer() -> BridgeTransferV1<ByteString> {
	BridgeTransferV1 {
		amount: 100,
		mc_tx_hash: reserve_and_user_transfer_tx(),
		recipient: TransferRecipient::Reserve,
	}
}

fn reserve_and_user_tx_user_transfer() -> BridgeTransferV1<ByteString> {
	BridgeTransferV1 {
		amount: 65,
		mc_tx_hash: reserve_and_user_transfer_tx(),
		recipient: TransferRecipient::Address { recipient: ByteString(hex!("9999").to_vec()) },
	}
}

fn reserve_transfer_tx() -> McTxHash {
	McTxHash(hex!("c000000000000000000000000000000000000000000000000000000000000002"))
}

fn user_transfer_1_tx() -> McTxHash {
	McTxHash(hex!("c000000000000000000000000000000000000000000000000000000000000003"))
}

fn user_transfer_2_tx() -> McTxHash {
	McTxHash(hex!("c000000000000000000000000000000000000000000000000000000000000004"))
}

fn invalid_transfer_1_tx() -> McTxHash {
	McTxHash(hex!("c000000000000000000000000000000000000000000000000000000000000005"))
}

fn invalid_transfer_2_tx() -> McTxHash {
	McTxHash(hex!("c000000000000000000000000000000000000000000000000000000000000006"))
}

fn complex_transfer_tx() -> McTxHash {
	McTxHash(hex!("c000000000000000000000000000000000000000000000000000000000000007"))
}

fn reserve_and_user_transfer_tx() -> McTxHash {
	McTxHash(hex!("c000000000000000000000000000000000000000000000000000000000000008"))
}

fn main_chain_scripts() -> MainChainScripts {
	MainChainScripts {
		token_policy_id: token_policy_id(),
		token_asset_name: token_asset_name(),
		illiquid_circulation_supply_validator_address:
			illiquid_circulation_supply_validator_address(),
		reserve_validator_address: reserve_validator_address(),
	}
}

macro_rules! with_migration_versions_and_caching {
	($(async fn $name:ident($data_source:ident: &dyn TokenBridgeDataSource<ByteString>) $body:block )*) => {
		$(
		mod $name {
			use super::*;
			#[allow(unused_imports)]
			use pretty_assertions::assert_eq;

			async fn $name($data_source: &dyn TokenBridgeDataSource<ByteString>) $body

			mod uncached {
				use super::*;
				#[allow(unused_imports)]
				use pretty_assertions::assert_eq;

				#[sqlx::test(migrations = "./testdata/bridge/migrations-tx-in-enabled")]
				async fn tx_in_enabled(pool: PgPool) {
					$name(&create_data_source(pool)).await
				}

				#[sqlx::test(migrations = "./testdata/bridge/migrations-tx-in-consumed")]
				async fn tx_in_consumed(pool: PgPool) {
					$name(&create_data_source(pool)).await
				}
			}

			mod cached {
				use super::*;

				#[sqlx::test(migrations = "./testdata/bridge/migrations-tx-in-enabled")]
				async fn tx_in_enabled(pool: PgPool) {
					$name(&create_cached_source(pool)).await
				}

				#[sqlx::test(migrations = "./testdata/bridge/migrations-tx-in-consumed")]
				async fn tx_in_consumed(pool: PgPool) {
					$name(&create_cached_source(pool)).await
				}
			}

		}
		)*
	}
}

fn main_chain_epoch_config() -> MainchainEpochConfig {
	MainchainEpochConfig {
		first_epoch_timestamp_millis: Timestamp::from_unix_millis(1650558070000),
		epoch_duration_millis: Duration::from_millis(1000 * 1000),
		first_epoch_number: 189,
		first_slot_number: 189000,
		slot_duration_millis: Duration::from_millis(1000),
	}
}

fn block_data_source_config() -> DbSyncBlockDataSourceConfig {
	DbSyncBlockDataSourceConfig {
		cardano_security_parameter: 432,
		cardano_active_slots_coeff: 0.05,
		block_stability_margin: 0,
	}
}

fn create_data_source(pool: PgPool) -> TokenBridgeDataSourceImpl {
	TokenBridgeDataSourceImpl::new(pool, None)
}

fn create_cached_source(pool: PgPool) -> CachedTokenBridgeDataSourceImpl {
	let blocks = Arc::new(BlockDataSourceImpl::from_config(
		pool.clone(),
		block_data_source_config(),
		&main_chain_epoch_config(),
	));
	let cache_lookahead = 32;
	CachedTokenBridgeDataSourceImpl::new(pool, None, blocks, cache_lookahead)
}

async fn assert_address_table_bridge_flow(pool: PgPool, tx_input_mode: DbSyncTxInputMode) {
	normalize_tx_out_addresses(&pool).await;
	let data_source = TokenBridgeDataSourceImpl::new_with_db_sync_config(
		pool,
		None,
		DbSyncQueryConfig { tx_input_mode, address_mode: DbSyncAddressMode::AddressTable },
	);

	let (transfers, new_checkpoint) = data_source
		.get_transfers(
			main_chain_scripts(),
			BridgeDataCheckpoint::Tx(init_ics_tx_hash()),
			5,
			block_4_hash(),
		)
		.await
		.unwrap();

	assert_eq!(
		transfers,
		vec![
			reserve_transfer(),
			user_transfer_1(),
			user_transfer_2(),
			invalid_transfer_1(),
			invalid_transfer_2(),
		]
	);
	assert_eq!(new_checkpoint, BridgeDataCheckpoint::Block(McBlockNumber(4)));
}

#[sqlx::test(migrations = "./testdata/bridge/migrations-tx-in-enabled")]
async fn address_table_bridge_flow_tx_in(pool: PgPool) {
	assert_address_table_bridge_flow(pool, DbSyncTxInputMode::TxIn).await;
}

#[sqlx::test(migrations = "./testdata/bridge/migrations-tx-in-consumed")]
async fn address_table_bridge_flow_consumed(pool: PgPool) {
	assert_address_table_bridge_flow(pool, DbSyncTxInputMode::Consumed).await;
}

with_migration_versions_and_caching! {
	async fn gets_transfers_from_init_to_block_2(data_source: &dyn TokenBridgeDataSource<ByteString>) {
		let data_checkpoint = BridgeDataCheckpoint::Tx(init_ics_tx_hash());
		let current_mc_block = block_2_hash();
		let max_transfers = 2;

		let (transfers, new_checkpoint) = data_source
			.get_transfers(main_chain_scripts(), data_checkpoint, max_transfers, current_mc_block)
			.await
			.unwrap();

		// There's two transfers done in block 2
		assert_eq!(transfers, vec![reserve_transfer(), user_transfer_1()]);

		// All transactions up to block 2 have been read, so the checkpoint advances to the block
		assert_eq!(new_checkpoint, BridgeDataCheckpoint::Block(McBlockNumber(2)))
	}

	async fn gets_transfers_from_init_to_block_4(data_source: &dyn TokenBridgeDataSource<ByteString>) {
		let data_checkpoint = BridgeDataCheckpoint::Tx(init_ics_tx_hash());
		let current_mc_block = block_4_hash();
		let max_transfers = 5;

		let (transfers, new_checkpoint) = data_source
			.get_transfers(main_chain_scripts(), data_checkpoint, max_transfers, current_mc_block)
			.await
			.unwrap();

		// There's three valid transfers and one invalid done between blocks 2 and 4
		assert_eq!(
			transfers,
			vec![reserve_transfer(), user_transfer_1(), user_transfer_2(), invalid_transfer_1(), invalid_transfer_2()]
		);

		assert_eq!(new_checkpoint, BridgeDataCheckpoint::Block(McBlockNumber(4)))
	}

	async fn accepts_block_checkpoint(data_source: &dyn TokenBridgeDataSource<ByteString>) {
		let data_checkpoint = BridgeDataCheckpoint::Block(McBlockNumber(1));
		let current_mc_block = block_4_hash();
		let max_transfers = 5;

		let (transfers, new_checkpoint) = data_source
			.get_transfers(main_chain_scripts(), data_checkpoint, max_transfers, current_mc_block)
			.await
			.unwrap();

		// There's three valid transfers and one invalid done between blocks 2 and 4
		assert_eq!(
			transfers,
			vec![reserve_transfer(), user_transfer_1(), user_transfer_2(), invalid_transfer_1(), invalid_transfer_2()]
		);

		assert_eq!(new_checkpoint, BridgeDataCheckpoint::Block(McBlockNumber(4)))
	}

	async fn returns_block_checkpoint_when_no_transfers_are_found(data_source: &dyn TokenBridgeDataSource<ByteString>) {
		let data_checkpoint = BridgeDataCheckpoint::Block(McBlockNumber(6));
		let current_mc_block = block_8_hash();
		let max_transfers = 32;

		let (transfers, new_checkpoint) = data_source
			.get_transfers(main_chain_scripts(), data_checkpoint, max_transfers, current_mc_block)
			.await
			.unwrap();

		assert_eq!(transfers, vec![]);

		assert_eq!(new_checkpoint, BridgeDataCheckpoint::Block(McBlockNumber(8)))
	}

	async fn returns_block_checkpoint_when_less_than_maximum_transfers_found(data_source: &dyn TokenBridgeDataSource<ByteString>) {
		let data_checkpoint = BridgeDataCheckpoint::Block(McBlockNumber(0));
		let current_mc_block = block_8_hash();
		let max_transfers = 32;

		let (transfers, new_checkpoint) = data_source
			.get_transfers(main_chain_scripts(), data_checkpoint, max_transfers, current_mc_block)
			.await
			.unwrap();

		assert_eq!(
			transfers,
			vec![reserve_transfer(), user_transfer_1(), user_transfer_2(), invalid_transfer_1(), invalid_transfer_2(), complex_transfer(), reserve_and_user_tx_reserve_transfer(), reserve_and_user_tx_user_transfer()]
		);

		assert_eq!(new_checkpoint, BridgeDataCheckpoint::Block(McBlockNumber(8)))
	}

	async fn truncates_output_and_returns_utxo_checkpoint_if_max_output_is_reached(data_source: &dyn TokenBridgeDataSource<ByteString>) {
		let data_checkpoint = BridgeDataCheckpoint::Tx(init_ics_tx_hash());
		let current_mc_block = block_2_hash();
		let max_transfers = 1;

		let (transfers, new_checkpoint) = data_source
			.get_transfers(main_chain_scripts(), data_checkpoint, max_transfers, current_mc_block)
			.await
			.unwrap();

		// There's two transfers done in block 2
		assert_eq!(transfers, vec![reserve_transfer()]);

		// `reserve_transfer_tx` makes no ICS transfer, so this is the looser of the two ways to
		// point at it. It will be presented again, with nothing left to handle.
		assert_eq!(new_checkpoint, BridgeDataCheckpoint::TxReserveTransfer(reserve_transfer_tx()))
	}

	async fn cuts_between_the_two_transfers_of_a_tx_when_the_limit_is_reached(data_source: &dyn TokenBridgeDataSource<ByteString>) {
		let data_checkpoint = BridgeDataCheckpoint::Block(McBlockNumber(0));
		let current_mc_block = block_8_hash();
		// Only the first of the two transfers of reserve_and_user_tx fits
		let max_transfers = 7;

		let (transfers, new_checkpoint) = data_source
			.get_transfers(main_chain_scripts(), data_checkpoint, max_transfers, current_mc_block)
			.await
			.unwrap();

		assert_eq!(
			transfers,
			vec![reserve_transfer(), user_transfer_1(), user_transfer_2(), invalid_transfer_1(), invalid_transfer_2(), complex_transfer(), reserve_and_user_tx_reserve_transfer()]
		);

		assert_eq!(
			new_checkpoint,
			BridgeDataCheckpoint::TxReserveTransfer(reserve_and_user_transfer_tx())
		)
	}

	async fn returns_a_single_transfer_of_a_two_transfer_tx_when_the_limit_is_one(data_source: &dyn TokenBridgeDataSource<ByteString>) {
		// The two transfers of `reserve_and_user_tx` do not have to be returned together, so a
		// limit of a single transfer still makes progress.
		let data_checkpoint = BridgeDataCheckpoint::Tx(complex_transfer_tx());
		let current_mc_block = block_8_hash();
		let max_transfers = 1;

		let (transfers, new_checkpoint) = data_source
			.get_transfers(main_chain_scripts(), data_checkpoint.clone(), max_transfers, current_mc_block)
			.await
			.unwrap();

		assert_eq!(transfers, vec![reserve_and_user_tx_reserve_transfer()]);

		assert_eq!(
			new_checkpoint,
			BridgeDataCheckpoint::TxReserveTransfer(reserve_and_user_transfer_tx())
		)
	}

	async fn reserve_transfer_checkpoint_skips_the_reserve_transfer_already_processed(data_source: &dyn TokenBridgeDataSource<ByteString>) {
		// `reserve_and_user_tx` makes two transfers, of which the runtime handled the reserve one
		// before it stopped. Only the ICS transfer is left to handle.
		let data_checkpoint =
			BridgeDataCheckpoint::TxReserveTransfer(reserve_and_user_transfer_tx());
		let current_mc_block = block_8_hash();
		let max_transfers = 32;

		let (transfers, new_checkpoint) = data_source
			.get_transfers(main_chain_scripts(), data_checkpoint, max_transfers, current_mc_block)
			.await
			.unwrap();

		assert_eq!(transfers, vec![reserve_and_user_tx_user_transfer()]);

		assert_eq!(new_checkpoint, BridgeDataCheckpoint::Block(McBlockNumber(8)))
	}

	async fn reserve_transfer_checkpoint_of_a_tx_without_ics_transfer_leaves_nothing_of_it(data_source: &dyn TokenBridgeDataSource<ByteString>) {
		// `complex_transfer_tx` makes only a reserve transfer, which has been handled, so only the
		// transfers of the transactions following it are returned.
		let data_checkpoint = BridgeDataCheckpoint::TxReserveTransfer(complex_transfer_tx());
		let current_mc_block = block_8_hash();
		let max_transfers = 32;

		let (transfers, new_checkpoint) = data_source
			.get_transfers(main_chain_scripts(), data_checkpoint, max_transfers, current_mc_block)
			.await
			.unwrap();

		assert_eq!(
			transfers,
			vec![reserve_and_user_tx_reserve_transfer(), reserve_and_user_tx_user_transfer()]
		);

		assert_eq!(new_checkpoint, BridgeDataCheckpoint::Block(McBlockNumber(8)))
	}

	async fn tx_checkpoint_returns_both_transfers_of_the_following_tx(data_source: &dyn TokenBridgeDataSource<ByteString>) {
		// Nothing of `reserve_and_user_tx` has been handled, so both of its transfers are returned.
		let data_checkpoint = BridgeDataCheckpoint::Tx(complex_transfer_tx());
		let current_mc_block = block_8_hash();
		let max_transfers = 32;

		let (transfers, new_checkpoint) = data_source
			.get_transfers(main_chain_scripts(), data_checkpoint, max_transfers, current_mc_block)
			.await
			.unwrap();

		assert_eq!(
			transfers,
			vec![reserve_and_user_tx_reserve_transfer(), reserve_and_user_tx_user_transfer()]
		);

		assert_eq!(new_checkpoint, BridgeDataCheckpoint::Block(McBlockNumber(8)))
	}

	async fn utxos_from_checkpoint_block_are_not_included_in_result(data_source: &dyn TokenBridgeDataSource<ByteString>) {
		let data_checkpoint = BridgeDataCheckpoint::Block(McBlockNumber(2));
		let current_mc_block = block_3_hash();
		let max_transfers = 10;

		let (transfers, new_checkpoint) = data_source
			.get_transfers(main_chain_scripts(), data_checkpoint, max_transfers, current_mc_block)
			.await
			.unwrap();

		// There's two transfers done in block 2
		assert_eq!(transfers, vec![]);

		assert_eq!(new_checkpoint, BridgeDataCheckpoint::Block(McBlockNumber(3)))
	}
}
