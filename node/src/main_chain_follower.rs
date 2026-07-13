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

use authority_selection_inherents::AuthoritySelectionDataSource;
use midnight_primitives_mainchain_follower::CandidatesDataSourceImpl;
use midnight_primitives_mainchain_follower::MidnightDataSourceMetrics;
use pallet_sidechain_rpc::SidechainRpcDataSource;
use partner_chains_db_sync_data_sources::{
	BlockDataSourceImpl, CachedTokenBridgeDataSourceImpl, DbSyncBlockDataSourceConfig,
	McFollowerMetrics, McHashDataSourceImpl, SidechainRpcDataSourceImpl,
};
use partner_chains_mock_data_sources::{
	AuthoritySelectionDataSourceMock, BlockDataSourceMock, McHashDataSourceMock,
	SidechainRpcDataSourceMock, TokenBridgeDataSourceMock,
};
use sc_service::error::Error as ServiceError;
use sidechain_mc_hash::McHashDataSource;
use sp_partner_chains_bridge::TokenBridgeDataSource;
use sqlx::{Pool, Postgres};

use super::cfg::midnight_cfg::MidnightCfg;
use midnight_primitives::BridgeRecipient;
use partner_chains_mock_data_sources::MockRegistrationsConfig;
use sidechain_domain::mainchain_epoch::{Duration, MainchainEpochConfig, Timestamp};
use std::{
	error::Error,
	str::FromStr as _,
	sync::Arc,
	time::{Duration as StdDuration, Instant},
};

use midnight_primitives_cnight_observation::{CNightAddresses, CardanoPosition};
use midnight_primitives_mainchain_follower::{
	CNightObservationDataSourceMock, FederatedAuthorityObservationDataSource,
	FederatedAuthorityObservationDataSourceImpl, FederatedAuthorityObservationDataSourceMock,
	MidnightCNightObservationDataSource, MidnightCNightObservationDataSourceImpl,
};
use parity_scale_codec::Decode;

// TODO: Decide if it should be experimental
// #[cfg(feature = "experimental")]

#[derive(Clone)]
pub struct DataSources {
	pub mc_hash: Arc<dyn McHashDataSource + Send + Sync>,
	pub authority_selection: Arc<dyn AuthoritySelectionDataSource + Send + Sync>,
	pub cnight_observation: Arc<dyn MidnightCNightObservationDataSource + Send + Sync>,
	pub sidechain_rpc: Arc<dyn SidechainRpcDataSource + Send + Sync>,
	pub federated_authority_observation:
		Arc<dyn FederatedAuthorityObservationDataSource + Send + Sync>,
	pub bridge: Arc<dyn TokenBridgeDataSource<BridgeRecipient> + Send + Sync>,
}

#[derive(Clone)]
pub struct DbPoolCfg {
	acquire_timeout: std::time::Duration,
	max_connections: u32,
}

pub(crate) async fn create_cached_main_chain_follower_data_sources(
	cfg: MidnightCfg,
	cnight_follower_genesis: Option<(CNightAddresses, CardanoPosition)>,
	mc_metrics_opt: Option<McFollowerMetrics>,
	midnight_metrics_opt: Option<MidnightDataSourceMetrics>,
) -> std::result::Result<DataSources, ServiceError> {
	if cfg.use_main_chain_follower_mock {
		let mock = create_mock_data_sources(cfg.clone()).await.map_err(|err| {
			ServiceError::Application(
				format!("Failed to create main chain follower mock: {err}. Check configuration.")
					.into(),
			)
		})?;

		Ok(mock)
	} else {
		create_cached_data_sources(
			cfg,
			cnight_follower_genesis,
			mc_metrics_opt,
			midnight_metrics_opt,
		)
		.await
		.map_err(|err| {
			ServiceError::Application(
				format!("Failed to create db-sync main chain follower: {err}").into(),
			)
		})
	}
}

