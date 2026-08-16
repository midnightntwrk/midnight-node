#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use midnight_primitives::{
		LedgerBlockContextProvider, LedgerStateProviderMut, MidnightSystemTransactionExecutor,
	};

	use alloc::vec::Vec;
	use midnight_node_ledger::types::{
		Hash, active_ledger_bridge as LedgerApi,
		active_version::{
			DeserializationError, LedgerApiError, SerializationError, TransactionError,
		},
	};

	use super::*;
	use core::marker::PhantomData;
	use frame_support::{
		dispatch::GetDispatchInfo,
		migrations::{FailedMigrationHandler, FailedMigrationHandling},
		traits::Contains,
	};

	pub const EXTRA_WEIGHT_TX_SIZE: Weight = Weight::from_parts(20_000_000_000, 0);

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		SystemTransactionApplied(SystemTransactionApplied),
		/// Safe mode was entered, locking out non-governance transactions. Set by
		/// [`EnterSafeModeOnFailedMigration`] when a multi-block migration fails (with
		/// the index of the failed migration in the batch, if known), or manually via
		/// [`Pallet::enter_safe_mode`].
		SafeModeEntered {
			migration: Option<u32>,
		},
		/// Safe mode was exited by governance; normal transaction processing resumes.
		SafeModeExited,
	}

	#[derive(Clone, Debug, PartialEq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
	pub struct SystemTransactionApplied {
		pub hash: Hash,
		pub serialized_system_transaction: Vec<u8>,
	}

	// Ledger errors mirrored from `LedgerApiError`. Flattened (rather than wrapped)
	// so the encoding fits within `MAX_MODULE_ERROR_ENCODED_SIZE`.
	#[pallet::error]
	pub enum Error<T> {
		#[codec(index = 1)]
		SystemTransactionNotAllowedForGovernance,
		#[codec(index = 2)]
		Deserialization(DeserializationError),
		#[codec(index = 3)]
		Serialization(SerializationError),
		#[codec(index = 4)]
		Transaction(TransactionError),
		#[codec(index = 5)]
		LedgerCacheError,
		#[codec(index = 6)]
		NoLedgerState,
		#[codec(index = 7)]
		LedgerStateScaleDecodingError,
		#[codec(index = 8)]
		ContractCallCostError,
		#[codec(index = 9)]
		BlockLimitExceededError,
		#[codec(index = 10)]
		FeeCalculationError,
		#[codec(index = 11)]
		HostApiError,
		#[codec(index = 12)]
		GetTransactionContextError,
		#[codec(index = 13)]
		ContractNotPresent,
		#[codec(index = 14)]
		BeneficiaryNotFound,
	}

	impl<T: Config> From<LedgerApiError> for Error<T> {
		fn from(value: LedgerApiError) -> Self {
			match value {
				LedgerApiError::Deserialization(e) => Error::<T>::Deserialization(e),
				LedgerApiError::Serialization(e) => Error::<T>::Serialization(e),
				LedgerApiError::Transaction(e) => Error::<T>::Transaction(e),
				LedgerApiError::LedgerCacheError => Error::<T>::LedgerCacheError,
				LedgerApiError::NoLedgerState => Error::<T>::NoLedgerState,
				LedgerApiError::LedgerStateScaleDecodingError => {
					Error::<T>::LedgerStateScaleDecodingError
				},
				LedgerApiError::ContractCallCostError => Error::<T>::ContractCallCostError,
				LedgerApiError::BlockLimitExceededError => Error::<T>::BlockLimitExceededError,
				LedgerApiError::FeeCalculationError => Error::<T>::FeeCalculationError,
				LedgerApiError::HostApiError => Error::<T>::HostApiError,
				LedgerApiError::GetTransactionContextError => {
					Error::<T>::GetTransactionContextError
				},
				LedgerApiError::ContractNotPresent => Error::<T>::ContractNotPresent,
				LedgerApiError::BeneficiaryNotFound => Error::<T>::BeneficiaryNotFound,
			}
		}
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type LedgerStateProviderMut: LedgerStateProviderMut;
		type LedgerBlockContextProvider: LedgerBlockContextProvider;

		/// Calls that remain dispatchable while safe mode is active. This should be the
		/// set of governance calls required to drive an on-chain recovery (see
		/// [`SafeModeFilter`]). Everything else is blocked while safe mode is active.
		type WhitelistedCalls: Contains<<Self as frame_system::Config>::RuntimeCall>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::type_value]
	pub fn DefaultTransactionSizeWeight() -> Weight {
		EXTRA_WEIGHT_TX_SIZE
	}

	#[pallet::storage]
	pub type ConfigurableSystemTxWeight<T> =
		StorageValue<_, Weight, ValueQuery, DefaultTransactionSizeWeight>;

	/// Whether the chain is in safe mode.
	///
	/// Set to `true` by [`EnterSafeModeOnFailedMigration`] when a multi-block migration
	/// fails (or manually via [`Pallet::enter_safe_mode`]). While `true`, the
	/// [`SafeModeFilter`] blocks all non-whitelisted (i.e. non-governance) transactions
	/// so the inconsistent state cannot be touched by users, until governance calls
	/// [`Pallet::exit_safe_mode`]. Kept in its own storage rather than reusing the
	/// migration cursor, since the failed-migration handler returns `ForceUnstuck`
	/// (which clears that cursor) so the chain keeps producing blocks.
	#[pallet::storage]
	pub type SafeMode<T> = StorageValue<_, bool, ValueQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight((ConfigurableSystemTxWeight::<T>::get(), DispatchClass::Operational))]
		pub fn send_mn_system_transaction(
			origin: OriginFor<T>,
			midnight_system_tx: Vec<u8>,
		) -> DispatchResult {
			ensure_root(origin)?;
			ensure!(
				LedgerApi::is_governance_allowed_system_tx(&midnight_system_tx),
				Error::<T>::SystemTransactionNotAllowedForGovernance
			);

			let runtime_version = <frame_system::Pallet<T>>::runtime_version().spec_version;
			let block_context = <T as Config>::LedgerBlockContextProvider::get_block_context();

			let hash = <T as Config>::LedgerStateProviderMut::mut_ledger_state(|state_key| {
				let result = LedgerApi::apply_system_transaction(
					&state_key,
					&midnight_system_tx.clone(),
					block_context,
					runtime_version,
				)
				.map_err(Error::<T>::from)?;
				Ok::<(Vec<u8>, Hash), Error<T>>((result.state_root, result.tx_hash))
			})?;

			Self::deposit_event(Event::<T>::SystemTransactionApplied(
				super::SystemTransactionApplied {
					hash,
					serialized_system_transaction: midnight_system_tx,
				},
			));

			Ok(())
		}

		/// Exit safe mode, resuming normal transaction processing.
		///
		/// Callable by root — in practice via a `pallet-federated-authority` governance
		/// motion — once the state that caused a migration failure has been repaired.
		#[pallet::call_index(1)]
		#[pallet::weight((T::DbWeight::get().writes(1), DispatchClass::Operational))]
		pub fn exit_safe_mode(origin: OriginFor<T>) -> DispatchResult {
			ensure_root(origin)?;
			SafeMode::<T>::put(false);
			Self::deposit_event(Event::<T>::SafeModeExited);
			Ok(())
		}

		/// Manually enter safe mode (for drills or an emergency lockdown).
		///
		/// Callable by root. The automatic path is [`EnterSafeModeOnFailedMigration`];
		/// this call is for deliberately locking the chain down out of band.
		#[pallet::call_index(2)]
		#[pallet::weight((T::DbWeight::get().writes(1), DispatchClass::Operational))]
		pub fn enter_safe_mode(origin: OriginFor<T>) -> DispatchResult {
			ensure_root(origin)?;
			SafeMode::<T>::put(true);
			Self::deposit_event(Event::<T>::SafeModeEntered { migration: None });
			Ok(())
		}
	}

	impl<T: Config> MidnightSystemTransactionExecutor for Pallet<T> {
		fn execute_system_transaction(
			serialized_system_transaction: Vec<u8>,
		) -> Result<Hash, DispatchError> {
			// Apply the System transaction
			let hash = <T as Config>::LedgerStateProviderMut::mut_ledger_state(|state_key| {
				let runtime_version = <frame_system::Pallet<T>>::runtime_version().spec_version;
				let block_context = <T as Config>::LedgerBlockContextProvider::get_block_context();
				let result = LedgerApi::apply_system_transaction(
					&state_key,
					&serialized_system_transaction.clone(),
					block_context,
					runtime_version,
				)
				.map_err(Error::<T>::from)?;
				Ok::<(Vec<u8>, Hash), Error<T>>((result.state_root, result.tx_hash))
			})?;

			// Emit System Transaction for the indexer
			Self::deposit_event(Event::<T>::SystemTransactionApplied(
				super::SystemTransactionApplied { hash, serialized_system_transaction },
			));

			Ok(hash)
		}
	}

	// ===== Safe mode (multi-block-migration failure recovery) =====

	/// [`FailedMigrationHandler`] that enters safe mode instead of freezing the chain
	/// when a multi-block migration fails.
	///
	/// `frame_support::migrations::FreezeChainOnFailedMigration` returns
	/// [`FailedMigrationHandling::KeepStuck`], which leaves the migration cursor stuck
	/// and forces the executive into `OnlyInherents` mode forever — blocking *every*
	/// extrinsic, including governance, so the chain can only be recovered off-chain.
	///
	/// This handler instead records the failure, enters [`SafeMode`], and returns
	/// [`FailedMigrationHandling::ForceUnstuck`] so the executive resumes including
	/// extrinsics. The [`SafeModeFilter`] then keeps *user* transactions locked out
	/// while letting whitelisted governance calls through, so the failed migration can
	/// be repaired on-chain (then re-enabled by [`Pallet::exit_safe_mode`]).
	///
	/// `ForceUnstuck` clears the migration cursor, so the failed migration will not be
	/// retried within the same runtime version — recovery requires a corrected runtime.
	pub struct EnterSafeModeOnFailedMigration<T>(PhantomData<T>);

	impl<T: Config> FailedMigrationHandler for EnterSafeModeOnFailedMigration<T> {
		fn failed(migration: Option<u32>) -> FailedMigrationHandling {
			SafeMode::<T>::put(true);
			Pallet::<T>::deposit_event(Event::<T>::SafeModeEntered { migration });
			log::error!(
				target: "runtime::midnight-system",
				"Multi-block migration {migration:?} failed; entering safe mode. Only \
				 whitelisted governance calls are now permitted until \
				 pallet_midnight_system::exit_safe_mode is called by governance.",
			);
			FailedMigrationHandling::ForceUnstuck
		}
	}

	/// Base-call-filter component implementing safe mode.
	///
	/// Intended to be composed into `frame_system::Config::BaseCallFilter`, e.g.
	/// `InsideBoth<SafeModeFilter<Runtime>, TxPause>`. While [`SafeMode`] is active only
	/// whitelisted governance calls are permitted. Two carve-outs keep the chain
	/// operable and recoverable:
	/// - inherents (`DispatchClass::Mandatory`) are always allowed, so block production
	///   never stalls;
	/// - root-origin dispatches bypass the base call filter entirely (see
	///   `construct_runtime`'s `filter_call`), so governance's privileged inner calls
	///   (e.g. `System::set_code`, `MultiBlockMigrations::force_*`, `exit_safe_mode`)
	///   are unaffected and need not be whitelisted.
	pub struct SafeModeFilter<T>(PhantomData<T>);

	impl<T: Config> Contains<<T as frame_system::Config>::RuntimeCall> for SafeModeFilter<T>
	where
		<T as frame_system::Config>::RuntimeCall: GetDispatchInfo,
	{
		fn contains(call: &<T as frame_system::Config>::RuntimeCall) -> bool {
			// Never filter inherents, or the chain cannot produce blocks.
			if call.get_dispatch_info().class == DispatchClass::Mandatory {
				return true;
			}
			// Outside safe mode everything is allowed (subject to other filters).
			if !SafeMode::<T>::get() {
				return true;
			}
			// In safe mode, only whitelisted governance recovery calls are allowed.
			T::WhitelistedCalls::contains(call)
		}
	}
}
