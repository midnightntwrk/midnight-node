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

//! Ledger-8 → ledger-9 fork.
//!
//! There is no live migration from L8 to L9. New L9 chains start from genesis
//! on L9; existing L8 chains stay on L8. This module exists so the fork-aware
//! dispatch keeps its shape (mirroring [`crate::fork::fork_7_to_8`]) — but
//! `fork_context_8_to_9` panics if anything tries to invoke a state migration.
//!
//! When/if a live L8→L9 fork is needed, replace the body below with a real
//! conversion modeled on `fork_7_to_8::fork_context_7_to_8`.

type Db8 = crate::ledger_8::DefaultDB;
type Db9 = crate::ledger_9::DefaultDB;

use crate::ledger_8::LedgerContext as LedgerContext8;
use crate::ledger_9::LedgerContext as LedgerContext9;

pub fn fork_context_8_to_9(
	_context8: LedgerContext8<Db8>,
) -> Result<LedgerContext9<Db9>, std::io::Error> {
	panic!(
		"L8 → L9 state migration is not implemented; L9 chains must start at genesis. \
		 If you need to migrate live L8 state, implement this function modeled on \
		 `fork_context_7_to_8`."
	)
}
