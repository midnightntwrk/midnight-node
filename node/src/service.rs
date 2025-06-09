// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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

//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

use crate::{
	extensions::ExtensionsFactory,
	inherent_data::{CreateInherentDataConfig, ProposalCIDP, VerifierCIDP},
	main_chain_follower::DataSources,
	rpc::GrandpaDeps,
};
use db_sync_follower::metrics::McFollowerMetrics;
use db_sync_follower::metrics::register_metrics_warn_errors;
use futures::FutureExt;
use midnight_node_runtime::storage::child::StateVersion;
use midnight_node_runtime::{self, RuntimeApi, opaque::Block};

use midnight_node_runtime::WASM_BINARY;
use midnight_primitives_ledger::LedgerMetrics;
use midnight_primitives_upgrade::UpgradeProposal;
use midnight_primitives_upgrade_api::UpgradeApi;
use parity_scale_codec::{Decode, Encode};
use sc_client_api::{Backend, BlockBackend, BlockImportOperation, ExecutorProvider};
use sc_consensus_aura::{ImportQueueParams, SlotProportion, StartAuraParams};
use sc_consensus_grandpa::SharedVoterState;
use sc_consensus_slots::BackoffAuthoringOnFinalizedHeadLagging;
use sc_executor::RuntimeVersionOf;
use sc_partner_chains_consensus_aura::import_queue as partner_chains_aura_import_queue;
use sc_service::{
	BuildGenesisBlock, Configuration, TaskManager, WarpSyncConfig, error::Error as ServiceError,
	resolve_state_version_from_wasm,
};
use sc_telemetry::{Telemetry, TelemetryWorker};
use sc_transaction_pool_api::OffchainTransactionPoolFactory;
use sidechain_domain::mainchain_epoch::MainchainEpochConfig;
use sidechain_mc_hash::McHashInherentDigest;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_consensus_aura::sr25519::AuthorityPair as AuraPair;
use sp_core::storage::Storage;
use sp_partner_chains_consensus_aura::block_proposal::PartnerChainsProposerFactory;
use sp_runtime::{
	BuildStorage,
	traits::{Block as BlockT, Hash as HashT, HashingFor, Header as HeaderT, Zero},
};
#[cfg(feature = "experimental")]
use sp_runtime::{Digest, DigestItem};
use std::{
	fs,
	marker::PhantomData,
	sync::{Arc, Mutex},
	time::Duration,
};
use time_source::SystemTimeSource;

pub struct StorageConfig {
	pub genesis_tx: Vec<u8>,
	pub cache_size: usize,
}

impl StorageConfig {
	fn genesis_tx_without_network_id(&self) -> &[u8] {
		&self.genesis_tx[1..]
	}
}

pub struct GenesisBlockBuilder<Block: BlockT, B, E> {
	genesis_storage: Storage,
	commit_genesis_state: bool,
	backend: Arc<B>,
	executor: E,
	genesis_tx: Vec<u8>,
	_phantom: PhantomData<Block>,
}

impl<Block: BlockT, B: Backend<Block>, E: RuntimeVersionOf> GenesisBlockBuilder<Block, B, E> {
	/// Constructs a new instance of [`GenesisBlockBuilder`].
	pub fn new(
		build_genesis_storage: &dyn BuildStorage,
		commit_genesis_state: bool,
		backend: Arc<B>,
		executor: E,
		genesis_tx: Vec<u8>,
	) -> sp_blockchain::Result<Self> {
		let genesis_storage =
			build_genesis_storage.build_storage().map_err(sp_blockchain::Error::Storage)?;
		Ok(Self {
			genesis_storage,
			commit_genesis_state,
			backend,
			executor,
			genesis_tx,
			_phantom: PhantomData::<Block>,
		})
	}
}

impl<Block: BlockT, B: Backend<Block>, E: RuntimeVersionOf> BuildGenesisBlock<Block>
	for GenesisBlockBuilder<Block, B, E>
{
	type BlockImportOperation = <B as Backend<Block>>::BlockImportOperation;

	fn build_genesis_block(self) -> sp_blockchain::Result<(Block, Self::BlockImportOperation)> {
		let Self { genesis_storage, commit_genesis_state, backend, executor, genesis_tx, _phantom } =
			self;

		let mut extrinsics = Vec::new();
		if let Ok(extrinsic) = <<Block as BlockT>::Extrinsic>::decode(&mut &genesis_tx[..]) {
			extrinsics.push(extrinsic);
		}

		let genesis_state_version =
			resolve_state_version_from_wasm::<_, HashingFor<Block>>(&genesis_storage, &executor)?;
		let mut op = backend.begin_operation()?;
		let state_root =
			op.set_genesis_state(genesis_storage, commit_genesis_state, genesis_state_version)?;
		let genesis_block =
			construct_genesis_block::<Block>(state_root, genesis_state_version, extrinsics);

		Ok((genesis_block, op))
	}
}

