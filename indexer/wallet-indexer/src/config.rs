// This file is part of midnight-indexer.
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

use crate::{application, infra};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    pub run_migrations: bool,

    #[serde(rename = "application")]
    pub application_config: application::Config,

    #[serde(rename = "infra")]
    pub infra_config: infra::Config,

    #[serde(rename = "telemetry")]
    pub telemetry_config: indexer_common::telemetry::Config,
}
