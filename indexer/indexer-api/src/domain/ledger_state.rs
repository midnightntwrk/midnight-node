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

use derive_more::derive::{Deref, From};
use indexer_common::{
    domain::{ByteVec, LedgerStateStorage, NetworkId, ProtocolVersion, ledger},
    error::BoxError,
};
use log::debug;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct LedgerStateCache(RwLock<LedgerState>);

impl LedgerStateCache {
    #[allow(missing_docs)]
    pub fn new(network_id: NetworkId) -> Self {
        Self(RwLock::new(LedgerState::new(network_id)))
    }

    /// Create a collapsed update from the given start index to the given end index for the given
    /// protocol version.
    pub async fn collapsed_update(
        &self,
        start_index: u64,
        end_index: u64,
        ledger_state_storage: &impl LedgerStateStorage,
        protocol_version: ProtocolVersion,
    ) -> Result<MerkleTreeCollapsedUpdate, LedgerStateCacheError> {
        // Acquire a read lock.
        let mut ledger_state_read = self.0.read().await;

        // Check if the current zswap state is stale and needs to be updated.
        if end_index >= ledger_state_read.zswap_first_free() {
            debug!(
                end_index,
                first_free = ledger_state_read.zswap_first_free();
                "zswap state is stale"
            );

            // Release the read lock and acquire a write lock.
            drop(ledger_state_read);
            let mut ledger_state_write = self.0.write().await;

            // Check if the state has been updated in the meantime.
            if end_index >= ledger_state_write.zswap_first_free() {
                debug!(
                    end_index,
                    first_free = ledger_state_write.zswap_first_free();
                    "zswap state is still stale, loading"
                );

                let ledger_state_and_protocol_version = ledger_state_storage
                    .load_ledger_state()
                    .await
                    .map_err(|error| LedgerStateCacheError::Load(error.into()))?
                    .map(|(ledger_state, _, protocol_version)| (ledger_state, protocol_version));

                match ledger_state_and_protocol_version {
                    Some((ledger_state, protocol_version)) => {
                        let ledger_state =
                            ledger::LedgerState::deserialize(ledger_state, protocol_version)?
                                .into();

                        *ledger_state_write = ledger_state;
                    }

                    None => return Err(LedgerStateCacheError::NotFound),
                }
            }

            ledger_state_read = ledger_state_write.downgrade();
        }

        debug!(start_index, end_index; "creating collapsed update");

        let collapsed_update =
            ledger_state_read.collapsed_update(start_index, end_index, protocol_version)?;

        Ok(collapsed_update)
    }
}

#[derive(Debug, Error)]
pub enum LedgerStateCacheError {
    #[error("cannot load ledger state")]
    Load(#[source] BoxError),

    #[error("no ledger state stored")]
    NotFound,

    #[error(transparent)]
    Ledger(#[from] ledger::Error),
}

/// Wrapper around LedgerState from indexer_common.
#[derive(Debug, Clone, From, Deref)]
pub struct LedgerState(ledger::LedgerState);

impl LedgerState {
    #[allow(missing_docs)]
    pub fn new(network_id: NetworkId) -> Self {
        Self(ledger::LedgerState::new(network_id))
    }

    /// Produce a collapsed Merkle Tree from this ledger state.
    pub fn collapsed_update(
        &self,
        start_index: u64,
        end_index: u64,
        protocol_version: ProtocolVersion,
    ) -> Result<MerkleTreeCollapsedUpdate, ledger::Error> {
        let update = self.0.collapsed_update(start_index, end_index)?;

        Ok(MerkleTreeCollapsedUpdate {
            start_index,
            end_index,
            update,
            protocol_version,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleTreeCollapsedUpdate {
    pub start_index: u64,
    pub end_index: u64,
    pub update: ByteVec,
    pub protocol_version: ProtocolVersion,
}
