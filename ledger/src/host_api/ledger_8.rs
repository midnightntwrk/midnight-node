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

	fn is_governance_allowed_system_tx(system_tx: PassFatPointerAndRead<&[u8]>) -> bool {
		Bridge::<Signature, Database>::is_governance_allowed_system_tx(system_tx)
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

	fn construct_distribute_night_cardano_bridge_system_tx(
		amount: PassFatPointerAndDecode<u128>,
		target_address_bytes: PassFatPointerAndRead<&[u8]>,
		nonce_bytes: PassFatPointerAndDecode<[u8; 32]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Database>::construct_distribute_night_cardano_bridge_system_tx(
			amount,
			target_address_bytes,
			nonce_bytes,
		)
	}

	fn construct_distribute_reserve_system_tx(
		amount: PassFatPointerAndDecode<u128>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Database>::construct_distribute_reserve_system_tx(amount)
	}

	fn construct_distribute_treasury_system_tx(
		amount: PassFatPointerAndDecode<u128>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		Bridge::<Signature, Database>::construct_distribute_treasury_system_tx(amount)
	}

	/// Build the supply-preserving `reserve -> locked/treasury` correction needed to bring the
	/// live pools up to `target_locked` / `target_treasury`, returned as a tagged-serialized
	/// `SystemTransaction::SeedPoolsFromReserve`.
	///
	/// Self-targeting by construction: it moves only the *gap* (`target - current`, saturating),
	/// so it no-ops cleanly on networks already at or above target (returning an empty `Vec`).
	/// This is the mechanism that makes `SeedPreviewLockedPool` safe to ship to every network —
	/// only an under-funded network (Preview: locked 0) receives a non-empty move. The
	/// `SeedPoolsFromReserve` variant exists only in ledger 8, so this is deliberately a
	/// ledger-8-only host function (it cannot live in the version-shared `common` module).
	fn construct_seed_pools_to_target_tx(
		state_key: PassFatPointerAndRead<&[u8]>,
		target_locked: PassFatPointerAndDecode<u128>,
		target_treasury: PassFatPointerAndDecode<u128>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, LedgerApiError>> {
		build_seed_pools_to_target_tx(state_key, target_locked, target_treasury)
	}

	/// Read the live NIGHT pools `(locked_pool, reserve_pool, treasury[NIGHT])` from the ledger
	/// state. Read-only; supports operational queries and direct verification of pool-rebalance
	/// migrations (e.g. confirming `SeedPreviewLockedPool` moved Preview's locked pool to target).
	fn get_night_pools(
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<(u128, u128, u128), LedgerApiError>> {
		build_night_pools(state_key)
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

/// Host-side body for `Ledger8Bridge::construct_seed_pools_to_target_tx`. Lives here (the
/// ledger-8-only module) rather than the version-shared `common` module because it references the
/// `SeedPoolsFromReserve` variant, which exists only in ledger 8.
#[cfg(feature = "std")]
fn build_seed_pools_to_target_tx(
	state_key: &[u8],
	target_locked: u128,
	target_treasury: u128,
) -> Result<Vec<u8>, LedgerApiError> {
	use crate::ledger_8::api::{self, SystemTransaction};
	use crate::ledger_8::coin_structure_local::coin::{NIGHT, TokenType};

	let api = api::new();
	let ledger = Bridge::<Signature, Database>::get_ledger(&api, state_key)?;
	let current_locked = ledger.state.locked_pool;
	let current_treasury_night = ledger
		.state
		.treasury
		.get(&TokenType::Unshielded(NIGHT))
		.copied()
		.unwrap_or(0);

	// Move only the gap to the target (saturating), so already-healthy networks no-op cleanly.
	let to_locked = target_locked.saturating_sub(current_locked);
	let to_treasury = target_treasury.saturating_sub(current_treasury_night);
	if to_locked == 0 && to_treasury == 0 {
		log::info!(
			target: "runtime::ledger",
			"construct_seed_pools_to_target_tx: pools already at/above target (locked={current_locked}, treasury_night={current_treasury_night}); nothing to do"
		);
		return Ok(Vec::new());
	}

	log::info!(
		target: "runtime::ledger",
		"construct_seed_pools_to_target_tx: locked {current_locked}->{target_locked} (+{to_locked}), treasury_night {current_treasury_night}->{target_treasury} (+{to_treasury})"
	);
	let system_tx = SystemTransaction::SeedPoolsFromReserve { to_locked, to_treasury };
	api.tagged_serialize(&system_tx)
}

/// Host-side body for `Ledger8Bridge::get_night_pools`: read-only `(locked, reserve, treasury[NIGHT])`.
#[cfg(feature = "std")]
fn build_night_pools(state_key: &[u8]) -> Result<(u128, u128, u128), LedgerApiError> {
	use crate::ledger_8::api;
	use crate::ledger_8::coin_structure_local::coin::{NIGHT, TokenType};

	let api = api::new();
	let ledger = Bridge::<Signature, Database>::get_ledger(&api, state_key)?;
	let treasury_night = ledger
		.state
		.treasury
		.get(&TokenType::Unshielded(NIGHT))
		.copied()
		.unwrap_or(0);
	Ok((ledger.state.locked_pool, ledger.state.reserve_pool, treasury_night))
}
