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

//! With nothing built yet, the on-disk arena replaces the start-up in-memory one.
//! Its own test binary, so no other test has touched the arena first.

use midnight_ledger_unsafe_helpers::{
	DefaultDB, LedgerState, Sp,
	arena_db::{install_persistent_storage, is_persistent},
};

#[test]
fn the_on_disk_arena_replaces_the_empty_in_memory_one() {
	let dir = tempfile::tempdir().unwrap();
	install_persistent_storage(dir.path()).expect("swap the empty start-up arena");
	assert!(is_persistent());
	let _state: Sp<LedgerState<DefaultDB>, DefaultDB> = Sp::new(LedgerState::new("undeployed"));
}
