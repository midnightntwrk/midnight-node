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
	rpc::{BeefyDeps, GrandpaDeps},
};
use futures::FutureExt;
use midnight_node_runtime::storage::child::StateVersion;
use midnight_node_runtime::{self, RuntimeApi, opaque::Block};
use midnight_primitives_ledger::{LedgerMetrics, LedgerStorage};
use parity_scale_codec::{Decode, Encode};
use partner_chains_db_sync_data_sources::McFollowerMetrics;
use partner_chains_db_sync_data_sources::register_metrics_warn_errors;
use sc_client_api::{Backend, BlockImportOperation, ExecutorProvider};
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
use sp_consensus_aura::sr25519::AuthorityPair as AuraPair;
use sp_consensus_beefy::ecdsa_crypto::AuthorityId as BeefyId;

use mmr_gadget::MmrGadget;
use sc_rpc::SubscriptionTaskExecutor;
use sp_core::storage::Storage;
use sp_partner_chains_consensus_aura::block_proposal::PartnerChainsProposerFactory;
use sp_runtime::{
	BuildStorage,
	traits::{Block as BlockT, Hash as HashT, HashingFor, Header as HeaderT, Zero},
};
use sp_runtime::{Digest, DigestItem};
use std::{
	marker::PhantomData,
	sync::{Arc, Mutex},
	time::Duration,
};
use time_source::SystemTimeSource;

pub struct StorageInit {
	pub genesis_state: Vec<u8>,
	pub cache_size: usize,
}

pub struct GenesisBlockBuilder<Block: BlockT, B, E> {
	genesis_storage: Storage,
	commit_genesis_state: bool,
	backend: Arc<B>,
	executor: E,
	genesis_extrinsics: Vec<Vec<u8>>,
	_phantom: PhantomData<Block>,
}

impl<Block: BlockT, B: Backend<Block>, E: RuntimeVersionOf> GenesisBlockBuilder<Block, B, E> {
	/// Constructs a new instance of [`GenesisBlockBuilder`].
	pub fn new(
		build_genesis_storage: &dyn BuildStorage,
		commit_genesis_state: bool,
		backend: Arc<B>,
		executor: E,
		genesis_extrinsics: Vec<Vec<u8>>,
	) -> sp_blockchain::Result<Self> {
		let genesis_storage =
			build_genesis_storage.build_storage().map_err(sp_blockchain::Error::Storage)?;
		Ok(Self {
			genesis_storage,
			commit_genesis_state,
			backend,
			executor,
			genesis_extrinsics,
			_phantom: PhantomData::<Block>,
		})
	}
}

