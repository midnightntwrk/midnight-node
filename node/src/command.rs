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

use crate::cfg::Cfg;
use crate::{
	cli::{self, Cli, Subcommand},
	cnight_genesis::generate_cnight_genesis,
	service::{self, StorageInit},
};
use clap::Parser;
use midnight_node_res::networks::MidnightNetwork as _;
use midnight_node_runtime::Block;
use midnight_primitives_cnight_observation::CNightAddresses;
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
		//For litep2p use `sc_network::Litep2pNetworkBackend<_, _>``
		service::new_full::<sc_network::NetworkWorker<_, _>>(
			config,
			epoch_config,
			data_sources,
			cfg.storage_monitor_params_cfg.into(),
			storage_config,
		)
		.await
		.map_err(sc_cli::Error::Service)
	})
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
			// Init tokio runtime
			let tokio_handle = sc_cli::build_runtime()?;
			tokio_handle.block_on(async {
				let data_sources =
					crate::main_chain_follower::create_cnight_observation_data_source(
						cfg.midnight_cfg.clone(),
						None,
					)
					.await?;

				let cnight_addresses_str = std::fs::read_to_string(&cmd.cnight_addresses)?;
				let addresses: CNightAddresses = serde_json::from_str(&cnight_addresses_str)
					.map_err(|e| {
						sc_cli::Error::Input(format!(
							"failed to read cnight addresses file as json: {e:?}"
						))
					})?;
				generate_cnight_genesis(
					addresses,
					data_sources,
					cmd.cardano_tip.clone(),
					&cmd.output.clone(),
				)
				.await
				.map_err(|e| {
					sc_cli::Error::Input(format!("cNGD genesis generation failed: {e}"))
				})?;

				Ok(())
			})
		},
	}
}
