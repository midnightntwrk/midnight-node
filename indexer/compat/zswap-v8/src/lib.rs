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

#![deny(unreachable_pub)]
#![deny(warnings)]
#![allow(unused_imports)]

#[macro_use]
extern crate tracing;
#[macro_use]
extern crate lazy_static;

pub const ZSWAP_TREE_HEIGHT: u8 = 32;

pub(crate) fn ciphertext_to_field(c: &CoinCiphertext) -> transient_crypto::curve::Fr {
    use transient_crypto::hash::{transient_commit, transient_hash};
    transient_commit(
        &c.ciph[..],
        transient_hash(&[
            transient_crypto::curve::Fr::from_le_bytes(b"midnight:zswap-ciphertext")
                .expect("Domain sep should be in range for field"),
            c.c.x().unwrap_or(0.into()),
            c.c.y().unwrap_or(0.into()),
        ]),
    )
}

mod construct;
pub mod error;
pub mod keys;
pub mod ledger;
pub mod local;
pub mod prove;
mod structure;
pub mod verify;

use midnight_onchain_runtime::{ops::Op, result_mode::ResultMode};
use storage::db::DB;

pub(crate) fn filter_invalid<M: ResultMode<D>, I: Iterator<Item = Op<M, D>>, D: DB>(
    iter: I,
) -> impl Iterator<Item = Op<M, D>> {
    iter.filter(|op| match op {
        Op::Idx { path, .. } => !path.is_empty(),
        Op::Ins { n, .. } => *n != 0,
        _ => true,
    })
}

pub use structure::*;
