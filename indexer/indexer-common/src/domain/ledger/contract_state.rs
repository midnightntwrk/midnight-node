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

use crate::domain::{
    PROTOCOL_VERSION_000_016_000, ProtocolVersion,
    ledger::{Error, RawTokenType, TaggedSerializableV6Ext},
};
use fastrace::trace;
use midnight_coin_structure_v6::coin::TokenType as TokenTypeV6;
use midnight_onchain_runtime_v6::state::ContractState as ContractStateV6;
use midnight_serialize_v6::tagged_deserialize as tagged_deserialize_v6;
use midnight_storage_v6::{DefaultDB as DefaultDBV6, arena::Sp as SpV6};

/// Facade for `ContractState` from `midnight_ledger` across supported (protocol) versions.
#[derive(Debug, Clone)]
pub enum ContractState {
    V6(ContractStateV6<DefaultDBV6>),
}

impl ContractState {
    /// Deserialize the given serialized contract state using the given protocol version.
    #[trace(properties = { "protocol_version": "{protocol_version}" })]
    pub fn deserialize(
        contract_state: impl AsRef<[u8]>,
        protocol_version: ProtocolVersion,
    ) -> Result<Self, Error> {
        if protocol_version.is_compatible(PROTOCOL_VERSION_000_016_000) {
            let contract_state = tagged_deserialize_v6(&mut contract_state.as_ref())
                .map_err(|error| Error::Io("cannot deserialize ContractStateV6", error))?;
            Ok(Self::V6(contract_state))
        } else {
            Err(Error::InvalidProtocolVersion(protocol_version))
        }
    }

    /// Get the token balances for this contract.
    pub fn balances(&self) -> Result<Vec<ContractBalance>, Error> {
        match self {
            Self::V6(contract_state) => {
                contract_state
                    .balance
                    .iter()
                    .filter_map(|entry| {
                        let (token_type_sp, amount_sp) = SpV6::into_inner(entry)?;
                        let token_type = SpV6::into_inner(token_type_sp)?;
                        let amount = SpV6::into_inner(amount_sp)?;

                        (amount > 0).then_some((token_type, amount))
                    })
                    .map(|(token_type, amount)| {
                        match token_type {
                            // For unshielded tokens extract the type directly.
                            TokenTypeV6::Unshielded(unshielded) => Ok(ContractBalance {
                                token_type: unshielded.0.0.into(),
                                amount,
                            }),

                            // For other tokens we serialize the type.
                            _ => {
                                let token_type =
                                    token_type.tagged_serialize_v6().map_err(|error| {
                                        Error::Io("cannot serialize TokenTypeV6", error)
                                    })?;

                                let len = token_type.len();
                                let token_type = RawTokenType::try_from(token_type.as_ref())
                                    .map_err(|_| Error::TokenTypeLen(len))?;

                                Ok(ContractBalance { token_type, amount })
                            }
                        }
                    })
                    .collect()
            }
        }
    }
}

/// Token balance of a contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContractBalance {
    /// Token type identifier.
    pub token_type: RawTokenType,

    /// Balance amount as u128.
    pub amount: u128,
}