/// Construct genesis block.
pub fn construct_genesis_block<Block: BlockT>(
	state_root: Block::Hash,
	state_version: StateVersion,
	extrinsics: Vec<<Block as BlockT>::Extrinsic>,
) -> Block {
	let extrinsics_root =
		<<<Block as BlockT>::Header as HeaderT>::Hashing as HashT>::ordered_trie_root(
			extrinsics.iter().map(Encode::encode).collect(),
			state_version,
		);

	#[cfg(feature = "experimental")]
	let block_digest = Digest {
		logs: vec![DigestItem::Consensus(
			midnight_node_runtime::VERSION_ID,
			midnight_node_runtime::VERSION.spec_version.encode(),
		)],
	};

	#[cfg(not(feature = "experimental"))]
	let block_digest = Default::default();

	Block::new(
		<<Block as BlockT>::Header as HeaderT>::new(
			Zero::zero(),
			extrinsics_root,
			state_root,
			Default::default(),
			block_digest,
		),
		extrinsics,
	)
}

/// Partial service components specific to Midnight
pub struct MidnightPartialComponents {
	runtime_upgrade_proposal: UpgradeProposal,
}

/// Only enable the benchmarking host functions when we actually want to benchmark.
#[cfg(feature = "runtime-benchmarks")]
pub type HostFunctions = (
	sp_io::SubstrateHostFunctions,
	frame_benchmarking::benchmarking::HostFunctions,
	midnight_node_ledger::host_api::ledger_bridge::HostFunctions,
	midnight_node_ledger::host_api::ledger_bridge_hf::HostFunctions,
);
/// Otherwise we only use the default Substrate host functions.
#[cfg(not(feature = "runtime-benchmarks"))]
pub type HostFunctions = (
	sp_io::SubstrateHostFunctions,
	midnight_node_ledger::host_api::ledger_bridge::HostFunctions,
	midnight_node_ledger::host_api::ledger_bridge_hf::HostFunctions,
);

/// A specialized `WasmExecutor` intended to use across the substrate node. It provides all the
/// required `HostFunctions`.
pub type RuntimeExecutor = sc_executor::WasmExecutor<HostFunctions>;

pub(crate) type FullClient = sc_service::TFullClient<Block, RuntimeApi, RuntimeExecutor>;
type FullBackend = sc_service::TFullBackend<Block>;
type FullSelectChain = sc_consensus::LongestChain<FullBackend, Block>;

/// The minimum period of blocks on which justifications will be
/// imported and generated.
const GRANDPA_JUSTIFICATION_PERIOD: u32 = 512;

type MidnightService = (
	sc_service::PartialComponents<
		FullClient,
		FullBackend,
		FullSelectChain,
		sc_consensus::DefaultImportQueue<Block>,
		sc_transaction_pool::FullPool<Block, FullClient>,
		(
			sc_consensus_grandpa::GrandpaBlockImport<
				FullBackend,
				Block,
				FullClient,
				FullSelectChain,
			>,
			sc_consensus_grandpa::LinkHalf<Block, FullClient, FullSelectChain>,
			Option<Telemetry>,
			DataSources,
			Option<McFollowerMetrics>,
		),
	>,
	MidnightPartialComponents,
);

