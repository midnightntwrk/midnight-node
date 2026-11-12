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

//! A thread holding the in-memory arena blocks the swap to the on-disk one.
//! Its own test binary, so no other test has touched the arena first.

use midnight_ledger_unsafe_helpers::arena_db::{
	InstallError, install_persistent_storage, is_persistent, try_default_storage,
};

#[test]
fn a_held_arena_is_not_replaced() {
	let held = try_default_storage().unwrap();
	let dir = tempfile::tempdir().unwrap();
	let err = install_persistent_storage(dir.path()).err();
	assert!(matches!(err, Some(InstallError::InUse)), "{err:?}");
	assert!(!is_persistent());
	assert!(std::sync::Arc::ptr_eq(&held, &try_default_storage().unwrap()));
}
