//! Crate providing implementations of Partner Chain Data Sources that read from Db-Sync Postgres.
//!
//! # Usage
//!
//! ## Adding to the node
//!
//! All data sources defined in this crate require a Postgres connection pool [PgPool] to run
//! queries, which should be shared between all data sources. For convenience, this crate provides
//! a helper function [get_connection_from_env] that will create a connection pool based on
//! configuration read from node environment.
//!
//! Each data source also accepts an optional Prometheus metrics client [McFollowerMetrics] for
//! reporting metrics to the Substrate's Prometheus metrics service. This client can be obtained
//! using the [register_metrics_warn_errors] function.
//!
//! In addition to these two common arguments, some data sources depend on [BlockDataSourceImpl]
//! which provides basic queries about blocks, and additional configuration for their data cache
//! size.
//!
//! An example node code that creates the data sources can look like the following:
//!
//! ```rust
//! # use std::error::Error;
//! # use std::sync::Arc;
//! use partner_chains_db_sync_data_sources::*;
//!
//! pub const CANDIDATES_FOR_EPOCH_CACHE_SIZE: usize = 64;
//! pub const STAKE_CACHE_SIZE: usize = 100;
//!
//! async fn create_data_sources(
//!     metrics_registry_opt: Option<&substrate_prometheus_endpoint::Registry>
//! ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!     let metrics = register_metrics_warn_errors(metrics_registry_opt);
//!     let pool = get_connection_from_env().await?;
//!
//!     // Block data source is shared by others for cache reuse
//!     let block = Arc::new(BlockDataSourceImpl::new_from_env(pool.clone()).await?);
//!
//!     let sidechain_rpc = SidechainRpcDataSourceImpl::new(block.clone(), metrics.clone());
//!
//!     let mc_hash = Arc::new(McHashDataSourceImpl::new(block.clone(), metrics.clone()));
//!
//!     let authority_selection =
//!         CandidatesDataSourceImpl::new(pool.clone(), metrics.clone())
//!     	.await?
//!     	.cached(CANDIDATES_FOR_EPOCH_CACHE_SIZE)?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Cardano DB Sync configuration
//!
//! The query layout and database schema policy are independent. [`DbSyncQueryConfig`] selects the
//! transaction-input and address representations, while [`DbSyncSchemaMode`] controls index
//! management.
//!
//! ### Transaction-input representation
//!
//! [`DbSyncTxInputMode`] supports:
//!
//! - `Auto`: use `tx_in` when it contains rows, otherwise use `tx_out.consumed_by_tx_id` when at
//!   least one output records a consuming transaction. Ambiguous empty schemas require an
//!   explicit mode. This is the backward-compatible default for initialized databases.
//! - `TxIn`: require `tx_in(tx_in_id, tx_out_id, tx_out_index)`. This corresponds to
//!   `insert_options.tx_out.value = "enable"`, or `force_tx_in = true` with `"consumed"`.
//! - `Consumed`: require `tx_out.consumed_by_tx_id`. This corresponds to
//!   `insert_options.tx_out.value = "consumed"`.
//!
//! The db-sync `prune`, `bootstrap`, and `disable` transaction-output modes are unsupported because
//! Partner Chains queries require historical transaction outputs. Schema detection only checks
//! columns and whether either representation contains evidence; it does not prove that old inputs or spends were
//! backfilled. Use an explicit mode in production and ensure its representation is complete for
//! the full block and epoch range the data source will query.
//!
//! ### Address representation
//!
//! [`DbSyncAddressMode`] supports both db-sync address layouts:
//!
//! - `Inline` requires `insert_options.tx_out.use_address_table = false` and reads
//!   `tx_out.address`. This is the backward-compatible default.
//! - `AddressTable` requires `insert_options.tx_out.use_address_table = true` and joins
//!   `tx_out.address_id` to `address.id` to read `address.address`.
//!
//! The configured mode is validated against the relations visible on the PostgreSQL connection's
//! `search_path`.
//!
//! ### Other required db-sync data
//!
//! Partner Chains data sources also require the following db-sync data to be retained:
//!
//! - `insert_options.ledger`: must be `"enable"` (default).
//! - `insert_options.multi_asset.enable`: must be `true` (default).
//! - `insert_options.metadata.enable`: must be `true` (default). If
//!   `insert_options.metadata.keys` filters retained metadata, it must include the C-to-M bridge
//!   key `6500973`.
//! - `insert_options.remove_jsonb_from_schema`: either value is supported when the JSON data is
//!   retained; text-backed values are cast to `jsonb` by the queries.
//! - `insert_options.plutus.enable`: must be `true` (default).
//!
//! The bridge requires complete `tx_metadata` history for key `6500973`. The presence of the table
//! and columns does not show whether db-sync filtered or omitted older rows, so operators must
//! validate metadata completeness separately. These data sources do not query db-sync governance
//! tables, so `insert_options.governance` is not a compatibility requirement at this version.
//!
//! The default Cardano DB Sync configuration meets these requirements, so Partner Chain node
//! operators that do not wish to use any custom configuration can use the defaults, otherwise
//! they must preserve the values described above. See [Db-Sync configuration docs] for more
//! information.
//!
//! ## Schema management and custom indexes
//!
//! [`DbSyncSchemaMode`] supports three policies:
//!
//! - `Apply` creates missing required indexes with `CREATE INDEX CONCURRENTLY`. This is the
//!   backward-compatible default.
//! - `Verify` performs read-only structural checks. It accepts a compatible valid, ready,
//!   non-partial index under any name, and fails when a required index is missing.
//! - `Skip` neither creates nor verifies indexes.
//!
//! The candidate/runtime manifest always requires btree indexes with leading keys
//! `ma_tx_out(ident)` and `ma_tx_out(tx_out_id)`. Existing composite indexes satisfy either
//! requirement. It additionally requires:
//!
//! - inline addresses: a hash or btree index on `tx_out(address)`;
//! - address-table storage: a hash or btree index on `address(address)` and a btree index on
//!   `tx_out(address_id)`;
//! - `tx_in` inputs: btree indexes on `tx_in(tx_in_id)` and
//!   `tx_in(tx_out_id, tx_out_index)`; or
//! - consumed inputs: a btree index on `tx_out(consumed_by_tx_id)`.
//!
//! [`CandidatesDataSourceImpl::new`] uses the default `Auto`/`Inline`/`Apply` policy; its
//! configurable constructor accepts all three axes. Query-only bridge constructors accept the
//! layout configuration but do not manage the schema.
//!
//! [PgPool]: sqlx::PgPool
//! [BlockDataSourceImpl]: crate::block::BlockDataSourceImpl
//! [McFollowerMetrics]: crate::metrics::McFollowerMetrics
//! [get_connection_from_env]: crate::data_sources::get_connection_from_env
//! [register_metrics_warn_errors]: crate::metrics::register_metrics_warn_errors
//! [Db-Sync configuration docs]: https://github.com/IntersectMBO/cardano-db-sync/blob/master/doc/configuration.md
#![deny(missing_docs)]
#![allow(rustdoc::private_intra_doc_links)]

