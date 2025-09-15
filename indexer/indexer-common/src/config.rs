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

use figment::{
    Figment,
    providers::{Env, Format, Yaml},
};
use serde::Deserialize;
use std::env;

const CONFIG_FILE: &str = "CONFIG_FILE";

/// Extension methods for "configuration structs" which can be deserialized.
pub trait ConfigExt
where
    Self: for<'de> Deserialize<'de>,
{
    /// Load the configuration from the file at the value of the `CONFIG_FILE` environment variable
    /// or `config.yaml` by default, with an overlay provided by environment variables prefixed with
    /// `"APP__"` and split/nested via `"__"`.
    fn load() -> Result<Self, Box<figment::Error>> {
        let config_file = env::var(CONFIG_FILE)
            .map(Yaml::file_exact)
            .unwrap_or(Yaml::file_exact("config.yaml"));

        let config = Figment::new()
            .merge(config_file)
            .merge(Env::prefixed("APP__").split("__"))
            .extract()?;

        Ok(config)
    }
}

impl<T> ConfigExt for T where T: for<'de> Deserialize<'de> {}

#[cfg(test)]
mod tests {
    use crate::config::{CONFIG_FILE, ConfigExt};
    use assert_matches::assert_matches;
    use serde::Deserialize;
    use std::env;

    #[test]
    fn test_load() {
        unsafe {
            env::set_var("APP__API__PORT", "4242");
        }

        let config = MainConfig::load();
        assert_matches!(
            config,
            Ok(MainConfig { config: Config { api: api::Config { port, .. } } }) if port == 4242
        );

        unsafe {
            env::set_var(CONFIG_FILE, "nonexistent.yaml");
        }
        let config = Config::load();
        assert!(config.is_err());
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct MainConfig {
        /// Application sepcific configuration.
        #[serde(flatten)]
        pub config: Config,
    }

    /// Application sepcific configuration.
    #[derive(Debug, Clone, Deserialize)]
    pub struct Config {
        pub api: api::Config,
    }

    mod api {
        use serde::Deserialize;

        #[derive(Debug, Clone, Deserialize)]
        pub struct Config {
            pub port: u16,
        }
    }
}
