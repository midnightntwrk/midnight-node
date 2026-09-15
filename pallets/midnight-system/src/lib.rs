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
		LedgerBlockContextProvider, LedgerStateProviderMut,
		MidnightSystemTransactionBridgeExecutor, MidnightSystemTransactionCNightExecutor,
	};

	use alloc::vec::Vec;
	use midnight_node_ledger::types::{
		Hash, SystemTransactionAppliedStateRoot, active_ledger_bridge as LedgerApi,
		active_version::{
			BlockContext, DeserializationError, LedgerApiError, SerializationError,
			SystemTransactionError, TransactionError,
		},
	};

	use super::*;

	pub const EXTRA_WEIGHT_TX_SIZE: Weight = Weight::from_parts(20_000_000_000, 0);

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		SystemTransactionApplied(SystemTransactionApplied),
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
		#[codec(index = 15)]
		SystemTransactionNotAllowedForCNight,
		#[codec(index = 16)]
		SystemTransactionNotAllowedForBridge,
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

	/// Shape shared by every `LedgerApi::apply_*_system_transaction` entry point.
	type ApplySystemTransactionFn = fn(
		&[u8],
		&[u8],
		BlockContext,
		u32,
	)
		-> Result<SystemTransactionAppliedStateRoot, LedgerApiError>;

	impl<T: Config> Pallet<T> {
		/// Applies a system transaction via the given ledger entry point and emits
		/// `SystemTransactionApplied` on success. `apply` is one of the caller-restricted
		/// `LedgerApi::apply_*_system_transaction` functions; `not_allowed` is the
		/// friendly error to surface when its allow-list guard rejects the transaction.
		fn apply_and_emit(
			serialized_system_transaction: Vec<u8>,
			apply: ApplySystemTransactionFn,
			not_allowed: Error<T>,
		) -> Result<Hash, DispatchError> {
			let hash = <T as Config>::LedgerStateProviderMut::mut_ledger_state(|state_key| {
				let runtime_version = <frame_system::Pallet<T>>::runtime_version().spec_version;
				let block_context = <T as Config>::LedgerBlockContextProvider::get_block_context();
				let result = apply(
					&state_key,
					&serialized_system_transaction.clone(),
					block_context,
					runtime_version,
				)
				.map_err(|e| match e {
					LedgerApiError::Transaction(TransactionError::SystemTransaction(
						SystemTransactionError::NotAllowedForCaller,
					)) => not_allowed,
					other => Error::<T>::from(other),
				})?;
				Ok::<(Vec<u8>, Hash), Error<T>>((result.state_root, result.tx_hash))
			})?;

			Self::deposit_event(Event::<T>::SystemTransactionApplied(
				super::SystemTransactionApplied { hash, serialized_system_transaction },
			));

			Ok(hash)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight((ConfigurableSystemTxWeight::<T>::get(), DispatchClass::Operational))]
		pub fn send_mn_system_transaction(
			origin: OriginFor<T>,
			midnight_system_tx: Vec<u8>,
		) -> DispatchResult {
			ensure_root(origin)?;
			Self::apply_and_emit(
				midnight_system_tx,
				LedgerApi::apply_governance_system_transaction,
				Error::<T>::SystemTransactionNotAllowedForGovernance,
			)?;
			Ok(())
		}
	}

	impl<T: Config> MidnightSystemTransactionCNightExecutor for Pallet<T> {
		fn execute_system_transaction(
			serialized_system_transaction: Vec<u8>,
		) -> Result<Hash, DispatchError> {
			Self::apply_and_emit(
				serialized_system_transaction,
				LedgerApi::apply_cnight_system_transaction,
				Error::<T>::SystemTransactionNotAllowedForCNight,
			)
		}

		fn is_block_limit_exceeded(err: &DispatchError) -> bool {
			*err == Error::<T>::BlockLimitExceededError.into()
		}
	}

	impl<T: Config> MidnightSystemTransactionBridgeExecutor for Pallet<T> {
		fn execute_system_transaction(
			serialized_system_transaction: Vec<u8>,
		) -> Result<Hash, DispatchError> {
			Self::apply_and_emit(
				serialized_system_transaction,
				LedgerApi::apply_bridge_system_transaction,
				Error::<T>::SystemTransactionNotAllowedForBridge,
			)
		}

		fn is_block_limit_exceeded(err: &DispatchError) -> bool {
			*err == Error::<T>::BlockLimitExceededError.into()
		}
	}
}