pub async fn create_mock_data_sources(
	cfg: MidnightCfg,
) -> std::result::Result<DataSources, Box<dyn Error + Send + Sync + 'static>> {
	let block = Arc::new(BlockDataSourceMock::new(cfg.mc_epoch_duration_millis as u32));

	let authority_selection_data_source_mock = AuthoritySelectionDataSourceMock {
		registrations_data: MockRegistrationsConfig::read_registrations(
			&cfg.mock_registrations_file.ok_or(missing("mock_registrations_file"))?,
		)?,
	};

	Ok(DataSources {
		sidechain_rpc: Arc::new(SidechainRpcDataSourceMock::new(block.clone())),
		mc_hash: Arc::new(McHashDataSourceMock::new(block)),
		authority_selection: Arc::new(authority_selection_data_source_mock),
		cnight_observation: Arc::new(CNightObservationDataSourceMock::new()),
		federated_authority_observation: Arc::new(
			FederatedAuthorityObservationDataSourceMock::new(),
		),
		bridge: Arc::new(TokenBridgeDataSourceMock::<BridgeRecipient>::new()),
	})
}

pub async fn create_index_if_not_exists(pool: &Pool<Postgres>) {
	// Check if index already exists
	let index_exists: bool = sqlx::query_scalar(
		r#"
			SELECT EXISTS (
				SELECT 1 FROM pg_indexes
				WHERE indexname = 'idx_multi_asset_policy_name_hex'
			)
		"#,
	)
	.fetch_one(pool)
	.await
	.unwrap_or(false);

	if index_exists {
		log::info!("Index idx_multi_asset_policy_name_hex already exists, skipping creation.");
	} else {
		log::info!("Creating idx_multi_asset_policy_name_hex index. This may take a while.");
		let index_query_result = sqlx::query(
			r#"
				CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_multi_asset_policy_name_hex
				ON multi_asset ((encode(policy, 'hex')), (encode(name, 'hex')));
			"#,
		)
		.execute(pool)
		.await;

		if let Err(e) = index_query_result {
			log::warn!(
				"Warning: failed to create idx_multi_asset_policy_name_hex index (is your db-sync readonly?). Performance may be degraded: {e}"
			);
		}
	}
}

const DB_SYNC_STARTUP_PROBE_WARN_THRESHOLD: StdDuration = StdDuration::from_millis(500);

async fn log_db_sync_startup_probe(block_data_source: &BlockDataSourceImpl) {
	let latest_tip_started = Instant::now();
	let latest_tip_result = block_data_source.get_latest_block_info().await;
	let latest_tip_elapsed = latest_tip_started.elapsed();

	let block_lookup = if let Ok(latest_tip) = &latest_tip_result {
		let block_lookup_started = Instant::now();
		let block_lookup_result =
			block_data_source.get_block_by_hash(latest_tip.hash.clone()).await;
		Some((block_lookup_started.elapsed(), block_lookup_result))
	} else {
		None
	};

	let latest_tip_state = match &latest_tip_result {
		Ok(_) => "present",
		Err(_) => "query_failed",
	};
	let block_lookup_state = match &block_lookup {
		Some((_, Ok(Some(_)))) => "confirmed",
		Some((_, Ok(None))) => "missing",
		Some((_, Err(_))) => "query_failed",
		None => "skipped",
	};
	let block_lookup_elapsed_ms = block_lookup
		.as_ref()
		.map(|(elapsed, _)| elapsed.as_millis().to_string())
		.unwrap_or_else(|| "n/a".to_string());

	log::info!(
		"DB-sync startup probe: latest_tip={} ({} ms), block_lookup={} ({} ms).",
		latest_tip_state,
		latest_tip_elapsed.as_millis(),
		block_lookup_state,
		block_lookup_elapsed_ms,
	);

	let mut slow_probes = Vec::new();
	if latest_tip_elapsed > DB_SYNC_STARTUP_PROBE_WARN_THRESHOLD {
		slow_probes.push(format!("latest_tip={} ms", latest_tip_elapsed.as_millis()));
	}
	if let Some((elapsed, _)) = &block_lookup
		&& *elapsed > DB_SYNC_STARTUP_PROBE_WARN_THRESHOLD
	{
		slow_probes.push(format!("block_lookup={} ms", elapsed.as_millis()));
	}

	if !slow_probes.is_empty() {
		log::warn!(
			"DB-sync startup probe reported slow reads (threshold: {} ms): {}.",
			DB_SYNC_STARTUP_PROBE_WARN_THRESHOLD.as_millis(),
			slow_probes.join(", "),
		);
	}
}

