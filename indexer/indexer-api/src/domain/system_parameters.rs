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

use indexer_common::domain::{BlockHash, TermsAndConditionsHash};
use sqlx::FromRow;

/// Terms and Conditions governance parameter.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct TermsAndConditions {
    #[sqlx(try_from = "i64")]
    pub block_height: u32,
    pub block_hash: BlockHash,
    #[sqlx(try_from = "i64")]
    pub timestamp: u64,
    pub hash: TermsAndConditionsHash,
    pub url: String,
}

/// D-Parameter governance parameter controlling validator committee composition.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct DParameter {
    #[sqlx(try_from = "i64")]
    pub block_height: u32,
    pub block_hash: BlockHash,
    #[sqlx(try_from = "i64")]
    pub timestamp: u64,
    #[sqlx(try_from = "i32")]
    pub num_permissioned_candidates: u16,
    #[sqlx(try_from = "i32")]
    pub num_registered_candidates: u16,
}
