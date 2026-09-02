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

use indexer_common::domain::{
    ContractAttributes, ContractBalance, SerializedContractAddress, SerializedContractState,
    SerializedZswapState,
};

/// A contract action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractAction {
    pub address: SerializedContractAddress,
    pub state: SerializedContractState,
    pub zswap_state: SerializedZswapState,
    pub extracted_balances: Vec<ContractBalance>,
    pub attributes: ContractAttributes,
}

impl From<indexer_common::domain::ContractAction> for ContractAction {
    fn from(contract_action: indexer_common::domain::ContractAction) -> Self {
        Self {
            address: contract_action.address,
            state: contract_action.state,
            zswap_state: Default::default(),
            extracted_balances: Default::default(),
            attributes: contract_action.attributes,
        }
    }
}
