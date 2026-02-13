#[cfg(feature = "std")]
use crate::ledger_8::Bridge;
use crate::{
	common::types::{
		GasCost, Hash, SystemTransactionAppliedStateRoot, TransactionAppliedStateRoot, Tx,
	},
	ledger_8::{BlockContext, types::LedgerApiError},
};
use alloc::vec::Vec;
use sp_runtime_interface::pass_by::{
	AllocateAndReturnByCodec, AllocateAndReturnFatPointer, PassFatPointerAndDecode,
	PassFatPointerAndRead,
};
use sp_runtime_interface::runtime_interface;

#[cfg(feature = "std")]
type Signature = crate::ledger_8::base_crypto_local::signatures::Signature;

#[cfg(feature = "std")]
type Database = crate::ledger_8::ledger_storage_local::db::ParityDb;

#[runtime_interface]
pub trait Ledger8Bridge {
	fn set_default_storage(&mut self) {
		Bridge::<Signature, Database>::set_default_storage(*self)
	}

	fn flush_storage(&mut self) {
		Bridge::<Signature, Database>::flush_storage(*self)
	}

	fn post_block_update(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Database>::post_block_update(*self, state_key, block_context)
	}

	// Current Enabled Version
	fn get_version() -> AllocateAndReturnFatPointer<Vec<u8>> {
		Bridge::<Signature, Database>::get_version()
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
		Bridge::<Signature, Database>::apply_transaction(
			*self,
			state_key,
			tx,
			block_context,
			true,
			runtime_version,
		)
	}

	fn apply_system_transaction(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		_runtime_version: u32,
	) -> AllocateAndReturnByCodec<Result<SystemTransactionAppliedStateRoot, LedgerApiError>> {
		Bridge::<Signature, Database>::apply_system_transaction(*self, state_key, tx, block_context)
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
		let (hash, _) = Bridge::<Signature, Database>::validate_transaction(
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
		Bridge::<Signature, Database>::validate_guaranteed_execution(
			*self,
			state_key,
			tx,
			block_context,
			runtime_version,
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
		Bridge::<Signature, Database>::get_contract_state(state_key, contract_address)
	}

	/*
	 * get_decoded_transaction()
	 */
	// Current Enabled Version
	fn get_decoded_transaction(
		transaction_bytes: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Tx, LedgerApiError>> {
		Bridge::<Signature, Database>::get_decoded_transaction(transaction_bytes)
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
		Bridge::<Signature, Database>::get_zswap_chain_state(state_key, contract_address)
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
		Bridge::<Signature, Database>::get_unclaimed_amount(state_key, beneficiary)
	}

	/*
	 * Returns the Ledger Parameters
	 */
	// Current Enabled Version
	fn get_ledger_parameters(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Database>::get_ledger_parameters(state_key)
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
		Bridge::<Signature, Database>::get_transaction_cost(
			state_key,
			tx,
			&block_context,
			max_weight,
		)
	}

	/*
	 * Returns the Zsawp state root
	 */
	// Current Enabled Version
	fn get_zswap_state_root(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Database>::get_zswap_state_root(state_key)
	}

	/*
	 * Returns the pure ledger state root (without StorableLedgerState wrapping)
	 */
	fn get_ledger_state_root(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Database>::get_ledger_state_root(state_key)
	}

	fn construct_cnight_generates_dust_event(
		value: PassFatPointerAndDecode<u128>,
		owner: PassFatPointerAndRead<&[u8]>,
		time: u64,
		action: u8,
		nonce: PassFatPointerAndDecode<[u8; 32]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Database>::construct_cnight_generates_dust_event(
			value, owner, time, action, nonce,
		)
	}

	fn construct_cnight_generates_dust_system_tx(
		events: PassFatPointerAndDecode<Vec<Vec<u8>>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Database>::construct_cnight_generates_dust_system_tx(events)
	}

	/// Ensures the correct ledger storage is initialized for this runtime version.
	/// Handles rollback: if new version's storage is initialized but we need this version's storage,
	/// drops new version's storage and initializes normal storage.
	/// Returns true if storage was (re)initialized, false if already correct.
	fn ensure_storage_initialized(&mut self) -> bool {
		use ledger_storage_ledger_8::{db::ParityDb, storage::try_get_default_storage};

		// If normal storage already exists, we're good
		if try_get_default_storage::<ParityDb>().is_some() {
			return false;
		}

		crate::drop_all_default_storage();
		// Initialize normal storage
		Bridge::<Signature, Database>::set_default_storage(*self);
		true
	}
}