impl<Block: BlockT, B: Backend<Block>, E: RuntimeVersionOf> BuildGenesisBlock<Block>
	for GenesisBlockBuilder<Block, B, E>
{
	type BlockImportOperation = <B as Backend<Block>>::BlockImportOperation;

	fn build_genesis_block(self) -> sp_blockchain::Result<(Block, Self::BlockImportOperation)> {
		let Self {
			genesis_storage,
			commit_genesis_state,
			backend,
			executor,
			genesis_extrinsics,
			_phantom,
		} = self;

		let mut extrinsics = Vec::new();
		for ext_bytes in genesis_extrinsics {
			let extrinsic = <<Block as BlockT>::Extrinsic>::decode(&mut &ext_bytes[..])
				.map_err(|e| sp_blockchain::Error::Application(Box::new(e)))?;
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

	let block_digest = Digest {
		logs: vec![DigestItem::Consensus(
			midnight_node_runtime::VERSION_ID,
			midnight_node_runtime::VERSION.spec_version.encode(),
		)],
	};

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

type MidnightService = sc_service::PartialComponents<
	FullClient,
	FullBackend,
	FullSelectChain,
	sc_consensus::DefaultImportQueue<Block>,
	sc_transaction_pool::TransactionPoolWrapper<Block, FullClient>,
	(
		sc_consensus_grandpa::GrandpaBlockImport<FullBackend, Block, FullClient, FullSelectChain>,
		sc_consensus_grandpa::LinkHalf<Block, FullClient, FullSelectChain>,
		sc_consensus_beefy::BeefyVoterLinks<Block, BeefyId>,
		sc_consensus_beefy::BeefyRPCLinks<Block, BeefyId>,
		Option<Telemetry>,
		DataSources,
		Option<McFollowerMetrics>,
	),
>;

#[allow(clippy::result_large_err)]
pub fn new_partial(
	config: &Configuration,
	epoch_config: MainchainEpochConfig,
	data_sources: DataSources,
	storage_config: StorageInit,
) -> Result<MidnightService, ServiceError> {
	let _mc_follower_metrics = register_metrics_warn_errors(config.prometheus_registry());

	// Init Ledger DB
	let parity_db_path = config.base_path.path().join("ledger_storage");
	midnight_node_ledger::init_storage_paritydb(
		&parity_db_path,
		&storage_config.genesis_state,
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

	let genesis_extrinsics: Result<Vec<Vec<u8>>, ServiceError> = config
		.chain_spec
		.properties()
		.get("genesis_extrinsics")
		.ok_or(ServiceError::Other("missing genesis extrinsics in chain spec".into()))?
		.as_array()
		.ok_or(ServiceError::Other("genesis_extrinsics is not a vec".into()))?
		.iter()
		.map(|v| {
			v.as_str()
				.ok_or(ServiceError::Other(format!("extrinsic not a string: {v:?}")))
				.map(|v| v.to_string())
		})
		.take_while(Result::is_ok)
		.map(|v| {
			let s = v.unwrap();
			hex::decode(&s).map_err(|e| {
				ServiceError::Other(format!("error decoding extrinsic as hex: {s:?}. Error: {e}"))
			})
		})
		.collect();

	let genesis_block_builder = GenesisBlockBuilder::<Block, _, _>::new(
		config.chain_spec.as_storage_builder(),
		true,
		backend.clone(),
		executor.clone(),
		genesis_extrinsics?,
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

	let ledger_storage = LedgerStorage::new(parity_db_path, storage_config.cache_size);

	client
		.execution_extensions()
		.set_extensions_factory(ExtensionsFactory::<Block>::new(
			Arc::new(Mutex::new(ledger_metrics)),
			ledger_storage,
		));

	let telemetry = telemetry.map(|(worker, telemetry)| {
		task_manager.spawn_handle().spawn("telemetry", None, worker.run());
		telemetry
	});

	let select_chain = sc_consensus::LongestChain::new(backend.clone());

	let transaction_pool = sc_transaction_pool::Builder::new(
		task_manager.spawn_essential_handle(),
		client.clone(),
		config.role.is_authority().into(),
	)
	.with_options(config.transaction_pool.clone())
	.with_prometheus(config.prometheus_registry())
	.build();

	let (grandpa_block_import, grandpa_link) = sc_consensus_grandpa::block_import(
		client.clone(),
		GRANDPA_JUSTIFICATION_PERIOD,
		&client,
		select_chain.clone(),
		telemetry.as_ref().map(|x| x.handle()),
	)?;

	let (_, beefy_voter_links, beefy_rpc_links) = sc_consensus_beefy::beefy_block_import_and_links(
		grandpa_block_import.clone(),
		backend.clone(),
		client.clone(),
		config.prometheus_registry().cloned(),
	);

	let sc_slot_config = sidechain_slots::runtime_api_client::slot_config(&*client)
		.map_err(sp_blockchain::Error::from)?;

	let mc_follower_metrics = register_metrics_warn_errors(config.prometheus_registry());

	let time_source = Arc::new(SystemTimeSource);
	let inherent_config = CreateInherentDataConfig::new(epoch_config, sc_slot_config, time_source);

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
			data_sources.cnight_observation.clone(),
			data_sources.governed_map.clone(),
			data_sources.federated_authority_observation.clone(),
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
		transaction_pool: Arc::new(transaction_pool),
		other: (
			grandpa_block_import,
			grandpa_link,
			beefy_voter_links,
			beefy_rpc_links,
			telemetry,
			data_sources,
			mc_follower_metrics,
		),
	};

	Ok(partial_components)
}

/// Builds a new service for a full client.
pub async fn new_full<Network: sc_network::NetworkBackend<Block, <Block as BlockT>::Hash>>(
	config: Configuration,
	epoch_config: MainchainEpochConfig,
	data_sources: DataSources,
	storage_monitor_params: sc_storage_monitor::StorageMonitorParams,
	storage_config: StorageInit,
) -> Result<TaskManager, ServiceError> {
	let database_source = config.database.clone();
	let new_partial_components =
		new_partial(&config, epoch_config.clone(), data_sources.clone(), storage_config)?;

	let sc_service::PartialComponents {
		client,
		backend,
		mut task_manager,
		import_queue,
		keystore_container,
		select_chain,
		transaction_pool,
		other:
			(
				block_import,
				grandpa_link,
				beefy_voter_links,
				beefy_rpc_links,
				mut telemetry,
				data_sources,
				_mc_follower_metrics_opt,
			),
	} = new_partial_components;

	let mut net_config = sc_network::config::FullNetworkConfiguration::<_, _, Network>::new(
		&config.network,
		config.prometheus_registry().cloned(),
	);
	let genesis_hash = client.chain_info().genesis_hash;

	let grandpa_protocol_name =
		sc_consensus_grandpa::protocol_standard_name(&genesis_hash, &config.chain_spec);
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

	let prometheus_registry = config.prometheus_registry().cloned();
	let beefy_gossip_proto_name =
		sc_consensus_beefy::gossip_protocol_name(genesis_hash, config.chain_spec.fork_id());
	// `beefy_on_demand_justifications_handler` is given to `beefy-gadget` task to be run,
	// while `beefy_req_resp_cfg` is added to `config.network.request_response_protocols`.
	let (beefy_on_demand_justifications_handler, beefy_req_resp_cfg) =
		sc_consensus_beefy::communication::request_response::BeefyJustifsRequestHandler::new::<
			_,
			Network,
		>(&genesis_hash, config.chain_spec.fork_id(), client.clone(), prometheus_registry.clone());

	// enable beefy
	let (beefy_notification_config, beefy_notification_service) =
		sc_consensus_beefy::communication::beefy_peers_set_config::<_, Network>(
			beefy_gossip_proto_name.clone(),
			metrics.clone(),
			Arc::clone(&peer_store_handle),
		);

	net_config.add_notification_protocol(beefy_notification_config);
	net_config.add_request_response_protocol(beefy_req_resp_cfg);

	let warp_sync = Arc::new(sc_consensus_grandpa::warp_proof::NetworkProvider::new(
		backend.clone(),
		grandpa_link.shared_authority_set().clone(),
		Vec::default(),
	));

	let (network, system_rpc_tx, tx_handler_controller, sync_service) =
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
			})?
			.run(client.clone(), task_manager.spawn_handle())
			.boxed(),
		);
	}

	let is_offchain_indexing_enabled = config.offchain_worker.indexing_enabled;
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

		move |subscription_executor: SubscriptionTaskExecutor| {
			let grandpa = GrandpaDeps {
				shared_voter_state: shared_voter_state.clone(),
				shared_authority_set: shared_authority_set.clone(),
				justification_stream: justification_stream.clone(),
				subscription_executor: subscription_executor.clone(),
				finality_provider: sc_consensus_grandpa::FinalityProofProvider::new_for_service(
					backend.clone(),
					Some(shared_authority_set.clone()),
				),
			};

			let beefy = BeefyDeps {
				beefy_finality_proof_stream: beefy_rpc_links.from_voter_justif_stream.clone(),
				beefy_best_block_stream: beefy_rpc_links.from_voter_best_beefy_stream.clone(),
				subscription_executor,
			};

			let deps = crate::rpc::FullDeps {
				client: client.clone(),
				pool: pool.clone(),
				grandpa,
				beefy,
				main_chain_follower_data_sources: main_chain_follower_data_sources.clone(),
				time_source: Arc::new(SystemTimeSource),
				main_chain_epoch_config: epoch_config.clone(),
				backend: backend.clone(),
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
		backend: backend.clone(),
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
				data_sources.cnight_observation.clone(),
				data_sources.governed_map.clone(),
				data_sources.federated_authority_observation.clone(),
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

		// beefy is enabled if its notification service exists
		let justifications_protocol_name = beefy_on_demand_justifications_handler.protocol_name();
		let network_params = sc_consensus_beefy::BeefyNetworkParams {
			network: Arc::new(network.clone()),
			sync: sync_service.clone(),
			gossip_protocol_name: beefy_gossip_proto_name,
			justifications_protocol_name,
			notification_service: beefy_notification_service,
			_phantom: core::marker::PhantomData::<Block>,
		};
		let payload_provider = sp_consensus_beefy::mmr::MmrRootProvider::new(client.clone());
		let beefy_params = sc_consensus_beefy::BeefyParams {
			client: client.clone(),
			backend: backend.clone(),
			payload_provider,
			runtime: client.clone(),
			key_store: keystore.clone(),
			network_params,
			min_block_delta: 8,
			prometheus_registry: prometheus_registry.clone(),
			links: beefy_voter_links,
			on_demand_justifications_handler: beefy_on_demand_justifications_handler,
			is_authority: role.is_authority(),
		};

		let gadget =
			sc_consensus_beefy::start_beefy_gadget::<_, _, _, _, _, _, _, BeefyId>(beefy_params);

		// BEEFY is part of consensus, if it fails we'll bring the node down with it to make
		// sure it is noticed.
		task_manager
			.spawn_essential_handle()
			.spawn_blocking("beefy-gadget", None, gadget);

		// When offchain indexing is enabled, MMR gadget should also run.
		if is_offchain_indexing_enabled {
			task_manager.spawn_essential_handle().spawn_blocking(
				"mmr-gadget",
				None,
				MmrGadget::start(
					client.clone(),
					backend,
					sp_mmr_primitives::INDEXING_PREFIX.to_vec(),
				),
			);
		}

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

	if let Some(database_path) = database_source.path() {
		sc_storage_monitor::StorageMonitorService::try_spawn(
			storage_monitor_params,
			database_path.to_path_buf(),
			&task_manager.spawn_essential_handle(),
		)
		.map_err(|e| ServiceError::Application(e.into()))?;
	}

	Ok(task_manager)
}
