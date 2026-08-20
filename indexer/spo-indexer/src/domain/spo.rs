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

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct SPO {
    pub spo_sk: String,
    pub pool_id: String,
    pub mainchain_pubkey: String,
    pub sidechain_pubkey: String,
    pub aura_pubkey: String,
}

#[derive(Debug, Clone)]
pub struct SPOEpochPerformance {
    pub spo_sk: String,
    pub epoch_no: u64,
    pub expected_blocks: u32,
    pub produced_blocks: u64,
    pub identity_label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SPOStatus {
    Valid,

    Invalid,
}

#[derive(Debug, Clone)]
pub struct SPOHistory {
    pub spo_sk: String,
    pub epoch_no: u64,
    pub status: SPOStatus,
}

impl fmt::Display for SPOStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SPOStatus::Valid => write!(f, "VALID"),
            SPOStatus::Invalid => write!(f, "INVALID"),
        }
    }
}
