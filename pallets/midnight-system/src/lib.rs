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
		Hash, LedgerEvent, active_ledger_bridge as LedgerApi,
		active_version::{
			DeserializationError, LedgerApiError, SerializationError, TransactionError,
		},
	};

	use super::*;

	pub const EXTRA_WEIGHT_TX_SIZE: Weight = Weight::from_parts(20_000_000_000, 0);

	/// Per-ledger-event deposit cost (one `frame_system::Events` state-trie write).
	/// Sized from the `pallet-midnight` `bench_block_full_of_events` guardrail;
	/// placeholder ref-time/proof-size pending the user's benchmark run.
	pub const PER_LEDGER_EVENT_WEIGHT: Weight = Weight::from_parts(5_000_000, 4096);

	/// Worst-case ledger events a single system transaction can deposit, sized to
	/// fit the governance-motion proof envelope (~1 MB / ~4 KiB per event). Bounds
	/// the pre-dispatch weight so a governed system tx stays within a motion's
	/// weight bound; the post-dispatch actual weight reflects the real count.
	/// Distinct from the per-block 50 MB benchmark ceiling in pallet-midnight.
	pub const MAX_SYSTEM_TX_LEDGER_EVENTS: u64 = 200;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		SystemTransactionApplied(SystemTransactionApplied),
		LedgerEvent(LedgerEvent),
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
		// Pre-dispatch weight bounds the base system-tx weight plus the worst-case
		// ledger-event deposit allowance; the actual event count refines it below.
		#[pallet::weight((
			ConfigurableSystemTxWeight::<T>::get()
				.saturating_add(PER_LEDGER_EVENT_WEIGHT.saturating_mul(MAX_SYSTEM_TX_LEDGER_EVENTS)),
			DispatchClass::Operational,
		))]
		pub fn send_mn_system_transaction(
			origin: OriginFor<T>,
			midnight_system_tx: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			ensure!(
				LedgerApi::is_governance_allowed_system_tx(&midnight_system_tx),
				Error::<T>::SystemTransactionNotAllowedForGovernance
			);

			let runtime_version = <frame_system::Pallet<T>>::runtime_version().spec_version;
			let block_context = <T as Config>::LedgerBlockContextProvider::get_block_context();

			let (tx_hash, ledger_events) =
				<T as Config>::LedgerStateProviderMut::mut_ledger_state(|state_key| {
					let result = LedgerApi::apply_system_transaction(
						&state_key,
						&midnight_system_tx.clone(),
						block_context,
						runtime_version,
					)
					.map_err(Error::<T>::from)?;
					// First tuple element is the new state key written back by
					// `mut_ledger_state`; the tx hash and events ride out as the payload.
					Ok::<(Vec<u8>, (Hash, Vec<LedgerEvent>)), Error<T>>((
						result.state_root,
						(result.tx_hash, result.events),
					))
				})?;

			Self::deposit_event(Event::<T>::SystemTransactionApplied(
				super::SystemTransactionApplied {
					hash: tx_hash,
					serialized_system_transaction: midnight_system_tx,
				},
			));

			let ledger_event_count = ledger_events.len() as u64;
			// One runtime event per ledger event
			for ledger_event in ledger_events {
				Self::deposit_event(Event::<T>::LedgerEvent(ledger_event));
			}

			// Refine to the base weight plus the events actually deposited.
			let actual_weight = ConfigurableSystemTxWeight::<T>::get()
				.saturating_add(PER_LEDGER_EVENT_WEIGHT.saturating_mul(ledger_event_count));
			Ok(Some(actual_weight).into())
		}
	}

	impl<T: Config> MidnightSystemTransactionExecutor for Pallet<T> {
		fn execute_system_transaction(
			serialized_system_transaction: Vec<u8>,
		) -> Result<Hash, DispatchError> {
			// Apply the System transaction
			let (tx_hash, ledger_events) =
				<T as Config>::LedgerStateProviderMut::mut_ledger_state(|state_key| {
					let runtime_version = <frame_system::Pallet<T>>::runtime_version().spec_version;
					let block_context =
						<T as Config>::LedgerBlockContextProvider::get_block_context();
					let result = LedgerApi::apply_system_transaction(
						&state_key,
						&serialized_system_transaction.clone(),
						block_context,
						runtime_version,
					)
					.map_err(Error::<T>::from)?;
					// First tuple element is the new state key written back by
					// `mut_ledger_state`; the tx hash and events ride out as the payload.
					Ok::<(Vec<u8>, (Hash, Vec<LedgerEvent>)), Error<T>>((
						result.state_root,
						(result.tx_hash, result.events),
					))
				})?;

			// Emit System Transaction for the indexer
			Self::deposit_event(Event::<T>::SystemTransactionApplied(
				super::SystemTransactionApplied { hash: tx_hash, serialized_system_transaction },
			));

			// One runtime event per ledger event
			for ledger_event in ledger_events {
				Self::deposit_event(Event::<T>::LedgerEvent(ledger_event));
			}

			Ok(tx_hash)
		}

		fn is_block_limit_exceeded(err: &DispatchError) -> bool {
			*err == Error::<T>::BlockLimitExceededError.into()
		}
	}
}
