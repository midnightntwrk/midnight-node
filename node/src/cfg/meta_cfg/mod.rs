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

use config::{File, FileFormat, FileSourceString};
use documented::{Documented, DocumentedFields as _};
use midnight_node_res::env;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use strum::EnumIter;

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

#[derive(Debug, Serialize, Deserialize, EnumIter)]
#[serde(rename_all = "lowercase")]
pub enum CfgPreset {
	// TODO: Change dev -> local
	Dev,
	QaNet,
	DevNet,
	#[serde(rename = "testnet-02")]
	TestNet02,
}

impl CfgPreset {
	pub fn load_config(&self) -> File<FileSourceString, FileFormat> {
		let str = match *self {
			CfgPreset::Dev => env::DEV,
			CfgPreset::QaNet => env::QANET,
			CfgPreset::DevNet => env::DEVNET,
			CfgPreset::TestNet02 => env::TESTNET_02,
		};
		File::from_str(str, FileFormat::Toml).required(false)
	}
}
