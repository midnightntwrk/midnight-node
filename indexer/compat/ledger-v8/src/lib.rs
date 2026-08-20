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

//! This crate implements transaction assembly and semantics for Midnight as a prototype.

#[macro_use]
extern crate tracing;

pub mod annotation;
pub mod construct;
pub mod dust;
pub mod error;
pub mod events;
#[path = "tracing.rs"]
mod ledger_tracing;
mod primitive;
mod prior_versions;
pub mod prove;
pub mod semantics;
pub mod structure;
mod utils;
pub mod verify;
pub mod zswap;

pub use ledger_tracing::{LogLevel, init_logger};

#[cfg(feature = "test-utilities")]
pub mod test_utilities;

#[cfg(feature = "unstable")]
const _: &str = env!(
    "MIDNIGHT_LEDGER_EXPERIMENTAL",
    "attempted to use experimental feature without setting `MIDNIGHT_LEDGER_EXPERIMENTAL`."
);
