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

use clap::{Args, CommandFactory as _, Parser};
use documented::{Documented, DocumentedFields as _, FieldInfo};
use sc_storage_monitor::StorageMonitorParams;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;

use super::{CfgHelp, HelpField, error::CfgError};

/// Parameters used to create the storage monitor.
#[derive(Default, Debug, Documented, Clone, Serialize, Deserialize, Validate)]
pub struct StorageMonitorParamsCfg {
	/// Required available space on database storage.
	///
	/// If available space for DB storage drops below the given threshold, node will
	/// be gracefully terminated.
	///
	/// If `0` is given monitoring will be disabled.
	pub threshold: u64,
	/// How often available space is polled.
	pub polling_period: u32,
}

#[derive(Parser)]
struct EmptyParser {}

impl StorageMonitorParamsCfg {
	pub fn command() -> clap::Command {
		let cmd = EmptyParser::command();
		StorageMonitorParams::augment_args(cmd)
	}
}

impl CfgHelp for StorageMonitorParamsCfg {
	fn help(cur_cfg: Option<&config::Config>) -> Result<Vec<HelpField>, CfgError> {
		let cmd = Self::command();
		let args = cmd.get_arguments();

		let docs = Self::field_docs();
		let type_map = docs.iter().fold(std::collections::BTreeMap::new(), |mut acc, it| {
			acc.insert(it.name.to_string(), it.field_type.to_string());
			acc
		});
		let mut help_fields = Vec::new();
		for arg in args {
			let name = arg.get_id().to_string();
			let doc = arg.get_help().map_or("<help missing>".to_string(), |h| h.to_string());
			let field_type = type_map.get(&name).unwrap().to_string();
			let current_value = cur_cfg.map(|c| c.get_string(&name).ok());
			let info = FieldInfo { name, doc, field_type, tags: vec![] };
			let field = HelpField { current_value, info };
			help_fields.push(field);
		}

		Ok(help_fields)
	}
}

impl From<StorageMonitorParamsCfg> for StorageMonitorParams {
	fn from(value: StorageMonitorParamsCfg) -> Self {
		StorageMonitorParams { threshold: value.threshold, polling_period: value.polling_period }
	}
}

// No test that we're in alignment with substrate defaults because we're
// deliberately defaulting to different values.
