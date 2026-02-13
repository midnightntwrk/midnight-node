#[cfg(feature = "std")]
use crate::hard_fork_test::{Bridge, LOG_TARGET};
use crate::{
	common::types::{
		GasCost, Hash, SystemTransactionAppliedStateRoot, TransactionAppliedStateRoot,
		TransactionDetails, Tx,
	},
	hard_fork_test::{BlockContext, types::LedgerApiError},
};

use alloc::vec::Vec;
use sp_runtime_interface::pass_by::{
	AllocateAndReturnByCodec, AllocateAndReturnFatPointer, PassFatPointerAndDecode,
	PassFatPointerAndRead,
};
use sp_runtime_interface::runtime_interface;

#[cfg(feature = "std")]
type Database = ledger_storage_hf::db::ParityDb;

#[cfg(feature = "std")]
type Signature = base_crypto::signatures::Signature;

#[runtime_interface]
pub trait LedgerBridgeHf {
	fn set_default_storage(&mut self) {
		Bridge::<Signature, Database>::set_default_storage(*self)
	}

	fn drop_default_storage(&mut self) {
		use ledger_storage::{
			db::ParityDb,
			storage::{try_get_default_storage, unsafe_drop_default_storage},
		};
		unsafe_drop_default_storage::<ParityDb>();

		match try_get_default_storage::<ParityDb>() {
			Some(_) => {
				log::error!(
					target: LOG_TARGET,
					"Pre Hard-fork Default Storage wasn't successfully dropped, still exists",
				);
			},
			None => {
				log::info!(
					target: LOG_TARGET,
					"Pre Hard-fork Default Storage was successfully dropped",
				);
			},
		};
	}

	fn flush_storage(&mut self) {
		Bridge::<Signature, Database>::flush_storage(*self)
	}

	fn pre_fetch_storage(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<(), LedgerApiError>> {
		Bridge::<Signature, Database>::pre_fetch_storage(*self, state_key)
	}

	fn post_block_update(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Database>::post_block_update(*self, state_key, block_context)
	}

	// Version for hard-fork
	fn get_version() -> AllocateAndReturnFatPointer<Vec<u8>> {
		Bridge::<Signature, Database>::get_version()
	}

	// Hard-fork Version
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

	fn validate_transaction(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		runtime_version: u32,
		// The Runtime's max weight as of now
		max_weight: u64,
	) -> AllocateAndReturnByCodec<Result<(Hash, TransactionDetails), LedgerApiError>> {
		let (hash, Some(tx_details)) = Bridge::<Signature, Database>::validate_transaction(
			*self,
			state_key,
			tx,
			block_context,
			runtime_version,
			max_weight,
			true,
		)?
		else {
			// This should never happen
			log::error!("error: transaction_details is None");
			return Err(LedgerApiError::HostApiError);
		};
		Ok((hash, tx_details))
	}

	#[version(2)]
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

	// Hard-fork Version
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

	// Hard-fork Version
	fn get_contract_state(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		contract_address: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Database>::get_contract_state(state_key, contract_address)
	}

	// Hard-fork Version
	fn get_decoded_transaction(
		transaction_bytes: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Tx, LedgerApiError>> {
		Bridge::<Signature, Database>::get_decoded_transaction(transaction_bytes)
	}

	// Hard-fork Version
	fn get_zswap_chain_state(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		contract_address: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Database>::get_zswap_chain_state(state_key, contract_address)
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
	/// Handles upgrade from normal: if normal storage is initialized but we need HF storage,
	/// drops normal storage and initializes HF storage.
	/// Returns true if storage was (re)initialized, false if already correct.
	fn ensure_storage_initialized(&mut self) -> bool {
		use ledger_storage_hf::{db::ParityDb, storage::try_get_default_storage};

		// If normal storage already exists, we're good
		if try_get_default_storage::<ParityDb>().is_some() {
			return false;
		}

		crate::drop_all_default_storage();
		// Initialize normal storage
		Bridge::<Signature, Database>::set_default_storage(*self);
		true
	}

	// Hard-fork Version
	fn get_unclaimed_amount(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		beneficiary: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<u128, LedgerApiError>> {
		Bridge::<Signature, Database>::get_unclaimed_amount(state_key, beneficiary)
	}

	// Hard-fork Version
	fn get_ledger_parameters(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Database>::get_ledger_parameters(state_key)
	}

	// Hard-fork Version
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

	// Hard-fork Version
	fn get_zswap_state_root(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Database>::get_zswap_state_root(state_key)
	}

	fn get_ledger_state_root(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Database>::get_ledger_state_root(state_key)
	}
}
