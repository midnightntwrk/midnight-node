#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;

pub use pallet::*;

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
}
