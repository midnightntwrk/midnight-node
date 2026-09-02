// This file is part of midnight-indexer.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! GraphQL types for the `contract(address, offset)` query: the contract as the topmost concept,
//! with its point-in-time state, maintenance authority, and a bounded recent-actions sub-query
//! (full enumeration uses the `contractActions` subscription). See ticket #1275.

use crate::{
    domain::{self, storage::Storage},
    infra::api::{
        ApiResult, ContextExt, OptionExt, ResultExt,
        v4::{HexEncodable, HexEncoded, contract_action::ContractAction, directives::beta},
    },
};
use async_graphql::{ComplexObject, Context, Enum, SimpleObject};
use indexer_common::domain::{
    ContractMaintenanceAuthority as DomainContractMaintenanceAuthority,
    ContractMaintenanceVerifyingKey as DomainContractMaintenanceVerifyingKey,
    SerializedContractAddress, SerializedContractState, VerifyingKeyKind as DomainVerifyingKeyKind,
    ledger::ContractState,
};
use std::marker::PhantomData;

/// Default number of recent actions returned by `Contract.actions` when no `limit` is given.
const DEFAULT_ACTIONS_LIMIT: i32 = 100;

/// Maximum number of recent actions returned by `Contract.actions`.
const MAX_ACTIONS_LIMIT: i32 = 500;

/// A contract, identified by address, resolved as of a given block (or the latest state if no
/// offset is given). The topmost contract concept; its actions are a sub-query.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(complex, directive = beta::apply())]
pub struct Contract<S>
where
    S: Storage,
{
    /// The hex-encoded contract address.
    pub address: HexEncoded,

    /// The hex-encoded serialized contract state as of the queried block (the latest contract
    /// action at or before it).
    pub state: HexEncoded,

    #[graphql(skip)]
    raw_state: SerializedContractState,

    #[graphql(skip)]
    transaction_id: u64,

    #[graphql(skip)]
    raw_address: SerializedContractAddress,

    #[graphql(skip)]
    _s: PhantomData<S>,
}

impl<S> From<domain::ContractAction> for Contract<S>
where
    S: Storage,
{
    fn from(action: domain::ContractAction) -> Self {
        Self {
            address: action.address.hex_encode(),
            state: action.state.hex_encode(),
            raw_state: action.state,
            transaction_id: action.transaction_id,
            raw_address: action.address,
            _s: PhantomData,
        }
    }
}

#[ComplexObject]
impl<S> Contract<S>
where
    S: Storage,
{
    /// The contract's maintenance authority as of the queried block.
    #[graphql(directive = beta::apply())]
    async fn maintenance_authority(
        &self,
        cx: &Context<'_>,
    ) -> ApiResult<ContractMaintenanceAuthority> {
        let storage = cx.get_storage::<S>();

        let protocol_version = storage
            .get_protocol_version_by_transaction_id(self.transaction_id)
            .await
            .map_err_into_server_error(|| {
                format!(
                    "get protocol version for transaction id {}",
                    self.transaction_id
                )
            })?
            .some_or_server_error(|| format!("no transaction with id {}", self.transaction_id))?;

        let authority =
            ContractState::deserialize(&self.raw_state, protocol_version.ledger_version())
                .map_err_into_server_error(|| "deserialize contract state")?
                .maintenance_authority()
                .map_err_into_server_error(|| "extract contract maintenance authority")?;

        Ok(authority.into())
    }

    /// Recent contract actions for this contract, newest first, optionally filtered by type;
    /// `limit` defaults to 100 and is capped at 500. Use the `contractActions` subscription to
    /// enumerate all actions.
    #[graphql(directive = beta::apply())]
    async fn actions(
        &self,
        cx: &Context<'_>,
        limit: Option<i32>,
        r#type: Option<ContractActionType>,
    ) -> ApiResult<Vec<ContractAction<S>>> {
        let storage = cx.get_storage::<S>();

        let limit = limit
            .unwrap_or(DEFAULT_ACTIONS_LIMIT)
            .clamp(1, MAX_ACTIONS_LIMIT) as u32;
        let variant = r#type.map(ContractActionType::variant_name);

        let actions = storage
            .get_recent_contract_actions_by_address(&self.raw_address, limit, variant)
            .await
            .map_err_into_server_error(|| {
                format!(
                    "get recent contract actions for address {}",
                    self.raw_address
                )
            })?;

        Ok(actions.into_iter().map(Into::into).collect())
    }
}

