use std::{env, error::Error, sync::Arc, time::Duration};

use authority_selection_inherents::AuthoritySelectionDataSource;
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use midnight_node_data_sources::{
	AuthoritySelectionDataSourceGrpcImpl, FederatedAuthorityObservationGrpcImpl,
	McHashDataSourceGrpcImpl, MidnightCNightObservationGrpcImpl, SidechainRpcDataSourceGrpcImpl,
};
use midnight_primitives_cnight_observation::{CardanoPosition, TimestampUnixMillis};
use midnight_primitives_mainchain_follower::{
	CandidatesDataSourceImpl, FederatedAuthorityObservationDataSource,
	FederatedAuthorityObservationDataSourceImpl, MidnightCNightObservationDataSource,
	MidnightCNightObservationDataSourceImpl,
	partner_chains_db_sync_data_sources::{
		BlockDataSourceImpl, McHashDataSourceImpl, SidechainRpcDataSourceImpl,
	},
};
use pallet_sidechain_rpc::SidechainRpcDataSource;
use sidechain_domain::{McBlockHash, McEpochNumber};
use sidechain_mc_hash::McHashDataSource;
use sp_timestamp::Timestamp;
use tokio::runtime::Runtime;

#[path = "../src/tests/common.rs"]
mod common;
#[path = "../src/tests/configuration.rs"]
mod configuration;

use common::{CNIGHT_OBSERVATION_POOL_CFG, STANDARD_POOL_CFG, get_connection};
use configuration::IntegrationTestConfig;

/// Generous over-fetch bound for parity tests; only affects the db-sync
/// side's SQL row limit, the indexer ignores it.
const UTXO_OVERESTIMATE: usize = 10_000;

type BenchResult<T> = Result<T, Box<dyn Error + Send + Sync + 'static>>;

const BENCH_SAMPLE_SIZE: usize = 30;
const BENCH_MEASUREMENT_TIME: Duration = Duration::from_secs(40);
const BENCH_MC_HASH_MEASUREMENT_TIME: Duration = Duration::from_secs(150);

struct BenchmarkContext {
	config: IntegrationTestConfig,
	cnight_grpc: Arc<MidnightCNightObservationGrpcImpl>,
	cnight_db_sync: Arc<MidnightCNightObservationDataSourceImpl>,
	authority_grpc: Arc<AuthoritySelectionDataSourceGrpcImpl>,
	authority_db_sync: Arc<CandidatesDataSourceImpl>,
	federated_grpc: Arc<FederatedAuthorityObservationGrpcImpl>,
	federated_db_sync: Arc<FederatedAuthorityObservationDataSourceImpl>,
	mc_hash_grpc: Arc<McHashDataSourceGrpcImpl>,
	mc_hash_db_sync: Arc<McHashDataSourceImpl>,
	sidechain_rpc_grpc: Arc<SidechainRpcDataSourceGrpcImpl>,
	sidechain_rpc_db_sync: Arc<SidechainRpcDataSourceImpl>,
}

impl BenchmarkContext {
	async fn load() -> BenchResult<Self> {
		let mut config = IntegrationTestConfig::from_env()?;
		apply_bench_env_overrides(&mut config);

		let cnight_db_sync =
			Arc::new(create_dbsync_cnight_observation_source(&config.postgres_uri).await?);
		let authority_db_sync = Arc::new(create_dbsync_authority_selection_source(&config).await?);
		let federated_db_sync =
			Arc::new(create_dbsync_federated_authority_source(&config.postgres_uri).await?);
		let block_data_source = Arc::new(create_block_data_source(&config).await?);
		let mc_hash_db_sync = Arc::new(McHashDataSourceImpl::new(block_data_source.clone(), None));
		let sidechain_rpc_db_sync =
			Arc::new(SidechainRpcDataSourceImpl::new(block_data_source, None));

		let cnight_grpc =
			Arc::new(MidnightCNightObservationGrpcImpl::connect(&config.grpc_endpoint).await?);
		let authority_grpc =
			Arc::new(AuthoritySelectionDataSourceGrpcImpl::connect(&config.grpc_endpoint).await?);
		let federated_grpc =
			Arc::new(FederatedAuthorityObservationGrpcImpl::connect(&config.grpc_endpoint).await?);
		let mc_hash_grpc = Arc::new(
			McHashDataSourceGrpcImpl::connect(
				&config.grpc_endpoint,
				config.block_source_config.clone(),
			)
			.await?,
		);
		let sidechain_rpc_grpc =
			Arc::new(SidechainRpcDataSourceGrpcImpl::connect(&config.grpc_endpoint).await?);

		Ok(Self {
			config,
			cnight_grpc,
			cnight_db_sync,
			authority_grpc,
			authority_db_sync,
			federated_grpc,
			federated_db_sync,
			mc_hash_grpc,
			mc_hash_db_sync,
			sidechain_rpc_grpc,
			sidechain_rpc_db_sync,
		})
	}
}

