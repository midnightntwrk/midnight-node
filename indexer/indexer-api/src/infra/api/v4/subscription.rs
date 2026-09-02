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

mod block;
mod bridge_events;
mod contract_action;
mod contract_event;
mod dust_generations;
mod dust_ledger_events;
mod dust_nullifier_transactions;
mod polling;
mod shielded;
mod shielded_nullifier_transactions;
mod unshielded;
mod zswap_ledger_events;

use crate::{
    domain::storage::Storage,
    infra::api::v4::subscription::{
        block::BlockSubscription, bridge_events::BridgeEventsSubscription,
        contract_action::ContractActionSubscription, contract_event::ContractEventsSubscription,
        dust_generations::DustGenerationsSubscription,
        dust_ledger_events::DustLedgerEventsSubscription,
        dust_nullifier_transactions::DustNullifierTransactionsSubscription,
        shielded::ShieldedTransactionsSubscription,
        shielded_nullifier_transactions::ShieldedNullifierTransactionsSubscription,
        unshielded::UnshieldedTransactionsSubscription,
        zswap_ledger_events::ZswapLedgerEventsSubscription,
    },
};
use async_graphql::MergedSubscription;
use indexer_common::domain::Subscriber;

#[derive(MergedSubscription)]
pub struct Subscription<S, B>(
    BlockSubscription<S, B>,
    BridgeEventsSubscription<S, B>,
    ContractActionSubscription<S, B>,
    ContractEventsSubscription<S, B>,
    DustGenerationsSubscription<S, B>,
    DustLedgerEventsSubscription<S, B>,
    DustNullifierTransactionsSubscription<S, B>,
    ShieldedNullifierTransactionsSubscription<S, B>,
    ShieldedTransactionsSubscription<S, B>,
    UnshieldedTransactionsSubscription<S, B>,
    ZswapLedgerEventsSubscription<S, B>,
)
where
    S: Storage,
    B: Subscriber;

impl<S, B> Default for Subscription<S, B>
where
    S: Storage,
    B: Subscriber,
{
    fn default() -> Self {
        Subscription(
            BlockSubscription::default(),
            BridgeEventsSubscription::default(),
            ContractActionSubscription::default(),
            ContractEventsSubscription::default(),
            DustGenerationsSubscription::default(),
            DustLedgerEventsSubscription::default(),
            DustNullifierTransactionsSubscription::default(),
            ShieldedNullifierTransactionsSubscription::default(),
            ShieldedTransactionsSubscription::default(),
            UnshieldedTransactionsSubscription::default(),
            ZswapLedgerEventsSubscription::default(),
        )
    }
}
