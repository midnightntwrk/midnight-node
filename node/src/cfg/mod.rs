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

use std::collections::BTreeMap;

use config::{Config, ConfigError, Environment, File, FileFormat};
use documented::FieldInfo;
use midnight_node_res::{
	default_cfg,
	networks::{
		CustomNetwork, InitialAuthorityData, InitialFederedatedAuthority, MainChainScripts,
		UndeployedNetwork,
	},
};
use pallet_cnight_observation::config::CNightGenesis;
use sc_cli::SubstrateCli;
use serde_valid::Validate as _;

use crate::chain_spec::{ChainSpecInitError, chain_config};

use self::{
	chain_spec_cfg::ChainSpecCfg, error::CfgError, meta_cfg::MetaCfg, midnight_cfg::MidnightCfg,
	shell_words_environment::ShellWordsEnvironment,
	storage_monitor_params_cfg::StorageMonitorParamsCfg, substrate_cfg::SubstrateCfg,
};

type CfgSourcesMap = BTreeMap<&'static str, config::Config>;

pub mod addresses;
pub mod chain_spec_cfg;
pub mod meta_cfg;
pub mod midnight_cfg;
pub mod storage_monitor_params_cfg;
pub mod substrate_cfg;
mod validation_utils;

pub mod error;
pub mod shell_words_environment;
pub(crate) mod util;

/// Contains all configuration for the node application
#[derive(Debug, Default)]
pub struct Cfg {
	pub config: Config,
	/// Configuration required to initialize the chainspec
	pub chain_spec_cfg: ChainSpecCfg,
	/// Used to select a meta configuration (preset and show_config)
	pub meta_cfg: MetaCfg,
	/// Configuration specific to Midnight
	pub midnight_cfg: MidnightCfg,
	/// A duplicate of `StorageMonitorParams`, instantiated using environment variables
	/// For the `StorageMonitorParams` implementation, see:
	/// polkadot-sdk/substrate/client/storage-monitor/src/lib.rs
	pub storage_monitor_params_cfg: StorageMonitorParamsCfg,
	/// Stores an argv used when no cli is specified
	pub substrate_cfg: SubstrateCfg,
}

impl SubstrateCli for Cfg {
	fn impl_name() -> String {
		"Midnight Node".into()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		env!("CARGO_PKG_DESCRIPTION").into()
	}

	fn author() -> String {
		env!("CARGO_PKG_AUTHORS").into()
	}

	fn support_url() -> String {
		"support.anonymous.an".into()
	}

	fn copyright_start_year() -> i32 {
		2017
	}