pub use crate::{
	data_sources::{ConnectionConfig, PgPool, get_connection_from_env},
	metrics::{McFollowerMetrics, register_metrics_warn_errors},
};
pub use db_sync_sqlx::{
	DbSyncAddressMode, DbSyncQueryConfig, DbSyncSchemaMode, DbSyncTxInputMode,
	ResolvedDbSyncAddressMode, ResolvedDbSyncQueryConfig, ResolvedDbSyncTxInputMode,
};

#[cfg(feature = "block-source")]
pub use crate::block::{BlockDataSourceImpl, DbSyncBlockDataSourceConfig};
#[cfg(feature = "bridge")]
pub use crate::bridge::{TokenBridgeDataSourceImpl, cache::CachedTokenBridgeDataSourceImpl};
#[cfg(feature = "candidate-source")]
pub use crate::candidates::CandidatesDataSourceImpl;
#[cfg(feature = "mc-hash")]
pub use crate::mc_hash::McHashDataSourceImpl;
#[cfg(feature = "sidechain-rpc")]
pub use crate::sidechain_rpc::SidechainRpcDataSourceImpl;
#[cfg(feature = "block-source")]
pub use sidechain_mc_hash::StableBlockByHashResult;

mod data_sources;
mod db_datum;
mod db_model;
mod metrics;

