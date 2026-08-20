// This file is part of midnight-ledger.
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

use crate::structure::TransactionHash;
use coin_structure::coin::QualifiedInfo;

#[derive(Clone)]
pub struct ZswapStateChanges {
    pub received_coins: Vec<QualifiedInfo>,
    pub spent_coins: Vec<QualifiedInfo>,
    pub source: TransactionHash,
}

impl ZswapStateChanges {
    pub fn can_merge(&self, other: &ZswapStateChanges) -> bool {
        self.source == other.source
    }

    pub fn merge(&mut self, other: ZswapStateChanges) {
        self.received_coins.extend(other.received_coins);
        self.spent_coins.extend(other.spent_coins);
    }
}

pub struct WithZswapStateChanges<T> {
    pub changes: Vec<ZswapStateChanges>,
    pub result: T,
}

impl<T> WithZswapStateChanges<T> {
    pub fn new(result: T) -> WithZswapStateChanges<T> {
        WithZswapStateChanges {
            changes: Vec::new(),
            result,
        }
    }
}

impl<T> WithZswapStateChanges<T> {
    pub fn add_change(mut self, change: ZswapStateChanges) -> Self {
        if let Some(last_change) = self.changes.last_mut() {
            if last_change.can_merge(&change) {
                last_change.merge(change);
            } else {
                self.changes.push(change);
            }
        } else {
            // No existing changes, just push
            self.changes.push(change);
        }

        WithZswapStateChanges {
            changes: self.changes,
            result: self.result,
        }
    }

    pub fn maybe_add_change(self, maybe_change: Option<ZswapStateChanges>) -> Self {
        match maybe_change {
            Some(change) => self.add_change(change),
            None => self,
        }
    }

    pub fn with_result(self, result: T) -> Self {
        WithZswapStateChanges {
            changes: self.changes,
            result,
        }
    }
}
