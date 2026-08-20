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

use indexer_common::domain::{ByteVec, TransactionHash};

/// A shielded nullifier transaction for the subscription stream.
#[derive(Debug, Clone)]
pub struct ShieldedNullifierTransaction {
    pub transaction_id: u64,
    pub transaction_hash: TransactionHash,
    pub block_hash: ByteVec,
    pub block_height: u32,
    pub nullifier: ByteVec,
}
