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

use base_crypto::signatures::Signature;
use ledger_storage::db::DB;
use transient_crypto::commitment::PedersenRandomness;

use mn_ledger::structure::SignatureKind;

pub fn get_root(state: &[u8], network_id: midnight_serialize::NetworkId) -> Vec<u8> {
	// Get empty state key
	use super::latest::api::Ledger;
	use ledger_storage::{DefaultDB, arena::VersionedArenaKey, storage::default_storage};
	use midnight_serialize::Serializable;

	let state: Ledger<DefaultDB> = midnight_serialize::deserialize(state, network_id)
		.expect("Failed to deserialize initial state");
	let state = default_storage::<DefaultDB>().arena.alloc(state);
	let mut bytes = vec![];
	VersionedArenaKey::serialize(&state.hash(), &mut bytes).unwrap();
	bytes
}

pub fn alloc_with_initial_tx<S: SignatureKind<D>, D: DB>(initial_tx: &[u8]) {
	use base_crypto::time::Timestamp;
	use ledger_storage::storage::default_storage;
	use midnight_serialize::Deserializable;
	use mn_ledger::{
		semantics::{TransactionContext, TransactionResult},
		structure::{ProofMarker, Transaction},
	};

	let tx: Transaction<S, ProofMarker, PedersenRandomness, D> =
		Deserializable::deserialize(&mut &initial_tx[..], 0)
			.expect("Failed to deserialize initial state");

	let state = mn_ledger::structure::LedgerState::<D>::new();
	let context = TransactionContext::default();
	let (mut state, res) = state.apply(&tx, &context);
	state = state.post_block_update(Timestamp::from_secs(0));

	assert!(matches!(res, TransactionResult::Success), "Failed to apply initial transaction");

	let state = default_storage::<D>().arena.alloc(state);
	state.persist();
	default_storage::<D>().with_backend(|backend| backend.flush_all_changes_to_db());
}

pub fn init_storage_paritydb(dir: &std::path::Path, initial_tx: &[u8], cache_size: usize) {
	use ledger_storage::{Storage, db::ParityDb, storage::set_default_storage};

	let res = set_default_storage(|| {
		std::fs::create_dir_all(dir)
			.unwrap_or_else(|_| panic!("Failed to create dir {}", dir.display()));

		let db = ParityDb::<sha2::Sha256>::open(dir);
		Storage::new(cache_size, db)
	});
	if res.is_err() {
		log::warn!("Warning: Failed to set default storage: {:?}", res);
	}

	alloc_with_initial_tx::<Signature, ParityDb>(initial_tx);
}
