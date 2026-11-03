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

#[cfg(feature = "std")]
use crate::ledger_9::Bridge;
use crate::{
	boundary::types::{
		GasCost, Hash, SystemTransactionAppliedStateRoot, TransactionAppliedStateRoot, Tx,
	},
	ledger_9::{BlockContext, types::LedgerApiError},
};
use alloc::vec::Vec;
use sp_runtime_interface::pass_by::{
	AllocateAndReturnByCodec, AllocateAndReturnFatPointer, PassFatPointerAndDecode,
	PassFatPointerAndRead,
};
use sp_runtime_interface::runtime_interface;

#[cfg(feature = "std")]
use midnight_primitives_ledger::LedgerStorageExt;

#[cfg(feature = "std")]
type Signature = crate::ledger_9::TransactionSignature;

// `Bridge<S, D>` instantiates `default_storage::<D>()` lookups against `Storage<D>`'s TypeId, so
// every host call must name the one instantiation the node registers: the ledger columns appended
// after Substrate's in the shared parity-db instance (column offset = NUM_COLUMNS_POLKADOT).
#[cfg(feature = "std")]
type Db = crate::ledger_9::ledger_storage_local::db::ParityDb<
	sha2::Sha256,
	crate::ledger_9::ledger_storage_local::db::paritydb::OwnedDb,
	{ LedgerStorageExt::COLUMN_OFFSET },
>;

#[cfg(feature = "std")]
use crate::ledger_8::Bridge as Bridge8;
#[cfg(feature = "std")]
type Signature8 = crate::ledger_8::TransactionSignature;

/// Translate a ledger-8 `LedgerApiError` into its ledger-9 counterpart.
///
/// The two are distinct types declared identically in `ledger_8/types.rs` and
/// `ledger_9/types.rs`, so their SCALE encodings are identical by construction. Round-tripping keeps this correct
/// when a variant is added, where a hand-written match would need editing in
/// lockstep. `ledger_8_error_encoding_matches_ledger_9` guards the assumption.
#[cfg(feature = "std")]
fn as_ledger_9_error(error: crate::ledger_8::types::LedgerApiError) -> LedgerApiError {
	use parity_scale_codec::{Decode, Encode};
	LedgerApiError::decode(&mut &error.encode()[..]).unwrap_or(LedgerApiError::HostApiError)
}

/// Serve a read-only ledger accessor from the ledger-8 bridge when `$state_key`
/// is a ledger-8 arena root, by returning early from the enclosing host function.
/// Falls through to the ledger-9 body otherwise.
///
/// For the ledger 8 -> 9 hard-fork, `system_version == 1` which means runtime code
/// is applied during the upgrade block rather than queued to be applied in the next
/// block. This code allows off-chain runtime calls to access historic block data
/// using the correct ledger api despite the runtime code/chain data skew.
///
/// This will not be needed for future forks; see:
/// - https://github.com/midnightntwrk/midnight-node/pull/1900
///
/// `$call` names the `Bridge` method and takes its arguments verbatim; only the
/// error translation is supplied here.
#[cfg(feature = "std")]
macro_rules! serve_pre_migration_v8_read {
	($state_key:expr, $call:ident($($arg:expr),* $(,)?)) => {
		if crate::is_ledger_8_state_key($state_key) {
			let result = Bridge8::<Signature8, Db>::$call($($arg),*)
;
			return result.map_err(as_ledger_9_error);
		}
	};
}

#[runtime_interface]
pub trait Ledger9Bridge {
	fn set_default_storage(&mut self) {
		Bridge::<Signature, Db>::set_default_storage(*self)
	}

	fn flush_storage(&mut self) {
		Bridge::<Signature, Db>::flush_storage(*self)
	}

	fn post_block_update(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Db>::post_block_update(*self, state_key, block_context)
	}

	fn apply_post_block_update(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Db>::apply_post_block_update(*self, state_key, block_context)
	}

	// Current Enabled Version
	fn get_version(&mut self) -> AllocateAndReturnFatPointer<Vec<u8>> {
		// Dispatch on storage mode even though `get_version` doesn't read storage today —
		// avoids a footgun if it grows a storage dependency later.
		Bridge::<Signature, Db>::get_version()
	}

	/*
	 * apply_transaction()
	 *
	 * `skew_tblock` is always false here: the tblock correction only covers blocks produced
	 * before the runtime upgrade that closed it, all of which are ledger 8 or older.
	 * See <https://github.com/midnightntwrk/midnight-node/issues/1924>
	 */
	fn apply_transaction(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		runtime_version: u32,
	) -> AllocateAndReturnByCodec<Result<TransactionAppliedStateRoot, LedgerApiError>> {
		Bridge::<Signature, Db>::apply_transaction(
			*self,
			state_key,
			tx,
			block_context,
			true,
			runtime_version,
			/* skew_tblock */ false,
		)
	}

