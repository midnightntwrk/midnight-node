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

use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct Manifest {
	pub package: Package,
}

#[derive(Deserialize)]
pub(crate) struct Package {
	pub version: String,
}

#[macro_export]
macro_rules! find_crate_version {
	($cargo_toml_path:literal) => {{
		let manifest_str = include_str!($cargo_toml_path);
		let manifest: crate::utils::Manifest =
			toml::from_str(&manifest_str).expect("Failed to parse manifest");

		manifest.package.version
	}};
}

pub(crate) use find_crate_version;