/// Contract action variant, used to filter `Contract.actions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum)]
pub enum ContractActionType {
    Deploy,
    Call,
    Update,
}

impl ContractActionType {
    /// The `CONTRACT_ACTION_VARIANT` value matching this type.
    fn variant_name(self) -> &'static str {
        match self {
            Self::Deploy => "Deploy",
            Self::Call => "Call",
            Self::Update => "Update",
        }
    }
}

/// The maintenance authority of a contract.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(directive = beta::apply())]
pub struct ContractMaintenanceAuthority {
    /// The committee of verifying keys authorised to maintain the contract.
    pub committee: Vec<ContractMaintenanceVerifyingKey>,

    /// The number of committee signatures required to authorise maintenance.
    pub threshold: u32,

    /// Monotonic counter guarding against replay of maintenance operations.
    pub counter: u32,
}

/// A verifying key in a contract maintenance authority committee.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(directive = beta::apply())]
pub struct ContractMaintenanceVerifyingKey {
    /// The signature scheme of the key.
    pub kind: ContractMaintenanceVerifyingKeyKind,

    /// The hex-encoded tagged-serialized verifying key.
    pub key: HexEncoded,
}

/// The signature scheme of a maintenance authority verifying key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum)]
pub enum ContractMaintenanceVerifyingKeyKind {
    Schnorr,
    Ecdsa,
}

impl From<DomainContractMaintenanceAuthority> for ContractMaintenanceAuthority {
    fn from(authority: DomainContractMaintenanceAuthority) -> Self {
        Self {
            committee: authority.committee.into_iter().map(Into::into).collect(),
            threshold: authority.threshold,
            counter: authority.counter,
        }
    }
}

impl From<DomainContractMaintenanceVerifyingKey> for ContractMaintenanceVerifyingKey {
    fn from(key: DomainContractMaintenanceVerifyingKey) -> Self {
        Self {
            kind: match key.kind {
                DomainVerifyingKeyKind::Schnorr => ContractMaintenanceVerifyingKeyKind::Schnorr,
                DomainVerifyingKeyKind::Ecdsa => ContractMaintenanceVerifyingKeyKind::Ecdsa,
            },
            key: key.key.hex_encode(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexer_common::domain::ByteVec;

    #[test]
    fn contract_action_type_variant_name_is_stable() {
        assert_eq!(ContractActionType::Deploy.variant_name(), "Deploy");
        assert_eq!(ContractActionType::Call.variant_name(), "Call");
        assert_eq!(ContractActionType::Update.variant_name(), "Update");
    }

    #[test]
    fn maintenance_authority_maps_committee_kinds_and_scalars() {
        let domain = DomainContractMaintenanceAuthority {
            committee: vec![
                DomainContractMaintenanceVerifyingKey {
                    kind: DomainVerifyingKeyKind::Schnorr,
                    key: ByteVec::from(vec![0xaa; 32]),
                },
                DomainContractMaintenanceVerifyingKey {
                    kind: DomainVerifyingKeyKind::Ecdsa,
                    key: ByteVec::from(vec![0xbb; 33]),
                },
            ],
            threshold: 2,
            counter: 7,
        };

        let authority = ContractMaintenanceAuthority::from(domain);
        assert_eq!(authority.threshold, 2);
        assert_eq!(authority.counter, 7);
        assert_eq!(authority.committee.len(), 2);
        assert!(matches!(
            authority.committee[0].kind,
            ContractMaintenanceVerifyingKeyKind::Schnorr
        ));
        assert!(matches!(
            authority.committee[1].kind,
            ContractMaintenanceVerifyingKeyKind::Ecdsa
        ));
    }
}
