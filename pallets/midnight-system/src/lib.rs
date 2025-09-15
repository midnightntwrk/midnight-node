#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use midnight_primitives::{
		LedgerBlockContextProvider, LedgerStateProviderMut, MidnightSystemTransactionExecutor,
	};

	use midnight_node_ledger::types::{
		Hash, active_ledger_bridge as LedgerApi, active_version::LedgerApiError,
	};
	use sp_std::vec::Vec;

	use super::*;

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

	#[pallet::error]
	pub enum Error<T> {
		#[codec(index = 0)]
		LedgerApiError(LedgerApiError),
	}

	impl<T: Config> From<LedgerApiError> for Error<T> {
		fn from(value: LedgerApiError) -> Self {
			Error::<T>::LedgerApiError(value)
		}
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type LedgerStateProviderMut: LedgerStateProviderMut;
		type LedgerBlockContextProvider: LedgerBlockContextProvider;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight({0})]
		pub fn send_mn_system_transaction(
			origin: OriginFor<T>,
			midnight_system_tx: Vec<u8>,
		) -> DispatchResult {
			ensure_root(origin)?;

			let runtime_version = <frame_system::Pallet<T>>::runtime_version().spec_version;
			let block_context = <T as Config>::LedgerBlockContextProvider::get_block_context();

			<T as Config>::LedgerStateProviderMut::mut_ledger_state(move |state_key| {
				let result = LedgerApi::apply_system_transaction(
					&state_key,
					&midnight_system_tx,
					block_context,
					runtime_version,
				)
				.map_err(Error::<T>::from)?;
				Ok::<(Vec<u8>, ()), Error<T>>((result.state_root, ()))
			})?;

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