	fn load_spec(&self, id: &str) -> Result<Box<dyn sc_service::ChainSpec>, String> {
		self.chain_spec_cfg
			.validate()
			.map_err(|e| format!("chainspec config failed to validate: {e}"))?;
		let maybe_chain_spec = match id {
			"" => {
				// Midnight-specific pre-generated genesis values
				let genesis_block =
					std::fs::read(self.chain_spec_cfg.chainspec_genesis_block.as_ref().unwrap())
						.map_err(|e| format!("failed to read genesis_block: {e}"))?;
				let genesis_state =
					std::fs::read(self.chain_spec_cfg.chainspec_genesis_state.as_ref().unwrap())
						.map_err(|e| format!("failed to read genesis_state: {e}"))?;

				let pc_chain_config_str = std::fs::read_to_string(
					self.chain_spec_cfg.chainspec_pc_chain_config.as_ref().unwrap(),
				)
				.map_err(|e| format!("failed to read pc chain config: {e}"))?;

				let pc_chain_config: serde_json::Value = serde_json::from_str(&pc_chain_config_str)
					.map_err(|e| format!("failed to read pc_chain_config as json: {e}"))?;

				let initial_authorities =
					InitialAuthorityData::load_from_pc_chain_config(&pc_chain_config);

				let cnight_genesis_str = std::fs::read_to_string(
					self.chain_spec_cfg.chainspec_cnight_genesis.as_ref().unwrap(),
				)
				.map_err(|e| format!("failed to read cnight-genesis: {e}"))?;

				let cnight_genesis: CNightGenesis = serde_json::from_str(&cnight_genesis_str)
					.map_err(|e| format!("failed to read cnight-genesis as json: {e}"))?;

				let main_chain_scripts =
					MainChainScripts::load_from_pc_chain_config(&pc_chain_config);

				let genesis_utxo = pc_chain_config
					.get("chain_parameters")
					.and_then(|v| v.get("genesis_utxo").and_then(|v| v.as_str()))
					.ok_or("failed to find genesis_utxo in pc_chain_config".to_string())?;

				let federated_authority_config_str = std::fs::read_to_string(
					self.chain_spec_cfg.chainspec_federated_authority_config.as_ref().unwrap(),
				)
				.map_err(|e| format!("failed to read federated_authority: {e}"))?;

				let federated_authority_config: serde_json::Value =
					serde_json::from_str(&federated_authority_config_str).map_err(|e| {
						format!("failed to read federated_authority_config as json: {e}")
					})?;

				let council_membership =
					InitialFederedatedAuthority::load_from_federated_authority_config(
						&federated_authority_config,
						"council",
					);

				let technical_committee_membership =
					InitialFederedatedAuthority::load_from_federated_authority_config(
						&federated_authority_config,
						"technical_committee",
					);

				let network: CustomNetwork = CustomNetwork {
					name: self.chain_spec_cfg.chainspec_name.as_ref().unwrap().clone(),
					id: self.chain_spec_cfg.chainspec_id.as_ref().unwrap().clone(),
					genesis_block,
					genesis_state,
					initial_authorities,
					cnight_genesis,
					chain_type: self.chain_spec_cfg.chainspec_chain_type.as_ref().unwrap().clone(),
					council_membership,
					technical_committee_membership,
					main_chain_scripts,
					genesis_utxo: genesis_utxo.to_string(),
				};
				chain_config(network)
			},
			"local" | "dev" => chain_config(UndeployedNetwork),
			path => crate::chain_spec::ChainSpec::from_json_file(std::path::PathBuf::from(path))
				.map_err(|err| ChainSpecInitError::ParseError(err.to_string())),
		};

		match maybe_chain_spec {
			Ok(chain_spec) => Ok(Box::new(chain_spec)),
			Err(e) => Err(e.to_string()),
		}
	}
}

impl Cfg {
	/// Create a new instance from all sources and run validation
	pub fn new() -> Result<Self, CfgError> {
		let cfg = Self::new_no_validation()?;
		cfg.validate()?;
		Ok(cfg)
	}

	/// Create a new instance from all sources without running validation
	pub fn new_no_validation() -> Result<Self, CfgError> {
		let config = Self::get_all_config()?;
		let cfg = Self::new_no_validation_from_config(config)?;
		Ok(cfg)
	}

	/// Create a new instance from a custom config without running validation
	pub fn new_no_validation_from_config(config: config::Config) -> Result<Self, ConfigError> {
		let chain_spec_cfg: ChainSpecCfg = config.clone().try_deserialize()?;
		let meta_cfg: MetaCfg = config.clone().try_deserialize()?;
		let midnight_cfg: MidnightCfg = config.clone().try_deserialize()?;
		let storage_monitor_params_cfg: StorageMonitorParamsCfg =
			config.clone().try_deserialize()?;
		let substrate_cfg: SubstrateCfg = config.clone().try_deserialize()?;

		let cfg = Self {
			config,
			meta_cfg,
			midnight_cfg,
			substrate_cfg,
			storage_monitor_params_cfg,
			chain_spec_cfg,
		};

		Ok(cfg)
	}

	fn get_env_source() -> Result<impl config::Source, ConfigError> {
		Config::builder()
			.add_source(Environment::default())
			.add_source(ShellWordsEnvironment::new(&["args", "append_args", "bootnodes"]))
			.build()
	}

