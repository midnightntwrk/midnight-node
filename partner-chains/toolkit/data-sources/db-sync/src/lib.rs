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
//! Partner Chains data sources require specific Db-Sync configuration to be set for them to
//! operate correctly:
//! - `insert_options.tx_out.value`: must be either `"enable"` (default) or `"consumed"`.
//!   The data sources in this crate that need to query transaction intputs automatically detect
//!   which option is used and adjust their queries accordingly. This requires the database to be
//!   already initialized by db-sync. When run for an uninitialized database, the data sources
//!   will default to the `"enable"` option.
//! - `insert_options.tx_out.use_address_table`: must be `false` (default).
//! - `insert_options.ledger`: must be `"enable"` (default).
//! - `insert_options.multi_asset`: must be `true` (default).
//! - `insert_options.governance`: must `"enable"` (default).
//! - `insert_options.remove_jsonb_from_schema`: must be `"disable"` (default).
//! - `insert_options.plutus`: must be `"enable"` (default).
//!
//! The default Cardano DB Sync configuration meets these requirements, so Partner Chain node
//! operators that do not wish to use any custom configuration can use the defaults, otherwise
//! they must preserve the values described above. See [Db-Sync configuration docs] for more
//! information.
//!
//! ## Custom Indexes
//!
//! In addition to indexes automatically created by Db-Sync itself, data sources in this crate
//! require additional ones to be created for some of the queries to execute efficiently. These
//! indexes are:
//! - `idx_ma_tx_out_ident ON ma_tx_out(ident)`
//! - `idx_tx_out_address ON tx_out USING hash (address)`
//!
//! The data sources in this crate automatically create these indexes when needed at node startup.
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
pub(crate) mod test_db {
	use sqlx::{PgPool, migrate::Migrator};
	use std::path::Path;
	use testcontainers::{
		ContainerAsync, GenericImage, ImageExt,
		core::{IntoContainerPort, WaitFor},
		runners::AsyncRunner,
	};

	const READY_MSG: &str = "database system is ready to accept connections";

	/// Postgres container owned by a single test. It is removed when dropped, including on panic.
	pub(crate) type TestDb = ContainerAsync<GenericImage>;

	/// Starts a fresh Postgres and applies the migrations from `migrations_dir`
	/// (relative to the crate root).
	pub(crate) async fn test_db(migrations_dir: &str) -> (PgPool, TestDb) {
		let (pool, container) = empty_test_db().await;
		let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(migrations_dir);
		Migrator::new(dir).await.unwrap().run(&pool).await.unwrap();
		(pool, container)
	}

	pub(crate) async fn empty_test_db() -> (PgPool, TestDb) {
		// The entrypoint logs the readiness message on stdout for its temporary init server
		// and on stderr for the final one.
		let container = GenericImage::new("postgres", "17.2")
			.with_exposed_port(5432.tcp())
			.with_wait_for(WaitFor::message_on_stderr(READY_MSG))
			.with_wait_for(WaitFor::message_on_stdout(READY_MSG))
			.with_env_var("POSTGRES_HOST_AUTH_METHOD", "trust")
			.with_cmd(["postgres", "-c", "fsync=off"])
			.start()
			.await
			.expect("failed to start postgres container");
		let url = format!(
			"postgres://postgres@{}:{}/postgres",
			container.get_host().await.unwrap(),
			container.get_host_port_ipv4(5432).await.unwrap()
		);
		let pool = PgPool::connect(&url).await.unwrap();
		(pool, container)
	}
}
