#[cfg(feature = "std")]
use crate::ledger_9::Bridge;
use crate::{
	common::types::{
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
use {
	midnight_primitives_ledger::{LedgerStorageDb, LedgerStorageExt},
	sp_externalities::{Externalities, ExternalitiesExt},
};

#[cfg(feature = "std")]
type Signature = crate::ledger_9::TransactionSignature;

// `Bridge<S, D>` instantiates `default_storage::<D>()` lookups against
// `Storage<D>`'s TypeId. The two storage modes register storages with different
// `D`s — separate uses the default ParityDb (column offset 0); unified uses
// ParityDb with column offset = NUM_COLUMNS_POLKADOT, sharing one parity-db
// instance with substrate state. Each host call therefore reads
// `LedgerStorageExt` and dispatches to the matching `D`.
#[cfg(feature = "std")]
type DbSeparate = crate::ledger_9::ledger_storage_local::db::ParityDb;
#[cfg(feature = "std")]
type DbUnified = crate::ledger_9::ledger_storage_local::db::ParityDb<
	sha2::Sha256,
	crate::ledger_9::ledger_storage_local::db::paritydb::OwnedDb,
	{ LedgerStorageExt::COLUMN_OFFSET },
>;

#[cfg(feature = "std")]
fn is_unified(mut ext: &mut dyn Externalities) -> bool {
	matches!(
		ext.extension::<LedgerStorageExt>().map(|e| &e.0.db),
		Some(LedgerStorageDb::UnifiedDb(_)),
	)
}

#[runtime_interface]
pub trait Ledger9Bridge {
	fn set_default_storage(&mut self) {
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::set_default_storage(*self)
		} else {
			Bridge::<Signature, DbSeparate>::set_default_storage(*self)
		}
	}

	fn flush_storage(&mut self) {
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::flush_storage(*self)
		} else {
			Bridge::<Signature, DbSeparate>::flush_storage(*self)
		}
	}

	fn post_block_update(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::post_block_update(*self, state_key, block_context)
		} else {
			Bridge::<Signature, DbSeparate>::post_block_update(*self, state_key, block_context)
		}
	}

	// Current Enabled Version
	fn get_version(&mut self) -> AllocateAndReturnFatPointer<Vec<u8>> {
		// Dispatch on storage mode even though `get_version` doesn't read storage today —
		// avoids a footgun if it grows a storage dependency later.
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::get_version()
		} else {
			Bridge::<Signature, DbSeparate>::get_version()
		}
	}

	/*
	 * apply_transaction()
	 */
	fn apply_transaction(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		runtime_version: u32,
	) -> AllocateAndReturnByCodec<Result<TransactionAppliedStateRoot, LedgerApiError>> {
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::apply_transaction(
				*self,
				state_key,
				tx,
				block_context,
				true,
				runtime_version,
			)
		} else {
			Bridge::<Signature, DbSeparate>::apply_transaction(
				*self,
				state_key,
				tx,
				block_context,
				true,
				runtime_version,
			)
		}
	}

	fn apply_system_transaction(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		_runtime_version: u32,
	) -> AllocateAndReturnByCodec<Result<SystemTransactionAppliedStateRoot, LedgerApiError>> {
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::apply_system_transaction(
				*self,
				state_key,
				tx,
				block_context,
			)
		} else {
			Bridge::<Signature, DbSeparate>::apply_system_transaction(
				*self,
				state_key,
				tx,
				block_context,
			)
		}
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
		let (hash, _) = if is_unified(*self) {
			Bridge::<Signature, DbUnified>::validate_transaction(
				*self,
				state_key,
				tx,
				block_context,
				runtime_version,
				max_weight,
				false,
			)?
		} else {
			Bridge::<Signature, DbSeparate>::validate_transaction(
				*self,
				state_key,
				tx,
				block_context,
				runtime_version,
				max_weight,
				false,
			)?
		};

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
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::validate_guaranteed_execution(
				*self,
				state_key,
				tx,
				block_context,
				runtime_version,
			)
		} else {
			Bridge::<Signature, DbSeparate>::validate_guaranteed_execution(
				*self,
				state_key,
				tx,
				block_context,
				runtime_version,
			)
		}
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
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::get_contract_state(state_key, contract_address)
		} else {
			Bridge::<Signature, DbSeparate>::get_contract_state(state_key, contract_address)
		}
	}

	/*
	 * get_decoded_transaction()
	 */
	// Current Enabled Version
	fn get_decoded_transaction(
		&mut self,
		transaction_bytes: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Tx, LedgerApiError>> {
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::get_decoded_transaction(transaction_bytes)
		} else {
			Bridge::<Signature, DbSeparate>::get_decoded_transaction(transaction_bytes)
		}
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
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::get_zswap_chain_state(state_key, contract_address)
		} else {
			Bridge::<Signature, DbSeparate>::get_zswap_chain_state(state_key, contract_address)
		}
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
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::get_unclaimed_amount(state_key, beneficiary)
		} else {
			Bridge::<Signature, DbSeparate>::get_unclaimed_amount(state_key, beneficiary)
		}
	}

	/*
	 * Returns the Ledger Parameters
	 */
	// Current Enabled Version
	fn get_ledger_parameters(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::get_ledger_parameters(state_key)
		} else {
			Bridge::<Signature, DbSeparate>::get_ledger_parameters(state_key)
		}
	}

	/*
	 * Returns the minimum bridge transfer amount from ledger parameters
	 * This is denominated in STARs (atomic night units)
	 */
	fn get_c_to_m_bridge_min_amount(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<u128, LedgerApiError>> {
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::get_c_to_m_bridge_min_amount(state_key)
		} else {
			Bridge::<Signature, DbSeparate>::get_c_to_m_bridge_min_amount(state_key)
		}
	}

	/*
	 * Returns the expected fee to pay for a submitting a transaction
	 */
	fn get_transaction_cost(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		max_weight: u64,
	) -> AllocateAndReturnByCodec<Result<GasCost, LedgerApiError>> {
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::get_transaction_cost(
				state_key,
				tx,
				&block_context,
				max_weight,
			)
		} else {
			Bridge::<Signature, DbSeparate>::get_transaction_cost(
				state_key,
				tx,
				&block_context,
				max_weight,
			)
		}
	}

	/*
	 * Returns the Zsawp state root
	 */
	// Current Enabled Version
	fn get_zswap_state_root(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::get_zswap_state_root(state_key)
		} else {
			Bridge::<Signature, DbSeparate>::get_zswap_state_root(state_key)
		}
	}

	fn is_governance_allowed_system_tx(&mut self, system_tx: PassFatPointerAndRead<&[u8]>) -> bool {
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::is_governance_allowed_system_tx(system_tx)
		} else {
			Bridge::<Signature, DbSeparate>::is_governance_allowed_system_tx(system_tx)
		}
	}

	/*
	 * Returns the pure ledger state root (without StorableLedgerState wrapping)
	 */
	fn get_ledger_state_root(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::get_ledger_state_root(state_key)
		} else {
			Bridge::<Signature, DbSeparate>::get_ledger_state_root(state_key)
		}
	}

	fn construct_cnight_generates_dust_event(
		&mut self,
		value: PassFatPointerAndDecode<u128>,
		owner: PassFatPointerAndRead<&[u8]>,
		time: u64,
		action: u8,
		nonce: PassFatPointerAndDecode<[u8; 32]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::construct_cnight_generates_dust_event(
				value, owner, time, action, nonce,
			)
		} else {
			Bridge::<Signature, DbSeparate>::construct_cnight_generates_dust_event(
				value, owner, time, action, nonce,
			)
		}
	}

	fn construct_cnight_generates_dust_system_tx(
		&mut self,
		events: PassFatPointerAndDecode<Vec<Vec<u8>>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::construct_cnight_generates_dust_system_tx(events)
		} else {
			Bridge::<Signature, DbSeparate>::construct_cnight_generates_dust_system_tx(events)
		}
	}

	fn construct_distribute_night_cardano_bridge_system_tx(
		&mut self,
		amount: PassFatPointerAndDecode<u128>,
		target_address_bytes: PassFatPointerAndRead<&[u8]>,
		nonce_bytes: PassFatPointerAndDecode<[u8; 32]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::construct_distribute_night_cardano_bridge_system_tx(
				amount,
				target_address_bytes,
				nonce_bytes,
			)
		} else {
			Bridge::<Signature, DbSeparate>::construct_distribute_night_cardano_bridge_system_tx(
				amount,
				target_address_bytes,
				nonce_bytes,
			)
		}
	}

	fn construct_distribute_reserve_system_tx(
		&mut self,
		amount: PassFatPointerAndDecode<u128>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::construct_distribute_reserve_system_tx(amount)
		} else {
			Bridge::<Signature, DbSeparate>::construct_distribute_reserve_system_tx(amount)
		}
	}

	fn construct_unlock_to_treasury_system_tx(
		&mut self,
		amount: PassFatPointerAndDecode<u128>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		if is_unified(*self) {
			Bridge::<Signature, DbUnified>::construct_unlock_to_treasury_system_tx(amount)
		} else {
			Bridge::<Signature, DbSeparate>::construct_unlock_to_treasury_system_tx(amount)
		}
	}

	/// Ensures the correct ledger storage is initialized for this runtime version.
	/// Handles rollback: if new version's storage is initialized but we need this version's storage,
	/// drops new version's storage and initializes normal storage.
	/// Returns true if storage was (re)initialized, false if already correct.
	fn ensure_storage_initialized(&mut self) -> bool {
		use ledger_storage_ledger_8::storage::try_get_default_storage;

		let unified = is_unified(*self);

		// If normal storage already exists, we're good
		let already_initialized = if unified {
			try_get_default_storage::<DbUnified>().is_some()
		} else {
			try_get_default_storage::<DbSeparate>().is_some()
		};
		if already_initialized {
			return false;
		}

		crate::drop_all_default_storage();
		// Initialize normal storage
		if unified {
			Bridge::<Signature, DbUnified>::set_default_storage(*self);
		} else {
			Bridge::<Signature, DbSeparate>::set_default_storage(*self);
		}
		true
	}
}
