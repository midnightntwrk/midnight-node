// This file is part of midnight-indexer.
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

#[derive(Debug, Clone)]
pub struct PoolMetadata {
    pub pool_id: String,
    pub hex_id: String,
    pub name: String,
    pub ticker: String,
    pub homepage_url: String,
    pub url: String,
}

impl PoolMetadata {
    /// Placeholder used when the upstream metadata fetch fails so we still
    /// persist a row keyed by `cardano_id`.
    pub fn placeholder(cardano_id: String) -> Self {
        Self {
            pool_id: cardano_id.clone(),
            hex_id: cardano_id,
            name: String::new(),
            ticker: String::new(),
            homepage_url: String::new(),
            url: String::new(),
        }
    }
}