pub fn new_partial(
	config: &Configuration,
	epoch_config: MainchainEpochConfig,
	data_sources: DataSources,
	proposed_wasm_file: &Option<String>,
	storage_config: StorageConfig,
) -> Result<MidnightService, ServiceError> {
	let _mc_follower_metrics = register_metrics_warn_errors(config.prometheus_registry());

	// Init Ledger DB
	let parity_db_path = config.base_path.path().join("ledger_storage");
	midnight_node_ledger::init_storage_paritydb(
		&parity_db_path,
		storage_config.genesis_tx_without_network_id(),
		storage_config.cache_size,
	);

	let telemetry = config
		.telemetry_endpoints
		.clone()
		.filter(|x| !x.is_empty())
		.map(|endpoints| -> Result<_, sc_telemetry::Error> {
			let worker = TelemetryWorker::new(16)?;
			let telemetry = worker.handle().new_telemetry(endpoints);
			Ok((worker, telemetry))
		})
		.transpose()?;

	let executor = sc_service::new_wasm_executor(&config.executor);
	let backend = sc_service::new_db_backend(config.db_config())?;

	let genesis_tx: Vec<u8> = if let Some(v) = config.chain_spec.properties().get("genesis_tx") {
		let serde_json::Value::String(tx) = v else {
			return Err(ServiceError::Other("genesis json hex string expected".into()));
		};
		hex::decode(tx)
			.map_err(|e| ServiceError::Other(format!("Can't decode the genesis tx: {}", e)))?
	} else {
		vec![]
	};

	let genesis_block_builder = GenesisBlockBuilder::<Block, _, _>::new(
		config.chain_spec.as_storage_builder(),
		true,
		backend.clone(),
		executor.clone(),
		genesis_tx,
	)
	.unwrap();

	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts_with_genesis_builder::<
			Block,
			RuntimeApi,
			_,
			GenesisBlockBuilder<Block, FullBackend, RuntimeExecutor>,
		>(
			config,
			telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
			executor,
			backend,
			genesis_block_builder,
			false,
		)?;
	let client = Arc::new(client);

	// Register Prometheus Ledger Metrics
	let ledger_metrics =
		config
			.prometheus_registry()
			.map(LedgerMetrics::register)
			.and_then(|result| match result {
				Ok(metrics) => {
					log::debug!(target: "prometheus", "Registered Ledger metrics");
					Some(metrics)
				},
				Err(_err) => {
					log::error!(target: "prometheus", "Failed to register Ledger metrics");
					None
				},
			});

	client
		.execution_extensions()
		.set_extensions_factory(ExtensionsFactory::<Block>::new(Arc::new(Mutex::new(
			ledger_metrics,
		))));

	// Get new runtime proposal by checking for manually provided wasm, falling back to embedded wasm, or getting existing values(noop)
	let potential_runtime_proposal = if let Some(wasm_file) = proposed_wasm_file {
		let wasm = fs::read(wasm_file)?;
		UpgradeProposal::from_embedded_runtime(&wasm)
			.map_err(|e| ServiceError::Application(Box::new(e)))?
	} else {
		match WASM_BINARY {
			Some(wasm) => UpgradeProposal::from_embedded_runtime(wasm)
				.map_err(|e| sc_cli::Error::Application(Box::new(e)))
				.map_err(|e| ServiceError::Application(Box::new(e)))?,
			None => {
				let api = client.runtime_api();
				let best_hash = client.info().best_hash;
				let (spec_version, runtime_hash) = api
					.get_current_version_info(best_hash)
					.map_err(|e| ServiceError::Application(Box::new(e)))?;
				UpgradeProposal::new(spec_version, runtime_hash)
			},
		}
	};

	let telemetry = telemetry.map(|(worker, telemetry)| {
		task_manager.spawn_handle().spawn("telemetry", None, worker.run());
		telemetry
	});

	let select_chain = sc_consensus::LongestChain::new(backend.clone());

	let transaction_pool = sc_transaction_pool::BasicPool::new_full(
		config.transaction_pool.clone(),
		config.role.is_authority().into(),
		config.prometheus_registry(),
		task_manager.spawn_essential_handle(),
		client.clone(),
	);

	let (grandpa_block_import, grandpa_link) = sc_consensus_grandpa::block_import(
		client.clone(),
		GRANDPA_JUSTIFICATION_PERIOD,
		&client,
		select_chain.clone(),
		telemetry.as_ref().map(|x| x.handle()),
	)?;

	let sc_slot_config = sidechain_slots::runtime_api_client::slot_config(&*client)
		.map_err(sp_blockchain::Error::from)?;

	let mc_follower_metrics = register_metrics_warn_errors(config.prometheus_registry());

	let time_source = Arc::new(SystemTimeSource);
	let inherent_config = CreateInherentDataConfig::new(epoch_config, sc_slot_config, time_source);

	#[cfg(feature = "experimental")]
	let import_queue = partner_chains_aura_import_queue::import_queue::<
		AuraPair,
		_,
		_,
		_,
		_,
		_,
		McHashInherentDigest,
	>(ImportQueueParams {
		block_import: grandpa_block_import.clone(),
		justification_import: Some(Box::new(grandpa_block_import.clone())),
		client: client.clone(),
		create_inherent_data_providers: VerifierCIDP::new(
			inherent_config,
			client.clone(),
			data_sources.mc_hash.clone(),
			data_sources.authority_selection.clone(),
			data_sources.native_token.clone(),
		),
		spawner: &task_manager.spawn_essential_handle(),
		registry: config.prometheus_registry(),
		check_for_equivocation: Default::default(),
		telemetry: telemetry.as_ref().map(|x| x.handle()),
		compatibility_mode: Default::default(),
	})?;

	#[cfg(not(feature = "experimental"))]
	let import_queue = partner_chains_aura_import_queue::import_queue::<
		AuraPair,
		_,
		_,
		_,
		_,
		_,
		McHashInherentDigest,
	>(ImportQueueParams {
		block_import: grandpa_block_import.clone(),
		justification_import: Some(Box::new(grandpa_block_import.clone())),
		client: client.clone(),
		create_inherent_data_providers: VerifierCIDP::new(
			inherent_config,
			client.clone(),
			data_sources.mc_hash.clone(),
			data_sources.authority_selection.clone(),
			data_sources.native_token.clone(),
		),
		spawner: &task_manager.spawn_essential_handle(),
		registry: config.prometheus_registry(),
		check_for_equivocation: Default::default(),
		telemetry: telemetry.as_ref().map(|x| x.handle()),
		compatibility_mode: Default::default(),
	})?;

	let partial_components = sc_service::PartialComponents {
		client: client.clone(),
		backend,
		task_manager,
		import_queue,
		keystore_container,
		select_chain,
		transaction_pool,
		other: (grandpa_block_import, grandpa_link, telemetry, data_sources, mc_follower_metrics),
	};

	let midnight_service_partial_components =
		MidnightPartialComponents { runtime_upgrade_proposal: potential_runtime_proposal };

	Ok((partial_components, midnight_service_partial_components))
}

