//! Db-Sync data source implementation that queries Cardano block information
use crate::{
	DataSourceError::*,
	data_sources::read_mc_epoch_config,
	db_model::{self, Block, BlockNumber, SlotNumber},
	metrics::McFollowerMetrics,
};
use chrono::{DateTime, NaiveDateTime, TimeDelta};
use derive_new::new;
use figment::{Figment, providers::Env};
use log::{debug, info, warn};
use serde::Deserialize;
use sidechain_domain::mainchain_epoch::{MainchainEpochConfig, MainchainEpochDerivation};
use sidechain_domain::*;
use sidechain_mc_hash::StableBlockByHashResult;
use sp_timestamp::Timestamp;
use sqlx::PgPool;
use std::{
	error::Error,
	sync::{Arc, Mutex},
};
use time_source::{SystemTimeSource, TimeSource};

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
enum StableBlockByHashError {
	#[error("Database query failed: {0}")]
	Database(String),
	#[error("Block with hash {0} was not found.")]
	BlockNotFound(McBlockHash),
	#[error("Latest block info was unavailable while checking block hash {0}.")]
	LatestBlockUnavailable(McBlockHash),
	#[error(
		"Block with hash {hash} is not stable yet: block {block_no} requires latest block >= {required_latest_block_no}, but latest block is {latest_block_no}."
	)]
	NotStableYet {
		hash: McBlockHash,
		block_no: u32,
		required_latest_block_no: u32,
		latest_block_no: u32,
		block: Block,
	},
	#[error(
		"Block with hash {hash} has timestamp {block_time}, outside allowed range [{min_allowed_time}..={max_allowed_time}] for reference timestamp {reference_timestamp}."
	)]
	TimestampOutOfRange {
		hash: McBlockHash,
		block_time: NaiveDateTime,
		min_allowed_time: NaiveDateTime,
		max_allowed_time: NaiveDateTime,
		reference_timestamp: NaiveDateTime,
		block: Block,
	},
}

/// Db-Sync data source that queries Cardano block information
///
/// This data source does not implement any data source interface used by one of the
/// Partner Chain toolkit's features, but is used internally by other data sources
/// that require access to Cardano block data
#[allow(clippy::too_many_arguments)]
#[derive(new)]
pub struct BlockDataSourceImpl {
	/// Postgres connection pool
	pool: PgPool,
	/// Cardano security parameter
	///
	/// This parameter controls how many confirmations (blocks on top) are required by
	/// the Cardano node to consider a block to be stable. This is a network-wide parameter.
	security_parameter: u32,
	/// Minimal age of a block to be considered valid stable in relation to some given timestamp.
	/// Must be equal to `security parameter / active slot coefficient`.
	min_slot_boundary_as_seconds: TimeDelta,
	/// a characteristic of Ouroboros Praos and is equal to `3 * security parameter / active slot coefficient`
	max_slot_boundary_as_seconds: TimeDelta,
	/// Cardano main chain epoch configuration
	mainchain_epoch_config: MainchainEpochConfig,
	/// Additional offset applied when selecting the latest stable Cardano block
	///
	/// Block producers SHOULD take this parameter into account.
	/// Block verification uses this value to determine if their observed Cardano tip is recent enough,
	/// when verified block references unstable Cardano block.
	/// Bigger value of this parameter across both block producers and validators makes attacks on partner-chain liveness not feasible.
	/// Recommended value is 10, that add 200 seconds of lag in observing Cardano, but is also safe unless Cardano tip is not older then 200 seconds.
	block_stability_margin: u32,
	/// Maximum tolerated age of the latest observed Cardano block before our view of
	/// Cardano is considered stale by [Self::is_cardano_ok].
	/// Should be set to block_stability_margin * expected block time.
	max_latest_block_age_seconds: u32,
	/// Number of contiguous Cardano blocks to be cached by this data source
	cache_size: u16,
	/// Internal block cache
	stable_blocks_cache: Arc<Mutex<BlocksCache>>,
	/// Prometheus metrics client
	metrics_opt: Option<McFollowerMetrics>,
	/// Time source used for current-time queries (e.g. Cardano sync health checks)
	time_source: Arc<dyn TimeSource + Send + Sync>,
}

impl BlockDataSourceImpl {
	/// Returns the latest _unstable_ Cardano block from the Db-Sync database
	pub async fn get_latest_block_info(
		&self,
	) -> Result<MainchainBlock, Box<dyn std::error::Error + Send + Sync>> {
		db_model::get_latest_block_info(&self.pool)
			.await?
			.map(From::from)
			.ok_or(ExpectedDataNotFound("No latest block on chain.".to_string()).into())
	}

