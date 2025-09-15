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

pub mod storage;

mod api;
mod block;
mod contract_action;
mod ledger_state;
mod transaction;
mod unshielded;

pub use api::*;
pub use block::*;
pub use contract_action::*;
pub use ledger_state::*;
pub use transaction::*;
pub use unshielded::*;

use indexer_common::domain::{PROTOCOL_VERSION_000_016_000, ProtocolVersion};

/// This must always point to the latest (highest) supported version.
pub const PROTOCOL_VERSION: ProtocolVersion = PROTOCOL_VERSION_000_016_000;
