// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
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

pub(crate) fn format_timestamp_utc(epoch_secs: u64) -> String {
	const SECS_PER_DAY: u64 = 86400;
	let days = epoch_secs / SECS_PER_DAY;
	let day_secs = epoch_secs % SECS_PER_DAY;
	let h = day_secs / 3600;
	let m = (day_secs % 3600) / 60;
	let s = day_secs % 60;

	// Civil date from days since 1970-01-01 (algorithm from Howard Hinnant)
	let z = days as i64 + 719468;
	let era = z.div_euclid(146097);
	let doe = z.rem_euclid(146097) as u64;
	let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
	let y = yoe as i64 + era * 400;
	let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
	let mp = (5 * doy + 2) / 153;
	let d = doy - (153 * mp + 2) / 5 + 1;
	let mo = if mp < 10 { mp + 3 } else { mp - 9 };
	let y = if mo <= 2 { y + 1 } else { y };

	format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}
