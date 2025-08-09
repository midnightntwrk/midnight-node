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

use config::{ConfigError, File, FileFormat, FileSourceString};
use documented::{Documented, DocumentedFields as _};
use midnight_node_res::get_config;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;

use super::{CfgHelp, HelpField, cfg_help, error::CfgError, util::get_keys};

#[derive(Debug, Serialize, Deserialize, Default, Validate, Documented)]
/// Meta parameters that change how config is read and displayed
pub struct MetaCfg {
	/// Use a preset of default config values
	pub cfg_preset: Option<CfgPreset>,
	/// Show configuration on startup
	pub show_config: bool,
	/// Show secrets in configuration
	pub show_secrets: bool,
}

impl CfgHelp for MetaCfg {
	fn help(cur_cfg: Option<&config::Config>) -> Result<Vec<HelpField>, CfgError> {
		cfg_help!(cur_cfg, Self)
	}
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CfgPreset(pub String);

impl CfgPreset {
	pub fn load_config(&self) -> Result<File<FileSourceString, FileFormat>, ConfigError> {
		let config_str = get_config(&self.0).map_or_else(
			|| {
				std::fs::read_to_string(&self.0)
					.map_err(|_| ConfigError::Message(format!("Failed to load config {}", self.0)))
			},
			Ok,
		)?;

		Ok(File::from_str(&config_str, FileFormat::Toml).required(false))
	}
}