pub const CANDIDATES_FOR_EPOCH_CACHE_SIZE: usize = 64;
pub const BRIDGE_TRANSFER_CACHE_LOOKAHEAD: u32 = 1000;

// FIXME: these should almost certainly be Cfg in MidnightCfg, so users can tweak as needed
const CANDIDATES_POOL_CFG: DbPoolCfg =
	DbPoolCfg { acquire_timeout: std::time::Duration::from_secs(30), max_connections: 5 };
const SIDECHAIN_POOL_CFG: DbPoolCfg =
	DbPoolCfg { acquire_timeout: std::time::Duration::from_secs(30), max_connections: 5 };
const MC_HASH_POOL_CFG: DbPoolCfg =
	DbPoolCfg { acquire_timeout: std::time::Duration::from_secs(30), max_connections: 5 };
const CNIGHT_OBSERVATION_POOL_CFG: DbPoolCfg =
	DbPoolCfg { acquire_timeout: std::time::Duration::from_secs(30), max_connections: 10 };
const FEDERATED_AUTHORITY_OBSERVATION_POOL_CFG: DbPoolCfg =
	DbPoolCfg { acquire_timeout: std::time::Duration::from_secs(30), max_connections: 5 };
const BRIDGE_POOL_CFG: DbPoolCfg =
	DbPoolCfg { acquire_timeout: std::time::Duration::from_secs(30), max_connections: 2 };
const ICS_POOL_CFG: DbPoolCfg =
	DbPoolCfg { acquire_timeout: std::time::Duration::from_secs(30), max_connections: 5 };

/// Recover the cNIGHT addresses + `next_cardano_position` the follower needs
/// directly from the chainspec's genesis storage.
///
/// The `cnight-observation` pallet's `genesis_build` writes these into storage
/// (`MainChainMappingValidatorAddress`, `CNightIdentifier`,
/// `MainChainAuthTokenAssetName`, `NextCardanoPosition`), so the values are
/// always present in any chainspec — built-from-config or raw — without needing
/// the separate cnight-genesis file. `BoundedVec<u8>` shares `Vec<u8>`'s SCALE
/// encoding, and the string fields are stored as their UTF-8 bytes, so this
/// reconstructs the exact `CNightAddresses` that built the spec.
pub fn cnight_follower_genesis_from_storage(
	genesis_storage: &sp_core::storage::Storage,
) -> Option<(CNightAddresses, CardanoPosition)> {
	let storage_value_key = |item: &[u8]| {
		let mut key = sp_crypto_hashing::twox_128(b"CNightObservation").to_vec();
		key.extend_from_slice(&sp_crypto_hashing::twox_128(item));
		key
	};
	let raw = |item: &[u8]| genesis_storage.top.get(&storage_value_key(item));

	let mapping_validator_address = String::from_utf8(
		Vec::<u8>::decode(&mut &raw(b"MainChainMappingValidatorAddress")?[..]).ok()?,
	)
	.ok()?;
	let auth_token_asset_name =
		String::from_utf8(Vec::<u8>::decode(&mut &raw(b"MainChainAuthTokenAssetName")?[..]).ok()?)
			.ok()?;
	let (policy_bytes, asset_bytes) =
		<(Vec<u8>, Vec<u8>)>::decode(&mut &raw(b"CNightIdentifier")?[..]).ok()?;
	let cnight_policy_id: [u8; 28] = policy_bytes.try_into().ok()?;
	let cnight_asset_name = String::from_utf8(asset_bytes).ok()?;
	let next_cardano_position =
		CardanoPosition::decode(&mut &raw(b"NextCardanoPosition")?[..]).ok()?;

	Some((
		CNightAddresses {
			mapping_validator_address,
			auth_token_asset_name,
			cnight_policy_id,
			cnight_asset_name,
		},
		next_cardano_position,
	))
}

