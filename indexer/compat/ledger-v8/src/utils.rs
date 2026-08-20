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

use std::io::Read;

use serialize::Serializable;
use storage::{Storable, arena::Sp, db::DB, storage::HashMap};

#[allow(unused)]
pub(crate) struct CapturingReader<R: Read> {
    inner: R,
    pub(crate) data: Vec<u8>,
}

impl<R: Read> Read for CapturingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buf)?;

        self.data.extend(&buf[..read]);
        Ok(read)
    }
}

impl<R: Read> CapturingReader<R> {
    #[allow(unused)]
    pub(crate) fn with_data(reader: R, data: Vec<u8>) -> Self {
        CapturingReader {
            inner: reader,
            data,
        }
    }
}

pub(crate) trait SortedIter {
    type Item<'a>
    where
        Self: 'a;
    fn sorted_iter(&self) -> impl Iterator<Item = Self::Item<'_>>;
}

pub(crate) trait KeySortedIter {
    type Item<'a>
    where
        Self: 'a;
    fn sorted_values_by_key(&self) -> impl Iterator<Item = Self::Item<'_>>;
}

impl<K: Ord + Serializable + Storable<D>, V: Storable<D>, D: DB> SortedIter for HashMap<K, V, D> {
    type Item<'a>
        = (Sp<K, D>, Sp<V, D>)
    where
        Self: 'a;
    fn sorted_iter(&self) -> impl Iterator<Item = Self::Item<'_>> {
        let mut items = self.iter().map(|sp| (*sp).clone()).collect::<Vec<_>>();
        items.sort_by_key(|a| a.0.clone());
        items.into_iter()
    }
}

impl<K: Ord + Serializable + Storable<D>, V: Storable<D>, D: DB> KeySortedIter
    for HashMap<K, V, D>
{
    type Item<'a>
        = Sp<V, D>
    where
        Self: 'a;
    fn sorted_values_by_key(&self) -> impl Iterator<Item = Self::Item<'_>> {
        let mut items = self.iter().map(|sp| (*sp).clone()).collect::<Vec<_>>();
        items.sort_by_key(|a| a.0.clone());
        items.into_iter().map(|(_, v)| v)
    }
}
