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

pub mod block;
pub mod bridge;
pub mod contract_action;
pub mod contract_event;
pub mod dust;
pub mod dust_generations;
pub mod ledger_events;
pub mod ledger_state;
pub mod shielded_nullifiers;
pub mod spo;
pub mod system_parameters;
pub mod transaction;
pub mod unshielded;
pub mod wallet;

use crate::domain::storage::{
    block::BlockStorage, bridge::BridgeStorage, contract_action::ContractActionStorage,
    contract_event::ContractEventStorage, dust::DustStorage,
    dust_generations::DustGenerationsStorage, ledger_events::LedgerEventStorage,
    ledger_state::LedgerStateStorage, shielded_nullifiers::ShieldedNullifiersStorage,
    spo::SpoStorage, system_parameters::SystemParametersStorage, transaction::TransactionStorage,
    unshielded::UnshieldedUtxoStorage, wallet::WalletStorage,
};

/// Storage abstraction.
#[trait_variant::make(Send)]
pub trait Storage
where
    Self: BlockStorage
        + BridgeStorage
        + ContractActionStorage
        + ContractEventStorage
        + DustStorage
        + DustGenerationsStorage
        + LedgerEventStorage
        + LedgerStateStorage
        + SpoStorage
        + SystemParametersStorage
        + TransactionStorage
        + UnshieldedUtxoStorage
        + WalletStorage
        + ShieldedNullifiersStorage
        + Clone
        + Send
        + Sync
        + 'static,
{
}

/// Just needed as a type argument for `infra::api::export_schema` which should not depend on any
/// features like "cloud" and hence types like `infra::postgres::PostgresStorage` cannot be used.
/// Once traits with async functions are object safe, this can go away and be replaced with
/// `Box<dyn Storage>` at the type level.
#[derive(Clone, Default)]
pub struct NoopStorage;

impl Storage for NoopStorage {}
