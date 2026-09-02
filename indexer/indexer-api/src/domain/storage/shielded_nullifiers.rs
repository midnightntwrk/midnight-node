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

use crate::domain::{shielded_nullifier::ShieldedNullifierTransaction, storage::NoopStorage};
use futures::{Stream, stream};
use std::num::NonZeroU32;

/// Storage for zswap (shielded) nullifier transaction queries.
#[trait_variant::make(Send)]
pub trait ShieldedNullifiersStorage
where
    Self: Clone + Send + Sync + 'static,
{
    /// Get transactions containing zswap nullifiers matching a prefix.
    async fn get_shielded_nullifier_transactions(
        &self,
        nullifier_prefixes: &[Vec<u8>],
        from_block: u64,
        to_block: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<ShieldedNullifierTransaction, sqlx::Error>> + Send;
}

#[allow(unused_variables)]
impl ShieldedNullifiersStorage for NoopStorage {
    async fn get_shielded_nullifier_transactions(
        &self,
        nullifier_prefixes: &[Vec<u8>],
        from_block: u64,
        to_block: u64,
        batch_size: NonZeroU32,
    ) -> impl Stream<Item = Result<ShieldedNullifierTransaction, sqlx::Error>> + Send {
        stream::empty()
    }
}
