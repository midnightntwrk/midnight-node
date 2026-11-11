// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
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

//! The in-memory arena is installed when the process starts, so ledger objects can be
//! built with no setup call, and the on-disk arena cannot replace it once it holds any.

use midnight_ledger_unsafe_helpers::{
	DefaultDB, LedgerState, Sp,
	arena_db::{InstallError, install_persistent_storage, is_persistent, try_default_storage},
};

#[test]
fn ledger_objects_need_no_setup_and_pin_the_in_memory_arena() {
	let _ledger: Sp<LedgerState<DefaultDB>, DefaultDB> = Sp::new(LedgerState::new("undeployed"));
	// Small objects are stored inline; one past the small-object limit is tracked.
	let state: Sp<[u32; 256], DefaultDB> = Sp::new(std::array::from_fn(|i| i as u32));
	assert!(try_default_storage().unwrap().arena.size() > 0);

	let dir = tempfile::tempdir().unwrap();
	let err = install_persistent_storage(dir.path()).err();
	assert!(matches!(err, Some(InstallError::AlreadyInstalled { persistent: false })));
	assert!(!is_persistent());
	drop(state);
}
