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

use serialize::tag_enforcement_test;
use serialize::{self, Deserializable, Serializable, Tagged};
use std::fmt::Debug;
use std::hash::Hash;
use storage::Storable;
use storage::arena::ArenaKey;
use storage::db::DB;
use storage::merkle_patricia_trie::HasSize;
use storage::merkle_patricia_trie::Monoid;
use storage::merkle_patricia_trie::Semigroup;
use storage::storable::Loader;

/// An annotation holding the size of a structure and its `Utxo` value
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serializable, Storable, Hash)]
#[storable(base)]
#[tag = "night-annotation"]
pub struct NightAnn {
    pub size: u64,
    pub value: u128,
}
tag_enforcement_test!(NightAnn);

impl Semigroup for NightAnn {
    fn append(&self, other: &Self) -> Self {
        NightAnn {
            size: self.size.append(&other.size),
            value: self.value.append(&other.value),
        }
    }
}

impl Monoid for NightAnn {
    fn empty() -> Self {
        NightAnn { size: 0, value: 0 }
    }
}

impl HasSize for NightAnn {
    fn get_size(&self) -> u64 {
        self.size
    }

    fn set_size(&self, x: u64) -> Self {
        NightAnn {
            size: x,
            value: self.value,
        }
    }
}