	fn apply_system_transaction(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		_runtime_version: u32,
	) -> AllocateAndReturnByCodec<Result<SystemTransactionAppliedStateRoot, LedgerApiError>> {
		Bridge::<Signature, Db>::apply_system_transaction(*self, state_key, tx, block_context)
	}

	fn apply_governance_system_transaction(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		_runtime_version: u32,
	) -> AllocateAndReturnByCodec<Result<SystemTransactionAppliedStateRoot, LedgerApiError>> {
		Bridge::<Signature, Db>::apply_governance_system_transaction(
			*self,
			state_key,
			tx,
			block_context,
		)
	}

	fn apply_cnight_system_transaction(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		_runtime_version: u32,
	) -> AllocateAndReturnByCodec<Result<SystemTransactionAppliedStateRoot, LedgerApiError>> {
		Bridge::<Signature, Db>::apply_cnight_system_transaction(
			*self,
			state_key,
			tx,
			block_context,
		)
	}

	fn apply_bridge_system_transaction(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		_runtime_version: u32,
	) -> AllocateAndReturnByCodec<Result<SystemTransactionAppliedStateRoot, LedgerApiError>> {
		Bridge::<Signature, Db>::apply_bridge_system_transaction(
			*self,
			state_key,
			tx,
			block_context,
		)
	}

	/*
	 * validate_transaction()
	 */
	fn validate_transaction(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		runtime_version: u32,
		// The Runtime's max weight as of now
		max_weight: u64,
	) -> AllocateAndReturnByCodec<Result<Hash, LedgerApiError>> {
		let (hash, _) = Bridge::<Signature, Db>::validate_transaction(
			*self,
			state_key,
			tx,
			block_context,
			runtime_version,
			max_weight,
			false,
		)?;

		Ok(hash)
	}

	/*
	 * validate_guaranteed_execution()
	 *
	 * Validates that the guaranteed part of a transaction will succeed.
	 * Used by pre_dispatch to reject transactions that would fail without paying fees.
	 */
	fn validate_guaranteed_execution(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		runtime_version: u32,
	) -> AllocateAndReturnByCodec<Result<(), LedgerApiError>> {
		Bridge::<Signature, Db>::validate_guaranteed_execution(
			*self,
			state_key,
			tx,
			block_context,
			runtime_version,
			/* skew_tblock */ false,
		)
	}

	/*
	 * get_contract_state()
	 */
	// Current Enabled Version
	fn get_contract_state(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		contract_address: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		serve_pre_migration_v8_read!(state_key, get_contract_state(state_key, contract_address));

		Bridge::<Signature, Db>::get_contract_state(state_key, contract_address)
	}