	/// For high-level validation between configuration fields.
	fn validate(&self) -> Result<(), CfgError> {
		self.chain_spec_cfg
			.validate()
			.map_err(|e| ConfigError::Message(e.to_string()))?;
		self.meta_cfg.validate().map_err(|e| ConfigError::Message(e.to_string()))?;
		self.midnight_cfg.validate().map_err(|e| ConfigError::Message(e.to_string()))?;
		self.storage_monitor_params_cfg
			.validate()
			.map_err(|e| ConfigError::Message(e.to_string()))?;
		self.substrate_cfg.validate().map_err(|e| ConfigError::Message(e.to_string()))?;
		Ok(())
	}

	/// Includes configuration from ONLY the DEFAULT set
	pub fn get_default_config() -> Result<Config, ConfigError> {
		Config::builder()
			.add_source(File::from_str(&default_cfg(), FileFormat::Toml))
			.build()
	}

	/// Includes configuration from ONLY the command line arguments
	pub fn get_cli_config() -> Result<Config, ConfigError> {
		let mut cfg = config::ConfigBuilder::<config::builder::DefaultState>::default();

		let argv: Vec<String> = std::env::args().collect();
		if argv.len() > 1 {
			cfg = cfg.set_default("args", argv[1..].to_vec())?;
		}

		cfg.build()
	}

	/// Includes configuration from ONLY the env preset
	pub fn get_preset_config() -> Result<Config, ConfigError> {
		let meta_cfg: MetaCfg = Config::builder()
			.add_source(File::from_str(&default_cfg(), FileFormat::Toml))
			.add_source(Self::get_env_source()?)
			.build()?
			.try_deserialize()?;

		let mut builder = Config::builder();
		if let Some(ref env_preset) = meta_cfg.cfg_preset {
			builder = builder.add_source(env_preset.load_config()?);
		}
		builder.build()
	}

	/// Includes configuration from the DEFAULT set, the env preset, and environment variables
	pub fn get_all_config() -> Result<Config, ConfigError> {
		let preset_cfg: MetaCfg = Config::builder()
			.add_source(File::from_str(&default_cfg(), FileFormat::Toml))
			.add_source(Self::get_env_source()?)
			.build()?
			.try_deserialize()?;

		let mut builder =
			Config::builder().add_source(File::from_str(&default_cfg(), FileFormat::Toml));
		if let Some(ref env_preset) = preset_cfg.cfg_preset {
			builder = builder.add_source(env_preset.load_config()?);
		}
		builder
			.add_source(Self::get_env_source()?)
			.add_source(Self::get_cli_config()?)
			.build()
	}

	pub fn get_sources() -> Result<CfgSourcesMap, ConfigError> {
		// TODO: Add more environment variable sources with different prefixes
		// e.g:
		// CONTAINER_CHAIN_ID
		// HELM_CHAIN_ID
		// VENDOR_CHAIN_ID (?)
		Ok(BTreeMap::from([
			("default", Self::get_default_config()?),
			("preset", Self::get_preset_config()?),
			("cli", Self::get_cli_config()?),
			("env-vars", Config::builder().add_source(Self::get_env_source()?).build()?),
		]))
	}

	fn render_header<T: std::io::Write>(mut buf: T, header: &str) -> Result<(), std::io::Error> {
		writeln!(buf)?;
		writeln!(buf, "{}", "=".repeat(80))?;
		writeln!(buf, "{header}")?;
		writeln!(buf, "{}", "=".repeat(80))?;
		Ok(())
	}

	fn render_fields<T: std::io::Write>(
		mut buf: T,
		show_secrets: bool,
		fields: &[HelpField],
	) -> Result<(), CfgError> {
		for field in fields {
			render_help_field(&mut buf, show_secrets, field)?;
		}
		Ok(())
	}