#[cfg(feature = "block-source")]
mod block;
#[cfg(feature = "bridge")]
mod bridge;
#[cfg(feature = "candidate-source")]
mod candidates;
#[cfg(feature = "mc-hash")]
mod mc_hash;
#[cfg(feature = "sidechain-rpc")]
mod sidechain_rpc;

#[derive(Debug)]
/// Wrapper error type for [sqlx::Error]
pub struct SqlxError(sqlx::Error);

impl From<sqlx::Error> for SqlxError {
	fn from(value: sqlx::Error) -> Self {
		SqlxError(value)
	}
}

impl From<SqlxError> for DataSourceError {
	fn from(e: SqlxError) -> Self {
		DataSourceError::InternalDataSourceError(e.0.to_string())
	}
}

impl From<SqlxError> for Box<dyn std::error::Error + Send + Sync> {
	fn from(e: SqlxError) -> Self {
		e.0.into()
	}
}

/// Error type returned by Db-Sync based data sources
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum DataSourceError {
	/// Indicates that the Db-Sync database rejected a request as invalid
	#[error("Bad request: `{0}`.")]
	BadRequest(String),
	/// Indicates that an internal error occured when querying the Db-Sync database
	#[error("Internal error of data source: `{0}`.")]
	InternalDataSourceError(String),
	/// Indicates that expected data was not found when querying the Db-Sync database
	#[error(
		"'{0}' not found. Possible causes: data source configuration error, db-sync not synced fully, or data not set on the main chain."
	)]
	ExpectedDataNotFound(String),
	/// Indicates that data returned by the Db-Sync database is invalid
	#[error(
		"Invalid data. {0} Possible cause is an error in Plutus scripts or data source is outdated."
	)]
	InvalidData(String),
}

#[cfg(test)]
mod tests {
	use ctor::{ctor, dtor};
	use db_sync_sqlx::{DbSyncIndexSpec, DbSyncQueryConfig, DbSyncSchemaMode, manage_indexes};
	use sqlx::PgPool;
	use std::sync::{OnceLock, mpsc};
	use testcontainers_modules::postgres::Postgres;
	use testcontainers_modules::testcontainers::{
		Container, ImageExt,
		bollard::query_parameters::{RemoveContainerOptions, StopContainerOptions},
		core::client::docker_client_instance,
		runners::SyncRunner,
	};

	static POSTGRES: OnceLock<Container<Postgres>> = OnceLock::new();

	pub(crate) async fn normalize_tx_out_addresses(pool: &PgPool) {
		sqlx::raw_sql(
			r#"
CREATE TABLE address (
    id bigserial PRIMARY KEY,
    address character varying NOT NULL UNIQUE,
    raw bytea NOT NULL,
    has_script boolean NOT NULL,
    payment_cred hash28type,
    stake_address_id bigint
);

INSERT INTO address (address, raw, has_script, payment_cred, stake_address_id)
SELECT DISTINCT ON (address)
    address,
    address_raw,
    address_has_script,
    payment_cred,
    stake_address_id
FROM tx_out
ORDER BY address, id;

ALTER TABLE tx_out ADD COLUMN address_id bigint;

UPDATE tx_out
SET address_id = address.id
FROM address
WHERE address.address = tx_out.address;

ALTER TABLE tx_out ALTER COLUMN address_id SET NOT NULL;
ALTER TABLE tx_out
    ADD CONSTRAINT tx_out_address_id_fkey
    FOREIGN KEY (address_id) REFERENCES address(id) ON DELETE CASCADE ON UPDATE RESTRICT;
ALTER TABLE tx_out
    DROP COLUMN address,
    DROP COLUMN address_raw,
    DROP COLUMN address_has_script,
    DROP COLUMN payment_cred;
"#,
		)
		.execute(pool)
		.await
		.unwrap();
	}

