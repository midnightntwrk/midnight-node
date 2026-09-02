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

use crate::domain::{
    ContractBalance, ContractMaintenanceAuthority, ContractMaintenanceVerifyingKey, LedgerVersion,
    TokenType, VerifyingKeyKind,
    ledger::{Error, TaggedSerializableExt},
};
use fastrace::trace;
use midnight_coin_structure_v2::coin::TokenType as MidnightTokenType;
use midnight_coin_structure_v3::coin::TokenType as MidnightTokenTypeV9;
use midnight_onchain_runtime_v3::state::ContractState as ContractStateV3;
// v8's maintenance authority committee is `Vec<VerifyingKey>` (Schnorr only). v9 generalised it to
// a `ContractMaintenanceVerifyingKey` enum (Schnorr | ECDSA), re-exported by the v9 runtime.
use midnight_onchain_runtime_v4::state::{
    ContractMaintenanceVerifyingKey as ContractMaintenanceVerifyingKeyV4,
    ContractState as ContractStateV4,
};
use midnight_serialize_v1::tagged_deserialize;
use midnight_storage_core_v1::DefaultDB;

/// Facade for `ContractState` from `midnight_ledger` across supported (protocol) versions.
#[derive(Debug, Clone)]
pub enum ContractState {
    V3(ContractStateV3<DefaultDB>),
    V4(ContractStateV4<DefaultDB>),
}

impl ContractState {
    /// Deserialize the given serialized contract state using the given protocol version.
    #[trace(properties = { "ledger_version": "{ledger_version}" })]
    pub fn deserialize(
        contract_state: impl AsRef<[u8]>,
        ledger_version: LedgerVersion,
    ) -> Result<Self, Error> {
        let contract_state = match ledger_version {
            LedgerVersion::V8 => {
                let contract_state = tagged_deserialize(&mut contract_state.as_ref())
                    .map_err(|error| Error::Deserialize("ContractStateV8", error))?;
                Self::V3(contract_state)
            }
            LedgerVersion::V9 => {
                let contract_state = tagged_deserialize(&mut contract_state.as_ref())
                    .map_err(|error| Error::Deserialize("ContractStateV9", error))?;
                Self::V4(contract_state)
            }
        };

        Ok(contract_state)
    }

    /// Get the token balances for this contract.
    pub fn balances(&self) -> Result<Vec<ContractBalance>, Error> {
        match self {
            Self::V3(contract_state) => {
                contract_state
                    .balance
                    .iter()
                    .filter_map(|entry| {
                        // Read via deref: `Sp::into_inner` returns `None` for lazy or shared
                        // entries, silently dropping all balances.
                        let (token_type, amount) = &*entry;
                        let (token_type, amount) = (**token_type, **amount);

                        (amount > 0).then_some((token_type, amount))
                    })
                    .map(|(token_type, amount)| {
                        match token_type {
                            // For unshielded tokens extract the type directly.
                            MidnightTokenType::Unshielded(unshielded) => Ok(ContractBalance {
                                token_type: unshielded.0.0.into(),
                                amount,
                            }),

                            // For other tokens we serialize the type.
                            _ => {
                                let token_type = token_type
                                    .tagged_serialize()
                                    .map_err(|error| Error::Serialize("TokenTypeV8", error))?;

                                let token_type = TokenType::try_from(token_type.as_ref())
                                    .map_err(Error::ByteArrayLen)?;

                                Ok(ContractBalance { token_type, amount })
                            }
                        }
                    })
                    .collect()
            }

            Self::V4(contract_state) => {
                contract_state
                    .balance
                    .iter()
                    .filter_map(|entry| {
                        // Read via deref: `Sp::into_inner` returns `None` for lazy or shared
                        // entries, silently dropping all balances.
                        let (token_type, amount) = &*entry;
                        let (token_type, amount) = (**token_type, **amount);

                        (amount > 0).then_some((token_type, amount))
                    })
                    .map(|(token_type, amount)| {
                        match token_type {
                            // For unshielded tokens extract the type directly.
                            MidnightTokenTypeV9::Unshielded(unshielded) => Ok(ContractBalance {
                                token_type: unshielded.0.0.into(),
                                amount,
                            }),

                            // For other tokens we serialize the type.
                            _ => {
                                let token_type = token_type
                                    .tagged_serialize()
                                    .map_err(|error| Error::Serialize("TokenTypeV9", error))?;

                                let token_type = TokenType::try_from(token_type.as_ref())
                                    .map_err(Error::ByteArrayLen)?;

                                Ok(ContractBalance { token_type, amount })
                            }
                        }
                    })
                    .collect()
            }
        }
    }