	pub fn render_help<T: std::io::Write>(mut buf: T) -> Result<(), CfgError> {
		let all_config = Self::get_all_config()?;
		let meta_cfg: MetaCfg = all_config.clone().try_deserialize().unwrap();
		let show_secrets = meta_cfg.show_secrets;

		Self::render_header(&mut buf, "ChainSpecCfg")?;
		Self::render_fields(&mut buf, show_secrets, &ChainSpecCfg::help(Some(&all_config))?)?;
		Self::render_header(&mut buf, "MetaCfg")?;
		Self::render_fields(&mut buf, show_secrets, &MetaCfg::help(Some(&all_config))?)?;
		Self::render_header(&mut buf, "MidnightCfg")?;
		Self::render_fields(&mut buf, show_secrets, &MidnightCfg::help(Some(&all_config))?)?;
		Self::render_header(&mut buf, "StorageMonitorParamsCfg")?;
		Self::render_fields(
			&mut buf,
			show_secrets,
			&StorageMonitorParamsCfg::help(Some(&all_config))?,
		)?;
		Self::render_header(&mut buf, "SubstrateCfg")?;
		Self::render_fields(&mut buf, show_secrets, &SubstrateCfg::help(Some(&all_config))?)?;

		writeln!(buf)?;
		writeln!(buf, "CONFIG PRESET: {:?}", meta_cfg.cfg_preset)?;
		match Cfg::new() {
			Ok(_) => writeln!(buf, "VALIDATION RESULT: Configuration validated successfully!")?,
			Err(e) => writeln!(buf, "VALIDATION RESULT: Configuration failed to validate: {e}")?,
		}
		if !show_secrets {
			writeln!(buf, "*note:* To show secret values, set SHOW_SECRETS=1")?;
		}
		Ok(())
	}

	pub fn help() {
		let mut buf = Vec::new();
		Self::render_help(&mut buf).unwrap();
		eprintln!("{}", String::from_utf8_lossy(&buf));
	}
}

pub(crate) trait CfgHelp {
	fn help(cur_cfg: Option<&config::Config>) -> Result<Vec<HelpField>, CfgError>;
}

/// Most common implementation for CfgHelp
macro_rules! cfg_help {
	($cur_cfg:ident, $t:ty) => {{
		let docs = Self::field_docs();
		let serde_keys = get_keys(Self::default())?;
		let mut help_fields = Vec::new();
		for (key, mut info) in serde_keys.iter().zip(docs) {
			info.name = key.to_string();
			let current_value = $cur_cfg.map(|c| {
				let value: Option<config::Value> = c.get(&info.name).ok();
				value.map(|v| format!("{}", v))
			});
			let field = HelpField { current_value, info };
			help_fields.push(field);
		}
		Ok(help_fields)
	}};
}
pub(crate) use cfg_help;

pub(crate) struct HelpField {
	current_value: Option<Option<String>>,
	info: FieldInfo,
}