/// Build the cNIGHT-observation data source.
///
/// Uses `BulkCachedCNightObservationDataSource` (in-memory sliding window) when
/// the cNIGHT genesis (addresses + next position) could be resolved — needed to
/// resolve the cNIGHT addresses we query db-sync for. Falls back to the per-call
/// db-backed source otherwise; sync is significantly slower in that case.
async fn build_cnight_observation_data_source(
	cnight_observation_window_size: u32,
	cnight_follower_genesis: Option<(CNightAddresses, CardanoPosition)>,
	cnight_observation_pool: Pool<Postgres>,
	db_sync_block_data_source_config: &DbSyncBlockDataSourceConfig,
	midnight_metrics_opt: Option<MidnightDataSourceMetrics>,
) -> Result<
	Arc<dyn MidnightCNightObservationDataSource + Send + Sync>,
	Box<dyn Error + Send + Sync + 'static>,
> {
	use midnight_primitives_mainchain_follower::data_source::{
		BulkCacheConfig, BulkCachedCNightObservationDataSource,
	};

	match cnight_follower_genesis {
		Some((cnight_addresses, next_cardano_position)) => {
			// Anchor the cache at the genesis observation position. On a
			// fresh sync, snapshot_end = next - 1 makes the first refresh's
			// `from_block = old_end + 1` land exactly on `next` (inclusive of
			// the boundary event). On a node restarting already (partially)
			// synced, the first refresh instead jumps the window forward to
			// the runtime's actual position (`plan_refresh`), so genesis
			// history is not re-pulled.
			let next_pos: u32 = next_cardano_position.block_number;
			let init_horizon = next_pos.saturating_sub(1);
			let window_size: u32 = cnight_observation_window_size;

			// Empty initial cache so the node starts up immediately. The
			// first follower call will see `tip_pos > horizon`, delegate
			// to db_fallback for that one call, and kick a background
			// refresh that populates the window. Subsequent calls hit
			// the cache.
			log::info!(
				"cNIGHT observation: sliding window cache (anchor = Cardano block {next_pos}, window = {window_size})"
			);
			let stability_margin = db_sync_block_data_source_config
				.cardano_security_parameter
				.saturating_add(db_sync_block_data_source_config.block_stability_margin);
			let db_fallback = Arc::new(MidnightCNightObservationDataSourceImpl::new(
				cnight_observation_pool.clone(),
				midnight_metrics_opt.clone(),
				1000,
			));
			Ok(Arc::new(BulkCachedCNightObservationDataSource::new(
				Vec::new(),
				BulkCacheConfig {
					window_start_block: init_horizon,
					window_end_block: init_horizon,
					window_size,
					stability_margin,
					pool: cnight_observation_pool,
					db_fallback,
					cnight_addresses,
					metrics_opt: midnight_metrics_opt,
				},
			)))
		},
		None => {
			log::warn!(
				"cNIGHT observation: no cNIGHT genesis found in the chainspec (or cnight-genesis file) \
				— falling back to per-call db-sync queries. Sync will be significantly slower.",
			);
			Ok(Arc::new(MidnightCNightObservationDataSourceImpl::new(
				cnight_observation_pool,
				midnight_metrics_opt,
				1000,
			)))
		},
	}
}

fn warn_deprecated_allow_non_ssl(cfg: &MidnightCfg) {
	if cfg.allow_non_ssl {
		log::warn!(
			"allow_non_ssl is set but ignored — all database connections use TLS. \
			 This flag will be removed in a future release."
		);
	}
}