	/*
	 * get_decoded_transaction()
	 */
	// Current Enabled Version
	fn get_decoded_transaction(
		&mut self,
		transaction_bytes: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Tx, LedgerApiError>> {
		Bridge::<Signature, Db>::get_decoded_transaction(transaction_bytes)
	}

	/*
	 * get_zswap_chain_state()
	 */
	// Current Enabled Version
	fn get_zswap_chain_state(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		contract_address: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		serve_pre_migration_v8_read!(state_key, get_zswap_chain_state(state_key, contract_address));

		Bridge::<Signature, Db>::get_zswap_chain_state(state_key, contract_address)
	}

	/*
	 * Returns the unclaimed amount for a provided beneficiary address
	 */
	// Current Enabled Version
	fn get_unclaimed_amount(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		beneficiary: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<u128, LedgerApiError>> {
		serve_pre_migration_v8_read!(state_key, get_unclaimed_amount(state_key, beneficiary));

		Bridge::<Signature, Db>::get_unclaimed_amount(state_key, beneficiary)
	}

	/*
	 * Returns the unclaimed Cardano-bridge transfer amount for a provided beneficiary address
	 */
	// Current Enabled Version
	fn get_bridge_receiving_amount(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		beneficiary: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<u128, LedgerApiError>> {
		serve_pre_migration_v8_read!(
			state_key,
			get_bridge_receiving_amount(state_key, beneficiary)
		);

		Bridge::<Signature, Db>::get_bridge_receiving_amount(state_key, beneficiary)
	}

	/*
	 * Returns the Ledger Parameters
	 */
	// Current Enabled Version
	fn get_ledger_parameters(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		serve_pre_migration_v8_read!(state_key, get_ledger_parameters(state_key));

		Bridge::<Signature, Db>::get_ledger_parameters(state_key)
	}

	/*
	 * Returns the minimum bridge transfer amount from ledger parameters
	 * This is denominated in STARs (atomic night units)
	 */
	fn get_c_to_m_bridge_min_amount(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<u128, LedgerApiError>> {
		serve_pre_migration_v8_read!(state_key, get_c_to_m_bridge_min_amount(state_key));

		Bridge::<Signature, Db>::get_c_to_m_bridge_min_amount(state_key)
	}

	/*
	 * Returns the expected fee to pay for a submitting a transaction
	 *
	 * No `serve_pre_migration_v8_read!` guard here, unlike the accessors above: a
	 * cost estimate is always requested for a transaction about to be submitted,
	 * and `get_ledger_version` reports ledger 9 as soon as the new code is live, so
	 * `tx` is a v9-format transaction that ledger-8 code cannot deserialize anyway.
	 * The same reasoning covers the transaction paths (`validate_transaction`,
	 * `apply_transaction`, ...): at the skew block they concern v9 transactions, and
	 * they resolve on their own one block later once the migration has run.
	 */
	fn get_transaction_cost(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		max_weight: u64,
	) -> AllocateAndReturnByCodec<Result<GasCost, LedgerApiError>> {
		Bridge::<Signature, Db>::get_transaction_cost(state_key, tx, &block_context, max_weight)
	}

	/*
	 * As v1, but `tx` may also be a `SystemTransaction`: the version dispatches on
	 * the serialized header tag. v1 could only price user transactions, which left
	 * the cNIGHT dust replay migration with no way to ask what a
	 * `CNightGeneratesDustUpdate` batch costs.
	 *
	 * A strict superset of v1 for `Transaction` bytes, and sp-runtime-interface
	 * always binds the runtime to the highest version, so every existing caller
	 * (`pallet_midnight::get_tx_weight` among them) moves here.
	 */
	// Current Enabled Version
	#[version(2)]
	fn get_transaction_cost(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		max_weight: u64,
	) -> AllocateAndReturnByCodec<Result<GasCost, LedgerApiError>> {
		Bridge::<Signature, Db>::get_any_transaction_cost(state_key, tx, &block_context, max_weight)
	}

	/*
	 * Returns the Zsawp state root
	 */
	// Current Enabled Version
	fn get_zswap_state_root(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		serve_pre_migration_v8_read!(state_key, get_zswap_state_root(state_key));

		Bridge::<Signature, Db>::get_zswap_state_root(state_key)
	}

	fn is_governance_allowed_system_tx(&mut self, system_tx: PassFatPointerAndRead<&[u8]>) -> bool {
		Bridge::<Signature, Db>::is_governance_allowed_system_tx(system_tx)
	}

	/*
	 * Returns the pure ledger state root (without StorableLedgerState wrapping)
	 */
	fn get_ledger_state_root(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		serve_pre_migration_v8_read!(state_key, get_ledger_state_root(state_key));

		Bridge::<Signature, Db>::get_ledger_state_root(state_key)
	}

	fn construct_cnight_generates_dust_event(
		&mut self,
		value: PassFatPointerAndDecode<u128>,
		owner: PassFatPointerAndRead<&[u8]>,
		time: u64,
		action: u8,
		nonce: PassFatPointerAndDecode<[u8; 32]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Db>::construct_cnight_generates_dust_event(
			value, owner, time, action, nonce,
		)
	}

	fn construct_cnight_generates_dust_system_tx(
		&mut self,
		events: PassFatPointerAndDecode<Vec<Vec<u8>>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Db>::construct_cnight_generates_dust_system_tx(events)
	}

	fn construct_distribute_night_cardano_bridge_system_tx(
		&mut self,
		amount: PassFatPointerAndDecode<u128>,
		target_address_bytes: PassFatPointerAndRead<&[u8]>,
		nonce_bytes: PassFatPointerAndDecode<[u8; 32]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Db>::construct_distribute_night_cardano_bridge_system_tx(
			amount,
			target_address_bytes,
			nonce_bytes,
		)
	}

	fn construct_distribute_reserve_system_tx(
		&mut self,
		amount: PassFatPointerAndDecode<u128>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Db>::construct_distribute_reserve_system_tx(amount)
	}

	fn construct_unlock_to_treasury_system_tx(
		&mut self,
		amount: PassFatPointerAndDecode<u128>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Db>::construct_unlock_to_treasury_system_tx(amount)
	}

	/// Ensures the correct ledger storage is initialized for this runtime version.
	/// Handles rollback: if new version's storage is initialized but we need this version's storage,
	/// drops new version's storage and initializes normal storage.
	/// Returns true if storage was (re)initialized, false if already correct.
	fn ensure_storage_initialized(&mut self) -> bool {
		use ledger_storage_ledger_8::storage::try_get_default_storage;

		// If normal storage already exists, we're good
		let already_initialized = try_get_default_storage::<Db>().is_some();
		if already_initialized {
			return false;
		}

		crate::drop_all_default_storage();
		// Initialize normal storage
		Bridge::<Signature, Db>::set_default_storage(*self);

		true
	}

	/// Translate the ledger state from ledger-v8 format to ledger-v9 format.
	///
	/// Called by `pallet_midnight`'s v8->v9 storage migration during the runtime
	/// upgrade that crosses into ledger-9. `state_key` is the pallet's `StateKey`
	/// (a v8 arena root); returns the new v9 arena root to store back, together
	/// with the synthetic cost (picoseconds) the translation consumed against
	/// the ledger's cost model, for the pallet to charge as this migration's
	/// weight.
	fn migrate_state_v8_to_v9(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<(Vec<u8>, u64), LedgerApiError>> {
		// Ensure the ledger arena is initialized before translating. The migration
		// runs in the Executive migrations tuple, before pallet_midnight's
		// on_initialize/on_runtime_upgrade have (re)initialized storage this block.
		// `set_default_storage` is idempotent — a no-op if the pre-fork ledger-8
		// blocks already set it (v8 and v9 share the same storage backend).
		// The migration output is persisted under a tagged root; the tag is the overlay's
		// `System::ParentHash`, which this early in the block is still the grandparent's hash
		// (see `overlay_parent_hash`).
		let parent_hash = crate::host_api::migration_8_to_9::overlay_parent_hash(*self);
		Bridge::<Signature, Db>::set_default_storage(*self);
		crate::host_api::migration_8_to_9::migrate_state_v8_to_v9::<Db>(state_key, parent_hash)
	}

	/// Initialize a process-wide temporary ledger ParityDb seeded with the
	/// undeployed-network genesis state.
	///
	/// Seeding with the undeployed genesis state is required because the
	/// chain-spec's `StateKey<T>` is derived from that exact byte payload;
	/// any other seed would produce a different arena root and host calls
	/// like `get_c_to_m_bridge_min_amount` would not resolve.
	///
	/// `init_ledger_storage_standalone` internally calls the storage-core's
	/// once-set `set_default_storage`, so subsequent calls across benchmark
	/// iterations are effectively no-ops; we still gate with a `OnceLock`
	/// to avoid the re-alloc + parity-db lock churn that would otherwise
	/// happen each iteration.
	#[cfg(feature = "runtime-benchmarks")]
	fn register_benchmark_ledger_storage(&mut self) {
		// Can't use `midnight-node-res` dependency, because it transitively depends
		// on this crate (via `pallet-cnight-observation`).
		const GENESIS_STATE_UNDEPLOYED: &[u8] =
			include_bytes!("../../../res/genesis/genesis_state_undeployed.mn");
		use std::sync::OnceLock;
		static INIT: OnceLock<()> = OnceLock::new();
		INIT.get_or_init(|| {
			let mut dir = std::env::temp_dir();
			dir.push(format!("midnight-bench-ledger-{}", std::process::id()));
			let _state_key =
				crate::init_ledger_storage_standalone(dir, GENESIS_STATE_UNDEPLOYED, 10_000);
		});
	}
}

#[cfg(all(test, feature = "std"))]
mod tests {
	use super::as_ledger_9_error;
	use crate::{ledger_8::types as v8, ledger_9::types as v9};

	/// `as_ledger_9_error` relies on the two versions' `LedgerApiError` sharing a
	/// SCALE encoding, which holds because `ledger_8/types.rs` and
	/// `ledger_9/types.rs` declare it identically. Pin that down — including a nested payload and
	/// the last variant, which is where a divergence would first show up — so a
	/// future edit to one version's enum fails here rather than silently turning
	/// every pre-migration read error into `HostApiError`.
	#[test]
	fn ledger_8_error_encoding_matches_ledger_9() {
		let cases = [
			(v8::LedgerApiError::NoLedgerState, v9::LedgerApiError::NoLedgerState),
			(v8::LedgerApiError::ContractNotPresent, v9::LedgerApiError::ContractNotPresent),
			(v8::LedgerApiError::BeneficiaryNotFound, v9::LedgerApiError::BeneficiaryNotFound),
			(
				v8::LedgerApiError::Deserialization(v8::DeserializationError::TypedArenaKey),
				v9::LedgerApiError::Deserialization(v9::DeserializationError::TypedArenaKey),
			),
			(
				v8::LedgerApiError::Serialization(v8::SerializationError::LedgerParameters),
				v9::LedgerApiError::Serialization(v9::SerializationError::LedgerParameters),
			),
		];

		for (from, expected) in cases {
			assert_eq!(as_ledger_9_error(from.clone()), expected, "mistranslated {from:?}");
		}
	}
}
