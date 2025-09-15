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

use indexer_common::domain::{ProtocolVersion, ScaleDecodeProtocolVersionError};
use subxt::config::{
    Hasher,
    polkadot::U256,
    substrate::{ConsensusEngineId, DigestItem, SubstrateHeader},
};

const VERSION_ID: ConsensusEngineId = *b"MNSV";

/// Extension methods for Substrate block headers.
pub trait SubstrateHeaderExt<N> {
    /// Try to decode the [ProtocolVersion] from this Substrate block header.
    fn protocol_version(&self) -> Result<Option<ProtocolVersion>, ScaleDecodeProtocolVersionError>;
}

impl<N: Copy + Into<U256> + TryFrom<U256>, H: Hasher> SubstrateHeaderExt<N>
    for SubstrateHeader<N, H>
{
    fn protocol_version(&self) -> Result<Option<ProtocolVersion>, ScaleDecodeProtocolVersionError> {
        self.digest
            .logs
            .iter()
            .filter_map(|item| match item {
                DigestItem::Consensus(VERSION_ID, data) => {
                    Some(ProtocolVersion::try_from(data.as_slice()))
                }

                _ => None,
            })
            .next()
            .transpose()
    }
}