fn bench_datasources(c: &mut Criterion) {
	let rt = Runtime::new().unwrap();
	let ctx = rt.block_on(BenchmarkContext::load()).expect("benchmark context init failed");

	bench_cnight_observation(c, &rt, &ctx);
	bench_authority_selection(c, &rt, &ctx);
	bench_federated_authority(c, &rt, &ctx);
	bench_mc_hash(c, &rt, &ctx);
	bench_sidechain_rpc(c, &rt, &ctx);
}

fn bench_cnight_observation(c: &mut Criterion, rt: &Runtime, ctx: &BenchmarkContext) {
	let cnight_config = ctx.config.cnight_config.clone();
	let start_position = start_position();
	let tip = ctx.config.params_config.tip.clone();
	let capacity = ctx.config.params_config.tx_capacity;

	let mut group = c.benchmark_group("cnight_observation/get_utxos_up_to_capacity");
	group.sample_size(BENCH_SAMPLE_SIZE);
	group.measurement_time(BENCH_MEASUREMENT_TIME);

	group.bench_function("grpc", |b| {
		let grpc = Arc::clone(&ctx.cnight_grpc);
		let cnight_config = cnight_config.clone();
		let start_position = start_position.clone();
		let tip = tip.clone();

		b.to_async(rt).iter(|| {
			let grpc = Arc::clone(&grpc);
			let cnight_config = cnight_config.clone();
			let start_position = start_position.clone();
			let tip = tip.clone();

			async move {
				let utxos = grpc
					.get_utxos_up_to_capacity(
						&cnight_config,
						&start_position,
						tip,
						capacity,
						UTXO_OVERESTIMATE,
					)
					.await
					.unwrap();
				black_box(utxos);
			}
		});
	});

	group.bench_function("dbsync", |b| {
		let db_sync = Arc::clone(&ctx.cnight_db_sync);
		let cnight_config = cnight_config.clone();
		let start_position = start_position.clone();
		let tip = tip.clone();

		b.to_async(rt).iter(|| {
			let db_sync = Arc::clone(&db_sync);
			let cnight_config = cnight_config.clone();
			let start_position = start_position.clone();
			let tip = tip.clone();

			async move {
				let utxos = db_sync
					.get_utxos_up_to_capacity(
						&cnight_config,
						&start_position,
						tip,
						capacity,
						UTXO_OVERESTIMATE,
					)
					.await
					.unwrap();
				black_box(utxos);
			}
		});
	});

	group.finish();
}

