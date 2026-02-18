// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Ledger 7 version wrapper — mirrors ledger_8.rs structure.
// Builders are not yet compiled here because the `BuildTxs` trait and CLI args
// types are defined against ledger 8 types. Individual builders will be enabled
// once the traits are parameterised per ledger version.
#[allow(unused_imports)]
pub use midnight_node_ledger_helpers::ledger_7 as ledger_helpers_local;