/// Builds a new service for a full client.
pub async fn new_full<Network: sc_network::NetworkBackend<Block, <Block as BlockT>::Hash>>(
	config: Configuration,
	epoch_config: MainchainEpochConfig,
	data_sources: DataSources,
	storage_monitor_params: sc_storage_monitor::StorageMonitorParams,
	potential_runtime_upgrade_proposal: &Option<String>,
	storage_config: StorageConfig,
) -> Result<TaskManager, ServiceError> {
	let database_source = config.database.clone();
	let new_partial_components = new_partial(
		&config,
		epoch_config.clone(),
		data_sources.clone(),
		potential_runtime_upgrade_proposal,
		storage_config,
	)?;

	let (
		sc_service::PartialComponents {
			client,
			backend,
			mut task_manager,
			import_queue,
			keystore_container,
			select_chain,
			transaction_pool,
			other:
				(block_import, grandpa_link, mut telemetry, data_sources, _mc_follower_metrics_opt),
		},
		MidnightPartialComponents { runtime_upgrade_proposal },
	) = new_partial_components;

	let mut net_config = sc_network::config::FullNetworkConfiguration::<_, _, Network>::new(
		&config.network,
		config.prometheus_registry().cloned(),
	);

	let grandpa_protocol_name = sc_consensus_grandpa::protocol_standard_name(
		&client.block_hash(0).ok().flatten().expect("Genesis block exists; qed"),
		&config.chain_spec,
	);
	let metrics = Network::register_notification_metrics(
		config.prometheus_config.as_ref().map(|cfg| &cfg.registry),
	);
	let peer_store_handle = net_config.peer_store_handle();
	let (grandpa_protocol_config, grandpa_notification_service) =
		sc_consensus_grandpa::grandpa_peers_set_config::<_, Network>(
			grandpa_protocol_name.clone(),
			metrics.clone(),
			Arc::clone(&peer_store_handle),
		);
	net_config.add_notification_protocol(grandpa_protocol_config);

	let warp_sync = Arc::new(sc_consensus_grandpa::warp_proof::NetworkProvider::new(
		backend.clone(),
		grandpa_link.shared_authority_set().clone(),
		Vec::default(),
	));

	let (network, system_rpc_tx, tx_handler_controller, network_starter, sync_service) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &config,
			net_config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue,
			block_announce_validator_builder: None,
			warp_sync_config: Some(WarpSyncConfig::WithProvider(warp_sync)),
			block_relay: None,
			metrics,
		})?;

	if config.offchain_worker.enabled {
		task_manager.spawn_handle().spawn(
			"offchain-workers-runner",
			"offchain-worker",
			sc_offchain::OffchainWorkers::new(sc_offchain::OffchainWorkerOptions {
				runtime_api_provider: client.clone(),
				is_validator: config.role.is_authority(),
				keystore: Some(keystore_container.keystore()),
				offchain_db: backend.offchain_storage(),
				transaction_pool: Some(OffchainTransactionPoolFactory::new(
					transaction_pool.clone(),
				)),
				network_provider: Arc::new(network.clone()),
				enable_http_requests: true,
				custom_extensions: |_| vec![],
			})
			.run(client.clone(), task_manager.spawn_handle())
			.boxed(),
		);
	}

	let role = config.role;
	let force_authoring = config.force_authoring;
	// Backoff with some additional time before stall. Around 1 day plus 1 session
	let backoff_authoring_blocks: Option<BackoffAuthoringOnFinalizedHeadLagging<_>> =
		Some(BackoffAuthoringOnFinalizedHeadLagging {
			unfinalized_slack: 15_600_u32,
			..Default::default()
		});

	let name = config.network.node_name.clone();
	let enable_grandpa = !config.disable_grandpa;
	let prometheus_registry = config.prometheus_registry().cloned();
	let shared_voter_state = SharedVoterState::empty();

	let rpc_extensions_builder = {
		let client = client.clone();
		let pool = transaction_pool.clone();
		let backend = backend.clone();
		let shared_voter_state = shared_voter_state.clone();
		let shared_authority_set = grandpa_link.shared_authority_set().clone();
		let justification_stream = grandpa_link.justification_stream();
		let main_chain_follower_data_sources = data_sources.clone();
		let epoch_config = epoch_config.clone();

		move |subscription_executor| {
			let grandpa = GrandpaDeps {
				shared_voter_state: shared_voter_state.clone(),
				shared_authority_set: shared_authority_set.clone(),
				justification_stream: justification_stream.clone(),
				subscription_executor,
				finality_provider: sc_consensus_grandpa::FinalityProofProvider::new_for_service(
					backend.clone(),
					Some(shared_authority_set.clone()),
				),
			};
			let deps = crate::rpc::FullDeps {
				client: client.clone(),
				pool: pool.clone(),
				grandpa,
				main_chain_follower_data_sources: main_chain_follower_data_sources.clone(),
				time_source: Arc::new(SystemTimeSource),
				main_chain_epoch_config: epoch_config.clone(),
			};
			crate::rpc::create_full(deps).map_err(Into::into)
		}
	};

	let _rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		network: network.clone(),
		client: client.clone(),
		keystore: keystore_container.keystore(),
		task_manager: &mut task_manager,
		transaction_pool: transaction_pool.clone(),
		rpc_builder: Box::new(rpc_extensions_builder),
		backend,
		system_rpc_tx,
		tx_handler_controller,
		sync_service: sync_service.clone(),
		config,
		telemetry: telemetry.as_mut(),
	})?;

	if role.is_authority() {
		let basic_authorship_proposer_factory = sc_basic_authorship::ProposerFactory::new(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool.clone(),
			prometheus_registry.as_ref(),
			telemetry.as_ref().map(|x| x.handle()),
		);
		let proposer_factory: PartnerChainsProposerFactory<_, _, McHashInherentDigest> =
			PartnerChainsProposerFactory::new(basic_authorship_proposer_factory);

		let sc_slot_config = sidechain_slots::runtime_api_client::slot_config(&*client)
			.map_err(sp_blockchain::Error::from)?;
		let time_source = Arc::new(SystemTimeSource);
		let inherent_config =
			CreateInherentDataConfig::new(epoch_config, sc_slot_config.clone(), time_source);

		#[cfg(not(feature = "experimental"))]
		let aura = sc_partner_chains_consensus_aura::start_aura::<
			AuraPair,
			_,
			_,
			_,
			_,
			_,
			_,
			_,
			_,
			_,
			_,
			McHashInherentDigest,
		>(StartAuraParams {
			slot_duration: sc_slot_config.slot_duration,
			client: client.clone(),
			select_chain,
			block_import,
			proposer_factory,
			create_inherent_data_providers: ProposalCIDP::new(
				inherent_config,
				client.clone(),
				data_sources.mc_hash.clone(),
				data_sources.authority_selection.clone(),
				runtime_upgrade_proposal,
				data_sources.native_token.clone(),
			),
			force_authoring,
			backoff_authoring_blocks,
			keystore: keystore_container.keystore(),
			sync_oracle: sync_service.clone(),
			justification_sync_link: sync_service.clone(),
			block_proposal_slot_portion: SlotProportion::new(2f32 / 3f32),
			max_block_proposal_slot_portion: None,
			telemetry: telemetry.as_ref().map(|x| x.handle()),
			compatibility_mode: Default::default(),
		})?;

		#[cfg(feature = "experimental")]
		let aura = sc_partner_chains_consensus_aura::start_aura::<
			AuraPair,
			_,
			_,
			_,
			_,
			_,
			_,
			_,
			_,
			_,
			_,
			McHashInherentDigest,
		>(StartAuraParams {
			slot_duration: sc_slot_config.slot_duration,
			client: client.clone(),
			select_chain,
			block_import,
			proposer_factory,
			create_inherent_data_providers: ProposalCIDP::new(
				inherent_config,
				client.clone(),
				data_sources.mc_hash.clone(),
				data_sources.authority_selection.clone(),
				runtime_upgrade_proposal,
				data_sources.native_token.clone(),
			),
			force_authoring,
			backoff_authoring_blocks,
			keystore: keystore_container.keystore(),
			sync_oracle: sync_service.clone(),
			justification_sync_link: sync_service.clone(),
			block_proposal_slot_portion: SlotProportion::new(2f32 / 3f32),
			max_block_proposal_slot_portion: None,
			telemetry: telemetry.as_ref().map(|x| x.handle()),
			compatibility_mode: Default::default(),
		})?;

		// the AURA authoring task is considered essential, i.e. if it
		// fails we take down the service with it.
		task_manager
			.spawn_essential_handle()
			.spawn_blocking("aura", Some("block-authoring"), aura);
	}

	if enable_grandpa {
		// if the node isn't actively participating in consensus then it doesn't
		// need a keystore, regardless of which protocol we use below.
		let keystore = if role.is_authority() { Some(keystore_container.keystore()) } else { None };

		let grandpa_config = sc_consensus_grandpa::Config {
			// FIXME #1578 make this available through chainspec
			gossip_duration: Duration::from_millis(333),
			justification_generation_period: GRANDPA_JUSTIFICATION_PERIOD,
			name: Some(name),
			observer_enabled: false,
			keystore,
			local_role: role,
			telemetry: telemetry.as_ref().map(|x| x.handle()),
			protocol_name: grandpa_protocol_name,
		};

		// start the full GRANDPA voter
		// NOTE: non-authorities could run the GRANDPA observer protocol, but at
		// this point the full voter should provide better guarantees of block
		// and vote data availability than the observer. The observer has not
		// been tested extensively yet and having most nodes in a network run it
		// could lead to finality stalls.
		let grandpa_config = sc_consensus_grandpa::GrandpaParams {
			config: grandpa_config,
			link: grandpa_link,
			network,
			sync: Arc::new(sync_service),
			notification_service: grandpa_notification_service,
			voting_rule: sc_consensus_grandpa::VotingRulesBuilder::default().build(),
			prometheus_registry,
			shared_voter_state,
			telemetry: telemetry.as_ref().map(|x| x.handle()),
			offchain_tx_pool_factory: OffchainTransactionPoolFactory::new(transaction_pool),
		};

		// the GRANDPA voter task is considered infallible, i.e.
		// if it fails we take down the service with it.
		task_manager.spawn_essential_handle().spawn_blocking(
			"grandpa-voter",
			None,
			sc_consensus_grandpa::run_grandpa_voter(grandpa_config)?,
		);
	}

	sc_storage_monitor::StorageMonitorService::try_spawn(
		storage_monitor_params,
		database_source.path().expect("db path available").into(),
		&task_manager.spawn_essential_handle(),
	)
	.map_err(|e| sc_service::Error::Other(e.to_string()))?;

	network_starter.start_network();
	Ok(task_manager)
}