pub async fn create_cached_data_sources(
	cfg: MidnightCfg,
	cnight_follower_genesis: Option<(CNightAddresses, CardanoPosition)>,
	mc_metrics_opt: Option<McFollowerMetrics>,
	midnight_metrics_opt: Option<MidnightDataSourceMetrics>,
) -> Result<DataSources, Box<dyn Error + Send + Sync + 'static>> {
	warn_deprecated_allow_non_ssl(&cfg);
	let postgres_uri = &cfg
		.db_sync_postgres_connection_string
		.ok_or(missing("db_sync_postgres_connection_string"))?;

	let db_sync_block_data_source_config = DbSyncBlockDataSourceConfig {
		cardano_security_parameter: cfg
			.cardano_security_parameter
			.ok_or(missing("cardano_security_parameter"))?,
		cardano_active_slots_coeff: cfg
			.cardano_active_slots_coeff
			.ok_or(missing("cardano_active_slots_coeff"))?,
		block_stability_margin: cfg
			.block_stability_margin
			.ok_or(missing("block_stability_margin"))?,
	};

	let mc = MainchainEpochConfig {
		first_epoch_timestamp_millis: Timestamp::from_unix_millis(
			cfg.mc_first_epoch_timestamp_millis,
		),
		epoch_duration_millis: Duration::from_millis(cfg.mc_epoch_duration_millis),
		first_epoch_number: cfg.mc_first_epoch_number,
		first_slot_number: cfg.mc_first_slot_number,
		slot_duration_millis: Duration::from_millis(cfg.mc_slot_duration_millis),
	};

	let candidates_pool =
		get_connection(postgres_uri, CANDIDATES_POOL_CFG, cfg.ssl_root_cert.as_deref())
			.await
			.map_err(|e| {
				log::warn!("Failed to connect to database for candidates data source: {e}");
				e
			})?;

	// All these pools are connections to the same database, so we can use any pool to create the index
	create_index_if_not_exists(&candidates_pool).await;

	let candidates_data_source =
		CandidatesDataSourceImpl::new(candidates_pool, midnight_metrics_opt.clone())
			.await
			.map_err(|e| {
				log::warn!("Failed to initialise candidates data source: {e}");
				e
			})?;
	let candidates_data_source_cached =
		candidates_data_source.cached(CANDIDATES_FOR_EPOCH_CACHE_SIZE).map_err(|e| {
			log::warn!("Failed to create candidates data source cache: {e}");
			e
		})?;

	let sidechain_pool =
		get_connection(postgres_uri, SIDECHAIN_POOL_CFG, cfg.ssl_root_cert.as_deref())
			.await
			.map_err(|e| {
				log::warn!("Failed to connect to database for sidechain data source: {e}");
				e
			})?;
	let sidechain_block_data_source = Arc::new(BlockDataSourceImpl::from_config(
		sidechain_pool,
		db_sync_block_data_source_config.clone(),
		&mc,
	));
	log_db_sync_startup_probe(sidechain_block_data_source.as_ref()).await;
	let sidechain_rpc = SidechainRpcDataSourceImpl::new(
		sidechain_block_data_source.clone(),
		mc_metrics_opt.clone(),
	);

	let mc_hash_pool = get_connection(postgres_uri, MC_HASH_POOL_CFG, cfg.ssl_root_cert.as_deref())
		.await
		.map_err(|e| {
			log::warn!("Failed to connect to database for mc_hash data source: {e}");
			e
		})?;
	let mc_hash_block_data_source = BlockDataSourceImpl::from_config(
		mc_hash_pool,
		db_sync_block_data_source_config.clone(),
		&mc,
	);
	let mc_hash =
		McHashDataSourceImpl::new(Arc::new(mc_hash_block_data_source), mc_metrics_opt.clone());

	let cnight_observation_pool =
		get_connection(postgres_uri, CNIGHT_OBSERVATION_POOL_CFG, cfg.ssl_root_cert.as_deref())
			.await
			.map_err(|e| {
				log::warn!("Failed to connect to database for cnight_observation data source: {e}");
				e
			})?;
	let cnight_observation = build_cnight_observation_data_source(
		cfg.cnight_observation_window_size,
		cnight_follower_genesis,
		cnight_observation_pool,
		&db_sync_block_data_source_config,
		midnight_metrics_opt.clone(),
	)
	.await?;

	let federated_authority_observation_pool = get_connection(
		postgres_uri,
		FEDERATED_AUTHORITY_OBSERVATION_POOL_CFG,
		cfg.ssl_root_cert.as_deref(),
	)
	.await
	.map_err(|e| {
		log::warn!(
			"Failed to connect to database for federated_authority_observation data source: {e}"
		);
		e
	})?;
	let federated_authority_observation = FederatedAuthorityObservationDataSourceImpl::new(
		federated_authority_observation_pool,
		midnight_metrics_opt,
		1000,
	);

	let bridge_pool = get_connection(postgres_uri, BRIDGE_POOL_CFG, cfg.ssl_root_cert.as_deref())
		.await
		.map_err(|e| {
			log::warn!("Failed to connect to database for bridge data source: {e}");
			e
		})?;

	let bridge = CachedTokenBridgeDataSourceImpl::new(
		bridge_pool,
		mc_metrics_opt,
		sidechain_block_data_source,
		BRIDGE_TRANSFER_CACHE_LOOKAHEAD,
	);

	Ok(DataSources {
		sidechain_rpc: Arc::new(sidechain_rpc),
		mc_hash: Arc::new(mc_hash),
		authority_selection: Arc::new(candidates_data_source_cached),
		cnight_observation,
		bridge: Arc::new(bridge),
		federated_authority_observation: Arc::new(federated_authority_observation),
	})
}

