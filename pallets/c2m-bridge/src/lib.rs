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

//! cNight Cardano-to-Midnight bridge transfer handling.
//!
//! This pallet implements the [`pallet_partner_chains_bridge::TransferHandler`] trait
//! with Midnight-specific logic.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use frame_support::pallet_prelude::*;
pub use pallet::*;

/// Hash of a Midnight ledger transaction, returned by the system transaction executor.
pub type MidnightTxHash = [u8; 32];

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use alloc::vec::Vec;
	use frame_system::pallet_prelude::*;
	use midnight_node_ledger::types::{
		active_ledger_bridge as LedgerApi, active_version::LedgerApiError,
	};
	use midnight_primitives::{BridgeRecipient, MidnightSystemTransactionExecutor};
	use sidechain_domain::McTxHash;
	use sp_partner_chains_bridge::{
		BridgeTransferV1, SubminimalTransfersConfig, TransferRecipient,
	};

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Provides access to the Midnight system transaction executor.
		type MidnightSystemTransactionExecutor: MidnightSystemTransactionExecutor;

		/// Origin for governance extrinsic calls.
		type GovernanceOrigin: EnsureOrigin<Self::RuntimeOrigin>;
	}

	/// Provides access to the minimum bridge transfer amount from the Midnight ledger.
	pub trait MinBridgeAmountProvider {
		/// Returns the minimum bridge transfer amount from ledger parameters.
		fn get_c_to_m_bridge_min_amount() -> Result<u128, LedgerApiError>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Emitted for each successfully handled bridge transfer.
		Transfer {
			/// Main chain transaction hash for correlation of PC with MC.
			mc_tx_hash: McTxHash,
			/// Amount of tokens that were transferred.
			amount: u64,
			/// Hash of the Midnight system transaction produced by the handler.
			result: MidnightTxHash,
			/// Beneficiary of the transfer.
			recipient: TransferRecipient<BridgeRecipient>,
		},
	}

	#[pallet::storage]
	pub type SubminimalTransfersConfiguration<T: Config> =
		StorageValue<_, SubminimalTransfersConfig, ValueQuery>;

	/// Block-scoped counter used for deterministic nonce generation per transfer.
	/// Because on_finalize kill call, it doesn't cost any storage operations.
	#[pallet::storage]
	pub type TransferCounter<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Genesis configuration of the pallet.
	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		/// Initial subminimal transfers configuration.
		pub subminimal_transfers_config: SubminimalTransfersConfig,
		#[allow(missing_docs)]
		pub _marker: PhantomData<T>,
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				subminimal_transfers_config: SubminimalTransfersConfig::default(),
				_marker: Default::default(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			let GenesisConfig { subminimal_transfers_config, _marker } = self;
			SubminimalTransfersConfiguration::<T>::put(subminimal_transfers_config.clone());
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_finalize(_n: BlockNumberFor<T>) {
			TransferCounter::<T>::kill();
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Update the subminimal transfers configuration.
		///
		/// Must be called via governance (e.g. `sudo` or council).
		#[pallet::call_index(0)]
		#[pallet::weight(T::DbWeight::get().writes(1))]
		pub fn set_subminimal_transfers_config(
			origin: OriginFor<T>,
			config: SubminimalTransfersConfig,
		) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;
			SubminimalTransfersConfiguration::<T>::put(config);
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Returns the current subminimal transfers configuration.
		pub fn get_subminimal_transfers_config() -> SubminimalTransfersConfig {
			SubminimalTransfersConfiguration::<T>::get()
		}

		fn next_counter() -> u32 {
			let counter = TransferCounter::<T>::get();
			TransferCounter::<T>::put(counter + 1);
			counter
		}

		/// Generate a deterministic unique nonce for a bridge transfer.
		///
		/// Uses the parent hash (unique per block) combined with an
		/// increasing counter (unique within a block) to guarantee uniqueness.
		fn generate_nonce(counter: u32) -> [u8; 32] {
			let parent_hash = frame_system::Pallet::<T>::parent_hash();
			let mut data = Vec::new();
			data.extend(b"midnight:bridge-transfer-nonce:");
			data.extend(parent_hash.as_ref());
			data.extend(&counter.to_le_bytes());
			sp_core::hashing::blake2_256(&data)
		}

		fn construct_and_execute(
			counter: u32,
			transfer: &BridgeTransferV1<BridgeRecipient>,
		) -> Option<MidnightTxHash> {
			let serialized_tx = Self::construct_tx(counter, transfer)?;
			match T::MidnightSystemTransactionExecutor::execute_system_transaction(
				serialized_tx.clone(),
			) {
				Ok(hash) => Some(hash),
				Err(e) => {
					log::error!("Failed to execute system transaction {serialized_tx:?}: {e:?}");
					None
				},
			}
		}

		fn construct_tx(
			counter: u32,
			transfer: &BridgeTransferV1<BridgeRecipient>,
		) -> Option<Vec<u8>> {
			let amount = transfer.amount;
			let construct_result = match &transfer.recipient {
				TransferRecipient::Address { recipient } => {
					let nonce = Self::generate_nonce(counter);
					LedgerApi::construct_distribute_night_cardano_bridge_system_tx(
						amount.into(),
						recipient.as_bytes(),
						nonce,
					)
				},
				TransferRecipient::Reserve => {
					LedgerApi::construct_distribute_reserve_system_tx(amount.into())
				},
				TransferRecipient::Invalid => {
					LedgerApi::construct_distribute_treasury_system_tx(amount.into())
				},
			};
			match construct_result {
				Ok(tx) => {
					log::debug!("Constructed tx for '{transfer:?}'");
					Some(tx)
				},
				Err(e) => {
					log::error!("Failed to construct tx for '{transfer:?}': {e}");
					None
				},
			}
		}

		fn execute_transfer(counter: u32, transfer: BridgeTransferV1<BridgeRecipient>) {
			let maybe_hash = Self::construct_and_execute(counter, &transfer);
			if let Some(hash) = maybe_hash {
				Self::deposit_event(Event::Transfer {
					mc_tx_hash: transfer.mc_tx_hash,
					amount: transfer.amount,
					result: hash,
					recipient: transfer.recipient,
				});
			}
		}
	}

	impl<T: Config> pallet_partner_chains_bridge::TransferHandler<BridgeRecipient> for Pallet<T> {
		fn handle_incoming_transfer(transfer: BridgeTransferV1<BridgeRecipient>) {
			let counter = Self::next_counter();
			Self::execute_transfer(counter, transfer);
		}
	}
}