/// Renders the help for each configuration field.
fn render_help_field<T: std::io::Write>(
	mut f: T,
	show_secrets: bool,
	field: &HelpField,
) -> Result<(), CfgError> {
	let HelpField { info: FieldInfo { name, doc, field_type, tags }, current_value } = field;

	let pad = 15;

	let default_cfg = Cfg::get_default_config()?;
	let default_value = default_cfg.get_string(name).unwrap_or_default();
	let doc = doc.replace('\n', &format!("\n{}", " ".repeat(pad)));
	writeln!(f)?;
	writeln!(f, "{:<pad$}{name}", "NAME: ")?;
	writeln!(f, "{:<pad$}{doc}", "HELP: ")?;
	writeln!(f, "{:<pad$}{field_type}", "TYPE: ")?;
	writeln!(f, "{:<pad$}{default_value}", "DEFAULT: ")?;
	if let Some(cur_value) = current_value {
		let sources: String = Cfg::get_sources()?
			.iter()
			.fold(Vec::new(), |mut sources, (source_name, cfg)| {
				let source_val: Option<config::Value> = cfg.get(name).ok();
				let source_val = source_val.map(|v| format!("{v}"));
				if cur_value.is_some() && cur_value == &source_val {
					sources.push(source_name.to_string());
				}
				sources
			})
			.join(" + ");
		writeln!(f, "{:<pad$}{sources}", "SOURCES: ")?;
		if let Some(val) = cur_value {
			if !show_secrets && tags.contains(&"secret".to_string()) {
				writeln!(f, "{:<pad$}<secret-hidden>", "CURRENT_VALUE: ")?;
			} else {
				writeln!(f, "{:<pad$}{val}", "CURRENT_VALUE: ")?;
			}
		} else {
			writeln!(f, "{:<pad$}<unset>", "CURRENT_VALUE: ")?;
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cfg::{meta_cfg::CfgPreset, util::get_keys};

	#[test]
	fn load_all_presets() {
		*midnight_node_res::CFG_ROOT.lock().unwrap() = Some("../".to_string());
		for config in midnight_node_res::list_configs() {
			println!("loading {config}...");
			let preset_cfg = Config::builder()
				.add_source(File::from_str(&default_cfg(), FileFormat::Toml))
				.add_source(CfgPreset(config).load_config().unwrap())
				.build()
				.unwrap();

			Cfg::new_no_validation_from_config(preset_cfg)
				.expect("Cfg failed to deserialize - check toml syntax");
		}
	}

	#[test]
	fn dev_cfg_preset_deserializes_without_errors() {
		*midnight_node_res::CFG_ROOT.lock().unwrap() = Some("../".to_string());
		let preset_cfg = Config::builder()
			.add_source(File::from_str(&default_cfg(), FileFormat::Toml))
			.add_source(CfgPreset("dev".to_string()).load_config().unwrap())
			.add_source(Environment::default())
			.build()
			.unwrap();

		let cfg = Cfg::new_no_validation_from_config(preset_cfg)
			.expect("Cfg failed to deserialize using dev preset");

		let _run_cmd: sc_cli::RunCmd = cfg.substrate_cfg.try_into().unwrap();
	}

	fn get_unused(preset_keys: &[String]) -> Vec<String> {
		*midnight_node_res::CFG_ROOT.lock().unwrap() = Some("../".to_string());
		let cfg_keys = [
			get_keys(ChainSpecCfg::default()).unwrap(),
			get_keys(MetaCfg::default()).unwrap(),
			get_keys(MidnightCfg::default()).unwrap(),
			get_keys(StorageMonitorParamsCfg::default()).unwrap(),
			get_keys(SubstrateCfg::default()).unwrap(),
		]
		.concat();

		let keys_not_in_cfg: Vec<String> =
			preset_keys.iter().filter(|&k| !cfg_keys.contains(k)).cloned().collect();
		keys_not_in_cfg
	}

	#[test]
	fn assert_no_ignored_defaults() {
		*midnight_node_res::CFG_ROOT.lock().unwrap() = Some("../".to_string());
		let default_cfg = Cfg::get_default_config().unwrap();
		let default_value: serde_json::Value = default_cfg.try_deserialize().unwrap();
		let default_keys = get_keys(default_value).unwrap();

		let keys_not_in_cfg = get_unused(&default_keys);

		assert_eq!(
			keys_not_in_cfg.len(),
			0,
			"there should be no unused configuration keys in default.toml. Unused keys: {keys_not_in_cfg:?}"
		);
	}

	#[test]
	fn assert_no_ignored_cfg_presets() {
		*midnight_node_res::CFG_ROOT.lock().unwrap() = Some("../".to_string());
		for config in midnight_node_res::list_configs() {
			let cfg = CfgPreset(config.clone());
			let preset_cfg =
				Config::builder().add_source(cfg.load_config().unwrap()).build().unwrap();
			let preset_value: serde_json::Value = preset_cfg.try_deserialize().unwrap();
			let preset_keys = get_keys(preset_value).unwrap();

			let keys_not_in_cfg = get_unused(&preset_keys);

			assert_eq!(
				keys_not_in_cfg.len(),
				0,
				"there should be no unused configuration keys in preset \"{config}\". Unused keys: {keys_not_in_cfg:?}"
			);
		}
	}
}
