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
use std::sync::LazyLock;

/// A minimal representation of a docker-compose service.
#[derive(Deserialize)]
pub struct Service {
	pub image: String,
}

/// A minimal representation of a docker-compose file.
#[derive(Deserialize)]
pub struct Compose {
	pub services: std::collections::HashMap<String, Service>,
}

/// Parsed docker-compose file shared across all integration tests.
static COMPOSE: LazyLock<Compose> = LazyLock::new(|| {
	let path = concat!(env!("CARGO_MANIFEST_DIR"), "/test-images.docker-compose.yml");
	let raw = std::fs::read_to_string(path).expect("failed to read test-images.docker-compose.yml");
	let expanded = shellexpand::env(&raw).expect("failed to expand env vars in compose file");
	serde_yaml::from_str(&expanded).expect("failed to parse compose file")
});

/// Returns `(name, tag)` for the given compose service, splitting on `:`.
/// The tag may contain a `@sha256:…` digest.
pub fn test_image(service: &str) -> (String, String) {
	let image = &COMPOSE.services[service].image;
	let (name, tag) = image.split_once(':').expect("image must contain a ':'");
	(name.to_string(), tag.to_string())
}
