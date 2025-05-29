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

/// Any keys parsed here will be parsed as shell words
#[derive(Clone, Debug)]
pub struct ShellWordsEnvironment {
	keys: Vec<String>,
}

impl ShellWordsEnvironment {
	pub fn new(keys: &[&str]) -> Self {
		let keys = keys.iter().map(|s| s.to_lowercase()).collect();
		Self { keys }
	}
}

impl config::Source for ShellWordsEnvironment {
	fn clone_into_box(&self) -> Box<dyn config::Source + Send + Sync> {
		Box::new((*self).clone())
	}

	fn collect(&self) -> Result<config::Map<String, config::Value>, config::ConfigError> {
		let uri: String = "the environment (shell words)".into();
		let mut map = config::Map::new();

		for (key, value) in std::env::vars() {
			let key = key.to_lowercase();
			if self.keys.contains(&key) {
				let words: Vec<config::Value> = shell_words::split(&value)
					.map_err(|e| config::ConfigError::Message(e.to_string()))?
					.into_iter()
					.map(|s| {
						config::Value::new(Some(&uri), config::ValueKind::String(s.to_string()))
					})
					.collect();
				let val = config::ValueKind::Array(words);
				map.insert(key, val.into());
			}
		}
		Ok(map)
	}
}
