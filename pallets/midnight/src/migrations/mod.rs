// This file is part of midnight-node.
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

/// # Multi-Block Migrations Module
/// A unique identifier across all pallets.
///
/// This constant represents a unique identifier for the migrations of this pallet.
/// It helps differentiate migrations for this pallet from those of others. Note that we don't
/// directly pull the crate name from the environment, since that would change if the crate were
/// ever to be renamed and could cause historic migrations to run again.
pub const PALLET_MIGRATIONS_ID: &[u8; 19] = b"pallet-midnight-mbm";

// See https://github.com/input-output-hk/midnight-substrate-prototype/pull/382
// for the example of such a migration.
// pub mod v1;
