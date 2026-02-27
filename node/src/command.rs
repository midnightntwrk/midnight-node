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

#![allow(clippy::result_large_err)]

use crate::{
	cfg::Cfg,
	cli::{self, Cli, Subcommand},
	genesis::creation::{
		cnight_genesis::generate_cnight_genesis,
		federated_authority_genesis::generate_federated_authority_genesis,
		ics_genesis::{IcsAddresses, generate_ics_genesis},
		permissioned_candidates_genesis::{
			PcChainConfig, PermissionedCandidatesAddresses,
			generate_permissioned_candidates_genesis,
		},
		reserve_genesis::{ReserveAddresses, generate_reserve_genesis},
	},
	genesis::verification::{
		verify_auth_script_common, verify_federated_authority_auth_script, verify_ics_auth_script,
		verify_ledger_state_genesis, verify_permissioned_candidates_auth_script,
	},
	service::{self, StorageInit},
};
use clap::Parser;
use midnight_node_res::networks::MidnightNetwork as _;
use midnight_node_runtime::Block;
use midnight_primitives_cnight_observation::CNightAddresses;
use midnight_primitives_federated_authority_observation::FederatedAuthorityAddresses;
use sc_cli::{CliConfiguration, LoggerBuilder, RunCmd, SubstrateCli};
use sc_keystore::LocalKeystore;
use sc_service::{BasePath, PartialComponents, config::KeystoreConfig};
use sidechain_domain::mainchain_epoch::MainchainEpochConfig;
use sp_core::{ByteArray, Pair, offchain::KeyTypeId};
use sp_keystore::KeystorePtr;

#[cfg(feature = "runtime-benchmarks")]
use {
	crate::benchmarking::{RemarkBuilder, inherent_benchmark_data},
	frame_benchmarking_cli::*,
	sp_runtime::traits::HashingFor,
};

pub(crate) fn safe_exit(code: i32) -> ! {
	use std::io::Write;
	let _ = std::io::stdout().lock().flush();
	let _ = std::io::stderr().lock().flush();
	std::process::exit(code)
}

/// Parse and run command line arguments
pub fn run() -> sc_cli::Result<()> {
	let first_arg_char = std::env::args().nth(1).map(|arg| arg.chars().next());
	let subcommand_used = first_arg_char.is_some() && first_arg_char != Some(Some('-'));

	match Cli::try_parse() {
		Ok(cli) => {
			let cfg = get_cfg(false)?;
			run_subcommand(cli.subcommand, cfg)
		},
		Err(e) if e.kind() == clap::error::ErrorKind::DisplayHelp => {
			// Only show current config settings for main command.
			if !subcommand_used {
				if std::env::args().any(|a| a == "--help") {
					let _ =
						RunCmd::try_parse_from(["midnight-node", "--help"]).unwrap_err().print();
					Cfg::help();
				} else {
					let _ = RunCmd::try_parse_from(["midnight-node", "-h"]).unwrap_err().print();
				}
			}
			let _ = e.print();
			safe_exit(e.exit_code())
		},
		Err(e) if e.kind() == clap::error::ErrorKind::DisplayVersion => e.exit(),
		Err(e) => {
			// Only show current config settings for main command.
			if !subcommand_used {
				let cfg = get_cfg(true)?;
				match run_node(cfg) {
					res @ Ok(_) => res,
					Err(e) => {
						Cfg::help();
						eprintln!("error: {e:?}");
						safe_exit(1)
					},
				}
			} else {
				eprintln!("{e}");
				safe_exit(2)
			}
		},
	}
}

fn get_cfg(validate: bool) -> sc_cli::Result<Cfg> {
	let cfg = if validate { Cfg::new() } else { Cfg::new_no_validation() };
	let cfg = cfg.map_err(|e| {
		let msg = format!("configuration error: {e}");
		eprintln!("{}", &msg);
		Cfg::help();
		sc_cli::Error::Input(msg)
	})?;

	if cfg.meta_cfg.show_config {
		Cfg::help();
	}

	Ok(cfg)
}