// Helper for users who only need native token observation data source
pub async fn create_cnight_observation_data_source(
	cfg: MidnightCfg,
	metrics_opt: Option<MidnightDataSourceMetrics>,
) -> Result<Arc<dyn MidnightCNightObservationDataSource>, Box<dyn Error + Send + Sync + 'static>> {
	warn_deprecated_allow_non_ssl(&cfg);
	let pool = get_connection(
		&cfg.db_sync_postgres_connection_string
			.ok_or(missing("db_sync_postgres_connection_string"))?,
		CNIGHT_OBSERVATION_POOL_CFG,
		cfg.ssl_root_cert.as_deref(),
	)
	.await?;

	midnight_primitives_mainchain_follower::db::create_cnight_observation_indexes(&pool).await?;
	midnight_primitives_mainchain_follower::db::apply_cnight_observation_autovacuum_tuning(&pool)
		.await?;

	Ok(Arc::new(MidnightCNightObservationDataSourceImpl::new(pool, metrics_opt, 1000)))
}

pub async fn create_federated_authority_observation_data_source(
	cfg: MidnightCfg,
	metrics_opt: Option<MidnightDataSourceMetrics>,
) -> Result<Arc<dyn FederatedAuthorityObservationDataSource>, Box<dyn Error + Send + Sync + 'static>>
{
	warn_deprecated_allow_non_ssl(&cfg);
	let pool = get_connection(
		&cfg.db_sync_postgres_connection_string
			.ok_or(missing("db_sync_postgres_connection_string"))?,
		FEDERATED_AUTHORITY_OBSERVATION_POOL_CFG,
		cfg.ssl_root_cert.as_deref(),
	)
	.await?;

	Ok(Arc::new(FederatedAuthorityObservationDataSourceImpl::new(pool, metrics_opt, 1000)))
}

pub async fn create_authority_selection_data_source(
	cfg: MidnightCfg,
	metrics_opt: Option<MidnightDataSourceMetrics>,
) -> Result<
	Arc<dyn AuthoritySelectionDataSource + Send + Sync>,
	Box<dyn Error + Send + Sync + 'static>,
> {
	let (data_source, _pool) =
		create_authority_selection_data_source_with_pool(cfg, metrics_opt).await?;
	Ok(data_source)
}

pub async fn create_authority_selection_data_source_with_pool(
	cfg: MidnightCfg,
	metrics_opt: Option<MidnightDataSourceMetrics>,
) -> Result<
	(Arc<dyn AuthoritySelectionDataSource + Send + Sync>, sqlx::PgPool),
	Box<dyn Error + Send + Sync + 'static>,
> {
	warn_deprecated_allow_non_ssl(&cfg);
	let pool = get_connection(
		&cfg.db_sync_postgres_connection_string
			.ok_or(missing("db_sync_postgres_connection_string"))?,
		CANDIDATES_POOL_CFG,
		cfg.ssl_root_cert.as_deref(),
	)
	.await?;

	let candidates_data_source = CandidatesDataSourceImpl::new(pool.clone(), metrics_opt).await?;
	let candidates_data_source_cached =
		candidates_data_source.cached(CANDIDATES_FOR_EPOCH_CACHE_SIZE)?;

	Ok((Arc::new(candidates_data_source_cached), pool))
}