fn bench_authority_selection(c: &mut Criterion, rt: &Runtime, ctx: &BenchmarkContext) {
	let epoch_number = ctx.config.params_config.epoch_number;
	let d_parameter_policy_id = ctx.config.d_parameter_policy_id.clone();
	let permissioned_candidates_policy = ctx.config.permissioned_candidates_policy.clone();
	let committee_candidate_address = ctx.config.committee_candidate_address.clone();

	let mut ariadne_group = c.benchmark_group("authority_selection/get_ariadne_parameters");
	ariadne_group.sample_size(BENCH_SAMPLE_SIZE);
	ariadne_group.measurement_time(BENCH_MEASUREMENT_TIME);

	ariadne_group.bench_function("grpc", |b| {
		let grpc = Arc::clone(&ctx.authority_grpc);
		let d_parameter_policy_id = d_parameter_policy_id.clone();
		let permissioned_candidates_policy = permissioned_candidates_policy.clone();

		b.to_async(rt).iter(|| {
			let grpc = Arc::clone(&grpc);
			let d_parameter_policy_id = d_parameter_policy_id.clone();
			let permissioned_candidates_policy = permissioned_candidates_policy.clone();

			async move {
				let ariadne = grpc
					.get_ariadne_parameters(
						epoch_number,
						d_parameter_policy_id,
						permissioned_candidates_policy,
					)
					.await
					.unwrap();
				black_box(ariadne);
			}
		});
	});

	ariadne_group.bench_function("dbsync", |b| {
		let db_sync = Arc::clone(&ctx.authority_db_sync);
		let d_parameter_policy_id = d_parameter_policy_id.clone();
		let permissioned_candidates_policy = permissioned_candidates_policy.clone();

		b.to_async(rt).iter(|| {
			let db_sync = Arc::clone(&db_sync);
			let d_parameter_policy_id = d_parameter_policy_id.clone();
			let permissioned_candidates_policy = permissioned_candidates_policy.clone();

			async move {
				let ariadne = db_sync
					.get_ariadne_parameters(
						epoch_number,
						d_parameter_policy_id,
						permissioned_candidates_policy,
					)
					.await
					.unwrap();
				black_box(ariadne);
			}
		});
	});

	ariadne_group.finish();

	let mut candidates_group = c.benchmark_group("authority_selection/get_candidates");
	candidates_group.sample_size(BENCH_SAMPLE_SIZE);
	candidates_group.measurement_time(BENCH_MEASUREMENT_TIME);

	candidates_group.bench_function("grpc", |b| {
		let grpc = Arc::clone(&ctx.authority_grpc);
		let committee_candidate_address = committee_candidate_address.clone();

		b.to_async(rt).iter(|| {
			let grpc = Arc::clone(&grpc);
			let committee_candidate_address = committee_candidate_address.clone();

			async move {
				let candidates =
					grpc.get_candidates(epoch_number, committee_candidate_address).await.unwrap();
				black_box(candidates);
			}
		});
	});

	candidates_group.bench_function("dbsync", |b| {
		let db_sync = Arc::clone(&ctx.authority_db_sync);
		let committee_candidate_address = committee_candidate_address.clone();

		b.to_async(rt).iter(|| {
			let db_sync = Arc::clone(&db_sync);
			let committee_candidate_address = committee_candidate_address.clone();

			async move {
				let candidates = db_sync
					.get_candidates(epoch_number, committee_candidate_address)
					.await
					.unwrap();
				black_box(candidates);
			}
		});
	});

	candidates_group.finish();

	let mut nonce_group = c.benchmark_group("authority_selection/get_epoch_nonce");
	nonce_group.sample_size(BENCH_SAMPLE_SIZE);
	nonce_group.measurement_time(BENCH_MEASUREMENT_TIME);

	nonce_group.bench_function("grpc", |b| {
		let grpc = Arc::clone(&ctx.authority_grpc);

		b.to_async(rt).iter(|| {
			let grpc = Arc::clone(&grpc);

			async move {
				let nonce = grpc.get_epoch_nonce(epoch_number).await.unwrap();
				black_box(nonce);
			}
		});
	});

	nonce_group.bench_function("dbsync", |b| {
		let db_sync = Arc::clone(&ctx.authority_db_sync);

		b.to_async(rt).iter(|| {
			let db_sync = Arc::clone(&db_sync);

			async move {
				let nonce = db_sync.get_epoch_nonce(epoch_number).await.unwrap();
				black_box(nonce);
			}
		});
	});

	nonce_group.finish();

	let mut data_epoch_group = c.benchmark_group("authority_selection/data_epoch");
	data_epoch_group.sample_size(BENCH_SAMPLE_SIZE);
	data_epoch_group.measurement_time(BENCH_MEASUREMENT_TIME);

	data_epoch_group.bench_function("grpc", |b| {
		let grpc = Arc::clone(&ctx.authority_grpc);

		b.to_async(rt).iter(|| {
			let grpc = Arc::clone(&grpc);

			async move {
				let data_epoch = grpc.data_epoch(epoch_number).await.unwrap();
				black_box(data_epoch);
			}
		});
	});

	data_epoch_group.bench_function("dbsync", |b| {
		let db_sync = Arc::clone(&ctx.authority_db_sync);

		b.to_async(rt).iter(|| {
			let db_sync = Arc::clone(&db_sync);

			async move {
				let data_epoch = db_sync.data_epoch(epoch_number).await.unwrap();
				black_box(data_epoch);
			}
		});
	});

	data_epoch_group.finish();
}

fn bench_federated_authority(c: &mut Criterion, rt: &Runtime, ctx: &BenchmarkContext) {
	let authority_config = ctx.config.authority_config.clone();
	let tip = ctx.config.params_config.tip.clone();

	let mut group = c.benchmark_group("federated_authority/get_federated_authority_data");
	group.sample_size(BENCH_SAMPLE_SIZE);
	group.measurement_time(BENCH_MEASUREMENT_TIME);

	group.bench_function("grpc", |b| {
		let grpc = Arc::clone(&ctx.federated_grpc);
		let authority_config = authority_config.clone();
		let tip = tip.clone();

		b.to_async(rt).iter(|| {
			let grpc = Arc::clone(&grpc);
			let authority_config = authority_config.clone();
			let tip = tip.clone();

			async move {
				let authorities =
					grpc.get_federated_authority_data(&authority_config, &tip).await.unwrap();
				black_box(authorities);
			}
		});
	});

	group.bench_function("dbsync", |b| {
		let db_sync = Arc::clone(&ctx.federated_db_sync);
		let authority_config = authority_config.clone();
		let tip = tip.clone();

		b.to_async(rt).iter(|| {
			let db_sync = Arc::clone(&db_sync);
			let authority_config = authority_config.clone();
			let tip = tip.clone();

			async move {
				let authorities =
					db_sync.get_federated_authority_data(&authority_config, &tip).await.unwrap();
				black_box(authorities);
			}
		});
	});

	group.finish();
}