fn run_node(cfg: Cfg) -> sc_cli::Result<()> {
	let run_cmd: RunCmd = cfg.substrate_cfg.clone().try_into()?;
	if cfg.midnight_cfg.wipe_chain_state
		&& let Some(base_path) = run_cmd.base_path()?
	{
		crate::util::remove_dir_contents(base_path.path())
			.map_err(|e| sc_cli::Error::Application(Box::new(e)))?;
	}

	let runner = cfg.create_runner(&run_cmd)?;
	let base_path = run_cmd
		.shared_params()
		.base_path()?
		.unwrap_or_else(|| BasePath::from_project("", "", "midnight-node"));
	let chain_id = run_cmd.shared_params().chain_id(run_cmd.shared_params().is_dev());
	let chain_spec = cfg.load_spec(&chain_id)?;
	let config_dir = base_path.config_dir(chain_spec.id());

	let properties = chain_spec.properties();
	let genesis_state_hex = properties.get("genesis_state").unwrap().as_str().unwrap();
	let genesis_state = hex::decode(genesis_state_hex).unwrap();
	let storage_config =
		StorageInit { genesis_state, cache_size: cfg.midnight_cfg.storage_cache_size };

	let keystore: KeystorePtr = {
		let res = run_cmd.keystore_params().unwrap().keystore_config(&config_dir)?;
		if let KeystoreConfig::Path { path, password } = res {
			LocalKeystore::open(path, password)?.into()
		} else {
			panic!("InMemory Keystore not supported")
		}
	};

	if let Some(seed_file) = &cfg.midnight_cfg.aura_seed_file {
		let seed = std::fs::read_to_string(seed_file).map_err(|e| {
			sc_cli::Error::Input(format!(
				"error when reading AURA seed file at {seed_file}. Error: {e}"
			))
		})?;
		let seed = seed.trim();
		let (keypair, _) = sp_core::sr25519::Pair::from_string_with_seed(seed, None)
			.map_err(|e| sc_cli::Error::Input(format!("Invalid AURA seed: {e}")))?;
		keystore
			.insert(KeyTypeId(*b"aura"), seed, &keypair.public().to_raw_vec())
			.unwrap();
		log::info!("AURA pubkey: {}", &keypair.public())
	}

	if let Some(seed_file) = &cfg.midnight_cfg.grandpa_seed_file {
		let seed = std::fs::read_to_string(seed_file).map_err(|e| {
			sc_cli::Error::Input(format!(
				"error when reading GRANDPA seed file at {seed_file}. Error: {e}"
			))
		})?;
		let seed = seed.trim();
		let (keypair, _) = sp_core::ed25519::Pair::from_string_with_seed(seed, None)
			.map_err(|e| sc_cli::Error::Input(format!("Invalid GRANDPA seed: {e}")))?;
		keystore
			.insert(KeyTypeId(*b"gran"), seed, &keypair.public().to_raw_vec())
			.unwrap();
		log::info!("GRANDPA pubkey: {}", &keypair.public())
	}

	if let Some(seed_file) = &cfg.midnight_cfg.cross_chain_seed_file {
		let seed = std::fs::read_to_string(seed_file).map_err(|e| {
			sc_cli::Error::Input(format!(
				"error when reading CROSS_CHAIN seed file at {seed_file}. Error: {e}"
			))
		})?;
		let seed = seed.trim();
		let (keypair, _) = sp_core::ecdsa::Pair::from_string_with_seed(seed, None)
			.map_err(|e| sc_cli::Error::Input(format!("Invalid CROSS_CHAIN seed: {e}")))?;
		keystore
			.insert(KeyTypeId(*b"crch"), seed, &keypair.public().to_raw_vec())
			.unwrap();
		log::info!("CROSS_CHAIN pubkey: {}", &keypair.public())
	}

	runner.run_node_until_exit(|config| async move {
		let epoch_config: MainchainEpochConfig = cfg.midnight_cfg.clone().into();

		// TODO: Add metrics
		let data_sources =
			crate::main_chain_follower::create_cached_main_chain_follower_data_sources(
				cfg.midnight_cfg.clone(),
				None,
			)
			.await?;

		// Build Prometheus push config if endpoint is configured
		log::debug!(
			"Prometheus push endpoint config: {:?}",
			cfg.midnight_cfg.prometheus_push_endpoint
		);
		let metrics_push_config =
			cfg.midnight_cfg.prometheus_push_endpoint.as_ref().map(|endpoint| {
				crate::metrics_push::MetricsPushConfig {
					endpoint: endpoint.clone(),
					interval: std::time::Duration::from_secs(
						cfg.midnight_cfg.prometheus_push_interval_secs.unwrap_or(15),
					),
					job_name: cfg
						.midnight_cfg
						.prometheus_push_job_name
						.clone()
						.unwrap_or_else(|| "midnight-node".to_string()),
					// Filled in by service::new_full from the Configuration
					peer_id: String::new(),
					node_name: String::new(),
				}
			});

		//For litep2p use `sc_network::Litep2pNetworkBackend<_, _>``
		service::new_full::<sc_network::NetworkWorker<_, _>>(
			config,
			epoch_config,
			data_sources,
			cfg.storage_monitor_params_cfg.into(),
			storage_config,
			metrics_push_config,
		)
		.await
		.map_err(sc_cli::Error::Service)
	})
}

/// Returns the CFG_PRESET from environment, defaulting to "dev"
fn get_cfg_preset() -> String {
	std::env::var("CFG_PRESET").unwrap_or_else(|_| "dev".to_string())
}

/// Returns the res/<cfg_preset> directory path
fn get_res_preset_dir() -> std::path::PathBuf {
	std::path::PathBuf::from("res").join(get_cfg_preset())
}