	/// Returns the latest _stable_ Cardano block from the Db-Sync database that is within
	/// acceptable bounds from `reference_timestamp`, accounting for the additional stability
	/// offset configured by [block_stability_margin][Self::block_stability_margin].
	pub async fn get_latest_stable_block_for(
		&self,
		reference_timestamp: Timestamp,
	) -> Result<Option<MainchainBlock>, Box<dyn std::error::Error + Send + Sync>> {
		let reference_timestamp = BlockDataSourceImpl::timestamp_to_db_type(reference_timestamp)?;
		let latest = self.get_latest_block_info().await?;
		let offset = self.security_parameter + self.block_stability_margin;
		let stable = latest.number.saturating_sub(offset).into();
		let block = self.get_latest_block(stable, reference_timestamp).await?;
		Ok(block.map(From::from))
	}

	/// Finds a block by its `hash` and classifies it relative to `reference_timestamp`,
	/// returning whether the block is unknown, found-but-not-yet-stable, or found-and-stable.
	pub async fn get_stable_block_for(
		&self,
		hash: McBlockHash,
		reference_timestamp: Timestamp,
	) -> Result<StableBlockByHashResult, Box<dyn std::error::Error + Send + Sync>> {
		let reference_timestamp = BlockDataSourceImpl::timestamp_to_db_type(reference_timestamp)?;
		self.get_stable_block_by_hash(hash, reference_timestamp).await
	}

	/// Finds a block by its `hash` and returns its info
	pub async fn get_block_by_hash(
		&self,
		hash: McBlockHash,
	) -> Result<Option<MainchainBlock>, Box<dyn std::error::Error + Send + Sync>> {
		let from_cache = if let Ok(cache) = self.stable_blocks_cache.lock() {
			cache.find_by_hash(hash.clone())
		} else {
			None
		};
		let block_opt = match from_cache {
			Some(block) => Some(block),
			None => db_model::get_block_by_hash(&self.pool, hash).await?,
		};
		Ok(block_opt.map(From::from))
	}

	/// Tests if our Cardano tip is at most BLOCK_MARGIN * 'expected block interval' old.
	/// Possibly returns `false` when it is just Cardano chain density drop.
	/// Invoke only when there is some other hint, like unknown block to validate,
	/// that Cardano has problems.
	pub async fn is_cardano_tip_fresh(
		&self,
	) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
		let block = self.get_latest_block_info().await?;
		let is_block_time_valid = self.is_block_fresh(&block);
		log::debug!(
			"is_cardano_tip_fresh block: {block:?}, is_block_time_valid: {is_block_time_valid}"
		);
		Ok(is_block_time_valid)
	}

	/// Tests if out Cardano view matches Praos requirements.
	/// Chain quality rule: at least one block in the last security_parameter/active_slots_coeff slots.
	/// Chain growth rule: at least security_parameters of block in the last 3*security_parameter/active_slots_coeff slots.
	/// See https://ouroboros-consensus.cardano.intersectmbo.org/docs/references/miscellaneous/cardano_praos_basics/.
	/// Unlike [Self::is_cardano_tip_fresh] it is not heuristic.
	pub async fn is_cardano_ok(&self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
		let current_time = self.time_source.get_current_time_millis();
		let current_time = BlockDataSourceImpl::timestamp_to_db_type(Timestamp::new(current_time))?;
		let latest = match db_model::get_latest_block_info(&self.pool).await? {
			None => return Ok(false),
			Some(block) => block,
		};
		// Praos chain quality rule check.
		if current_time - self.min_slot_boundary_as_seconds > latest.time {
			log::debug!(
				"is_cardano_ok current time: {current_time}, latest: {latest:?}, the latest block is too old."
			);
			return Ok(false);
		}
		let stable = BlockNumber(latest.block_no.0.saturating_sub(self.security_parameter));
		// Praos chain growth rule is guaranteed by 'get_latest_block'.
		let block = self.get_latest_block(stable, current_time).await?;
		log::debug!(
			"is_cardano_ok current time: {current_time}, stable: {stable:?}, block: {block:?}"
		);
		Ok(block.is_some())
	}
}