	fn address_index_spec() -> DbSyncIndexSpec {
		DbSyncIndexSpec {
			name: "idx_address_address",
			relation: "address",
			access_methods: &["hash", "btree"],
			keys: &["address"],
			create_sql: "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_address_address ON address USING hash(address)",
		}
	}

	async fn create_address_table(pool: &PgPool) {
		sqlx::query(
			"CREATE TABLE address (id bigint PRIMARY KEY, address character varying NOT NULL)",
		)
		.execute(pool)
		.await
		.unwrap();
	}

	#[sqlx::test]
	async fn verify_and_apply_accept_desc_btree_index_without_creating_duplicate(pool: PgPool) {
		create_address_table(&pool).await;
		sqlx::query("CREATE INDEX operator_address_lookup ON address(address DESC)")
			.execute(&pool)
			.await
			.unwrap();

		manage_indexes(&pool, DbSyncSchemaMode::Verify, &[address_index_spec()])
			.await
			.unwrap();
		manage_indexes(&pool, DbSyncSchemaMode::Apply, &[address_index_spec()])
			.await
			.unwrap();

		let midnight_index: Option<String> =
			sqlx::query_scalar("SELECT to_regclass('idx_address_address')::text")
				.fetch_one(&pool)
				.await
				.unwrap();
		assert_eq!(midnight_index, None, "apply must not create a duplicate compatible index");
	}

	#[sqlx::test]
	async fn verify_rejects_index_without_required_leading_key(pool: PgPool) {
		create_address_table(&pool).await;
		sqlx::query("CREATE INDEX operator_wrong_address_lookup ON address(id, address)")
			.execute(&pool)
			.await
			.unwrap();

		let error = manage_indexes(&pool, DbSyncSchemaMode::Verify, &[address_index_spec()])
			.await
			.expect_err("the required address key is not the leading index key");

		assert!(
			error.to_string().contains("address USING hash or btree (address)"),
			"unexpected verification error: {error}"
		);
	}

	#[sqlx::test]
	async fn apply_creates_an_index_that_verify_accepts(pool: PgPool) {
		create_address_table(&pool).await;

		manage_indexes(&pool, DbSyncSchemaMode::Apply, &[address_index_spec()])
			.await
			.unwrap();
		manage_indexes(&pool, DbSyncSchemaMode::Verify, &[address_index_spec()])
			.await
			.unwrap();
	}

	#[sqlx::test]
	async fn auto_rejects_an_empty_schema_when_both_input_layouts_are_possible(pool: PgPool) {
		sqlx::raw_sql(
			"CREATE TABLE tx_out (address character varying NOT NULL, consumed_by_tx_id bigint); \
			 CREATE TABLE tx_in (tx_in_id bigint, tx_out_id bigint, tx_out_index smallint);",
		)
		.execute(&pool)
		.await
		.unwrap();

		let error = DbSyncQueryConfig::default()
			.resolve(&pool)
			.await
			.expect_err("an empty dual-layout schema cannot be inferred safely");
		assert!(error.to_string().contains("both supported representations are empty"));
	}

	fn init_postgres() -> Container<Postgres> {
		Postgres::default().with_tag("17.2").start().unwrap()
	}

	#[ctor]
	fn on_startup() {
		let postgres = POSTGRES.get_or_init(init_postgres);
		let database_url = &format!(
			"postgres://postgres:postgres@127.0.0.1:{}/postgres",
			postgres.get_host_port_ipv4(5432).unwrap()
		);
		// Needed for sqlx::test macro annotation
		unsafe {
			std::env::set_var("DATABASE_URL", database_url);
		}
	}

	#[dtor]
	fn on_shutdown() {
		let (tx, rx) = mpsc::channel();
		std::thread::spawn(move || {
			let runtime =
				tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
			runtime.block_on(async {
				let docker = docker_client_instance().await.unwrap();
				let id = POSTGRES.get().unwrap().id();
				docker.stop_container(id, None::<StopContainerOptions>).await.unwrap();
				docker.remove_container(id, None::<RemoveContainerOptions>).await.unwrap();
				tx.send(());
			});
		});
		let _: () = rx.recv().unwrap();
	}
}
