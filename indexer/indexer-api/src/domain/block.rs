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

use indexer_common::domain::{BlockAuthor, BlockHash, ProtocolVersion};
use sqlx::prelude::FromRow;

/// Relevant block data from the perspective of the Indexer API.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Block {
    #[sqlx(try_from = "i64")]
    pub id: u64,

    pub hash: BlockHash,

    #[sqlx(try_from = "i64")]
    pub height: u32,

    #[sqlx(try_from = "i64")]
    pub protocol_version: ProtocolVersion,

    pub parent_hash: BlockHash,

    #[cfg_attr(
        feature = "standalone",
        sqlx(try_from = "indexer_common::infra::sqlx::SqlxOption<&'a [u8]>")
    )]
    pub author: Option<BlockAuthor>,

    #[sqlx(try_from = "i64")]
    pub timestamp: u64,
}