/// Create a database pool for ICS genesis queries
pub async fn create_ics_genesis_pool(
	cfg: MidnightCfg,
) -> Result<sqlx::PgPool, Box<dyn Error + Send + Sync + 'static>> {
	warn_deprecated_allow_non_ssl(&cfg);
	let pool = get_connection(
		&cfg.db_sync_postgres_connection_string
			.ok_or(missing("db_sync_postgres_connection_string"))?,
		ICS_POOL_CFG,
		cfg.ssl_root_cert.as_deref(),
	)
	.await?;
	Ok(pool)
}

fn build_ssl_connect_options(
	connection_string: &str,
	ssl_root_cert: Option<&str>,
) -> Result<
	(sqlx::postgres::PgSslMode, sqlx::postgres::PgConnectOptions),
	Box<dyn Error + Send + Sync + 'static>,
> {
	let ssl_mode = if ssl_root_cert.is_some() {
		sqlx::postgres::PgSslMode::VerifyFull
	} else {
		log::warn!(
			"No ssl_root_cert configured: using PgSslMode::Require (encrypted but no certificate validation). Set ssl_root_cert for full MITM protection."
		);
		sqlx::postgres::PgSslMode::Require
	};
	let mut options =
		sqlx::postgres::PgConnectOptions::from_str(connection_string)?.ssl_mode(ssl_mode);
	if let Some(cert_path) = ssl_root_cert {
		options = options.ssl_root_cert(cert_path);
	}
	Ok((ssl_mode, options))
}

async fn get_connection(
	connection_string: &str,
	pool_cfg: DbPoolCfg,
	ssl_root_cert: Option<&str>,
) -> Result<sqlx::PgPool, Box<dyn Error + Send + Sync + 'static>> {
	let (ssl_mode, connect_options) = build_ssl_connect_options(connection_string, ssl_root_cert)?;
	log::info!("Database connection SSL mode: {ssl_mode:?}");

	let pool = sqlx::postgres::PgPoolOptions::new()
		.max_connections(pool_cfg.max_connections)
		.acquire_timeout(pool_cfg.acquire_timeout)
		.connect_with(connect_options.clone())
		.await
		.map_err(|e| {
			log::debug!(
				"Database connection details: host={}, port={}, database={}; error: {e}",
				connect_options.get_host(),
				connect_options.get_port(),
				connect_options.get_database().unwrap_or("cexplorer"),
			);
			PostgresConnectionError(e.to_string())
		})?;
	Ok(pool)
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("Could not connect to database; error: {0}")]
struct PostgresConnectionError(String);

fn missing(field: &str) -> sc_service::Error {
	ServiceError::Application(format!("Missing {field}. Check configuration.").into())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn connection_error_redacts_infrastructure_details() {
		let error = PostgresConnectionError("pool timed out".to_string());
		let message = error.to_string();

		assert!(
			message.contains("Could not connect to database"),
			"error must indicate connection failure"
		);
		assert!(message.contains("pool timed out"), "error must contain underlying error");
		assert!(!message.contains("localhost"), "error must not contain host");
		assert!(!message.contains("5432"), "error must not contain default port");
		assert!(!message.contains("cexplorer"), "error must not contain database name");
	}

	const TEST_CONN_STR: &str = "postgres://user:pass@localhost:5432/testdb";

	#[test]
	fn ssl_mode_is_verify_full_when_root_cert_provided() {
		let (mode, _opts) =
			build_ssl_connect_options(TEST_CONN_STR, Some("/path/to/ca.pem")).unwrap();
		assert!(matches!(mode, sqlx::postgres::PgSslMode::VerifyFull));
	}

	#[test]
	fn ssl_mode_is_require_when_no_root_cert() {
		let (mode, _opts) = build_ssl_connect_options(TEST_CONN_STR, None).unwrap();
		assert!(matches!(mode, sqlx::postgres::PgSslMode::Require));
	}

	#[test]
	fn ssl_mode_is_never_disable() {
		for cert in [None, Some("/path/to/ca.pem")] {
			let (mode, _opts) = build_ssl_connect_options(TEST_CONN_STR, cert).unwrap();
			assert!(!matches!(mode, sqlx::postgres::PgSslMode::Disable));
		}
	}

	#[test]
	fn invalid_connection_string_returns_error() {
		let result = build_ssl_connect_options("not-a-valid-uri", None);
		assert!(result.is_err());
	}
}
