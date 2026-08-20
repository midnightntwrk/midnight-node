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

//! System parameters domain types for chain-indexer.

use indexer_common::domain::{BlockHash, TermsAndConditionsHash};

/// D-Parameter from the node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DParameter {
    pub num_permissioned_candidates: u16,
    pub num_registered_candidates: u16,
}

/// Terms and Conditions from the node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermsAndConditions {
    pub hash: TermsAndConditionsHash,
    pub url: String,
}

/// System parameters change detected during block processing.
#[derive(Debug, Clone)]
pub struct SystemParametersChange {
    pub block_height: u64,
    pub block_hash: BlockHash,
    pub timestamp: u64,
    pub d_parameter: Option<DParameter>,
    pub terms_and_conditions: Option<TermsAndConditions>,
}
