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

use indexer_common::domain::{CardanoRewardAddress, DustPublicKey, DustUtxoId};

/// Domain representation of DUST registration events from the NativeTokenObservation pallet.
#[derive(Debug, Clone, PartialEq)]
pub enum DustRegistrationEvent {
    /// Cardano stake key registered with DUST address.
    Registration {
        cardano_stake_key: CardanoRewardAddress,
        dust_address: DustPublicKey,
    },

    /// Cardano stake key deregistered from DUST address.
    Deregistration {
        cardano_stake_key: CardanoRewardAddress,
        dust_address: DustPublicKey,
    },

    /// UTXO mapping added for registration.
    MappingAdded {
        cardano_stake_key: CardanoRewardAddress,
        dust_address: DustPublicKey,
        utxo_id: DustUtxoId,
        utxo_index: u32,
    },

    /// UTXO mapping removed from registration.
    MappingRemoved {
        cardano_stake_key: CardanoRewardAddress,
        dust_address: DustPublicKey,
        utxo_id: DustUtxoId,
        utxo_index: u32,
    },
}
