use crate::grpc::handle::IndexerHandle;
use crate::grpc::{
	midnight_state::midnight_state_client::MidnightStateClient,
	requests::{
		mc_hash_data_source_acropolis::{
			get_block_by_hash, get_latest_stable_block, get_stable_block,
		},
		sidechain_rpc_data_source_acropolis::get_latest_block,
	},
};
use midnight_primitives_mainchain_follower::partner_chains_db_sync_data_sources::DbSyncBlockDataSourceConfig;
use sidechain_domain::{MainchainBlock, McBlockHash};
use sidechain_mc_hash::{McHashDataSource, StableBlockByHashResult};
use sp_timestamp::Timestamp;
use tonic::{async_trait, transport::Endpoint};

/// `McHashDataSource` served by the Midnight indexer
/// (remote gRPC or in-process direct calls; see `IndexerHandle`).
pub struct McHashDataSourceGrpcImpl {
	api: IndexerHandle,
	/// Cardano security parameter
	///
	/// This parameter controls how many confirmations (blocks on top) are required by
	/// the Cardano node to consider a block to be stable. This is a network-wide parameter.
	security_parameter: u32,
	/// Additional offset applied when selecting the latest stable Cardano block
	///
	/// This parameter should be 0 by default and should only be increased to 1 in networks
	/// struggling with frequent block rejections due to Db-Sync or Cardano node lag.
	block_stability_margin: u32,
	/// `security_parameter / active_slots_coeff` (`k/f`) in seconds — the
	/// minimum age for a block to be stable relative to a reference timestamp.
	min_slot_boundary_secs: u64,
	/// `3k/f` in seconds — the maximum age for a block to still be usable
	/// relative to a reference timestamp.
	max_slot_boundary_secs: u64,
	/// Freshness heuristic bound, mirroring the db-sync backend:
	/// `max(block_stability_margin, 1) * expected_block_interval`.
	max_latest_block_age_secs: u64,
}

impl McHashDataSourceGrpcImpl {
	/// Connect to the indexer at `endpoint` (an `http://…` URL).
	pub async fn connect(
		endpoint: impl AsRef<str>,
		config: DbSyncBlockDataSourceConfig,
	) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
		let endpoint_str = endpoint.as_ref();

		let endpoint = Endpoint::from_shared(endpoint_str.to_string())
			.map_err(|e| format!("Invalid gRPC endpoint `{}`: {}", endpoint_str, e))?
			.tcp_nodelay(true)
			.http2_keep_alive_interval(std::time::Duration::from_secs(30))
			.keep_alive_while_idle(true);

		let channel = endpoint.connect().await.map_err(|e| {
			format!("Failed to connect to gRPC server at `{}`: {}", endpoint_str, e)
		})?;

		Ok(Self::from_api(IndexerHandle::Grpc(MidnightStateClient::new(channel)), config))
	}

	/// Build over the in-process indexer service (no transport, no codec).
	#[cfg(feature = "embedded")]
	pub fn direct(
		service: acropolis_module_midnight_state::grpc::service::MidnightStateService,
		config: DbSyncBlockDataSourceConfig,
	) -> Self {
		Self::from_api(IndexerHandle::Direct(service), config)
	}

	/// Build over an already-established indexer handle.
	pub fn from_api(api: IndexerHandle, config: DbSyncBlockDataSourceConfig) -> Self {
		// Cardano slots are fixed at one second (enforced by the mainchain
		// epoch invariants), so slot boundaries in slots equal seconds.
		let k = f64::from(config.cardano_security_parameter);
		let f = config.cardano_active_slots_coeff;
		let min_slot_boundary_secs = (k / f).round() as u64;
		let expected_block_interval_secs = (1.0 / f).round() as u64;
		let max_latest_block_age_secs =
			u64::from(config.block_stability_margin.max(1)) * expected_block_interval_secs;

		Self {
			api,
			security_parameter: config.cardano_security_parameter,
			block_stability_margin: config.block_stability_margin,
			min_slot_boundary_secs,
			max_slot_boundary_secs: 3 * min_slot_boundary_secs,
			max_latest_block_age_secs,
		}
	}

	/// Allowable range check mirroring db-sync: block timestamp within
	/// `[reference - 3k/f, reference - k/f]`.
	fn timestamp_in_allowable_range(&self, reference: Timestamp, block_ts_secs: u64) -> bool {
		let reference_secs = reference.as_millis() / 1000;
		let min = reference_secs.saturating_sub(self.max_slot_boundary_secs);
		let max = reference_secs.saturating_sub(self.min_slot_boundary_secs);
		(min..=max).contains(&block_ts_secs)
	}

	fn now_secs() -> u64 {
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.map(|d| d.as_secs())
			.unwrap_or_default()
	}
}

#[async_trait]
impl McHashDataSource for McHashDataSourceGrpcImpl {
	async fn get_latest_stable_block_for(
		&self,
		as_of_timestamp: Timestamp,
	) -> Result<Option<MainchainBlock>, Box<dyn std::error::Error + Send + Sync>> {
		let stability_offset = self.security_parameter + self.block_stability_margin;
		get_latest_stable_block(&self.api, stability_offset, as_of_timestamp.as_millis())
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
	}

	async fn get_stable_block_for(
		&self,
		hash: McBlockHash,
		as_of_timestamp: Timestamp,
	) -> Result<StableBlockByHashResult, Box<dyn std::error::Error + Send + Sync>> {
		let stability_offset = self.security_parameter + self.block_stability_margin;
		if let Some(info) =
			get_stable_block(&self.api, hash.clone(), stability_offset, as_of_timestamp.as_millis())
				.await
				.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
		{
			return Ok(StableBlockByHashResult::BlockStable { info });
		}

		// The indexer only answers whether the block is stable; reconstruct
		// the db-sync classification of *why* it is not.
		match get_block_by_hash(&self.api, hash)
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
		{
			None => Ok(StableBlockByHashResult::BlockNotFound),
			Some(info) if self.timestamp_in_allowable_range(as_of_timestamp, info.timestamp) => {
				Ok(StableBlockByHashResult::NotEnoughConfirmations { info })
			},
			Some(info) => Ok(StableBlockByHashResult::BlockTimestampOutRange { info }),
		}
	}

	async fn get_block_by_hash(
		&self,
		hash: McBlockHash,
	) -> Result<Option<MainchainBlock>, Box<dyn std::error::Error + Send + Sync>> {
		get_block_by_hash(&self.api, hash)
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
	}

	async fn is_cardano_tip_fresh(&self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
		let latest = get_latest_block(&self.api)
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
		let age_secs = Self::now_secs().saturating_sub(latest.timestamp);
		Ok(age_secs < self.max_latest_block_age_secs)
	}

	async fn is_cardano_ok(&self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
		// Praos chain quality rule: at least one block in the last `k/f` slots.
		let latest = get_latest_block(&self.api)
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
		let now = Self::now_secs();
		if now.saturating_sub(latest.timestamp) > self.min_slot_boundary_secs {
			return Ok(false);
		}
		// Praos chain growth rule: a stable block (>= k confirmations) exists
		// with a timestamp in the allowable range relative to now.
		let stable = get_latest_stable_block(&self.api, self.security_parameter, now * 1000)
			.await
			.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
		Ok(stable.is_some())
	}
}
