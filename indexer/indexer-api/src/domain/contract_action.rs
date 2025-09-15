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

use indexer_common::domain::ledger::{
    SerializedContractAddress, SerializedContractEntryPoint, SerializedContractState,
    SerializedZswapState,
};
use serde::Deserialize;
use sqlx::FromRow;

/// A contract action.

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ContractAction {
    #[sqlx(try_from = "i64")]
    pub id: u64,

    pub address: SerializedContractAddress,

    pub state: SerializedContractState,

    #[sqlx(json)]
    pub attributes: ContractAttributes,

    pub chain_state: SerializedZswapState,

    #[sqlx(try_from = "i64")]
    pub transaction_id: u64,
}

/// Attributes for a specific [ContractAction].
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
pub enum ContractAttributes {
    #[default]
    Deploy,

    Call {
        entry_point: SerializedContractEntryPoint,
    },

    Update,
}