    /// Get the maintenance authority for this contract.
    pub fn maintenance_authority(&self) -> Result<ContractMaintenanceAuthority, Error> {
        match self {
            Self::V3(contract_state) => {
                let authority = &contract_state.maintenance_authority;
                // v8 committee keys are all Schnorr (`Vec<VerifyingKey>`, no scheme tag).
                let committee = authority
                    .committee
                    .iter()
                    .map(|key| {
                        let key = key
                            .tagged_serialize()
                            .map_err(|error| Error::Serialize("VerifyingKeyV8", error))?;
                        Ok(ContractMaintenanceVerifyingKey {
                            kind: VerifyingKeyKind::Schnorr,
                            key,
                        })
                    })
                    .collect::<Result<Vec<_>, Error>>()?;

                Ok(ContractMaintenanceAuthority {
                    committee,
                    threshold: authority.threshold,
                    counter: authority.counter,
                })
            }

            Self::V4(contract_state) => {
                let authority = &contract_state.maintenance_authority;
                let committee = authority
                    .committee
                    .iter()
                    .map(|key| {
                        let (kind, key) = match key {
                            ContractMaintenanceVerifyingKeyV4::Schnorr(key) => {
                                (VerifyingKeyKind::Schnorr, key.tagged_serialize())
                            }
                            ContractMaintenanceVerifyingKeyV4::ECDSA(key) => {
                                (VerifyingKeyKind::Ecdsa, key.tagged_serialize())
                            }
                        };
                        let key = key.map_err(|error| Error::Serialize("VerifyingKeyV9", error))?;
                        Ok(ContractMaintenanceVerifyingKey { kind, key })
                    })
                    .collect::<Result<Vec<_>, Error>>()?;

                Ok(ContractMaintenanceAuthority {
                    committee,
                    threshold: authority.threshold,
                    counter: authority.counter,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{
        ByteArray, LedgerVersion, TokenType,
        ledger::{ContractState, TaggedSerializableExt},
    };
    use midnight_base_crypto_v1::hash::HashOutput;
    use midnight_coin_structure_v2::coin::{TokenType as MidnightTokenType, UnshieldedTokenType};
    use midnight_coin_structure_v3::coin::{
        TokenType as MidnightTokenTypeV9, UnshieldedTokenType as UnshieldedTokenTypeV9,
    };
    use midnight_onchain_runtime_v3::state::ContractState as ContractStateV3;
    use midnight_onchain_runtime_v4::state::ContractState as ContractStateV4;
    use midnight_storage_core_v1::DefaultDB;

    #[test]
    fn test_balances_v8() {
        let mut contract_state = ContractStateV3::<DefaultDB>::default();
        contract_state.balance = contract_state.balance.insert(
            MidnightTokenType::Unshielded(UnshieldedTokenType(HashOutput(TOKEN_TYPE.0))),
            AMOUNT,
        );
        let contract_state = contract_state
            .tagged_serialize()
            .expect("contract state can be serialized");

        let balances = ContractState::deserialize(contract_state, LedgerVersion::V8)
            .expect("contract state can be deserialized")
            .balances()
            .expect("balances can be extracted");

        assert_eq!(balances.len(), 1);
        assert_eq!(balances[0].token_type, TOKEN_TYPE);
        assert_eq!(balances[0].amount, AMOUNT);
    }

    #[test]
    fn test_balances_v9() {
        let mut contract_state = ContractStateV4::<DefaultDB>::default();
        contract_state.balance = contract_state.balance.insert(
            MidnightTokenTypeV9::Unshielded(UnshieldedTokenTypeV9(HashOutput(TOKEN_TYPE.0))),
            AMOUNT,
        );
        let contract_state = contract_state
            .tagged_serialize()
            .expect("contract state can be serialized");

        let balances = ContractState::deserialize(contract_state, LedgerVersion::V9)
            .expect("contract state can be deserialized")
            .balances()
            .expect("balances can be extracted");

        assert_eq!(balances.len(), 1);
        assert_eq!(balances[0].token_type, TOKEN_TYPE);
        assert_eq!(balances[0].amount, AMOUNT);
    }

    const TOKEN_TYPE: TokenType = ByteArray([7; 32]);
    const AMOUNT: u128 = 1_000_000;
}
