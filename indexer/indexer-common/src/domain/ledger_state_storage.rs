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

#![cfg_attr(coverage_nightly, coverage(off))]

use crate::domain::{ProtocolVersion, ledger::SerializedLedgerState};
use std::{convert::Infallible, error::Error as StdError};

/// Abstraction for ledger state storage.
#[trait_variant::make(Send)]
pub trait LedgerStateStorage: Sync + 'static {
    type Error: StdError + Send + Sync + 'static;

    /// Load the last index.
    async fn load_last_index(&self) -> Result<Option<u64>, Self::Error>;

    /// Load the ledger state, block height and protocol version.
    async fn load_ledger_state(
        &self,
    ) -> Result<Option<(SerializedLedgerState, u32, ProtocolVersion)>, Self::Error>;

    /// Save the given ledger state, block_height and highest zswap state index.
    async fn save(
        &mut self,
        ledger_state: &SerializedLedgerState,
        block_height: u32,
        highest_zswap_state_index: Option<u64>,
        protocol_version: ProtocolVersion,
    ) -> Result<(), Self::Error>;
}

pub struct NoopLedgerStateStorage;

impl LedgerStateStorage for NoopLedgerStateStorage {
    type Error = Infallible;

    async fn load_last_index(&self) -> Result<Option<u64>, Self::Error> {
        unimplemented!()
    }

    async fn load_ledger_state(
        &self,
    ) -> Result<Option<(SerializedLedgerState, u32, ProtocolVersion)>, Self::Error> {
        unimplemented!()
    }

    #[allow(unused_variables)]
    async fn save(
        &mut self,
        ledger_state: &SerializedLedgerState,
        block_height: u32,
        highest_zswap_state_index: Option<u64>,
        protocol_version: ProtocolVersion,
    ) -> Result<(), Self::Error> {
        unimplemented!()
    }
}