fn bench_mc_hash(c: &mut Criterion, rt: &Runtime, ctx: &BenchmarkContext) {
	let timestamp = ctx.config.params_config.timestamp;
	let tip = ctx.config.params_config.tip.clone();

	let mut latest_stable_group = c.benchmark_group("mc_hash/get_latest_stable_block_for");
	latest_stable_group.sample_size(BENCH_SAMPLE_SIZE);
	latest_stable_group.measurement_time(BENCH_MC_HASH_MEASUREMENT_TIME);

	latest_stable_group.bench_function("grpc", |b| {
		let grpc = Arc::clone(&ctx.mc_hash_grpc);

		b.to_async(rt).iter(|| {
			let grpc = Arc::clone(&grpc);

			async move {
				let block = grpc.get_latest_stable_block_for(timestamp).await.unwrap();
				black_box(block);
			}
		});
	});

	latest_stable_group.bench_function("dbsync", |b| {
		let db_sync = Arc::clone(&ctx.mc_hash_db_sync);

		b.to_async(rt).iter(|| {
			let db_sync = Arc::clone(&db_sync);

			async move {
				let block = db_sync.get_latest_stable_block_for(timestamp).await.unwrap();
				black_box(block);
			}
		});
	});

	latest_stable_group.finish();

	let mut stable_block_group = c.benchmark_group("mc_hash/get_stable_block_for");
	stable_block_group.sample_size(BENCH_SAMPLE_SIZE);
	stable_block_group.measurement_time(BENCH_MC_HASH_MEASUREMENT_TIME);

	stable_block_group.bench_function("grpc", |b| {
		let grpc = Arc::clone(&ctx.mc_hash_grpc);
		let tip = tip.clone();

		b.to_async(rt).iter(|| {
			let grpc = Arc::clone(&grpc);
			let tip = tip.clone();

			async move {
				let block = grpc.get_stable_block_for(tip, timestamp).await.unwrap();
				black_box(block);
			}
		});
	});

	stable_block_group.bench_function("dbsync", |b| {
		let db_sync = Arc::clone(&ctx.mc_hash_db_sync);
		let tip = tip.clone();

		b.to_async(rt).iter(|| {
			let db_sync = Arc::clone(&db_sync);
			let tip = tip.clone();

			async move {
				let block = db_sync.get_stable_block_for(tip, timestamp).await.unwrap();
				black_box(block);
			}
		});
	});

	stable_block_group.finish();

	let mut block_by_hash_group = c.benchmark_group("mc_hash/get_block_by_hash");
	block_by_hash_group.sample_size(BENCH_SAMPLE_SIZE);
	block_by_hash_group.measurement_time(BENCH_MEASUREMENT_TIME);

	block_by_hash_group.bench_function("grpc", |b| {
		let grpc = Arc::clone(&ctx.mc_hash_grpc);
		let tip = tip.clone();

		b.to_async(rt).iter(|| {
			let grpc = Arc::clone(&grpc);
			let tip = tip.clone();

			async move {
				let block = grpc.get_block_by_hash(tip).await.unwrap();
				black_box(block);
			}
		});
	});

	block_by_hash_group.bench_function("dbsync", |b| {
		let db_sync = Arc::clone(&ctx.mc_hash_db_sync);
		let tip = tip.clone();

		b.to_async(rt).iter(|| {
			let db_sync = Arc::clone(&db_sync);
			let tip = tip.clone();

			async move {
				let block = db_sync.get_block_by_hash(tip).await.unwrap();
				black_box(block);
			}
		});
	});

	block_by_hash_group.finish();
}