fn run_subcommand(subcommand: Subcommand, cfg: Cfg) -> sc_cli::Result<()> {
	let epoch_config: MainchainEpochConfig = cfg.midnight_cfg.clone().into();

	let storage_config = StorageInit {
		genesis_state: midnight_node_res::networks::UndeployedNetwork.genesis_state().to_vec(),
		cache_size: cfg.midnight_cfg.storage_cache_size,
	};

	match subcommand {
		Subcommand::Key(ref cmd) => cmd.run(&cfg),
		Subcommand::PartnerChains(cmd) => {
			let midnight_cfg = cfg.midnight_cfg.clone();
			let make_dependencies = |config: sc_service::Configuration| {
				let data_sources = config.tokio_handle.block_on(
					crate::main_chain_follower::create_cached_main_chain_follower_data_sources(
						midnight_cfg,
						None,
					),
				)?;
				let PartialComponents { client, task_manager, other, .. } =
					service::new_partial(&config, epoch_config, data_sources, storage_config)?;
				Ok((client, task_manager, other.5.authority_selection))
			};

			partner_chains_node_commands::run::<_, _, _, _, cli::MidnightBlockProducerMetadata, _, _>(
				&cfg,
				make_dependencies,
				cmd.clone(),
			)
		},
		Subcommand::BuildSpec(ref cmd) => {
			let runner = cfg.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run(config.chain_spec, config.network))
		},
		Subcommand::CheckBlock(ref cmd) => {
			let runner = cfg.create_runner(cmd)?;
			runner.async_run(|config| {
				let data_sources = config.tokio_handle.block_on(
					crate::main_chain_follower::create_cached_main_chain_follower_data_sources(
						cfg.midnight_cfg.clone(),
						None,
					),
				)?;
				let PartialComponents { client, task_manager, import_queue, .. } =
					service::new_partial(&config, epoch_config, data_sources, storage_config)?;
				Ok((cmd.run(client, import_queue), task_manager))
			})
		},
		Subcommand::ExportBlocks(ref cmd) => {
			let runner = cfg.create_runner(cmd)?;
			runner.async_run(|config| {
				let data_sources = config.tokio_handle.block_on(
					crate::main_chain_follower::create_cached_main_chain_follower_data_sources(
						cfg.midnight_cfg.clone(),
						None,
					),
				)?;
				let PartialComponents { client, task_manager, .. } =
					service::new_partial(&config, epoch_config, data_sources, storage_config)?;
				Ok((cmd.run(client, config.database), task_manager))
			})
		},
		Subcommand::ExportState(ref cmd) => {
			let runner = cfg.create_runner(cmd)?;
			runner.async_run(|config| {
				let data_sources = config.tokio_handle.block_on(
					crate::main_chain_follower::create_cached_main_chain_follower_data_sources(
						cfg.midnight_cfg.clone(),
						None,
					),
				)?;
				let PartialComponents { client, task_manager, .. } =
					service::new_partial(&config, epoch_config, data_sources, storage_config)?;
				Ok((cmd.run(client, config.chain_spec), task_manager))
			})
		},
		Subcommand::ImportBlocks(ref cmd) => {
			let runner = cfg.create_runner(cmd)?;
			runner.async_run(|config| {
				let data_sources = config.tokio_handle.block_on(
					crate::main_chain_follower::create_cached_main_chain_follower_data_sources(
						cfg.midnight_cfg.clone(),
						None,
					),
				)?;
				let PartialComponents { client, task_manager, import_queue, .. } =
					service::new_partial(&config, epoch_config, data_sources, storage_config)?;
				Ok((cmd.run(client, import_queue), task_manager))
			})
		},
		Subcommand::PurgeChain(ref cmd) => {
			let runner = cfg.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run(config.database))
		},
		Subcommand::Revert(ref cmd) => {
			let runner = cfg.create_runner(cmd)?;
			runner.async_run(|config| {
				let data_sources = config.tokio_handle.block_on(
					crate::main_chain_follower::create_cached_main_chain_follower_data_sources(
						cfg.midnight_cfg.clone(),
						None,
					),
				)?;
				let PartialComponents { client, task_manager, backend, .. } =
					service::new_partial(&config, epoch_config, data_sources, storage_config)?;
				let aux_revert = Box::new(|client, _, blocks| {
					sc_consensus_grandpa::revert(client, blocks)?;
					Ok(())
				});
				Ok((cmd.run(client, backend, Some(aux_revert)), task_manager))
			})
		},
		#[cfg(feature = "runtime-benchmarks")]
		Subcommand::Benchmark(ref cmd) => {
			log::warn!("Runtime benchmarking will be replaced by frame-omni-bencher.");
			let runner = cfg.create_runner(cmd)?;

			runner.sync_run(|config| {
				// This switch needs to be in the client, since the client decides
				// which sub-commands it wants to support.
				match cmd {
					BenchmarkCmd::Pallet(cmd) => {
						if !cfg!(feature = "runtime-benchmarks") {
							return Err(
								"Runtime benchmarking wasn't enabled when building the node. \
							You can enable it with `--features runtime-benchmarks`."
									.into(),
							)
						}

						cmd.run_with_spec::<HashingFor<Block>, service::HostFunctions>(Some(config.chain_spec))
					},
					BenchmarkCmd::Block(cmd) => {
                        let data_sources = config.tokio_handle.block_on(
                            crate::main_chain_follower::create_cached_main_chain_follower_data_sources(
                                cfg.midnight_cfg.clone(),
                                None,
                            ),
                        )?;
						// ensure that we keep the task manager alive
						let partial = service::new_partial(
                            &config,
                            epoch_config,
                            data_sources,
                            storage_config,
                        )?;

						cmd.run(partial.client)
					},
					#[cfg(not(feature = "runtime-benchmarks"))]
					BenchmarkCmd::Storage(_) => Err(
						"Storage benchmarking can be enabled with `--features runtime-benchmarks`."
							.into(),
					),
					#[cfg(feature = "runtime-benchmarks")]
					BenchmarkCmd::Storage(cmd) => {
						// ensure that we keep the task manager alive
                        let data_sources = config.tokio_handle.block_on(
                            crate::main_chain_follower::create_cached_main_chain_follower_data_sources(
                                cfg.midnight_cfg.clone(),
                                None,
                            ),
                        )?;
						// ensure that we keep the task manager alive
						let partial = service::new_partial(
                            &config,
                            epoch_config,
                            data_sources,
                            storage_config,
                        )?;
						let db = partial.backend.expose_db();
						let storage = partial.backend.expose_storage();

						cmd.run(config, partial.client, db, storage, None)
					},
					BenchmarkCmd::Overhead(cmd) => {
                        let data_sources = config.tokio_handle.block_on(
                            crate::main_chain_follower::create_cached_main_chain_follower_data_sources(
                                cfg.midnight_cfg.clone(),
                                None,
                            ),
                        )?;
						// ensure that we keep the task manager alive
						let partial = service::new_partial(
                            &config,
                            epoch_config,
                            data_sources,
                            storage_config,
                        )?;
						let ext_builder = RemarkBuilder::new(partial.client.clone());

						cmd.run(
							config.chain_spec.name().to_string(),
							partial.client,
							inherent_benchmark_data()?,
							Vec::new(),
							&ext_builder,
							false,
						)
					},
					BenchmarkCmd::Extrinsic(cmd) => {
						// ensure that we keep the task manager alive
                        let data_sources = config.tokio_handle.block_on(
                            crate::main_chain_follower::create_cached_main_chain_follower_data_sources(
                                cfg.midnight_cfg.clone(),
                                None,
                            ),
                        )?;
						let partial = service::new_partial(
                            &config,
                            epoch_config,
                            data_sources,
                            storage_config,
                        )?;
						// Register the *Remark* and *TKA* builders.
						let ext_factory = ExtrinsicFactory(vec![
							Box::new(RemarkBuilder::new(partial.client.clone())),
						]);

						cmd.run(
							partial.client,
							inherent_benchmark_data()?,
							Vec::new(),
							&ext_factory,
						)
					},
					BenchmarkCmd::Machine(cmd) =>
						cmd.run(&config, SUBSTRATE_REFERENCE_HARDWARE.clone()),
				}
			})
		},
		Subcommand::ChainInfo(ref cmd) => {
			let runner = cfg.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run::<Block>(&config))
		},
		Subcommand::GenerateCNightGenesis(ref cmd) => {
			// Init logging
			LoggerBuilder::new(std::env::var("RUST_LOG").unwrap_or("".to_string())).init()?;

			// Resolve default paths based on CFG_PRESET
			let res_dir = get_res_preset_dir();
			let cnight_addresses = cmd
				.cnight_addresses
				.clone()
				.unwrap_or_else(|| res_dir.join("cnight-addresses.json"));
			let output = cmd.output.clone().unwrap_or_else(|| res_dir.join("cnight-config.json"));

			// Init tokio runtime
			let tokio_handle = sc_cli::build_runtime()?;
			tokio_handle.block_on(async {
				let data_sources =
					crate::main_chain_follower::create_cnight_observation_data_source(
						cfg.midnight_cfg.clone(),
						None,
					)
					.await?;

				let cnight_addresses_str = std::fs::read_to_string(&cnight_addresses)?;
				let addresses: CNightAddresses = serde_json::from_str(&cnight_addresses_str)
					.map_err(|e| {
						sc_cli::Error::Input(format!(
							"failed to read cnight addresses file as json: {e:?}"
						))
					})?;
				generate_cnight_genesis(addresses, data_sources, cmd.cardano_tip.clone(), &output)
					.await
					.map_err(|e| {
						sc_cli::Error::Input(format!("cNGD genesis generation failed: {e}"))
					})?;

				Ok(())
			})
		},
		Subcommand::GenerateIcsGenesis(ref cmd) => {
			// Init logging
			LoggerBuilder::new(std::env::var("RUST_LOG").unwrap_or("".to_string())).init()?;

			// Resolve default paths based on CFG_PRESET
			let res_dir = get_res_preset_dir();
			let ics_addresses =
				cmd.ics_addresses.clone().unwrap_or_else(|| res_dir.join("ics-addresses.json"));
			let output = cmd.output.clone().unwrap_or_else(|| res_dir.join("ics-config.json"));

			// Init tokio runtime
			let tokio_handle = sc_cli::build_runtime()?;
			tokio_handle.block_on(async {
				let pool =
					crate::main_chain_follower::create_ics_genesis_pool(cfg.midnight_cfg.clone())
						.await?;

				let ics_addresses_str = std::fs::read_to_string(&ics_addresses)?;
				let addresses: IcsAddresses =
					serde_json::from_str(&ics_addresses_str).map_err(|e| {
						sc_cli::Error::Input(format!(
							"failed to read ICS addresses file as json: {e:?}"
						))
					})?;
				generate_ics_genesis(addresses, &pool, cmd.cardano_tip.clone(), &output)
					.await
					.map_err(|e| {
						sc_cli::Error::Input(format!("ICS genesis generation failed: {e}"))
					})?;

				Ok(())
			})
		},
		Subcommand::GenerateReserveGenesis(ref cmd) => {
			// Init logging
			LoggerBuilder::new(std::env::var("RUST_LOG").unwrap_or("".to_string())).init()?;

			// Resolve default paths based on CFG_PRESET
			let res_dir = get_res_preset_dir();
			let reserve_addresses = cmd
				.reserve_addresses
				.clone()
				.unwrap_or_else(|| res_dir.join("reserve-addresses.json"));
			let output = cmd.output.clone().unwrap_or_else(|| res_dir.join("reserve-config.json"));

			// Init tokio runtime
			let tokio_handle = sc_cli::build_runtime()?;
			tokio_handle.block_on(async {
				let pool =
					crate::main_chain_follower::create_ics_genesis_pool(cfg.midnight_cfg.clone())
						.await?;

				let reserve_addresses_str = std::fs::read_to_string(&reserve_addresses)?;
				let addresses: ReserveAddresses = serde_json::from_str(&reserve_addresses_str)
					.map_err(|e| {
						sc_cli::Error::Input(format!(
							"failed to read reserve addresses file as json: {e:?}"
						))
					})?;
				generate_reserve_genesis(addresses, &pool, cmd.cardano_tip.clone(), &output)
					.await
					.map_err(|e| {
						sc_cli::Error::Input(format!("Reserve genesis generation failed: {e}"))
					})?;

				Ok(())
			})
		},
		Subcommand::GenerateFederatedAuthorityGenesis(ref cmd) => {
			// Init logging
			LoggerBuilder::new(std::env::var("RUST_LOG").unwrap_or("".to_string())).init()?;

			// Resolve default paths based on CFG_PRESET
			let res_dir = get_res_preset_dir();
			let federated_authority_addresses = cmd
				.federated_authority_addresses
				.clone()
				.unwrap_or_else(|| res_dir.join("federated-authority-addresses.json"));
			let output = cmd
				.output
				.clone()
				.unwrap_or_else(|| res_dir.join("federated-authority-config.json"));

			// Init tokio runtime
			let tokio_handle = sc_cli::build_runtime()?;
			tokio_handle.block_on(async {
				let data_sources =
					crate::main_chain_follower::create_federated_authority_observation_data_source(
						cfg.midnight_cfg.clone(),
						None,
					)
					.await?;

				let fed_auth_addresses_str =
					std::fs::read_to_string(&federated_authority_addresses)?;
				let federated_authority_addresses: FederatedAuthorityAddresses =
					serde_json::from_str(&fed_auth_addresses_str).map_err(|e| {
						sc_cli::Error::Input(format!(
							"failed to read federated authority addresses file as json: {e}"
						))
					})?;

				generate_federated_authority_genesis(
					federated_authority_addresses,
					data_sources,
					cmd.cardano_tip.clone(),
					&output,
				)
				.await
				.map_err(|e| {
					sc_cli::Error::Input(format!(
						"federated authority genesis generation failed: {e}"
					))
				})?;

				Ok(())
			})
		},
		Subcommand::GeneratePermissionedCandidatesGenesis(ref cmd) => {
			// Init logging
			LoggerBuilder::new(std::env::var("RUST_LOG").unwrap_or("".to_string())).init()?;

			// Resolve default paths based on CFG_PRESET
			let res_dir = get_res_preset_dir();
			let permissioned_candidates_addresses = cmd
				.permissioned_candidates_addresses
				.clone()
				.unwrap_or_else(|| res_dir.join("permissioned-candidates-addresses.json"));
			let pc_config_path =
				cmd.pc_config.clone().unwrap_or_else(|| res_dir.join("pc-chain-config.json"));
			let output = cmd
				.output
				.clone()
				.unwrap_or_else(|| res_dir.join("permissioned-candidates-config.json"));

			// Read security_parameter from pc-chain-config.json if env var is not set
			let mut midnight_cfg = cfg.midnight_cfg.clone();
			if midnight_cfg.cardano_security_parameter.is_none() {
				let pc_config_str = std::fs::read_to_string(&pc_config_path).map_err(|e| {
					sc_cli::Error::Input(format!(
						"failed to read pc-chain-config.json at {}: {e}",
						pc_config_path.display()
					))
				})?;
				let pc_config: PcChainConfig =
					serde_json::from_str(&pc_config_str).map_err(|e| {
						sc_cli::Error::Input(format!("failed to parse pc-chain-config.json: {e}"))
					})?;
				midnight_cfg.cardano_security_parameter =
					Some(pc_config.cardano.security_parameter);
				log::info!(
					"Using security_parameter={} from {}",
					pc_config.cardano.security_parameter,
					pc_config_path.display()
				);
			}

			// Init tokio runtime
			let tokio_handle = sc_cli::build_runtime()?;
			tokio_handle.block_on(async {
				let (data_source, pool) =
					crate::main_chain_follower::create_authority_selection_data_source_with_pool(
						midnight_cfg,
						None,
					)
					.await?;

				// Get the epoch number for the given cardano tip
				let epoch = midnight_primitives_mainchain_follower::get_epoch_for_block_hash(
					&pool,
					&cmd.cardano_tip,
				)
				.await
				.map_err(|e| {
					sc_cli::Error::Input(format!("failed to get epoch for block hash: {e}"))
				})?
				.ok_or_else(|| {
					sc_cli::Error::Input(format!(
						"block hash {} not found in db-sync",
						cmd.cardano_tip
					))
				})?;

				log::info!("Resolved cardano tip {} to epoch {}", cmd.cardano_tip, epoch.0);

				let addresses_str = std::fs::read_to_string(&permissioned_candidates_addresses)?;
				let addresses: PermissionedCandidatesAddresses =
					serde_json::from_str(&addresses_str).map_err(|e| {
						sc_cli::Error::Input(format!(
							"failed to read permissioned candidates addresses file as json: {e}"
						))
					})?;

				generate_permissioned_candidates_genesis(addresses, data_source, epoch, &output)
					.await
					.map_err(|e| {
						sc_cli::Error::Input(format!(
							"permissioned candidates genesis generation failed: {e}"
						))
					})?;

				Ok(())
			})
		},
		Subcommand::GenerateGenesisConfig(ref cmd) => {
			// Init logging
			LoggerBuilder::new(std::env::var("RUST_LOG").unwrap_or("".to_string())).init()?;

			// Resolve default paths based on CFG_PRESET
			let res_dir = get_res_preset_dir();

			// cNight paths
			let cnight_addresses = cmd
				.cnight_addresses
				.clone()
				.unwrap_or_else(|| res_dir.join("cnight-addresses.json"));
			let cnight_output =
				cmd.cnight_output.clone().unwrap_or_else(|| res_dir.join("cnight-config.json"));

			// Federated authority paths
			let federated_authority_addresses = cmd
				.federated_authority_addresses
				.clone()
				.unwrap_or_else(|| res_dir.join("federated-authority-addresses.json"));
			let federated_authority_output = cmd
				.federated_authority_output
				.clone()
				.unwrap_or_else(|| res_dir.join("federated-authority-config.json"));

			// Permissioned candidates paths
			let permissioned_candidates_addresses = cmd
				.permissioned_candidates_addresses
				.clone()
				.unwrap_or_else(|| res_dir.join("permissioned-candidates-addresses.json"));
			let pc_config_path =
				cmd.pc_config.clone().unwrap_or_else(|| res_dir.join("pc-chain-config.json"));
			let permissioned_candidates_output = cmd
				.permissioned_candidates_output
				.clone()
				.unwrap_or_else(|| res_dir.join("permissioned-candidates-config.json"));

			// Read security_parameter from pc-chain-config.json if env var is not set
			let mut midnight_cfg_for_perm_cand = cfg.midnight_cfg.clone();
			if midnight_cfg_for_perm_cand.cardano_security_parameter.is_none() {
				let pc_config_str = std::fs::read_to_string(&pc_config_path).map_err(|e| {
					sc_cli::Error::Input(format!(
						"failed to read pc-chain-config.json at {}: {e}",
						pc_config_path.display()
					))
				})?;
				let pc_config: PcChainConfig =
					serde_json::from_str(&pc_config_str).map_err(|e| {
						sc_cli::Error::Input(format!("failed to parse pc-chain-config.json: {e}"))
					})?;
				midnight_cfg_for_perm_cand.cardano_security_parameter =
					Some(pc_config.cardano.security_parameter);
				log::info!(
					"Using security_parameter={} from {}",
					pc_config.cardano.security_parameter,
					pc_config_path.display()
				);
			}

			// Init tokio runtime
			let tokio_handle = sc_cli::build_runtime()?;
			tokio_handle.block_on(async {
				// 1. Generate cNight genesis
				log::info!("Generating cNight genesis config...");
				let cnight_data_source =
					crate::main_chain_follower::create_cnight_observation_data_source(
						cfg.midnight_cfg.clone(),
						None,
					)
					.await?;

				let cnight_addresses_str = std::fs::read_to_string(&cnight_addresses)?;
				let cnight_addresses_parsed: CNightAddresses =
					serde_json::from_str(&cnight_addresses_str).map_err(|e| {
						sc_cli::Error::Input(format!(
							"failed to read cnight addresses file as json: {e:?}"
						))
					})?;

				generate_cnight_genesis(
					cnight_addresses_parsed,
					cnight_data_source,
					cmd.cardano_tip.clone(),
					&cnight_output,
				)
				.await
				.map_err(|e| {
					sc_cli::Error::Input(format!("cNight genesis generation failed: {e}"))
				})?;

				// 2. Generate federated authority genesis
				log::info!("Generating federated authority genesis config...");
				let fed_auth_data_source =
					crate::main_chain_follower::create_federated_authority_observation_data_source(
						cfg.midnight_cfg.clone(),
						None,
					)
					.await?;

				let fed_auth_addresses_str =
					std::fs::read_to_string(&federated_authority_addresses)?;
				let fed_auth_addresses_parsed: FederatedAuthorityAddresses =
					serde_json::from_str(&fed_auth_addresses_str).map_err(|e| {
						sc_cli::Error::Input(format!(
							"failed to read federated authority addresses file as json: {e}"
						))
					})?;

				generate_federated_authority_genesis(
					fed_auth_addresses_parsed,
					fed_auth_data_source,
					cmd.cardano_tip.clone(),
					&federated_authority_output,
				)
				.await
				.map_err(|e| {
					sc_cli::Error::Input(format!(
						"federated authority genesis generation failed: {e}"
					))
				})?;

				// 3. Generate permissioned candidates genesis
				log::info!("Generating permissioned candidates genesis config...");
				let (perm_cand_data_source, pool) =
					crate::main_chain_follower::create_authority_selection_data_source_with_pool(
						midnight_cfg_for_perm_cand,
						None,
					)
					.await?;

				// Get the epoch number for the given cardano tip
				let epoch = midnight_primitives_mainchain_follower::get_epoch_for_block_hash(
					&pool,
					&cmd.cardano_tip,
				)
				.await
				.map_err(|e| {
					sc_cli::Error::Input(format!("failed to get epoch for block hash: {e}"))
				})?
				.ok_or_else(|| {
					sc_cli::Error::Input(format!(
						"block hash {} not found in db-sync",
						cmd.cardano_tip
					))
				})?;

				log::info!("Resolved cardano tip {} to epoch {}", cmd.cardano_tip, epoch.0);

				let perm_cand_addresses_str =
					std::fs::read_to_string(&permissioned_candidates_addresses)?;
				let perm_cand_addresses_parsed: PermissionedCandidatesAddresses =
					serde_json::from_str(&perm_cand_addresses_str).map_err(|e| {
						sc_cli::Error::Input(format!(
							"failed to read permissioned candidates addresses file as json: {e}"
						))
					})?;

				generate_permissioned_candidates_genesis(
					perm_cand_addresses_parsed,
					perm_cand_data_source,
					epoch,
					&permissioned_candidates_output,
				)
				.await
				.map_err(|e| {
					sc_cli::Error::Input(format!(
						"permissioned candidates genesis generation failed: {e}"
					))
				})?;

				// 4. Generate reserve genesis
				log::info!("Generating reserve genesis config...");
				let reserve_addresses = cmd
					.reserve_addresses
					.clone()
					.unwrap_or_else(|| res_dir.join("reserve-addresses.json"));
				let reserve_output = cmd
					.reserve_output
					.clone()
					.unwrap_or_else(|| res_dir.join("reserve-config.json"));

				let reserve_pool =
					crate::main_chain_follower::create_ics_genesis_pool(cfg.midnight_cfg.clone())
						.await?;

				let reserve_addresses_str = std::fs::read_to_string(&reserve_addresses)?;
				let reserve_addresses_parsed: ReserveAddresses =
					serde_json::from_str(&reserve_addresses_str).map_err(|e| {
						sc_cli::Error::Input(format!(
							"failed to read reserve addresses file as json: {e:?}"
						))
					})?;

				generate_reserve_genesis(
					reserve_addresses_parsed,
					&reserve_pool,
					cmd.cardano_tip.clone(),
					&reserve_output,
				)
				.await
				.map_err(|e| {
					sc_cli::Error::Input(format!("reserve genesis generation failed: {e}"))
				})?;

				log::info!("All genesis config files generated successfully!");
				Ok(())
			})
		},
		Subcommand::VerifyLedgerStateGenesis(ref cmd) => {
			// Init logging
			LoggerBuilder::new(std::env::var("RUST_LOG").unwrap_or("".to_string())).init()?;

			let result = verify_ledger_state_genesis::verify_ledger_state_genesis(
				&cmd.chain_spec,
				cmd.cnight_config.as_deref(),
				cmd.ledger_parameters_config.as_deref(),
				cmd.network.as_deref(),
			)
			.map_err(|e| sc_cli::Error::Input(format!("Genesis verification failed: {e}")))?;

			result.print_summary();

			if result.all_passed() {
				Ok(())
			} else {
				Err(sc_cli::Error::Input("Some verification checks failed".to_string()))
			}
		},
		Subcommand::VerifyCardanoTipFinalized(ref cmd) => {
			// Init logging
			LoggerBuilder::new(std::env::var("RUST_LOG").unwrap_or("".to_string())).init()?;

			// Resolve default paths based on CFG_PRESET
			let res_dir = get_res_preset_dir();
			let pc_config_path =
				cmd.pc_config.clone().unwrap_or_else(|| res_dir.join("pc-chain-config.json"));

			// Load security_parameter from pc-chain-config.json
			let pc_config_content = std::fs::read_to_string(&pc_config_path).map_err(|e| {
				sc_cli::Error::Input(format!("Failed to read {}: {}", pc_config_path.display(), e))
			})?;
			let pc_config: PcChainConfig =
				serde_json::from_str(&pc_config_content).map_err(|e| {
					sc_cli::Error::Input(format!(
						"Failed to parse {}: {}",
						pc_config_path.display(),
						e
					))
				})?;
			let security_parameter = pc_config.cardano.security_parameter;
			log::info!(
				"Using security_parameter={} from {}",
				security_parameter,
				pc_config_path.display()
			);

			// Init tokio runtime
			let tokio_handle = sc_cli::build_runtime()?;
			tokio_handle.block_on(async {
				let pool =
					crate::main_chain_follower::create_ics_genesis_pool(cfg.midnight_cfg.clone())
						.await?;

				// Get the block number for the provided tip
				let tip_block_number =
					verify_auth_script_common::get_block_number(&pool, &cmd.cardano_tip)
						.await
						.map_err(|e| {
							sc_cli::Error::Input(format!(
								"Failed to get block number for tip {}: {}",
								cmd.cardano_tip, e
							))
						})?;

				// Get the latest block number from db-sync
				let latest_block_number: (i32,) = sqlx::query_as(
					r#"
					SELECT block_no
					FROM block
					WHERE block_no IS NOT NULL
					ORDER BY block_no DESC
					LIMIT 1
					"#,
				)
				.fetch_one(&pool)
				.await
				.map_err(|e| {
					sc_cli::Error::Input(format!("Failed to get latest block number: {}", e))
				})?;

				let confirmations = latest_block_number.0 as u32 - tip_block_number;
				let is_finalized = confirmations >= security_parameter;

				println!("\n=== Cardano Tip Finalization Check ===\n");
				println!("Cardano tip:          {}", cmd.cardano_tip);
				println!("Tip block number:     {}", tip_block_number);
				println!("Latest block number:  {}", latest_block_number.0);
				println!("Confirmations:        {}", confirmations);
				println!("Security parameter:   {}", security_parameter);
				println!();

				if is_finalized {
					println!(
						"RESULT: FINALIZED (confirmations {} >= security_parameter {})",
						confirmations, security_parameter
					);
					Ok(())
				} else {
					println!(
						"RESULT: NOT FINALIZED (confirmations {} < security_parameter {})",
						confirmations, security_parameter
					);
					Err(sc_cli::Error::Input(format!(
						"Block is not finalized: {} confirmations < {} security_parameter",
						confirmations, security_parameter
					)))
				}
			})
		},
		Subcommand::VerifyAuthScript(ref cmd) => {
			// Init logging
			LoggerBuilder::new(std::env::var("RUST_LOG").unwrap_or("".to_string())).init()?;

			// Resolve default paths based on CFG_PRESET
			let res_dir = get_res_preset_dir();
			let federated_authority_addresses = cmd
				.federated_authority_addresses
				.clone()
				.unwrap_or_else(|| res_dir.join("federated-authority-addresses.json"));
			let ics_addresses =
				cmd.ics_addresses.clone().unwrap_or_else(|| res_dir.join("ics-addresses.json"));
			let permissioned_candidates_addresses = cmd
				.permissioned_candidates_addresses
				.clone()
				.unwrap_or_else(|| res_dir.join("permissioned-candidates-addresses.json"));
			let authorization_addresses = cmd
				.authorization_addresses
				.clone()
				.unwrap_or_else(|| res_dir.join("authorization-addresses.json"));

			// Init tokio runtime
			let tokio_handle = sc_cli::build_runtime()?;
			tokio_handle.block_on(async {
				let pool =
					crate::main_chain_follower::create_ics_genesis_pool(cfg.midnight_cfg.clone())
						.await?;

				let mut all_passed = true;

				// 1. Verify Federated Authority
				let fa_result =
					verify_federated_authority_auth_script::verify_federated_authority_auth_script(
						&federated_authority_addresses,
						Some(&authorization_addresses),
						&pool,
						&cmd.cardano_tip,
					)
					.await
					.map_err(|e| {
						sc_cli::Error::Input(format!(
							"Federated authority auth script verification failed: {e}"
						))
					})?;
				fa_result.print_summary();
				if !fa_result.all_passed() {
					all_passed = false;
				}

				// 2. Verify ICS
				let ics_result = verify_ics_auth_script::verify_ics_auth_script(
					&ics_addresses,
					Some(&authorization_addresses),
					&pool,
					&cmd.cardano_tip,
				)
				.await
				.map_err(|e| {
					sc_cli::Error::Input(format!("ICS auth script verification failed: {e}"))
				})?;
				ics_result.print_summary();
				if !ics_result.all_passed() {
					all_passed = false;
				}

				// 3. Verify Permissioned Candidates
				let pc_result =
					verify_permissioned_candidates_auth_script::verify_permissioned_candidates_auth_script(
						&permissioned_candidates_addresses,
						Some(&authorization_addresses),
						&pool,
						&cmd.cardano_tip,
					)
					.await
					.map_err(|e| {
						sc_cli::Error::Input(format!(
							"Permissioned candidates auth script verification failed: {e}"
						))
					})?;
				pc_result.print_summary();
				if !pc_result.all_passed() {
					all_passed = false;
				}

				println!("\n=== Overall Auth Script Verification ===\n");
				if all_passed {
					println!("RESULT: ALL CHECKS PASSED");
					Ok(())
				} else {
					println!("RESULT: SOME CHECKS FAILED");
					Err(sc_cli::Error::Input("Some verification checks failed".to_string()))
				}
			})
		},
		Subcommand::VerifyFederatedAuthorityAuthScript(ref cmd) => {
			// Init logging
			LoggerBuilder::new(std::env::var("RUST_LOG").unwrap_or("".to_string())).init()?;

			// Resolve default paths based on CFG_PRESET
			let res_dir = get_res_preset_dir();
			let federated_authority_addresses = cmd
				.federated_authority_addresses
				.clone()
				.unwrap_or_else(|| res_dir.join("federated-authority-addresses.json"));
			let authorization_addresses = cmd
				.authorization_addresses
				.clone()
				.unwrap_or_else(|| res_dir.join("authorization-addresses.json"));

			// Init tokio runtime
			let tokio_handle = sc_cli::build_runtime()?;
			tokio_handle.block_on(async {
				let pool =
					crate::main_chain_follower::create_ics_genesis_pool(cfg.midnight_cfg.clone())
						.await?;

				let result =
					verify_federated_authority_auth_script::verify_federated_authority_auth_script(
						&federated_authority_addresses,
						Some(&authorization_addresses),
						&pool,
						&cmd.cardano_tip,
					)
					.await
					.map_err(|e| {
						sc_cli::Error::Input(format!(
							"Federated authority auth script verification failed: {e}"
						))
					})?;

				result.print_summary();

				if result.all_passed() {
					Ok(())
				} else {
					Err(sc_cli::Error::Input("Some verification checks failed".to_string()))
				}
			})
		},
		Subcommand::VerifyIcsAuthScript(ref cmd) => {
			// Init logging
			LoggerBuilder::new(std::env::var("RUST_LOG").unwrap_or("".to_string())).init()?;

			// Resolve default paths based on CFG_PRESET
			let res_dir = get_res_preset_dir();
			let ics_addresses =
				cmd.ics_addresses.clone().unwrap_or_else(|| res_dir.join("ics-addresses.json"));
			let authorization_addresses = cmd
				.authorization_addresses
				.clone()
				.unwrap_or_else(|| res_dir.join("authorization-addresses.json"));

			// Init tokio runtime
			let tokio_handle = sc_cli::build_runtime()?;
			tokio_handle.block_on(async {
				let pool =
					crate::main_chain_follower::create_ics_genesis_pool(cfg.midnight_cfg.clone())
						.await?;

				let result = verify_ics_auth_script::verify_ics_auth_script(
					&ics_addresses,
					Some(&authorization_addresses),
					&pool,
					&cmd.cardano_tip,
				)
				.await
				.map_err(|e| {
					sc_cli::Error::Input(format!("ICS auth script verification failed: {e}"))
				})?;

				result.print_summary();

				if result.all_passed() {
					Ok(())
				} else {
					Err(sc_cli::Error::Input("Some verification checks failed".to_string()))
				}
			})
		},
		Subcommand::VerifyPermissionedCandidatesAuthScript(ref cmd) => {
			// Init logging
			LoggerBuilder::new(std::env::var("RUST_LOG").unwrap_or("".to_string())).init()?;

			// Resolve default paths based on CFG_PRESET
			let res_dir = get_res_preset_dir();
			let permissioned_candidates_addresses = cmd
				.permissioned_candidates_addresses
				.clone()
				.unwrap_or_else(|| res_dir.join("permissioned-candidates-addresses.json"));
			let authorization_addresses = cmd
				.authorization_addresses
				.clone()
				.unwrap_or_else(|| res_dir.join("authorization-addresses.json"));

			// Init tokio runtime
			let tokio_handle = sc_cli::build_runtime()?;
			tokio_handle.block_on(async {
				let pool =
					crate::main_chain_follower::create_ics_genesis_pool(cfg.midnight_cfg.clone())
						.await?;

				let result =
					verify_permissioned_candidates_auth_script::verify_permissioned_candidates_auth_script(
						&permissioned_candidates_addresses,
						Some(&authorization_addresses),
						&pool,
						&cmd.cardano_tip,
					)
					.await
					.map_err(|e| {
						sc_cli::Error::Input(format!(
							"Permissioned candidates auth script verification failed: {e}"
						))
					})?;

				result.print_summary();

				if result.all_passed() {
					Ok(())
				} else {
					Err(sc_cli::Error::Input("Some verification checks failed".to_string()))
				}
			})
		},
	}
}
