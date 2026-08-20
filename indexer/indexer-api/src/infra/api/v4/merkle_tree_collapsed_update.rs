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

use crate::{
    domain,
    infra::api::v4::{HexEncodable, HexEncoded},
};
use async_graphql::SimpleObject;
use derive_more::Debug;

/// A Merkle tree collapsed update between two indices.
#[derive(Debug, Clone, SimpleObject)]
pub struct MerkleTreeCollapsedUpdate {
    /// The start index.
    pub start_index: u64,

    /// The end index.
    pub end_index: u64,

    /// The hex-encoded value.
    #[debug(skip)]
    pub update: HexEncoded,

    /// The protocol version.
    pub protocol_version: u32,
}

impl From<domain::MerkleTreeCollapsedUpdate> for MerkleTreeCollapsedUpdate {
    fn from(value: domain::MerkleTreeCollapsedUpdate) -> Self {
        let domain::MerkleTreeCollapsedUpdate {
            start_index,
            end_index,
            update,
            protocol_version,
        } = value;

        Self {
            start_index,
            end_index,
            update: update.hex_encode(),
            protocol_version: protocol_version.into(),
        }
    }
}

// TODO: Remove once deprecated fields are removed from the schema.
/// A Merkle tree collapsed update between two indices.
#[derive(Debug, SimpleObject)]
pub struct CollapsedMerkleTree {
    /// The start index.
    pub start_index: u64,

    /// The end index.
    pub end_index: u64,

    /// The hex-encoded value.
    #[debug(skip)]
    pub update: HexEncoded,

    /// The protocol version.
    pub protocol_version: u32,
}

impl From<MerkleTreeCollapsedUpdate> for CollapsedMerkleTree {
    fn from(update: MerkleTreeCollapsedUpdate) -> Self {
        let MerkleTreeCollapsedUpdate {
            start_index,
            end_index,
            update,
            protocol_version,
        } = update;

        Self {
            start_index,
            end_index,
            update,
            protocol_version,
        }
    }
}
