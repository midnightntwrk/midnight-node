// This file is part of midnight-node.
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

//! Ledger-9 test utilities.
//!
//! Ledger 9 is built with the `test-utilities` feature, so this is a straight re-export of
//! the ledger's own module. The `ledger_8` copy of this file vendors the same surface by
//! hand — see the note there for why v8 cannot enable the upstream feature.

pub use crate::ledger_9::mn_ledger::test_utilities::*;