/// Configuration for [BlockDataSourceImpl]
#[derive(Debug, Clone, Deserialize)]
pub struct DbSyncBlockDataSourceConfig {
	/// Cardano security parameter, ie. the number of confirmations needed to stabilize a block
	pub cardano_security_parameter: u32,
	/// Expected fraction of Cardano slots that will have a block produced
	///
	/// This value can be found in `shelley-genesis.json` file used by the Cardano node,
	/// example: `"activeSlotsCoeff": 0.05`.
	pub cardano_active_slots_coeff: f64,
	/// Additional offset applied when selecting the latest stable Cardano block
	///
	/// This parameter should be 0 by default and should only be increased to 1 in networks
	/// struggling with frequent block rejections due to Db-Sync or Cardano node lag.
	pub block_stability_margin: u32,
}

impl DbSyncBlockDataSourceConfig {
	/// Reads the config from environment
	pub fn from_env() -> std::result::Result<Self, Box<dyn Error + Send + Sync + 'static>> {
		let config: Self = Figment::new()
			.merge(Env::raw())
			.extract()
			.map_err(|e| format!("Failed to read block data source config: {e}"))?;
		info!("Using block data source configuration: {config:?}");
		Ok(config)
	}
}

impl BlockDataSourceImpl {
	/// Creates a new instance of [BlockDataSourceImpl], reading configuration from the environment.
	pub async fn new_from_env(
		pool: PgPool,
	) -> std::result::Result<Self, Box<dyn Error + Send + Sync + 'static>> {
		Self::new_from_env_with_metrics(pool, None).await
	}

	/// Creates a new instance of [BlockDataSourceImpl], reading configuration from the
	/// environment and wiring in an optional Prometheus metrics client.
	pub async fn new_from_env_with_metrics(
		pool: PgPool,
		metrics_opt: Option<McFollowerMetrics>,
	) -> std::result::Result<Self, Box<dyn Error + Send + Sync + 'static>> {
		Ok(Self::from_config_with_metrics(
			pool,
			DbSyncBlockDataSourceConfig::from_env()?,
			&read_mc_epoch_config()?,
			metrics_opt,
		))
	}

	/// Creates a new instance of [BlockDataSourceImpl], using passed configuration.
	pub fn from_config(
		pool: PgPool,
		config: DbSyncBlockDataSourceConfig,
		mc_epoch_config: &MainchainEpochConfig,
	) -> BlockDataSourceImpl {
		Self::from_config_with_metrics(pool, config, mc_epoch_config, None)
	}

	/// Creates a new instance of [BlockDataSourceImpl], using passed configuration and an
	/// optional Prometheus metrics client.
	pub fn from_config_with_metrics(
		pool: PgPool,
		DbSyncBlockDataSourceConfig {
			cardano_security_parameter,
			cardano_active_slots_coeff,
			block_stability_margin,
		}: DbSyncBlockDataSourceConfig,
		mc_epoch_config: &MainchainEpochConfig,
		metrics_opt: Option<McFollowerMetrics>,
	) -> BlockDataSourceImpl {
		let k: f64 = cardano_security_parameter.into();
		let slot_duration_ms: f64 = mc_epoch_config.slot_duration_millis.millis() as f64;
		let slot_duration_seconds: f64 = slot_duration_ms / 1000f64;
		let min_slot_boundary = (slot_duration_ms * k / cardano_active_slots_coeff).round() as i64;
		let max_slot_boundary = 3 * min_slot_boundary;
		let expected_blocks_interval_seconds: u32 =
			((slot_duration_seconds / cardano_active_slots_coeff).round() as u64).try_into()
				.unwrap_or_else(|_| panic!("Invalid Cardano observability configuration: slot duration {slot_duration_seconds}[s] is too big or active slots coefficient {cardano_active_slots_coeff} is too small"));
		let max_latest_block_age_seconds =
			block_stability_margin.max(1).saturating_mul(expected_blocks_interval_seconds);
		let cache_size = 100;
		BlockDataSourceImpl::new(
			pool,
			cardano_security_parameter,
			TimeDelta::milliseconds(min_slot_boundary),
			TimeDelta::milliseconds(max_slot_boundary),
			mc_epoch_config.clone(),
			block_stability_margin,
			max_latest_block_age_seconds,
			cache_size,
			BlocksCache::new_arc_mutex(),
			metrics_opt,
			Arc::new(SystemTimeSource),
		)
	}
	async fn get_latest_block(
		&self,
		max_block: BlockNumber,
		reference_timestamp: NaiveDateTime,
	) -> Result<Option<Block>, Box<dyn std::error::Error + Send + Sync>> {
		let min_time = self.min_block_allowed_time(reference_timestamp);
		let min_slot = self.date_time_to_slot(min_time)?;
		let max_time = self.max_allowed_block_time(reference_timestamp);
		let max_slot = self.date_time_to_slot(max_time)?;
		Ok(db_model::get_highest_block(
			&self.pool, max_block, min_time, min_slot, max_time, max_slot,
		)
		.await?)
	}

	fn min_block_allowed_time(&self, reference_timestamp: NaiveDateTime) -> NaiveDateTime {
		reference_timestamp - self.max_slot_boundary_as_seconds
	}

	fn max_allowed_block_time(&self, reference_timestamp: NaiveDateTime) -> NaiveDateTime {
		reference_timestamp - self.min_slot_boundary_as_seconds
	}

	/// Rules for stable block selection and verification mandates that timestamp of the block
	/// falls in a given range, calculated from the reference timestamp, which is either
	/// PC current time or PC block timestamp. Related to Praos "chain growth rule".
	fn is_stable_block_time_valid(&self, block: &Block, timestamp: NaiveDateTime) -> bool {
		self.min_block_allowed_time(timestamp) <= block.time
			&& block.time <= self.max_allowed_block_time(timestamp)
	}

	/// This is heuristic (unlike [Self::is_stable_block_time_valid] or [Self::is_latest_block_time_valid]) of judging the latest
	/// observed block timestamp valid.
	fn is_block_fresh(&self, block: &MainchainBlock) -> bool {
		let current_time_secs = self.time_source.get_current_time_millis() / 1000;
		let delta_secs = current_time_secs.saturating_sub(block.timestamp);
		delta_secs < self.max_latest_block_age_seconds.into()
	}

	fn observe_latest_cardano_block_metrics(&self, latest_block: &Block) {
		if let Some(metrics) = &self.metrics_opt {
			metrics.latest_cardano_block_number().set(u64::from(latest_block.block_no.0));
			metrics.latest_cardano_block_slot().set(latest_block.slot_no.0);
		}
	}

	fn observe_referenced_cardano_block_metrics(&self, block: &Block) {
		if let Some(metrics) = &self.metrics_opt {
			metrics.referenced_cardano_block_number().set(u64::from(block.block_no.0));
			metrics.referenced_cardano_block_slot().set(block.slot_no.0);
		}
	}

	async fn get_stable_block_by_hash(
		&self,
		hash: McBlockHash,
		reference_timestamp: NaiveDateTime,
	) -> Result<StableBlockByHashResult, Box<dyn std::error::Error + Send + Sync>> {
		if let Some(block) =
			self.get_stable_block_by_hash_from_cache(hash.clone(), reference_timestamp)
		{
			debug!("Block by hash: {hash} found in cache.");
			return Ok(StableBlockByHashResult::BlockStable { info: From::from(block) });
		}
		debug!("Block by hash: {hash}, not found in cache, serving from database.");
		match self.get_stable_block_by_hash_from_db(hash, reference_timestamp).await {
			Ok(block) => {
				self.fill_cache(&block).await?;
				Ok(StableBlockByHashResult::BlockStable { info: From::from(block) })
			},
			Err(StableBlockByHashError::BlockNotFound(_)) => {
				Ok(StableBlockByHashResult::BlockNotFound)
			},
			Err(StableBlockByHashError::TimestampOutOfRange { block, .. }) => {
				Ok(StableBlockByHashResult::BlockTimestampOutRange { info: From::from(block) })
			},
			Err(StableBlockByHashError::NotStableYet { block, .. }) => {
				Ok(StableBlockByHashResult::NotEnoughConfirmations { info: From::from(block) })
			},
			Err(err) => {
				warn!("Get stable block by hash failed: {err}");
				Err(err.into())
			},
		}
	}

	fn get_stable_block_by_hash_from_cache(
		&self,
		hash: McBlockHash,
		reference_timestamp: NaiveDateTime,
	) -> Option<Block> {
		if let Ok(cache) = self.stable_blocks_cache.lock() {
			cache
				.find_by_hash(hash)
				.filter(|block| self.is_stable_block_time_valid(block, reference_timestamp))
		} else {
			None
		}
	}

	/// Returns block by given hash from the database if it is stable in reference to given timestamp
	async fn get_stable_block_by_hash_from_db(
		&self,
		hash: McBlockHash,
		reference_timestamp: NaiveDateTime,
	) -> Result<Block, StableBlockByHashError> {
		let latest_block = db_model::get_latest_block_info(&self.pool)
			.await
			.map_err(|err| StableBlockByHashError::Database(format!("{err:?}")))?;
		let Some(latest_block) = latest_block else {
			return Err(StableBlockByHashError::LatestBlockUnavailable(hash));
		};
		self.observe_latest_cardano_block_metrics(&latest_block);

		let block = db_model::get_block_by_hash(&self.pool, hash.clone())
			.await
			.map_err(|err| StableBlockByHashError::Database(format!("{err:?}")))?;
		let Some(block) = block else {
			return Err(StableBlockByHashError::BlockNotFound(hash));
		};
		self.observe_referenced_cardano_block_metrics(&block);

		let required_latest_block_no = block.block_no.saturating_add(self.security_parameter);
		let is_stable = required_latest_block_no <= latest_block.block_no;
		let min_allowed_time = self.min_block_allowed_time(reference_timestamp);
		let max_allowed_time = self.max_allowed_block_time(reference_timestamp);
		let is_time_valid = self.is_stable_block_time_valid(&block, reference_timestamp);

		// Check for timestamps relation first, because TimestampOutOfRange is final error, but NotStableYet is transient one.
		if !is_time_valid {
			return Err(StableBlockByHashError::TimestampOutOfRange {
				hash,
				block_time: block.time,
				min_allowed_time,
				max_allowed_time,
				reference_timestamp,
				block,
			});
		}
		if !is_stable {
			return Err(StableBlockByHashError::NotStableYet {
				hash,
				block_no: block.block_no.0,
				required_latest_block_no: required_latest_block_no.0,
				latest_block_no: latest_block.block_no.0,
				block,
			});
		}

		Ok(block)
	}

	/// Caches stable blocks for lookup by hash.
	async fn fill_cache(
		&self,
		from_block: &Block,
	) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		let from_block_no = from_block.block_no;
		let size = u32::from(self.cache_size);
		let latest_block =
			db_model::get_latest_block_info(&self.pool)
				.await?
				.ok_or(InternalDataSourceError(
					"No latest block when filling the caches.".to_string(),
				))?;
		let stable_block_num = latest_block.block_no.saturating_sub(self.security_parameter);

		let to_block_no = from_block_no.saturating_add(size).min(stable_block_num);
		let blocks = if to_block_no > from_block_no {
			db_model::get_blocks_by_numbers(&self.pool, from_block_no, to_block_no).await?
		} else {
			vec![from_block.clone()]
		};

		if let Ok(mut cache) = self.stable_blocks_cache.lock() {
			cache.update(blocks);
			debug!("Cached blocks {} to {} for by hash lookups.", from_block_no.0, to_block_no.0);
		}
		Ok(())
	}

	fn date_time_to_slot(
		&self,
		dt: NaiveDateTime,
	) -> Result<SlotNumber, Box<dyn std::error::Error + Send + Sync>> {
		let millis: u64 = dt
			.and_utc()
			.timestamp_millis()
			.try_into()
			.map_err(|_| BadRequest(format!("Datetime out of range: {dt:?}")))?;
		let ts = sidechain_domain::mainchain_epoch::Timestamp::from_unix_millis(millis);
		let slot = self
			.mainchain_epoch_config
			.timestamp_to_mainchain_slot_number(ts)
			.unwrap_or(self.mainchain_epoch_config.first_slot_number);
		Ok(SlotNumber(slot))
	}

	fn timestamp_to_db_type(
		timestamp: Timestamp,
	) -> Result<NaiveDateTime, Box<dyn std::error::Error + Send + Sync>> {
		let millis: Option<i64> = timestamp.as_millis().try_into().ok();
		let dt = millis
			.and_then(DateTime::from_timestamp_millis)
			.ok_or(BadRequest(format!("Timestamp out of range: {timestamp:?}")))?;
		Ok(NaiveDateTime::new(dt.date_naive(), dt.time()))
	}
}

/// Helper structure for caching stable blocks.
#[derive(new)]
pub(crate) struct BlocksCache {
	/// Continuous main chain blocks. All blocks should be stable. Used to query by hash.
	#[new(default)]
	from_last_by_hash: Vec<Block>,
}

impl BlocksCache {
	fn find_by_hash(&self, hash: McBlockHash) -> Option<Block> {
		self.from_last_by_hash.iter().find(|b| b.hash == hash.0).cloned()
	}

	pub fn update(&mut self, from_last_by_hash: Vec<Block>) {
		self.from_last_by_hash = from_last_by_hash;
	}

	pub fn new_arc_mutex() -> Arc<Mutex<Self>> {
		Arc::new(Mutex::new(Self::new()))
	}
}