fn bench_sidechain_rpc(c: &mut Criterion, rt: &Runtime, ctx: &BenchmarkContext) {
	let mut group = c.benchmark_group("sidechain_rpc/get_latest_block_info");
	group.sample_size(BENCH_SAMPLE_SIZE);
	group.measurement_time(BENCH_MEASUREMENT_TIME);

	group.bench_function("grpc", |b| {
		let grpc = Arc::clone(&ctx.sidechain_rpc_grpc);

		b.to_async(rt).iter(|| {
			let grpc = Arc::clone(&grpc);

			async move {
				let block = grpc.get_latest_block_info().await.unwrap();
				black_box(block);
			}
		});
	});

	group.bench_function("dbsync", |b| {
		let db_sync = Arc::clone(&ctx.sidechain_rpc_db_sync);

		b.to_async(rt).iter(|| {
			let db_sync = Arc::clone(&db_sync);

			async move {
				let block = db_sync.get_latest_block_info().await.unwrap();
				black_box(block);
			}
		});
	});

	group.finish();
}

criterion_group!(benches, bench_datasources);
criterion_main!(benches);

fn start_position() -> CardanoPosition {
	CardanoPosition {
		block_hash: McBlockHash([0; 32]),
		block_number: 0,
		block_timestamp: TimestampUnixMillis(0),
		tx_index_in_block: 0,
	}
}

fn apply_bench_env_overrides(config: &mut IntegrationTestConfig) {
	if let Some(postgres_uri) =
		bench_env("BENCH_POSTGRES_URI").or_else(|| bench_env("CNIGHT_TEST_POSTGRES_URI"))
	{
		config.postgres_uri = postgres_uri;
	}

	if let Some(grpc_endpoint) = bench_env("BENCH_GRPC_ENDPOINT") {
		config.grpc_endpoint = grpc_endpoint;
	}

	if let Some(epoch_number) = bench_env_parse("BENCH_EPOCH_NUMBER") {
		config.params_config.epoch_number = McEpochNumber(epoch_number);
	}

	if let Some(tx_capacity) = bench_env_parse("BENCH_TX_CAPACITY") {
		config.params_config.tx_capacity = tx_capacity;
	}

	if let Some(timestamp) = bench_env_parse("BENCH_TIMESTAMP") {
		config.params_config.timestamp = Timestamp::new(normalize_unix_timestamp(timestamp));
	}

	if let Some(tip_hash) = bench_env("BENCH_TIP_HASH") {
		config.params_config.tip = parse_mc_block_hash(&tip_hash);
	}
}

fn bench_env(var: &str) -> Option<String> {
	env::var(var).ok().filter(|value| !value.is_empty())
}

fn bench_env_parse<T>(var: &str) -> Option<T>
where
	T: std::str::FromStr,
	<T as std::str::FromStr>::Err: std::fmt::Display,
{
	bench_env(var).map(|value| {
		value.parse::<T>().unwrap_or_else(|err| panic!("invalid {var}={value}: {err}"))
	})
}

fn parse_mc_block_hash(value: &str) -> McBlockHash {
	let bytes =
		hex::decode(value).unwrap_or_else(|err| panic!("invalid BENCH_TIP_HASH hex: {err}"));
	let len = bytes.len();
	let hash = bytes
		.try_into()
		.unwrap_or_else(|_| panic!("invalid BENCH_TIP_HASH length: expected 32 bytes, got {len}"));
	McBlockHash(hash)
}

fn normalize_unix_timestamp(timestamp: u64) -> u64 {
	const UNIX_MILLIS_THRESHOLD: u64 = 1_000_000_000_000;

	if timestamp < UNIX_MILLIS_THRESHOLD { timestamp * 1000 } else { timestamp }
}

async fn create_dbsync_cnight_observation_source(
	connection_string: &str,
) -> BenchResult<MidnightCNightObservationDataSourceImpl> {
	Ok(MidnightCNightObservationDataSourceImpl::new(
		get_connection(connection_string, CNIGHT_OBSERVATION_POOL_CFG, true).await?,
		None,
		1000,
	))
}

async fn create_dbsync_authority_selection_source(
	config: &IntegrationTestConfig,
) -> BenchResult<CandidatesDataSourceImpl> {
	CandidatesDataSourceImpl::new(
		get_connection(&config.postgres_uri, STANDARD_POOL_CFG, true).await?,
		None,
	)
	.await
}

async fn create_dbsync_federated_authority_source(
	connection_string: &str,
) -> BenchResult<FederatedAuthorityObservationDataSourceImpl> {
	Ok(FederatedAuthorityObservationDataSourceImpl::new(
		get_connection(connection_string, STANDARD_POOL_CFG, true).await?,
		None,
		1000,
	))
}

async fn create_block_data_source(
	config: &IntegrationTestConfig,
) -> BenchResult<BlockDataSourceImpl> {
	Ok(BlockDataSourceImpl::from_config(
		get_connection(&config.postgres_uri, STANDARD_POOL_CFG, true).await?,
		config.block_source_config.clone(),
		&config.epoch_config,
	))
}
